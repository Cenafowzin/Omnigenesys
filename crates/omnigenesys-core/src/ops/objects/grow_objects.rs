use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    access::WriteHandle,
    filter::{CompiledFilter, ObjectFilter, Scalar, Scene, Subject},
    grid::LayerId,
    object::{Lattice, Object, Value},
    operator::{
        BuildContext, BuildError, Context, Dimensions, OpError, Operator, OperatorType, Scope,
    },
    rng::{Chance, pick},
};

use super::common::{Amount, Count, always, chance, props, reads_besides, scene, too_few};

/// Grows objects into the empty slots next to them, one slot at a time,
/// from a frontier of objects that may still grow (Jamis Buck's growing
/// tree, 2011; pesquisa 2026-10-25, item 11). Each new object goes into
/// the frontier and remembers the one it grew from (`parent`), so the tree
/// the growth made is not lost.
///
/// How the frontier is used is the whole method:
/// - `pick`: which object grows next — `oldest` (a queue: grows ring by
///   ring), `newest` (a stack: long corridors), `random` (Prim-like).
/// - `expand`: `all` tries each empty slot next to it, in the order left,
///   right, up, down, each with `chance`; `one` takes one of them at random.
/// - `retire`: an object leaves the frontier after one `visit`, or when it
///   is `exhausted` (no slot next to it passes `where` any more).
/// - `on_exhausted`: when the frontier empties before `count` is reached,
///   `stop`, or `reseed` it with every object that passes `from` now —
///   those grown too, when `from` takes them (Isaac puts back only the
///   start: `from` the start room).
///
/// The Binding of Isaac grows a floor with `oldest`, `all`, `visit` and a
/// chance of 0.5, where a new room may not touch more than one room.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GrowObjectsConfig {
    /// Object layer, with a lattice, to grow on.
    pub layer: String,
    /// Objects to grow from: the frontier at the start (all, if left out).
    #[serde(default)]
    pub from: Vec<ObjectFilter>,
    /// Objects to add.
    pub count: Count,
    /// Which empty slots a new object may take, looked at when it is
    /// taken (so it counts the objects grown just before).
    #[serde(rename = "where", default)]
    pub filter: Vec<ObjectFilter>,
    /// Chance each try takes its slot (0 to 1).
    #[serde(default = "always")]
    #[schemars(range(min = 0, max = 1))]
    pub chance: f64,
    #[serde(default)]
    pub pick: Pick,
    #[serde(default)]
    pub expand: Expand,
    #[serde(default)]
    pub retire: Retire,
    #[serde(default)]
    pub on_exhausted: OnExhausted,
    /// What the new objects are; the kind of the one each grows from, if
    /// left out.
    #[serde(default)]
    pub kind: Option<String>,
    /// Props every new object gets.
    #[serde(default)]
    pub props: BTreeMap<String, Scalar>,
    /// The prop that names the object each new one grew from.
    #[serde(default = "parent")]
    pub parent: String,
}

