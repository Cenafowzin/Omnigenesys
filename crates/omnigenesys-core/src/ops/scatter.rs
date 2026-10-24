//! Places tiles cell by cell by chance. `uniform`: the same chance in every
//! cell (`Scatter`); `NoiseScatter` will sit next to it.

mod uniform;

pub use uniform::{Scatter, ScatterConfig};
