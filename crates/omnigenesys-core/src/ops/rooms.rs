//! The classic dungeon: rooms placed at random (Rogue), the connections
//! between them, and the roles they play (PLANNING §2.13). Isaac's floors
//! moved to the object operators (`ops::objects`); these follow with the
//! dungeon recipe.
//! Rooms and connections are objects, so each step reads what the one
//! before made instead of guessing it back from tiles.

mod assign_rooms;
mod common;
mod connect_rooms;
mod place_rooms;

pub use assign_rooms::{AssignRooms, AssignRoomsConfig, Pick, RoleRule};
pub use connect_rooms::{ConnectRooms, ConnectRoomsConfig, CorridorShape, Strategy};
pub use place_rooms::{PlaceRooms, PlaceRoomsConfig};

#[cfg(test)]
mod tests;
