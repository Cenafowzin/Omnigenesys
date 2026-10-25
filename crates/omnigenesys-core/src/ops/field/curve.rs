use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    access::{Reach, ReadHandle, WriteHandle},
    condition::CompiledCondition,
    mask::CellMask,
    operator::{
        BuildContext, BuildError, Context, Dimensions, OpError, Operator, OperatorType, Scope,
    },
};

/// Remaps a scalar layer through a curve, cell by cell, into a layer (which
/// may be the same one): sharper peaks, flatter plains, terraces, a slope
/// that eases in. The value is read between 0 and 1 (clamped) and comes out
/// between 0 and 1; `invert` turns it into `1 − value` afterwards.
///
/// The kinds are the ones terrain tools share (`docs/pesquisa/relatorios/
/// 2026-10-25-gradientes-e-curvas.md`): smoothstep (libnoise, FastNoise),
/// power (Red Blob, libnoise's Exponent), a rational sigmoid (Schlick's
/// bias, Lague's falloff), FastNoise's terraces, and control points through
/// a monotone cubic (the curve editors of Houdini and Unity).
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CurveConfig {
    /// The scalar layer read.
    pub input: String,
    /// The scalar layer written; may be `input`.
    pub layer: String,
    pub curve: CurveKind,
    /// `1 − value`, after the curve.
    #[serde(default)]
    pub invert: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum CurveKind {
    /// The value as it is.
    Linear,
    /// Eases in and out: `x²(3 − 2x)`, or `6x⁵ − 15x⁴ + 10x³` for
    /// `quintic`, flatter at the ends.
    Smooth {
        #[serde(default)]
        order: SmoothOrder,
    },
    /// `x^exponent`: above 1 lowers the middle (sharp peaks, wide plains),
    /// below 1 raises it.
    Power { exponent: f64 },
    /// `xᵃ / (xᵃ + kᵃ(1 − x)ᵃ)` with `k = midpoint / (1 − midpoint)`: an S
    /// that crosses ½ at `midpoint`, steeper as `exponent` grows. Exponent
    /// 1 bends without the S (Schlick's bias).
    Sigmoid {
        exponent: f64,
        #[serde(default = "half")]
        midpoint: f64,
    },
    /// `steps` flat levels; `smoothness` 0 is a staircase, larger values
    /// round the risers until the curve is a line again.
    Terrace {
        steps: u32,
        #[serde(default)]
        smoothness: f64,
    },
    /// Through `[in, out]` points with rising `in`, both between 0 and 1,
    /// at least two; flat before the first and after the last.
    Points {
        points: Vec<[f64; 2]>,
        #[serde(default)]
        interpolation: Interpolation,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SmoothOrder {
    #[default]
    Cubic,
    Quintic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Interpolation {
    /// Straight segments.
    Linear,
    /// A smooth curve that never overshoots the points (Fritsch–Carlson).
    #[default]
    Monotone,
}

fn half() -> f64 {
    0.5
}

pub struct Curve {
    input: ReadHandle,
    out: WriteHandle,
    shape: Shape,
    invert: bool,
    when: Option<CompiledCondition>,
}

/// A `CurveKind` checked and ready to evaluate.
enum Shape {
    Linear,
    Smooth(SmoothOrder),
    Power(f64),
    Sigmoid { a: f64, k: f64 },
    Terrace { steps: f64, smoothness: f64 },
    /// The points and, for a monotone curve, the slope at each.
    Points { xs: Vec<f64>, ys: Vec<f64>, slopes: Option<Vec<f64>> },
}

impl Shape {
    fn new(kind: CurveKind) -> Result<Shape, BuildError> {
        let invalid = |field: String, message: String| BuildError::Invalid { field, message };
        let positive = |field: &str, v: f64| {
            if v.is_finite() && v > 0.0 {
                Ok(v)
            } else {
                Err(invalid(format!("curve/{field}"), format!("{v} is not above 0")))
            }
        };
        Ok(match kind {
            CurveKind::Linear => Shape::Linear,
            CurveKind::Smooth { order } => Shape::Smooth(order),
            CurveKind::Power { exponent } => Shape::Power(positive("exponent", exponent)?),
            CurveKind::Sigmoid { exponent, midpoint } => {
                let a = positive("exponent", exponent)?;
                if !(midpoint > 0.0 && midpoint < 1.0) {
                    return Err(invalid(
                        "curve/midpoint".to_string(),
                        format!("{midpoint} is not between 0 and 1, both left out"),
                    ));
                }
                Shape::Sigmoid {
                    a,
                    k: (midpoint / (1.0 - midpoint)).powf(a),
                }
            }
            CurveKind::Terrace { steps, smoothness } => {
                if steps == 0 {
                    return Err(invalid(
                        "curve/steps".to_string(),
                        "needs at least 1".to_string(),
                    ));
                }
                if !(smoothness.is_finite() && smoothness >= 0.0) {
                    return Err(invalid(
                        "curve/smoothness".to_string(),
                        format!("{smoothness} is below 0"),
                    ));
                }
                Shape::Terrace {
                    steps: f64::from(steps),
                    smoothness,
                }
            }
            CurveKind::Points {
                points,
                interpolation,
            } => {
                if points.len() < 2 {
                    return Err(invalid(
                        "curve/points".to_string(),
                        format!("needs at least two points, got {}", points.len()),
                    ));
                }
                for (i, &[x, y]) in points.iter().enumerate() {
                    if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
                        return Err(invalid(
                            format!("curve/points/{i}"),
                            format!("[{x}, {y}] is not between 0 and 1"),
                        ));
                    }
                    if i > 0 && x <= points[i - 1][0] {
                        return Err(invalid(
                            format!("curve/points/{i}"),
                            format!("{x} does not come after {}", points[i - 1][0]),
                        ));
                    }
                }
                let (xs, ys): (Vec<f64>, Vec<f64>) = points.iter().map(|&[x, y]| (x, y)).unzip();
                let slopes = match interpolation {
                    Interpolation::Linear => None,
                    Interpolation::Monotone => Some(monotone_slopes(&xs, &ys)),
                };
                Shape::Points { xs, ys, slopes }
            }
        })
    }

    #[inline]
    fn eval(&self, x: f64) -> f64 {
        match self {
            Shape::Linear => x,
            Shape::Smooth(SmoothOrder::Cubic) => x * x * (3.0 - 2.0 * x),
            Shape::Smooth(SmoothOrder::Quintic) => x * x * x * (x * (6.0 * x - 15.0) + 10.0),
            Shape::Power(e) => x.powf(*e),
            Shape::Sigmoid { a, k } => {
                let up = x.powf(*a);
                // Both terms are 0 only if x is 0 and 1 at once.
                up / (up + k * (1.0 - x).powf(*a))
            }
            Shape::Terrace { steps, smoothness } => {
                // FastNoise2's Terrace: round to a level, then pull back
                // towards the line inside a band around each riser.
                let x = x * steps;
                let mut r = x.round();
                if *smoothness != 0.0 {
                    let diff = r - x;
                    let d = ((0.5 - diff.abs()) * (1.0 + 1.0 / smoothness)).min(0.5);
                    r += if diff < 0.0 { 0.5 - d } else { d - 0.5 };
                }
                r / steps
            }
            Shape::Points { xs, ys, slopes } => {
                let last = xs.len() - 1;
                if x <= xs[0] {
                    return ys[0];
                }
                if x >= xs[last] {
                    return ys[last];
                }
                // The segment [xs[i], xs[i + 1]] holding x.
                let i = xs.partition_point(|&p| p <= x) - 1;
                let h = xs[i + 1] - xs[i];
                let t = (x - xs[i]) / h;
                match slopes {
                    None => ys[i] + (ys[i + 1] - ys[i]) * t,
                    Some(m) => {
                        let (t2, t3) = (t * t, t * t * t);
                        (2.0 * t3 - 3.0 * t2 + 1.0) * ys[i]
                            + (t3 - 2.0 * t2 + t) * h * m[i]
                            + (-2.0 * t3 + 3.0 * t2) * ys[i + 1]
                            + (t3 - t2) * h * m[i + 1]
                    }
                }
            }
        }
    }
}

/// Fritsch–Carlson: the slope at each point of a cubic Hermite curve that
/// rises (or falls) only where the points do, so it never leaves their
/// range.
fn monotone_slopes(xs: &[f64], ys: &[f64]) -> Vec<f64> {
    let n = xs.len();
    let d: Vec<f64> = (0..n - 1)
        .map(|i| (ys[i + 1] - ys[i]) / (xs[i + 1] - xs[i]))
        .collect();
    let mut m = vec![0.0; n];
    m[0] = d[0];
    m[n - 1] = d[n - 2];
    for i in 1..n - 1 {
        m[i] = if d[i - 1] * d[i] > 0.0 {
            (d[i - 1] + d[i]) / 2.0
        } else {
            0.0
        };
    }
    for i in 0..n - 1 {
        if d[i] == 0.0 {
            m[i] = 0.0;
            m[i + 1] = 0.0;
            continue;
        }
        let (a, b) = (m[i] / d[i], m[i + 1] / d[i]);
        let r = a.hypot(b);
        if r > 3.0 {
            m[i] = 3.0 / r * a * d[i];
            m[i + 1] = 3.0 / r * b * d[i];
        }
    }
    m
}

impl OperatorType for Curve {
    const NAME: &'static str = "Curve";
    const DIMENSIONS: Dimensions = Dimensions::Any;
    const SCOPE: Scope = Scope::Local;
    type Config = CurveConfig;

    fn build(c: CurveConfig, bcx: &mut BuildContext<'_>) -> Result<Curve, BuildError> {
        Ok(Curve {
            shape: Shape::new(c.curve)?,
            input: bcx.read_scalar(&c.input, Reach::ZERO)?,
            out: bcx.write_scalar(&c.layer)?,
            invert: c.invert,
            when: bcx.step_conditions(),
        })
    }
}

impl Operator for Curve {
    fn execute(&self, ctx: &mut Context<'_>) -> Result<(), OpError> {
        let parts = ctx.parts();
        let mut grid = parts.grid;
        let region = grid.region();
        let mask = match &self.when {
            Some(c) => grid.prepare(c)?.eval(region),
            None => CellMask::full(region),
        };
        // A copy of the input: the output may be the same layer.
        let input = grid.scalars(self.input)?.clone();
        let out = grid.scalars_mut(self.out)?;
        for c in mask.iter_ones() {
            let x = f64::from(*input.get(c).unwrap_or(&0.0)).clamp(0.0, 1.0);
            let y = self.shape.eval(x).clamp(0.0, 1.0);
            out.set(c, (if self.invert { 1.0 - y } else { y }) as f32)?;
        }
        Ok(())
    }
}
