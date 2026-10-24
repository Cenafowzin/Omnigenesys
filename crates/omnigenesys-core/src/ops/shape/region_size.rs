use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    access::{Reach, ReadHandle, WriteHandle},
    condition::TileSpec,
    coords::GridSize,
    operator::{
        BuildContext, BuildError, Context, Dimensions, OpError, Operator, OperatorType, Scope,
    },
    tile::TileSet,
};

use super::regions::{Connectivity, Regions};

/// Finds the connected regions of some tiles (BRTS run labelling,
/// PLANNING §2.9) and writes, for every cell of a region, the region's
/// size — and, if asked, its rank (1 = the largest; on a tie, the first in
/// scan order) — into scalar layers; 0 outside the regions. A step after it
/// decides what to do: fill the regions under 6 cells, everything but the
/// largest, the caves above a size.
///
/// With `cap`, sizes stop counting at `cap` (a region of 500 cells reads
/// as `cap`), and the step is **local**: a region smaller than `cap` fits
/// in the `cap − 1` cells it reads around, and a larger one shows at least
/// `cap` of its cells there, so every chunk agrees. Without `cap` — and
/// always with `rank`, which compares regions anywhere — it needs the whole
/// map: a world without end cannot have it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RegionSizeConfig {
    /// The tile layer to read.
    pub from: String,
    /// The tiles that make up a region.
    pub region: TileSpec,
    pub connectivity: Connectivity,
    /// Scalar layer that gets each region's size.
    pub layer: String,
    /// Scalar layer that gets each region's rank (1 = largest).
    #[serde(default)]
    pub rank: Option<String>,
    /// Count no further than this (at least 1); makes the step local.
    #[serde(default)]
    #[schemars(range(min = 1))]
    pub cap: Option<u32>,
}

pub struct RegionSize {
    from: ReadHandle,
    region: TileSet,
    connectivity: Connectivity,
    size: WriteHandle,
    rank: Option<WriteHandle>,
    cap: Option<u32>,
}

impl OperatorType for RegionSize {
    const NAME: &'static str = "RegionSize";
    const DIMENSIONS: Dimensions = Dimensions::Any;
    const SCOPE: Scope = Scope::Global;
    type Config = RegionSizeConfig;

    fn build(c: RegionSizeConfig, bcx: &mut BuildContext<'_>) -> Result<RegionSize, BuildError> {
        let invalid = |field: &str, message: &str| BuildError::Invalid {
            field: field.to_string(),
            message: message.to_string(),
        };
        let ids = c
            .region
            .names()
            .iter()
            .map(|n| bcx.tile(n))
            .collect::<Result<Vec<_>, _>>()?;
        let reach = match c.cap {
            Some(0) => return Err(invalid("cap", "must be at least 1")),
            Some(_) if c.rank.is_some() => {
                return Err(invalid(
                    "cap",
                    "rank compares regions across the whole map; it cannot have a cap",
                ));
            }
            Some(cap) => {
                bcx.set_scope(Scope::Local);
                Reach::uniform(cap - 1)
            }
            None => {
                bcx.set_scope(Scope::Global);
                Reach::ZERO
            }
        };
        Ok(RegionSize {
            from: bcx.read(&c.from, reach)?,
            region: TileSet::new(&ids),
            connectivity: c.connectivity,
            size: bcx.write_scalar(&c.layer)?,
            rank: c.rank.as_deref().map(|r| bcx.write_scalar(r)).transpose()?,
            cap: c.cap,
        })
    }
}

impl Operator for RegionSize {
    fn execute(&self, ctx: &mut Context<'_>) -> Result<(), OpError> {
        let parts = ctx.parts();
        let mut grid = parts.grid;
        let size: GridSize = grid.region().size;
        let mut writes = vec![self.size];
        writes.extend(self.rank);
        let mut split = grid.split(&[self.from], &writes)?;
        let tiles = split.reads[0].as_tiles().expect("checked when built");
        let regions = Regions::label(tiles.as_slice(), size, &self.region, self.connectivity);

        let cap = self.cap.map_or(u64::MAX, u64::from);
        let mut sizes: Vec<(u32, u64)> = regions.roots().collect();
        {
            let out = split.writes[0]
                .as_scalar_mut()
                .expect("checked when built")
                .as_mut_slice();
            out.fill(0.0);
            let lookup: std::collections::BTreeMap<u32, u64> = sizes.iter().copied().collect();
            regions.paint(out, |root| lookup.get(&root).map(|&s| s.min(cap) as f32));
        }
        if self.rank.is_some() {
            // Largest first; on a tie, the first region in scan order (the
            // smaller root).
            sizes.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            let rank: std::collections::BTreeMap<u32, f32> = sizes
                .iter()
                .enumerate()
                .map(|(i, &(root, _))| (root, (i + 1) as f32))
                .collect();
            let out = split.writes[1]
                .as_scalar_mut()
                .expect("checked when built")
                .as_mut_slice();
            out.fill(0.0);
            regions.paint(out, |root| rank.get(&root).copied());
        }
        Ok(())
    }
}