pub(super) fn parent() -> String {
    "parent".to_string()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Pick {
    #[default]
    Oldest,
    Newest,
    Random,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Expand {
    #[default]
    All,
    One,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Retire {
    #[default]
    Visit,
    Exhausted,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum OnExhausted {
    #[default]
    Stop,
    Reseed,
}

pub struct GrowObjects {
    layer: WriteHandle,
    from: CompiledFilter,
    amount: Amount,
    filter: CompiledFilter,
    chance: Chance,
    pick: Pick,
    expand: Expand,
    retire: Retire,
    on_exhausted: OnExhausted,
    kind: Option<String>,
    props: Vec<(String, Value)>,
    parent: String,
}

impl OperatorType for GrowObjects {
    const NAME: &'static str = "GrowObjects";
    const DIMENSIONS: Dimensions = Dimensions::TwoD;
    const SCOPE: Scope = Scope::Global;
    type Config = GrowObjectsConfig;

    fn build(c: GrowObjectsConfig, bcx: &mut BuildContext<'_>) -> Result<GrowObjects, BuildError> {
        let layer = bcx.write_objects(&c.layer)?;
        if bcx.lattice(layer.layer()).is_none() {
            return Err(BuildError::Invalid {
                field: "layer".into(),
                message: format!(
                    "layer '{}' has no lattice: give it one (\"lattice\": {{\"columns\", \"rows\", \"cell\"}}) \
                     to grow objects into its slots",
                    c.layer
                ),
            });
        }
        // An object that stays until exhausted, with rolls that can fail,
        // could be picked forever.
        if c.retire == Retire::Exhausted && c.chance < 1.0 {
            return Err(BuildError::Invalid {
                field: "chance".into(),
                message: "with `retire: exhausted` every try takes its slot; \
                          use `retire: visit` for a chance"
                    .into(),
            });
        }
        Ok(GrowObjects {
            layer,
            from: bcx.filter("from", &c.from, layer.layer())?,
            amount: Amount::new(c.count)?,
            filter: bcx.filter("where", &c.filter, layer.layer())?,
            chance: chance(c.chance, "chance")?,
            pick: c.pick,
            expand: c.expand,
            retire: c.retire,
            on_exhausted: c.on_exhausted,
            kind: c.kind,
            props: props(&c.props),
            parent: c.parent,
        })
    }
}

/// The empty slots next to object `i`: for each side in the order left,
/// right, up, down, the slots along that side.
fn around(scene: &Scene, layer: LayerId, lattice: &Lattice, i: usize) -> Vec<usize> {
    let bbox = scene.objects(layer)[i].bbox;
    let own = lattice.footprint(scene.map(), bbox);
    let mut out = Vec::new();
    for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
        for &s in &own {
            let (c, r) = lattice.slot(s);
            let (x, y) = (i64::from(c) + dx, i64::from(r) + dy);
            if x < 0 || y < 0 || x >= i64::from(lattice.columns) || y >= i64::from(lattice.rows) {
                continue;
            }
            let n = lattice.index(x as u32, y as u32);
            if !own.contains(&n) && !out.contains(&n) && scene.in_slot(layer, n).is_none() {
                out.push(n);
            }
        }
    }
    out
}

impl Operator for GrowObjects {
    fn execute(&self, ctx: &mut Context<'_>) -> Result<(), OpError> {
        let parts = ctx.parts();
        let mut grid = parts.grid;
        let rng = parts.rng;
        let map = grid.map();
        let n = self.amount.draw(rng);

        let (from, filter) = (
            grid.prepare_filter(&self.from)?,
            grid.prepare_filter(&self.filter)?,
        );
        let mut layers = self.from.layers();
        layers.extend(self.filter.layers());
        let reads = reads_besides(&layers, &[self.layer]);
        let mut split = grid.split(&reads, &[self.layer])?;
        let mut scene = scene(map, &split, &reads, &[self.layer]);
        let id = self.layer.layer();
        let lattice = *scene.lattice(id).expect("checked when built");

        let seeds = |scene: &Scene| -> Vec<usize> {
            (0..scene.objects(id).len())
                .filter(|&i| from.test(scene, id, Subject::Object(i)))
                .collect()
        };
        let mut frontier = seeds(&scene);
        let mut grown: Vec<Object> = Vec::new();
        // Objects added since the frontier was last filled: a reseed that
        // grows nothing would loop forever.
        let mut since_reseed = 0;
        let takes = |scene: &Scene, s: usize| {
            scene.in_slot(id, s).is_none() && filter.test(scene, id, Subject::Slot(s))
        };

        while (grown.len() as u32) < n {
            if frontier.is_empty() {
                if self.on_exhausted == OnExhausted::Reseed && since_reseed > 0 {
                    frontier = seeds(&scene);
                    since_reseed = 0;
                    continue;
                }
                break;
            }
            let at = match self.pick {
                Pick::Oldest => 0,
                Pick::Newest => frontier.len() - 1,
                Pick::Random => pick(rng.next_u32(), frontier.len() as u32) as usize,
            };
            let i = frontier[at];
            if self.retire == Retire::Visit {
                frontier.remove(at);
            }
            let slots = around(&scene, id, &lattice, i);
            let tries: Vec<usize> = match self.expand {
                Expand::All => slots,
                Expand::One => {
                    let ok: Vec<usize> = slots.into_iter().filter(|&s| takes(&scene, s)).collect();
                    if ok.is_empty() {
                        Vec::new()
                    } else {
                        vec![ok[pick(rng.next_u32(), ok.len() as u32) as usize]]
                    }
                }
            };
            let mut took = false;
            for s in tries {
                if (grown.len() as u32) >= n {
                    break;
                }
                // Looked at again now: an earlier slot of this same visit
                // may have changed what is around this one.
                if !takes(&scene, s) || !self.chance.hit(rng.next_u32()) {
                    continue;
                }
                let from = &scene.objects(id)[i];
                let mut props = self.props.clone();
                props.push((self.parent.clone(), Value::Object(from.id)));
                let (column, row) = lattice.slot(s);
                let object = Object {
                    id: split.ids.fresh(),
                    kind: self.kind.clone().unwrap_or_else(|| from.kind.clone()),
                    bbox: lattice.bounds(map, column, row),
                    props,
                };
                let new = scene.push(id, object.clone());
                grown.push(object);
                frontier.push(new);
                since_reseed += 1;
                took = true;
            }
            if self.retire == Retire::Exhausted
                && !took
                && let Some(at) = frontier.iter().position(|&f| f == i)
            {
                frontier.remove(at);
            }
        }
        let got = grown.len() as u32;
        let layer = split.writes[0]
            .as_objects_mut()
            .expect("checked when built");
        for o in grown {
            layer.insert(o);
        }
        if got < self.amount.min() {
            return Err(too_few(
                "grew",
                got,
                self.amount.min(),
                "no object left that can grow into a slot that passes `where`",
            ));
        }
        Ok(())
    }
}
