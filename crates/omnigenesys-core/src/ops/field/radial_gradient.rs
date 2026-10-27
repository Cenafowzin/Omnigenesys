use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    access::WriteHandle,
    condition::CompiledCondition,
    mask::CellMask,
    operator::{
        BuildContext, BuildError, Context, Dimensions, OpError, Operator, OperatorType, Scope,
    },
};

/// Writes the distance from a point into a scalar layer: 0 at `x`, `y`, 1
/// at the edge of the shape around it and beyond. The shape is the unit
/// ball of the Lp norm (a superellipse): diamond at `shape` 0, circle at
/// 0.5, square at 1, stretched by the two radii and turned by `rotation`.
/// The centre is placed by the map's width and height, the radii by its
/// shorter side.
/// A `Curve` step after it shapes the slope; one with `invert` makes it 1
/// at the centre, as an island wants.
///
/// The map's edges, never a chunk's: every chunk computes the same value
/// for the same cell. In 3D every level gets the same field.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RadialGradientConfig {
    /// A scalar layer.
    pub layer: String,
    /// The centre across the map: 0 the left edge, 1 the right.
    #[serde(default = "half")]
    pub x: f64,
    /// The centre down the map: 0 the top edge, 1 the bottom.
    #[serde(default = "half")]
    pub y: f64,
    /// Half the shape's width, as a fraction of the map's shorter side,
    /// so equal radii make a circle on any map: 0.5 from the middle
    /// reaches the two nearest edges.
    #[serde(default = "half")]
    pub radius_x: f64,
    /// Half the shape's height, as a fraction of the map's shorter side.
    #[serde(default = "half")]
    pub radius_y: f64,
    /// Turns the shape, in degrees, clockwise (y grows downwards).
    #[serde(default)]
    pub rotation: f64,
    /// 0 diamond, 0.5 circle, 1 square: the norm's exponent is
    /// `1 / (1 − shape)`, so the corners fill evenly along the way.
    #[serde(default = "half")]
    #[schemars(range(min = 0, max = 1))]
    pub shape: f64,
}

fn half() -> f64 {
    0.5
}

pub struct RadialGradient {
    layer: WriteHandle,
    centre: (f64, f64),
    radius: (f64, f64),
    cos: f64,
    sin: f64,
    norm: Norm,
    when: Option<CompiledCondition>,
}

/// The norm `shape` picks, with the exact cases apart: they are common,
/// and cheaper than two `powf`.
#[derive(Debug, Clone, Copy)]
enum Norm {
    Diamond,
    Circle,
    Square,
    P(f64),
}

impl Norm {
    fn from_shape(shape: f64) -> Norm {
        match shape {
            0.0 => Norm::Diamond,
            0.5 => Norm::Circle,
            1.0 => Norm::Square,
            s => Norm::P(1.0 / (1.0 - s)),
        }
    }

    /// `(|u|^p + |v|^p)^(1/p)`, scaled by the larger of the two: the terms
    /// stay at or below 1, so a large `p` neither underflows inside the
    /// shape nor overflows outside it.
    #[inline]
    fn of(self, u: f64, v: f64) -> f64 {
        let (u, v) = (u.abs(), v.abs());
        match self {
            Norm::Diamond => u + v,
            Norm::Circle => u.hypot(v),
            Norm::Square => u.max(v),
            Norm::P(p) => {
                let m = u.max(v);
                if m == 0.0 {
                    return 0.0;
                }
                m * ((u / m).powf(p) + (v / m).powf(p)).powf(1.0 / p)
            }
        }
    }
}

impl OperatorType for RadialGradient {
    const NAME: &'static str = "RadialGradient";
    const DIMENSIONS: Dimensions = Dimensions::Any;
    const SCOPE: Scope = Scope::Local;
    type Config = RadialGradientConfig;

    fn build(
        c: RadialGradientConfig,
        bcx: &mut BuildContext<'_>,
    ) -> Result<RadialGradient, BuildError> {
        let invalid = |field: &str, message: String| BuildError::Invalid {
            field: field.to_string(),
            message,
        };
        for (field, v) in [("x", c.x), ("y", c.y), ("rotation", c.rotation)] {
            if !v.is_finite() {
                return Err(invalid(field, format!("{v} is not a number")));
            }
        }
        for (field, v) in [("radius_x", c.radius_x), ("radius_y", c.radius_y)] {
            if !(v.is_finite() && v > 0.0) {
                return Err(invalid(field, format!("{v} is not above 0")));
            }
        }
        if !(0.0..=1.0).contains(&c.shape) {
            return Err(invalid(
                "shape",
                format!("{} is not between 0 and 1", c.shape),
            ));
        }
        let (sin, cos) = c.rotation.to_radians().sin_cos();
        Ok(RadialGradient {
            layer: bcx.write_scalar(&c.layer)?,
            centre: (c.x, c.y),
            radius: (c.radius_x, c.radius_y),
            cos,
            sin,
            norm: Norm::from_shape(c.shape),
            when: bcx.step_conditions(),
        })
    }
}

impl Operator for RadialGradient {
    fn execute(&self, ctx: &mut Context<'_>) -> Result<(), OpError> {
        let parts = ctx.parts();
        let mut grid = parts.grid;
        let region = grid.region();
        let mask = grid
            .mask(self.when.as_ref())?
            .unwrap_or_else(|| CellMask::full(region));
        let map = grid.map();
        let (w, h) = (f64::from(map.size.width), f64::from(map.size.height));
        let (cx, cy) = (self.centre.0 * w, self.centre.1 * h);
        let side = w.min(h);
        let (rx, ry) = (self.radius.0 * side, self.radius.1 * side);
        let layer = grid.scalars_mut(self.layer)?;
        for c in mask.iter_ones() {
            // From the centre to the centre of the cell, in cells, turned
            // back by the rotation and measured in radii.
            let px = f64::from(c.x - map.origin.x) + 0.5 - cx;
            let py = f64::from(c.y - map.origin.y) + 0.5 - cy;
            let u = (px * self.cos + py * self.sin) / rx;
            let v = (py * self.cos - px * self.sin) / ry;
            layer.set(c, self.norm.of(u, v).min(1.0) as f32)?;
        }
        Ok(())
    }
}
