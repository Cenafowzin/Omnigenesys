use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    access::WriteHandle,
    filter::{CompiledFilter, ObjectFilter, Scalar, Subject},
    object::Value,
    operator::{
        BuildContext, BuildError, Context, Dimensions, OpError, Operator, OperatorType, Scope,
    },
    rng::Chance,
};

use super::common::{
    Amount, Count, Order, OrderKey, always, chance, filters, one, passing, prepare_all, props,
    reads_besides, scene, too_few,
};

/// Picks objects and sets props on them: the objects that pass `where`
/// (or the first `fallback` some object passes), sorted by `order_by`, the
/// first `count` of them. One decision per step — the boss room, then the
/// shop, then the treasure room, each a step of its own, in sight (Isaac
/// Rebirth places its special rooms one after another; pesquisa
/// 2026-10-25, item 4).
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TagObjectsConfig {
    /// Object layer whose objects are picked.
    pub layer: String,
    #[serde(rename = "where", default)]
    pub filter: Vec<ObjectFilter>,
    /// Tried in order when no object passes `where`.
    #[serde(default)]
    pub fallback: Vec<Vec<ObjectFilter>>,
    #[serde(default)]
    pub order_by: Vec<OrderKey>,
    #[serde(default = "one")]
    pub count: Count,
    /// Chance the step picks anything at all (0 to 1). A failed roll
    /// changes nothing and is not a failure: the objects stay free for
    /// the steps after it.
    #[serde(default = "always")]
    #[schemars(range(min = 0, max = 1))]
    pub chance: f64,
    /// Props the picked objects get.
    pub set: BTreeMap<String, Scalar>,
}

pub struct TagObjects {
    layer: WriteHandle,
    filters: Vec<CompiledFilter>,
    order: Order,
    amount: Amount,
    chance: Chance,
    set: Vec<(String, Value)>,
}

impl OperatorType for TagObjects {
    const NAME: &'static str = "TagObjects";
    const DIMENSIONS: Dimensions = Dimensions::Any;
    const SCOPE: Scope = Scope::Global;
    type Config = TagObjectsConfig;

    fn build(c: TagObjectsConfig, bcx: &mut BuildContext<'_>) -> Result<TagObjects, BuildError> {
        if c.set.is_empty() {
            return Err(BuildError::Invalid {
                field: "set".into(),
                message: "say which props the picked objects get".into(),
            });
        }
        let layer = bcx.write_objects(&c.layer)?;
        Ok(TagObjects {
            layer,
            filters: filters(bcx, &c.filter, &c.fallback, layer.layer())?,
            order: Order::new(&c.order_by, bcx, layer.layer())?,
            amount: Amount::new(c.count)?,
            chance: chance(c.chance, "chance")?,
            set: props(&c.set),
        })
    }
}

impl Operator for TagObjects {
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
        let scene = scene(map, &split, &reads, &[self.layer]);
        let id = self.layer.layer();

        let all: Vec<Subject> = (0..scene.objects(id).len()).map(Subject::Object).collect();
        let mut found = passing(&filters, &scene, id, &all);
        order.sort(&scene, id, &mut found, rng)?;
        found.truncate(n as usize);
        let got = found.len() as u32;
        if got < self.amount.min() {
            return Err(too_few(
                "found",
                got,
                self.amount.min(),
                "no other object passes `where`",
            ));
        }
        let layer = split.writes[0]
            .as_objects_mut()
            .expect("checked when built");
        let picked: Vec<usize> = found
            .into_iter()
            .filter_map(|s| match s {
                Subject::Object(i) => Some(i),
                Subject::Slot(_) | Subject::Box(_) => None,
            })
            .collect();
        for (i, o) in layer.iter_mut().enumerate() {
            if picked.contains(&i) {
                for (k, v) in &self.set {
                    o.set(k, v.clone());
                }
            }
        }
        Ok(())
    }
}
