//! The operators that ship with the core, one folder per kind of work.
//! Each operator is one procedural method (CLAUDE.md, PLANNING §2.14);
//! what a map does with them is in the pipeline's steps and conditions.
//!
//! - `terrain`: `Fill`.
//! - `scatter`: `Scatter`.
//! - `field`: scalar layers — `Noise`, `RadialGradient`,
//!   `LinearGradient`, `Combine`, `Curve`.
//! - `shape`: `CellularAutomaton`, `RegionSize`.
//! - `validate`: `Check`.
//! - `objects`: `PlaceObjects`, `GrowObjects`, `MeasureObjects`,
//!   `TagObjects`, `ConnectObjects`.
//! - `rooms`: the classic dungeon's rooms, connections and roles
//!   (`PlaceRooms`, `ConnectRooms`, `AssignRooms`; to be split into the
//!   object operators with the dungeon recipe, §2.14).
//!
//! The folders are for reading the code; the public paths stay flat
//! (`ops::Fill`), so moving an operator between them breaks nobody.

mod field;
mod objects;
mod rooms;
mod scatter;
mod shape;
mod terrain;
mod validate;

pub use field::{
    Combine, CombineConfig, CombineInput, CombineOp, Curve, CurveConfig, CurveKind, Interpolation,
    LinearGradient, LinearGradientConfig, Noise, NoiseConfig, NoiseKind, RadialGradient,
    RadialGradientConfig, SmoothOrder,
};
pub use objects::{
    ConnectObjects, ConnectObjectsConfig, Count, CountRange, CountWord, Expand, Frontier,
    GrowObjects, GrowObjectsConfig, Link, Measure, MeasureObjects, MeasureObjectsConfig,
    OnExhausted, OrderKey, PlaceObjects, PlaceObjectsConfig, Retire, TagObjects, TagObjectsConfig,
};
pub use rooms::{
    AssignRooms, AssignRoomsConfig, ConnectRooms, ConnectRoomsConfig, CorridorShape, Pick,
    PlaceRooms, PlaceRoomsConfig, RoleRule, Strategy,
};
pub use scatter::{Scatter, ScatterConfig};
pub use shape::{
    CellularAutomaton, CellularAutomatonConfig, Connectivity, Outside, RegionSize, RegionSizeConfig,
};
pub use terrain::{Fill, FillConfig};
pub use validate::{Check, CheckConfig};

use crate::operator::{OperatorType, Registry};

fn add<O: OperatorType>(r: &mut Registry) {
    r.register::<O>()
        .expect("built-in operator names are unique");
}

pub(crate) fn register_builtins(r: &mut Registry) {
    add::<AssignRooms>(r);
    add::<CellularAutomaton>(r);
    add::<Check>(r);
    add::<Combine>(r);
    add::<ConnectObjects>(r);
    add::<ConnectRooms>(r);
    add::<Curve>(r);
    add::<Fill>(r);
    add::<GrowObjects>(r);
    add::<LinearGradient>(r);
    add::<MeasureObjects>(r);
    add::<Noise>(r);
    add::<PlaceObjects>(r);
    add::<PlaceRooms>(r);
    add::<RadialGradient>(r);
    add::<RegionSize>(r);
    add::<Scatter>(r);
    add::<TagObjects>(r);
}
