//! Dungeons: rooms placed on a grid (Isaac) or at random (Rogue), the
//! connections between them, and the roles they play (PLANNING §2.13).
//! Rooms and connections are objects, so each step reads what the one
//! before made instead of guessing it back from tiles.

mod assign_rooms;
mod common;
mod connect_rooms;
mod place_rooms;
mod room_grid;

pub use assign_rooms::{AssignRooms, AssignRoomsConfig, Pick, RoleRule};
pub use connect_rooms::{ConnectRooms, ConnectRoomsConfig, CorridorShape, Strategy};
pub use place_rooms::{PlaceRooms, PlaceRoomsConfig};
pub use room_grid::{RoomGrid, RoomGridConfig};

#[cfg(test)]
mod tests;
