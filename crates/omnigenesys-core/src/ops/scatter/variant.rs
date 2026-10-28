use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    access::WriteHandle,
    condition::CompiledCondition,
    mask::CellMask,
    operator::{
        BuildContext, BuildError, Context, Dimensions, OpError, Operator, OperatorType, Scope,
    },
    rng::pick,
};

/// Picks a variant for each cell by `weights` — which of the bushes, which
/// of the rocks — and writes its index (0, 1, …) into a scalar layer; with
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
    /// A scalar layer that gets the index.
    pub layer: String,
    /// How often each variant comes, against the others: `[3, 1]` gives
    /// variant 0 three times as often as 1. From 1 to 256 of them, each
    /// from 0 to 1000000, not all 0.
    pub weights: Vec<u32>,
}

const MAX_VARIANTS: usize = 256;
const MAX_WEIGHT: u32 = 1_000_000;

pub struct PickVariant {
    layer: WriteHandle,
    /// Running totals: variant `i` takes the draws below `ends[i]`.
    ends: Vec<u32>,
    when: Option<CompiledCondition>,
}

impl OperatorType for PickVariant {
    const NAME: &'static str = "PickVariant";
    const DIMENSIONS: Dimensions = Dimensions::Any;
    const SCOPE: Scope = Scope::Local;
    type Config = PickVariantConfig;

    fn build(c: PickVariantConfig, bcx: &mut BuildContext<'_>) -> Result<PickVariant, BuildError> {
        let invalid = |message: String| BuildError::Invalid {
            field: "weights".into(),
            message,
        };
        if !(1..=MAX_VARIANTS).contains(&c.weights.len()) {
            return Err(invalid(format!(
                "{} weights; from 1 to {MAX_VARIANTS}",
                c.weights.len()
            )));
        }
        if let Some((i, w)) = c.weights.iter().enumerate().find(|(_, w)| **w > MAX_WEIGHT) {
            return Err(invalid(format!("weight {i} is {w}, above {MAX_WEIGHT}")));
        }
        let ends: Vec<u32> = c
            .weights
            .iter()
            .scan(0, |sum, w| {
                *sum += w;
                Some(*sum)
            })
            .collect();
        if ends.last() == Some(&0) {
            return Err(invalid("every weight is 0: nothing could be picked".into()));
        }
        Ok(PickVariant {
            layer: bcx.write_scalar(&c.layer)?,
            ends,
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
        let total = *self.ends.last().expect("checked when built");
        let layer = grid.scalars_mut(self.layer)?;
        for c in mask.iter_ones() {
            let x = pick(rng.at(c)[0], total);
            // The first variant whose running total passes the draw; one
            // with weight 0 never does.
            let i = self.ends.partition_point(|&end| end <= x);
            layer.set(c, i as f32)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
