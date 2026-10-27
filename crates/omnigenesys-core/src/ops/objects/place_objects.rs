use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    access::WriteHandle,
    bounds::Bounds,
    coords::{Coord, GridSize},
    filter::{CompiledFilter, ObjectFilter, Scalar, Subject},
    object::{Lattice, Object, Value},
    operator::{
        BuildContext, BuildError, Context, Dimensions, OpError, Operator, OperatorType, Scope,
    },
    rng::{Chance, StepRng, pick},
};

use super::common::{
    Amount, Count, CountRange, CountWord, Order, OrderKey, always, chance, filters, one, passing,
    prepare_all, props, reads_besides, scene, too_few,
};

/// Puts new objects on a layer, one at a time: the candidates that pass
/// `where` (or, if none does, the first `fallback` some candidate passes)
/// are sorted by `order_by`, and the first one is taken. After each one
/// the candidates are looked at again, so `neighbours` and `overlaps`
/// count the objects just placed.
///
/// - On a layer with a lattice the candidates are its empty slots, every
///   one: nothing is tried at random and given up after so many tries
///   (pesquisa 2026-10-25, item 8). The start room of a floor, a secret
///   room in the empty slot with the most rooms around it.
/// - On a layer without one, `tries` boxes of `size` are drawn at random
///   places inside the map for each object. With no `order_by` the first
///   that passes is taken — the classic rejection of Rogue and of
///   Wolverson's tutorial; with one, the best of them — Mitchell's best
///   candidate (pesquisa 2026-10-26-masmorra-em-metodos, item 1). Rooms
///   kept one cell apart: `where: overlaps {margin: 1, max: 0}`.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlaceObjectsConfig {
    /// Object layer that gets the objects.
    pub layer: String,
    /// What the new objects are: what an engine reads them as.
    pub kind: String,
    #[serde(default = "one")]
    pub count: Count,
    /// Without a lattice: the size of each object's box, drawn for each
    /// candidate. With one, the slot gives the size.
    #[serde(default)]
    pub size: Option<Size>,
    /// Without a lattice: boxes drawn for each object (1 to 1000; 30 if
    /// left out).
    #[serde(default)]
    #[schemars(range(min = 1, max = 1000))]
    pub tries: Option<u32>,
    /// Which candidates may take one.
    #[serde(rename = "where", default)]
    pub filter: Vec<ObjectFilter>,
    /// Tried in order when no candidate passes `where`.
    #[serde(default)]
    pub fallback: Vec<Vec<ObjectFilter>>,
    #[serde(default)]
    pub order_by: Vec<OrderKey>,
    /// Chance the step places anything at all (0 to 1). A failed roll
    /// places nothing and is not a failure.
    #[serde(default = "always")]
    #[schemars(range(min = 0, max = 1))]
    pub chance: f64,
    /// Props every new object gets.
    #[serde(default)]
    pub props: BTreeMap<String, Scalar>,
}

/// A box's width and height, in cells; each is a whole number or a range
/// (both ends counted).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Size {
    pub width: Side,
    pub height: Side,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(
    untagged,
    expecting = "a whole number, or {\"min\": ..., \"max\": ...}"
)]
pub enum Side {
    Exact(u32),
    Range(CountRange),
}

/// Boxes drawn above this many per object are refused: the step would look
/// at too many candidates for one pick.
const MAX_TRIES: u32 = 1000;
const DEFAULT_TRIES: u32 = 30;

/// Where candidates come from.
#[derive(Debug, Clone, Copy)]
enum Places {
    Slots(Lattice),
    Drawn {
        width: (u32, u32),
        height: (u32, u32),
        tries: u32,
    },
}

pub struct PlaceObjects {
    layer: WriteHandle,
    kind: String,
    amount: Amount,
    places: Places,
    filters: Vec<CompiledFilter>,
    order: Order,
    chance: Chance,
    props: Vec<(String, Value)>,
}

impl OperatorType for PlaceObjects {
    const NAME: &'static str = "PlaceObjects";
    const DIMENSIONS: Dimensions = Dimensions::TwoD;
    const SCOPE: Scope = Scope::Global;
    type Config = PlaceObjectsConfig;

