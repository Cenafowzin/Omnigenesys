use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    access::WriteHandle,
    condition::CompiledCondition,
    operator::{
        BuildContext, BuildError, Context, Dimensions, OpError, Operator, OperatorType, Scope,
    },
    tile::TileId,
};

/// Sets every cell of a layer to one tile; with step conditions, only the
/// cells where they hold.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FillConfig {
    pub layer: String,
    pub tile: String,
}

pub struct Fill {
    layer: WriteHandle,
    tile: TileId,
    when: Option<CompiledCondition>,
}

impl OperatorType for Fill {
    const NAME: &'static str = "Fill";
    const DIMENSIONS: Dimensions = Dimensions::Any;
    const SCOPE: Scope = Scope::Local;
    type Config = FillConfig;

    fn build(config: FillConfig, bcx: &mut BuildContext<'_>) -> Result<Fill, BuildError> {
        Ok(Fill {
            layer: bcx.write(&config.layer)?,
            tile: bcx.tile(&config.tile)?,
            when: bcx.step_conditions(),
        })
    }
}

impl Operator for Fill {
    fn execute(&self, ctx: &mut Context<'_>) -> Result<(), OpError> {
        let mut parts = ctx.parts();
        let grid = &mut parts.grid;
        // The mask is taken before writing, so conditions on the same layer
        // see the map as it was when the step started.
        let mask = grid.mask(self.when.as_ref())?;
        let layer = grid.tiles_mut(self.layer)?;
        match mask {
            None => layer.cells_mut().for_each(|t| *t = self.tile),
            Some(mask) => {
                for c in mask.iter_ones() {
                    layer.set(c, self.tile)?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
