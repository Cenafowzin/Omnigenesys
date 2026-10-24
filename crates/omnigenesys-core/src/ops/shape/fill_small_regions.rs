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

/// Fills with `fill` every connected region of some tiles that has fewer
/// than `below` cells: pockets and specks, whatever else the map has
/// (PLANNING §2.9).
///
/// Local, reading `below - 1` cells around: a region with fewer than
/// `below` cells fits inside that window whole, and a larger one shows at
/// least `below` of its cells in it, so every chunk decides the same.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FillSmallRegionsConfig {
    pub layer: String,
    /// The tiles that make up a region.
    pub region: TileSpec,
    /// What the cells of the small regions become.
    pub fill: String,
    pub connectivity: Connectivity,
    /// Regions with fewer cells than this are filled; at least 1.
    #[schemars(range(min = 1))]
    pub below: u32,
}

pub struct FillSmallRegions {
    layer: WriteHandle,
    region: TileSet,
    fill: TileId,
    connectivity: Connectivity,
    below: u32,
}

impl OperatorType for FillSmallRegions {
    const NAME: &'static str = "FillSmallRegions";
    const DIMENSIONS: Dimensions = Dimensions::Any;
    const SCOPE: Scope = Scope::Local;
    type Config = FillSmallRegionsConfig;

    fn build(
        config: FillSmallRegionsConfig,
        bcx: &mut BuildContext<'_>,
    ) -> Result<FillSmallRegions, BuildError> {
        if config.below == 0 {
            return Err(BuildError::Invalid {
                field: "below".to_string(),
                message: "must be at least 1".to_string(),
            });
        }
        let (region, fill) = regions::resolve(bcx, &config.region, &config.fill)?;
        let reach = Reach::uniform(config.below - 1);
        Ok(FillSmallRegions {
            layer: bcx.read_write(&config.layer, reach)?,
            region,
            fill,
            connectivity: config.connectivity,
            below: config.below,
        })
    }
}

impl Operator for FillSmallRegions {
    fn execute(&self, ctx: &mut Context<'_>) -> Result<(), OpError> {
        let parts = ctx.parts();
        let mut grid = parts.grid;
        let size = grid.region().size;
        let layer = grid.tiles_mut(self.layer)?;
        let regions = Regions::label(layer.as_slice(), size, &self.region, self.connectivity);
        let below = u64::from(self.below);
        regions.fill(layer.as_mut_slice(), self.fill, |root| {
            regions.size_of(root) < below
        });
        Ok(())
    }
}
