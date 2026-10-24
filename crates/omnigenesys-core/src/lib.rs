//! Engine-agnostic procedural map generation.
//!
//! Implementation order for the spatial base is PLANNING §8.10:
//! `Coord` → `Bounds` → `Layer<T>` → `AnyLayer` → `Grid` → iterator on `Layer`.
//! Declare each module here as it is written.
pub mod access;
pub mod bounds;
pub mod condition;
pub mod coords;
pub mod export;
pub mod grid;
pub mod layer;
pub mod mask;
pub mod noise;
pub mod object;
pub mod operator;
pub mod ops;
pub mod pipeline;
pub mod rng;
pub mod spec;
pub mod tile;
