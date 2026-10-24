//! Reshapes what earlier steps left: smoothing noise into caves, keeping
//! the largest region, filling small ones; later erosion.

mod bits;
mod cellular_automaton;
mod fill_small_regions;
mod keep_largest_region;
mod regions;

pub use cellular_automaton::{CellularAutomaton, CellularAutomatonConfig, Outside};
pub use fill_small_regions::{FillSmallRegions, FillSmallRegionsConfig};
pub use keep_largest_region::{KeepLargestRegion, KeepLargestRegionConfig};
pub use regions::Connectivity;
