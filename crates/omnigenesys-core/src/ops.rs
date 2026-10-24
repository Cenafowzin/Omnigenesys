//! The operators that ship with the core, one folder per kind of work:
//!
//! - `terrain`: lays down the base of a layer (`Fill`, `FillBorder`).
//! - `rooms`: dungeons — rooms, connections, roles (`RoomGrid`,
//!   `PlaceRooms`, `ConnectRooms`, `AssignRooms`).
//! - `field`: scalar layers, from noise to tiles (`Noise`, `Falloff`,
//!   `Classify`).
//! - `scatter`: places tiles cell by cell by chance (`Scatter`).
//! - `shape`: reshapes what earlier steps left (`CellularAutomaton`,
//!   `KeepLargestRegion`, `FillSmallRegions`).
//!
//! The folders are for reading the code; the public paths stay flat
//! (`ops::Fill`), so moving an operator between them breaks nobody.

mod field;
mod rooms;
mod scatter;
mod shape;
mod terrain;

pub use field::{
    Band, Classify, ClassifyConfig, Falloff, FalloffConfig, FalloffShape, Noise, NoiseConfig,
    NoiseKind,
};
pub use rooms::{
    AssignRooms, AssignRoomsConfig, ConnectRooms, ConnectRoomsConfig, CorridorShape, Pick,
    PlaceRooms, PlaceRoomsConfig, RoleRule, RoomGrid, RoomGridConfig, Strategy,
};
pub use scatter::{Scatter, ScatterConfig};
pub use shape::{
    CellularAutomaton, CellularAutomatonConfig, Connectivity, FillSmallRegions,
    FillSmallRegionsConfig, KeepLargestRegion, KeepLargestRegionConfig, Outside,
};
pub use terrain::{Fill, FillBorder, FillBorderConfig, FillConfig};

use crate::operator::Registry;

pub(crate) fn register_builtins(r: &mut Registry) {
    r.register::<RoomGrid>()
        .expect("built-in operator names are unique");
    r.register::<PlaceRooms>()
        .expect("built-in operator names are unique");
    r.register::<ConnectRooms>()
        .expect("built-in operator names are unique");
    r.register::<AssignRooms>()
        .expect("built-in operator names are unique");
    r.register::<CellularAutomaton>()
        .expect("built-in operator names are unique");
    r.register::<Classify>()
        .expect("built-in operator names are unique");
    r.register::<Falloff>()
        .expect("built-in operator names are unique");
    r.register::<Fill>()
        .expect("built-in operator names are unique");
    r.register::<FillBorder>()
        .expect("built-in operator names are unique");
    r.register::<FillSmallRegions>()
        .expect("built-in operator names are unique");
    r.register::<KeepLargestRegion>()
        .expect("built-in operator names are unique");
    r.register::<Noise>()
        .expect("built-in operator names are unique");
    r.register::<Scatter>()
        .expect("built-in operator names are unique");
}
