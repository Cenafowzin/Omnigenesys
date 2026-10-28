//! Decides cell by cell by chance. `uniform`: a tile, the same chance in
//! every cell (`Scatter`); `variant`: which variant of something a cell
//! gets, by weights (`PickVariant`).

mod uniform;
mod variant;

pub use uniform::{Scatter, ScatterConfig};
pub use variant::{PickVariant, PickVariantConfig};
