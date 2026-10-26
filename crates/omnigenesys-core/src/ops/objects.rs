//! Objects: placing them on a lattice, growing them, measuring them,
//! picking them and connecting them (desenho-pipelines §3.4; pesquisa
//! 2026-10-25-operadores-de-objetos). Each step is one method; which
//! objects it works on is said by object filters (`where`), and what a
//! map decides with them — the start, the boss, the secret room — is in
//! the steps, never in an operator.

mod common;
mod connect_objects;
mod grow_objects;
mod measure_objects;
mod place_objects;
mod tag_objects;

pub use common::{Count, CountRange, CountWord, OrderKey};
pub use connect_objects::{ConnectObjects, ConnectObjectsConfig, Strategy as Link};
pub use grow_objects::{
    Expand, GrowObjects, GrowObjectsConfig, OnExhausted, Pick as Frontier, Retire,
};
pub use measure_objects::{Measure, MeasureObjects, MeasureObjectsConfig};
pub use place_objects::{PlaceObjects, PlaceObjectsConfig, Side, Size};
pub use tag_objects::{TagObjects, TagObjectsConfig};

#[cfg(test)]
mod tests;
