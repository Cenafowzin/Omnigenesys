//! The operators that ship with the core, one folder per kind of work:
//!
//! - `terrain`: lays down the base of a layer (`Fill`, `FillBorder`).
//! - `scatter`: places tiles cell by cell by chance (`Scatter`).
//! - `shape`: reshapes what earlier steps left (`CellularAutomaton`).
//!
//! The folders are for reading the code; the public paths stay flat
//! (`ops::Fill`), so moving an operator between them breaks nobody.

mod scatter;
mod shape;
mod terrain;

pub use scatter::{Scatter, ScatterConfig};
pub use shape::{CellularAutomaton, CellularAutomatonConfig, Outside};
pub use terrain::{Fill, FillBorder, FillBorderConfig, FillConfig};

use crate::operator::Registry;

pub(crate) fn register_builtins(r: &mut Registry) {
    r.register::<CellularAutomaton>()
        .expect("built-in operator names are unique");
    r.register::<Fill>()
        .expect("built-in operator names are unique");
    r.register::<FillBorder>()
        .expect("built-in operator names are unique");
    r.register::<Scatter>()
        .expect("built-in operator names are unique");
}
