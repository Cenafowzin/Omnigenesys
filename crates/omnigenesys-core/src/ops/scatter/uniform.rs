use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    access::WriteHandle,
    condition::CompiledCondition,
    mask::CellMask,
    operator::{
        BuildContext, BuildError, Context, Dimensions, OpError, Operator, OperatorType, Scope,
    },
    rng::Chance,
    tile::TileId,
};

/// Sets each cell to one tile with probability `chance`; with step
/// conditions, only among the cells where they hold.
///
/// Each cell decides with `rng.at(cell)`, so the result does not depend on
/// the order cells are visited, on chunking, or on the other steps.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScatterConfig {
    pub layer: String,
    pub tile: String,
    /// Probability per cell, from 0 to 1.
    pub chance: f64,
}

pub struct Scatter {
    layer: WriteHandle,
    tile: TileId,
    chance: Chance,
    when: Option<CompiledCondition>,
}

impl OperatorType for Scatter {
    const NAME: &'static str = "Scatter";
    const DIMENSIONS: Dimensions = Dimensions::Any;
    const SCOPE: Scope = Scope::Local;
    type Config = ScatterConfig;

    fn build(config: ScatterConfig, bcx: &mut BuildContext<'_>) -> Result<Scatter, BuildError> {
        let chance = Chance::new(config.chance).ok_or_else(|| BuildError::Invalid {
            field: "chance".to_string(),
            message: format!("{} is not between 0 and 1", config.chance),
        })?;
        Ok(Scatter {
            layer: bcx.write(&config.layer)?,
            tile: bcx.tile(&config.tile)?,
            chance,
            when: bcx.step_conditions(),
        })
    }
}

impl Operator for Scatter {
    fn execute(&self, ctx: &mut Context<'_>) -> Result<(), OpError> {
        let parts = ctx.parts();
        let mut grid = parts.grid;
        let rng = parts.rng;
        let region = grid.region();
        // Candidates first (the conditions see the map as the step found
        // it), then the draw only where a candidate is.
        let candidates = match &self.when {
            Some(c) => grid.prepare(c)?.eval(region),
            None => CellMask::full(region),
        };
        let layer = grid.tiles_mut(self.layer)?;
        for c in candidates.iter_ones() {
            if self.chance.hit(rng.at(c)[0]) {
                layer.set(c, self.tile)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