    fn build(
        c: PlaceObjectsConfig,
        bcx: &mut BuildContext<'_>,
    ) -> Result<PlaceObjects, BuildError> {
        let invalid = |field: &str, message: String| BuildError::Invalid {
            field: field.into(),
            message,
        };
        let layer = bcx.write_objects(&c.layer)?;
        let places = match (bcx.lattice(layer.layer()), c.size) {
            (Some(lattice), None) => {
                if c.tries.is_some() {
                    return Err(invalid(
                        "tries",
                        format!(
                            "layer '{}' has a lattice: every empty slot is a candidate, nothing is drawn",
                            c.layer
                        ),
                    ));
                }
                Places::Slots(lattice)
            }
            (Some(_), Some(_)) => {
                return Err(invalid(
                    "size",
                    format!(
                        "layer '{}' has a lattice: each object takes a slot's size",
                        c.layer
                    ),
                ));
            }
            (None, None) => {
                return Err(invalid(
                    "size",
                    format!(
                        "layer '{}' has no lattice: give the objects a size \
                         (\"size\": {{\"width\", \"height\"}}), or the layer a lattice",
                        c.layer
                    ),
                ));
            }
            (None, Some(size)) => {
                if c.count == Count::Word(CountWord::All) {
                    return Err(invalid(
                        "count",
                        "without a lattice there is no \"all\": give a number or a range".into(),
                    ));
                }
                let tries = c.tries.unwrap_or(DEFAULT_TRIES);
                if !(1..=MAX_TRIES).contains(&tries) {
                    return Err(invalid(
                        "tries",
                        format!("{tries} is not between 1 and {MAX_TRIES}"),
                    ));
                }
                Places::Drawn {
                    width: side(size.width, "size.width")?,
                    height: side(size.height, "size.height")?,
                    tries,
                }
            }
        };
        Ok(PlaceObjects {
            layer,
            kind: c.kind,
            amount: Amount::new(c.count)?,
            places,
            filters: filters(bcx, &c.filter, &c.fallback, layer.layer())?,
            order: Order::new(&c.order_by, bcx, layer.layer())?,
            chance: chance(c.chance, "chance")?,
            props: props(&c.props),
        })
    }
}

/// A side checked: at least 1, and `min` not above `max`.
fn side(s: Side, field: &str) -> Result<(u32, u32), BuildError> {
    let (lo, hi) = match s {
        Side::Exact(n) => (n, n),
        Side::Range(CountRange { min, max }) => (min, max),
    };
    if lo == 0 || lo > hi {
        return Err(BuildError::Invalid {
            field: field.into(),
            message: format!("{lo} to {hi}: sizes are 1 or more, and min is not above max"),
        });
    }
    Ok((lo, hi))
}

/// A whole number from `lo` to `hi`, both counted.
fn between(rng: &mut StepRng, (lo, hi): (u32, u32)) -> u32 {
    lo + pick(rng.next_u32(), hi - lo + 1)
}

impl Places {
    /// This pick's candidates: the empty slots, or boxes drawn inside the
    /// map (a box larger than the map is no candidate).
    fn candidates(
        self,
        scene: &crate::filter::Scene,
        layer: crate::grid::LayerId,
        map: Bounds,
        rng: &mut StepRng,
    ) -> Vec<Subject> {
        match self {
            Places::Slots(_) => scene
                .empty_slots(layer)
                .into_iter()
                .map(Subject::Slot)
                .collect(),
            Places::Drawn {
                width,
                height,
                tries,
            } => (0..tries)
                .filter_map(|_| {
                    let w = between(rng, width);
                    let h = between(rng, height);
                    if w > map.size.width || h > map.size.height {
                        return None;
                    }
                    let x = pick(rng.next_u32(), map.size.width - w + 1);
                    let y = pick(rng.next_u32(), map.size.height - h + 1);
                    Some(Subject::Box(Bounds::new(
                        map.origin + Coord::new(x as i32, y as i32, 0),
                        GridSize::new(w, h, 1),
                    )))
                })
                .collect(),
        }
    }
}

impl Operator for PlaceObjects {
    fn execute(&self, ctx: &mut Context<'_>) -> Result<(), OpError> {
        let parts = ctx.parts();
        let mut grid = parts.grid;
        let rng = parts.rng;
        let map = grid.map();
        if !self.chance.hit(rng.next_u32()) {
            return Ok(());
        }
        let n = self.amount.draw(rng);

        let filters = prepare_all(&grid, &self.filters)?;
        let order = self.order.prepare(&grid)?;
        let layers: Vec<_> = self.filters.iter().flat_map(|f| f.layers()).collect();
        let reads = reads_besides(&layers, &[self.layer]);
        let mut split = grid.split(&reads, &[self.layer])?;
        let mut scene = scene(map, &split, &reads, &[self.layer]);
        let id = self.layer.layer();

        let mut placed = Vec::new();
        while (placed.len() as u32) < n {
            let all = self.places.candidates(&scene, id, map, rng);
            let mut found = passing(&filters, &scene, id, &all);
            order.sort(&scene, id, &mut found, rng)?;
            let bbox = match (found.first(), self.places) {
                (Some(&Subject::Slot(slot)), Places::Slots(lattice)) => {
                    let (column, row) = lattice.slot(slot);
                    lattice.bounds(map, column, row)
                }
                (Some(&Subject::Box(b)), _) => b,
                _ => break,
            };
            let object = Object {
                id: split.ids.fresh(),
                kind: self.kind.clone(),
                bbox,
                props: self.props.clone(),
            };
            scene.push(id, object.clone());
            placed.push(object);
        }
        let got = placed.len() as u32;
        let layer = split.writes[0]
            .as_objects_mut()
            .expect("checked when built");
        for o in placed {
            layer.insert(o);
        }
        if got < self.amount.min() {
            let why = match self.places {
                Places::Slots(_) => "no other empty slot passes `where`".to_string(),
                Places::Drawn { tries, .. } => {
                    format!("none of the {tries} boxes drawn for the next one passes `where`")
                }
            };
            return Err(too_few("placed", got, self.amount.min(), &why));
        }
        Ok(())
    }
}
