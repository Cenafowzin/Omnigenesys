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

/// Sets the cells within `thickness` of the edge of the map to one tile;
/// with step conditions, only those where they hold. A map narrower than
/// two thicknesses is all border.
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
        let layer = grid.tiles_mut(self.layer)?;
        let (w, h) = (region.size.width as usize, region.size.height as usize);
        let t = self.thickness as usize;
        let cells = layer.as_mut_slice();
        for (y, row) in cells.chunks_exact_mut(w.max(1)).enumerate() {
            // Top and bottom rows whole; the others, the two ends only.
            let left = t.min(w);
            let spans = if y < t || y + t >= h {
                [0..w, w..w]
            } else {
                [0..left, w.saturating_sub(t).max(left)..w]
            };
            for x in spans.into_iter().flatten() {
                let c = region.origin + Coord::new(x as i32, y as i32, 0);
                if mask.as_ref().is_none_or(|m| m.get(c)) {
                    row[x] = self.tile;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
