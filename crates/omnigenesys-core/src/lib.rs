//! Engine-agnostic procedural map generation.
//!
//! Implementation order for the spatial base is PLANNING §8.10:
//! `Coord` → `Bounds` → `Layer<T>` → `AnyLayer` → `Grid` → iterator on `Layer`.
//! Declare each module here as it is written.
