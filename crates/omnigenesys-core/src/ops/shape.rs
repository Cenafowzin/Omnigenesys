//! Reshapes what earlier steps left: smoothing noise into caves, and later
//! keeping the largest region and erosion.

mod cellular_automaton;

pub use cellular_automaton::{CellularAutomaton, CellularAutomatonConfig, Outside};
