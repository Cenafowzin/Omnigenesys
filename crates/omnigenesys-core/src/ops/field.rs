//! Scalar fields: noise, shaping it, and turning it into tiles.

mod classify;
mod falloff;
mod noise;

pub use classify::{Band, Classify, ClassifyConfig};
pub use falloff::{Falloff, FalloffConfig, FalloffShape};
pub use noise::{Noise, NoiseConfig, NoiseKind};

#[cfg(test)]
mod tests;
