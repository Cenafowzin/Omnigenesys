use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    access::WriteHandle,
    condition::CompiledCondition,
    mask::CellMask,
    operator::{
        BuildContext, BuildError, Context, Dimensions, OpError, Operator, OperatorType, Scope,
    },
};

use crate::ops::weights::Weights;

/// Picks a variant for each cell by `weights` — which of the bushes, which
/// of the rocks — and writes its index (0, 1, …) into an index layer; with
/// step conditions, only where they hold (the cells of the tile it is a
/// variant of). The engine's adapter turns tile and index into a prefab.
///
/// Each cell draws with `rng.at(cell)`: the variant is fixed by the seed,
/// the same in every engine and in any chunk, and changing another step
/// leaves it alone (Unity's WeightedRandomTile and Unreal PCG's weighted
/// mesh entries, decided by the generator and written as data; pesquisa
/// 2026-10-27-espalhar-ancoras-e-variantes, §4). Whole-number weights and
/// one 32-bit draw: no fraction on the way.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PickVariantConfig {
    /// An index layer (`"kind": "index"`) that gets the variant.
    pub layer: String,
    /// How often each variant comes, against the others: `[3, 1]` gives
    /// variant 0 three times as often as 1. From 1 to 256 of them, each
    /// from 0 to 1000000, not all 0.
    pub weights: Vec<u32>,
}

pub struct PickVariant {
    layer: WriteHandle,
    weights: Weights,
    when: Option<CompiledCondition>,
}

impl OperatorType for PickVariant {
    const NAME: &'static str = "PickVariant";
    const DIMENSIONS: Dimensions = Dimensions::Any;
    const SCOPE: Scope = Scope::Local;
    type Config = PickVariantConfig;

    fn build(c: PickVariantConfig, bcx: &mut BuildContext<'_>) -> Result<PickVariant, BuildError> {
        Ok(PickVariant {
            layer: bcx.write_index(&c.layer)?,
            weights: Weights::new(&c.weights)?,
            when: bcx.step_conditions(),
        })
    }
}

impl Operator for PickVariant {
    fn execute(&self, ctx: &mut Context<'_>) -> Result<(), OpError> {
        let parts = ctx.parts();
        let mut grid = parts.grid;
        let rng = parts.rng;
        let region = grid.region();
        let mask = grid
            .mask(self.when.as_ref())?
            .unwrap_or_else(|| CellMask::full(region));
        let layer = grid.indices_mut(self.layer)?;
        for c in mask.iter_ones() {
            layer.set(c, self.weights.pick(rng.at(c)[0]))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
