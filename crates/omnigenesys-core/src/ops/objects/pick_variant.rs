use std::collections::HashMap;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    access::WriteHandle,
    coords::Coord,
    filter::{CompiledFilter, ObjectFilter, Subject},
    object::Value,
    operator::{
        BuildContext, BuildError, Context, Dimensions, OpError, Operator, OperatorType, Scope,
    },
    ops::weights::Weights,
};

use super::common::{filters, passing, prepare_all, reads_besides, scene};

/// Picks a variant for each object by `weights` — which of the lamp posts,
/// which of the trees — and writes its index (0, 1, …) as the whole prop
/// `prop`; with `where`, only on the objects that pass it. The engine's
/// adapter turns type and prop into a prefab. `PickVariant`'s method, on
/// objects instead of cells.
///
/// Each object draws with `rng.at` of its box's corner: the variant is
/// fixed by the seed, and adding, removing or reordering other objects
/// leaves it alone. Objects whose boxes share a corner take the next draws
/// there, in the layer's order.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PickObjectVariantConfig {
    /// Object layer whose objects get a variant.
    pub layer: String,
    /// Which objects (all of the layer, if left out).
    #[serde(rename = "where", default)]
    pub filter: Vec<ObjectFilter>,
    /// The prop that gets the variant (a whole number).
    pub prop: String,
    /// How often each variant comes, against the others: `[3, 1]` gives
    /// variant 0 three times as often as 1. From 1 to 256 of them, each
    /// from 0 to 1000000, not all 0.
    pub weights: Vec<u32>,
}

pub struct PickObjectVariant {
    layer: WriteHandle,
    filters: Vec<CompiledFilter>,
    prop: String,
    weights: Weights,
}

impl OperatorType for PickObjectVariant {
    const NAME: &'static str = "PickObjectVariant";
    const DIMENSIONS: Dimensions = Dimensions::Any;
    const SCOPE: Scope = Scope::Global;
    type Config = PickObjectVariantConfig;

    fn build(
        c: PickObjectVariantConfig,
        bcx: &mut BuildContext<'_>,
    ) -> Result<PickObjectVariant, BuildError> {
        if c.prop.is_empty() {
            return Err(BuildError::Invalid {
                field: "prop".into(),
                message: "say which prop gets the variant".into(),
            });
        }
        let weights = Weights::new(&c.weights)?;
        let layer = bcx.write_objects(&c.layer)?;
        Ok(PickObjectVariant {
            layer,
            filters: filters(bcx, &c.filter, &[], layer.layer())?,
            prop: c.prop,
            weights,
        })
    }
}

impl Operator for PickObjectVariant {
    fn execute(&self, ctx: &mut Context<'_>) -> Result<(), OpError> {
        let parts = ctx.parts();
        let mut grid = parts.grid;
        let rng = parts.rng;
        let map = grid.map();

        let filters = prepare_all(&grid, &self.filters)?;
        let layers: Vec<_> = self.filters.iter().flat_map(|f| f.layers()).collect();
        let reads = reads_besides(&layers, &[self.layer]);
        let mut split = grid.split(&reads, &[self.layer])?;
        let scene = scene(map, &split, &reads, &[self.layer]);
        let id = self.layer.layer();

        let all: Vec<Subject> = (0..scene.objects(id).len()).map(Subject::Object).collect();
        let mut picked = vec![false; all.len()];
        for s in passing(&filters, &scene, id, &all) {
            if let Subject::Object(i) = s {
                picked[i] = true;
            }
        }
        let layer = split.writes[0]
            .as_objects_mut()
            .expect("checked when built");
        // Draws taken at each corner so far: every object counts, picked
        // or not, so changing `where` never moves another object's draw.
        let mut taken: HashMap<Coord, u32> = HashMap::new();
        for (i, o) in layer.iter_mut().enumerate() {
            let corner = o.bbox.origin;
            let draw = taken.entry(corner).or_insert(0);
            let r = rng.at_draw(corner, *draw)[0];
            *draw += 1;
            if picked[i] {
                o.set(&self.prop, Value::Int(i64::from(self.weights.pick(r))));
            }
        }
        Ok(())
    }
}
