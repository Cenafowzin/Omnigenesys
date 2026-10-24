use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    access::WriteHandle,
    condition::CompiledCondition,
    coords::Coord,
    operator::{
        BuildContext, BuildError, Context, Dimensions, OpError, Operator, OperatorType, Scope,
    },
    tile::TileId,
};

/// Sets the cells within `thickness` of the edge of the whole map to one
/// tile; with step conditions, only those where they hold. A map narrower
/// than two thicknesses is all border.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FillBorderConfig {
    pub layer: String,
    pub tile: String,
    /// Cells from each edge, at least 1.
    #[schemars(range(min = 1))]
    pub thickness: u32,
}

pub struct FillBorder {
    layer: WriteHandle,
    tile: TileId,
    thickness: u32,
    when: Option<CompiledCondition>,
}

impl OperatorType for FillBorder {
    const NAME: &'static str = "FillBorder";
    const DIMENSIONS: Dimensions = Dimensions::TwoD;
    const SCOPE: Scope = Scope::Local;
    type Config = FillBorderConfig;

    fn build(
        config: FillBorderConfig,
        bcx: &mut BuildContext<'_>,
    ) -> Result<FillBorder, BuildError> {
        if config.thickness == 0 {
            return Err(BuildError::Invalid {
                field: "thickness".to_string(),
                message: "must be at least 1".to_string(),
            });
        }
        Ok(FillBorder {
            layer: bcx.write(&config.layer)?,
            tile: bcx.tile(&config.tile)?,
            thickness: config.thickness,
            when: bcx.step_conditions(),
        })
    }
}

impl Operator for FillBorder {
    fn execute(&self, ctx: &mut Context<'_>) -> Result<(), OpError> {
        let mut parts = ctx.parts();
        let grid = &mut parts.grid;
        let region = grid.region();
        let mask = match &self.when {
            Some(c) => Some(grid.prepare(c)?.eval(region)),
            None => None,
        };
        let map = grid.map();
        let layer = grid.tiles_mut(self.layer)?;
        let t = i64::from(self.thickness);
        // Within `thickness` of an edge of the whole map — never the
        // chunk's — so every chunk draws the same border.
        let (x0, y0) = (i64::from(map.origin.x), i64::from(map.origin.y));
        let x1 = x0 + i64::from(map.size.width) - 1;
        let y1 = y0 + i64::from(map.size.height) - 1;
        let near_edge = |c: Coord| {
            let (x, y) = (i64::from(c.x), i64::from(c.y));
            x - x0 < t || x1 - x < t || y - y0 < t || y1 - y < t
        };
        for c in region.iter() {
            if near_edge(c) && mask.as_ref().is_none_or(|m| m.get(c)) {
                layer.set(c, self.tile)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
