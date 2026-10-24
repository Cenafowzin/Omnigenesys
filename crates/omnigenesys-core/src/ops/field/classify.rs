use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    access::{Reach, ReadHandle, WriteHandle},
    condition::CompiledCondition,
    mask::CellMask,
    operator::{
        BuildContext, BuildError, Context, Dimensions, OpError, Operator, OperatorType, Scope,
    },
    tile::TileId,
};

/// Turns a scalar layer into tiles by bands: each cell gets the tile of the
/// first band its value is below (water under 0.35, sand under 0.4, grass
/// under 0.7, rock above). The biome table of Red Blob Games' "Making maps
/// with noise functions", in one dimension.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassifyConfig {
    /// The scalar layer to read.
    pub from: String,
    /// The tile layer to write.
    pub layer: String,
    /// In order, each `max` larger than the last; the last may leave `max`
    /// out to take every value above. A value above every `max` leaves its
    /// cell as it was.
    pub bands: Vec<Band>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Band {
    /// Values below this get the band's tile.
    #[serde(default)]
    pub max: Option<f64>,
    pub tile: String,
}

pub struct Classify {
    from: ReadHandle,
    layer: WriteHandle,
    bands: Vec<(f32, TileId)>,
    when: Option<CompiledCondition>,
}

impl OperatorType for Classify {
    const NAME: &'static str = "Classify";
    const DIMENSIONS: Dimensions = Dimensions::Any;
    const SCOPE: Scope = Scope::Local;
    type Config = ClassifyConfig;

    fn build(c: ClassifyConfig, bcx: &mut BuildContext<'_>) -> Result<Classify, BuildError> {
        let invalid = |message: String| BuildError::Invalid {
            field: "bands".to_string(),
            message,
        };
        if c.bands.is_empty() {
            return Err(invalid("needs at least one band".to_string()));
        }
        let mut bands = Vec::with_capacity(c.bands.len());
        let mut last = f32::NEG_INFINITY;
        for (i, b) in c.bands.iter().enumerate() {
            let max = match b.max {
                Some(m) if !m.is_finite() => {
                    return Err(invalid(format!("band {i}: max {m} is not a number")));
                }
                Some(m) => m as f32,
                None if i + 1 == c.bands.len() => f32::INFINITY,
                None => {
                    return Err(invalid(format!(
                        "band {i}: only the last band may leave out max"
                    )));
                }
            };
            if max <= last {
                return Err(invalid(format!(
                    "band {i}: max {max} is not above the band before ({last})"
                )));
            }
            last = max;
            bands.push((max, bcx.tile(&b.tile)?));
        }
        Ok(Classify {
            from: bcx.read_scalar(&c.from, Reach::ZERO)?,
            layer: bcx.write(&c.layer)?,
            bands,
            when: bcx.step_conditions(),
        })
    }
}

impl Operator for Classify {
    fn execute(&self, ctx: &mut Context<'_>) -> Result<(), OpError> {
        let parts = ctx.parts();
        let mut grid = parts.grid;
        let region = grid.region();
        let mask = match &self.when {
            Some(c) => grid.prepare(c)?.eval(region),
            None => CellMask::full(region),
        };
        let (field, tiles) = grid.scalars_and_tiles_mut(self.from, self.layer)?;
        for c in mask.iter_ones() {
            let v = *field.get(c).unwrap_or(&f32::NAN);
            // NaN is below nothing: the cell stays.
            if let Some(&(_, tile)) = self.bands.iter().find(|(max, _)| v < *max) {
                tiles.set(c, tile)?;
            }
        }
        Ok(())
    }
}
