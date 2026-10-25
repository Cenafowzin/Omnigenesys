//! Scalar fields: noise, distance fields, arithmetic between fields, curves.

mod combine;
mod curve;
mod linear_gradient;
mod noise;
mod radial_gradient;

pub use combine::{Combine, CombineConfig, CombineInput, CombineOp};
pub use curve::{Curve, CurveConfig, CurveKind, Interpolation, SmoothOrder};
pub use linear_gradient::{LinearGradient, LinearGradientConfig};
pub use noise::{Noise, NoiseConfig, NoiseKind};
pub use radial_gradient::{RadialGradient, RadialGradientConfig};

#[cfg(test)]
mod tests;
