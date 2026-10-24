//! Reshapes and measures what earlier steps left: the cellular automaton,
//! the size of connected regions; later erosion.

mod bits;
mod cellular_automaton;
mod region_size;
mod regions;

pub use cellular_automaton::{CellularAutomaton, CellularAutomatonConfig, Outside};
pub use region_size::{RegionSize, RegionSizeConfig};
pub use regions::Connectivity;
