//! Scalar fields: noise, distance fields, arithmetic between fields.

mod combine;
mod gradient;
mod noise;

pub use combine::{Combine, CombineConfig, CombineOp};
pub use gradient::{Gradient, GradientConfig, GradientShape};
pub use noise::{Noise, NoiseConfig, NoiseKind};

#[cfg(test)]
mod tests;
