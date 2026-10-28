use schemars::JsonSchema;
use serde::Deserialize;

use super::each_cell;
use crate::{
    access::WriteHandle,
    condition::CompiledCondition,
    operator::{
        BuildContext, BuildError, Context, Dimensions, OpError, Operator, OperatorType, Scope,
    },
};

/// Writes a ramp across the map into a scalar layer: 0 on one side, 1 on
/// the other, in any direction. `start` and `end` say where along the way
/// the ramp runs, as fractions of the crossing; before `start` it is 0,
/// after `end` it is 1.
///
/// The map's edges, never a chunk's: every chunk computes the same value
/// for the same cell. In 3D every level gets the same ramp.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LinearGradientConfig {
    /// A scalar layer.
    pub layer: String,
    /// The direction it grows in, in degrees: 0 left to right, 90 top to
    /// bottom, 180 right to left.
    #[serde(default)]
    pub angle: f64,
    /// Where the ramp leaves 0, from 0 (the first corner the direction
    /// meets) to 1 (the last).
    #[serde(default)]
    #[schemars(range(min = 0, max = 1))]
    pub start: f64,
    /// Where it reaches 1; above `start`.
    #[serde(default = "one")]
    #[schemars(range(min = 0, max = 1))]
    pub end: f64,
}

fn one() -> f64 {
    1.0
}

pub struct LinearGradient {
    layer: WriteHandle,
    cos: f64,
    sin: f64,
    start: f64,
    end: f64,
    when: Option<CompiledCondition>,
}

impl OperatorType for LinearGradient {
    const NAME: &'static str = "LinearGradient";
    const DIMENSIONS: Dimensions = Dimensions::Any;
    const SCOPE: Scope = Scope::Local;
    type Config = LinearGradientConfig;

    fn build(
        c: LinearGradientConfig,
        bcx: &mut BuildContext<'_>,
    ) -> Result<LinearGradient, BuildError> {
        let invalid = |field: &str, message: String| BuildError::Invalid {
            field: field.to_string(),
            message,
        };
        if !c.angle.is_finite() {
            return Err(invalid("angle", format!("{} is not a number", c.angle)));
        }
        for (field, v) in [("start", c.start), ("end", c.end)] {
            if !(0.0..=1.0).contains(&v) {
                return Err(invalid(field, format!("{v} is not between 0 and 1")));
            }
        }
        if c.start >= c.end {
            return Err(invalid(
                "end",
                format!("{} is not above start {}", c.end, c.start),
            ));
        }
        let (sin, cos) = c.angle.to_radians().sin_cos();
        Ok(LinearGradient {
            layer: bcx.write_scalar(&c.layer)?,
            cos,
            sin,
            start: c.start,
            end: c.end,
            when: bcx.step_conditions(),
        })
    }
}

impl Operator for LinearGradient {
    fn execute(&self, ctx: &mut Context<'_>) -> Result<(), OpError> {
        let parts = ctx.parts();
        let mut grid = parts.grid;
        let region = grid.region();
        let mask = grid.mask(self.when.as_ref())?;
        let map = grid.map();
        let (w, h) = (f64::from(map.size.width), f64::from(map.size.height));
        // How far the map reaches along the direction, corner to corner.
        let span = w * self.cos.abs() + h * self.sin.abs();
        let out = grid.scalars_mut(self.layer)?.as_mut_slice();
        each_cell(region, mask.as_ref(), |c, i| {
            // From the middle of the map to the centre of the cell.
            let px = f64::from(c.x - map.origin.x) + 0.5 - w / 2.0;
            let py = f64::from(c.y - map.origin.y) + 0.5 - h / 2.0;
            let along = (px * self.cos + py * self.sin) / span + 0.5;
            let v = ((along - self.start) / (self.end - self.start)).clamp(0.0, 1.0);
            out[i] = v as f32;
        });
        Ok(())
    }
}
