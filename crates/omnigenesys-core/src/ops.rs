//! The operators that ship with the core, one folder per kind of work.
//! Each operator is one procedural method (CLAUDE.md, PLANNING §2.14);
//! what a map does with them is in the pipeline's steps and conditions.
//!
//! - `terrain`: `Fill`.
//! - `scatter`: `Scatter`.
//! - `field`: scalar layers — `Noise`, `Gradient`, `Combine`.
//! - `shape`: `CellularAutomaton`, `RegionSize`.
//! - `validate`: `Check`.
//! - `rooms`: dungeons — rooms, connections, roles (`RoomGrid`,
//!   `PlaceRooms`, `ConnectRooms`, `AssignRooms`; being split, §2.14).
//!
//! The folders are for reading the code; the public paths stay flat
//! (`ops::Fill`), so moving an operator between them breaks nobody.

mod field;
mod rooms;
mod scatter;
mod shape;
mod terrain;
mod validate;

pub use field::{
    Combine, CombineConfig, CombineOp, Gradient, GradientConfig, GradientShape, Noise, NoiseConfig,
    NoiseKind,
};
pub use rooms::{
    AssignRooms, AssignRoomsConfig, ConnectRooms, ConnectRoomsConfig, CorridorShape, Pick,
    PlaceRooms, PlaceRoomsConfig, RoleRule, RoomGrid, RoomGridConfig, Strategy,
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
    add::<ConnectRooms>(r);
    add::<Fill>(r);
    add::<Gradient>(r);
    add::<Noise>(r);
    add::<PlaceRooms>(r);
    add::<RegionSize>(r);
    add::<RoomGrid>(r);
    add::<Scatter>(r);
}
