use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    access::WriteHandle,
    filter::{CompiledFilter, ObjectFilter, Scalar, Subject},
    object::{Object, Value},
    operator::{
        BuildContext, BuildError, Context, Dimensions, OpError, Operator, OperatorType, Scope,
    },
    rng::Chance,
};

use super::common::{
    Amount, Count, Order, OrderKey, always, chance, filters, one, passing, props, reads_besides,
    scene, too_few,
};

/// Puts new objects in empty slots of a lattice, one at a time: the slots
/// that pass `where` (or, if none does, the first `fallback` some slot
/// passes) are sorted by `order_by`, and the first one is taken. After each
/// one the slots are looked at again, so `neighbours` counts the objects
/// just placed. The start room of a floor, a secret room in the empty slot
/// with the most rooms around it.
///
/// Candidates are every empty slot, so nothing is tried at random and
/// given up after so many tries (pesquisa 2026-10-25, item 8).
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlaceObjectsConfig {
    /// Object layer, with a lattice, that gets the objects.
    pub layer: String,
    /// What the new objects are: what an engine reads them as.
    pub kind: String,
    #[serde(default = "one")]
    pub count: Count,
    /// Which empty slots may take one.
    #[serde(rename = "where", default)]
    pub filter: Vec<ObjectFilter>,
    /// Tried in order when no slot passes `where`.
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

pub struct PlaceObjects {
    layer: WriteHandle,
    kind: String,
    amount: Amount,
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
        let layer = bcx.write_objects(&c.layer)?;
        if bcx.lattice(layer.layer()).is_none() {
            return Err(BuildError::Invalid {
                field: "layer".into(),
                message: format!(
                    "layer '{}' has no lattice: give it one (\"lattice\": {{\"columns\", \"rows\", \"cell\"}}) \
                     to place objects in its slots",
                    c.layer
                ),
            });
        }
        Ok(PlaceObjects {
            layer,
            kind: c.kind,
            amount: Amount::new(c.count)?,
            filters: filters(bcx, &c.filter, &c.fallback, layer.layer())?,
            order: Order::new(&c.order_by, bcx, layer.layer())?,
            chance: chance(c.chance, "chance")?,
            props: props(&c.props),
        })
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

        let layers: Vec<_> = self.filters.iter().flat_map(|f| f.layers()).collect();
        let reads = reads_besides(&layers, &[self.layer]);
        let mut split = grid.split(&reads, &[self.layer])?;
        let mut scene = scene(map, &split, &reads, &[self.layer]);
        let id = self.layer.layer();
        let lattice = *scene.lattice(id).expect("checked when built");

        let mut placed = Vec::new();
        while (placed.len() as u32) < n {
            let empty: Vec<Subject> = scene
                .empty_slots(id)
                .into_iter()
                .map(Subject::Slot)
                .collect();
            let mut found = passing(&self.filters, &scene, id, &empty);
            self.order.sort(&scene, id, &mut found, rng)?;
            let Some(&Subject::Slot(slot)) = found.first() else {
                break;
            };
            let (column, row) = lattice.slot(slot);
            let object = Object {
                id: split.ids.fresh(),
                kind: self.kind.clone(),
                bbox: lattice.bounds(map, column, row),
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
            return Err(too_few(
                "placed",
                got,
                self.amount.min(),
                "no other empty slot passes `where`",
            ));
        }
        Ok(())
    }
}
