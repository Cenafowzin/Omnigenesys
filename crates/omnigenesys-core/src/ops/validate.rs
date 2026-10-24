//! Steps that look at the map and fail the generation when it is not what
//! the design asks for: the rules that make a game throw a level away.

mod check;

pub use check::{Check, CheckConfig};
