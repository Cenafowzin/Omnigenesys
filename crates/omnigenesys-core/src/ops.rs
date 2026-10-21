//! The operators that ship with the core.

mod fill;

pub use fill::{Fill, FillConfig};

use crate::operator::Registry;

pub(crate) fn register_builtins(r: &mut Registry) {
    r.register::<Fill>()
        .expect("built-in operator names are unique");
}
