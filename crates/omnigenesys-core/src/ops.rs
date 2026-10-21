//! The operators that ship with the core.

mod fill;
mod scatter;

pub use fill::{Fill, FillConfig};
pub use scatter::{Scatter, ScatterConfig};

use crate::operator::Registry;

pub(crate) fn register_builtins(r: &mut Registry) {
    r.register::<Fill>()
        .expect("built-in operator names are unique");
    r.register::<Scatter>()
        .expect("built-in operator names are unique");
}
