use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    access::{Reach, WriteHandle},
    condition::TileSpec,
    operator::{
        BuildContext, BuildError, Context, Dimensions, OpError, Operator, OperatorType, Scope,
    },
    tile::{TileId, TileSet},
};

use super::regions::{self, Connectivity, Regions};

/// Keeps only the largest connected region of some tiles and fills every
/// other region with `fill`: the step that makes a cave one cave
/// (PLANNING §2.9). On a tie, the first region in scan order (x, then y,
/// then z) stays.
///
/// Global: a region can cross any chunk, so this step needs the whole map.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeepLargestRegionConfig {
    pub layer: String,
    /// The tiles that make up a region (the floor of a cave).
    pub region: TileSpec,
    /// What the cells of the other regions become (the wall).
    pub fill: String,
    pub connectivity: Connectivity,
    /// Fail the step when the largest region is smaller than this.
    #[serde(default)]
    pub min_cells: Option<u64>,
}

pub struct KeepLargestRegion {
    layer: WriteHandle,
    layer_name: String,
    region: TileSet,
    fill: TileId,
    connectivity: Connectivity,
    min_cells: Option<u64>,
}

impl OperatorType for KeepLargestRegion {
    const NAME: &'static str = "KeepLargestRegion";
    const DIMENSIONS: Dimensions = Dimensions::Any;
    const SCOPE: Scope = Scope::Global;
    type Config = KeepLargestRegionConfig;

    fn build(
        config: KeepLargestRegionConfig,
        bcx: &mut BuildContext<'_>,
    ) -> Result<KeepLargestRegion, BuildError> {
        let (region, fill) = regions::resolve(bcx, &config.region, &config.fill)?;
        Ok(KeepLargestRegion {
            // The read reaches the whole map; `Scope::Global` is what says
            // so to the chunk planner, not the reach.
            layer: bcx.read_write(&config.layer, Reach::ZERO)?,
            layer_name: config.layer,
            region,
            fill,
            connectivity: config.connectivity,
            min_cells: config.min_cells,
        })
    }
}

impl Operator for KeepLargestRegion {
    fn execute(&self, ctx: &mut Context<'_>) -> Result<(), OpError> {
        let parts = ctx.parts();
        let mut grid = parts.grid;
        let size = grid.region().size;
        let layer = grid.tiles_mut(self.layer)?;
        let regions = Regions::label(layer.as_slice(), size, &self.region, self.connectivity);
        let Some((keep, cells)) = regions.largest() else {
            return Err(OpError::Failed {
                message: format!("layer '{}' has no cell of the region", self.layer_name),
            });
        };
        if let Some(min) = self.min_cells
            && cells < min
        {
            return Err(OpError::Failed {
                message: format!(
                    "the largest region has {cells} cells, fewer than min_cells ({min})"
                ),
            });
        }
        regions.fill(layer.as_mut_slice(), self.fill, |root| root != keep);
        Ok(())
    }
}
