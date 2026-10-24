use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    access::{Reach, WriteHandle},
    condition::CompiledCondition,
    mask::CellMask,
    operator::{
        BuildContext, BuildError, Context, Dimensions, OpError, Operator, OperatorType, Scope,
    },
};

/// Pulls a scalar layer down toward the edges of the map: noise becomes an
/// island. Red Blob Games' island shaping, "Making maps with noise
/// functions": with `d` the distance from the centre (0 there, 1 at the
/// edge), `e = lerp(e, 1 − d, mix)`.
///
/// The centre and the edges are the whole map's, not the chunk's: every
/// chunk of a map computes the same value for the same cell.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FalloffConfig {
    /// A scalar layer.
    pub layer: String,
    #[serde(default)]
    pub shape: FalloffShape,
    /// 0 leaves the layer as it was, 1 makes it the shape alone.
    #[serde(default = "default_mix")]
    #[schemars(range(min = 0, max = 1))]
    pub mix: f64,
}

fn default_mix() -> f64 {
    0.5
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum FalloffShape {
    /// `1 − (1 − x²)(1 − y²)`: fills the map, square corners rounded.
    #[default]
    Square,
    /// `(x² + y²) / √2`: a round island in the middle.
    Round,
}

pub struct Falloff {
    layer: WriteHandle,
    shape: FalloffShape,
    mix: f32,
    when: Option<CompiledCondition>,
}

impl OperatorType for Falloff {
    const NAME: &'static str = "Falloff";
    const DIMENSIONS: Dimensions = Dimensions::Any;
    const SCOPE: Scope = Scope::Local;
    type Config = FalloffConfig;

    fn build(c: FalloffConfig, bcx: &mut BuildContext<'_>) -> Result<Falloff, BuildError> {
        if !(0.0..=1.0).contains(&c.mix) {
            return Err(BuildError::Invalid {
                field: "mix".to_string(),
                message: format!("{} is not between 0 and 1", c.mix),
            });
        }
        Ok(Falloff {
            layer: bcx.read_write_scalar(&c.layer, Reach::ZERO)?,
            shape: c.shape,
            mix: c.mix as f32,
            when: bcx.step_conditions(),
        })
    }
}

impl Operator for Falloff {
    fn execute(&self, ctx: &mut Context<'_>) -> Result<(), OpError> {
        let parts = ctx.parts();
        let mut grid = parts.grid;
        let region = grid.region();
        let mask = match &self.when {
            Some(c) => grid.prepare(c)?.eval(region),
            None => CellMask::full(region),
        };
        let map = grid.map();
        let (w, h) = (f64::from(map.size.width), f64::from(map.size.height));
        let layer = grid.scalars_mut(self.layer)?;
        for c in mask.iter_ones() {
            // -1..1 across the map, at the centre of the cell.
            let nx = (2.0 * (f64::from(c.x - map.origin.x) + 0.5) / w - 1.0) as f32;
            let ny = (2.0 * (f64::from(c.y - map.origin.y) + 0.5) / h - 1.0) as f32;
            let d = match self.shape {
                FalloffShape::Square => 1.0 - (1.0 - nx * nx) * (1.0 - ny * ny),
                FalloffShape::Round => ((nx * nx + ny * ny) / std::f32::consts::SQRT_2).min(1.0),
            };
            let e = *layer.get(c).unwrap_or(&0.0);
            layer.set(c, (e + (1.0 - d - e) * self.mix).clamp(0.0, 1.0))?;
        }
        Ok(())
    }
}
