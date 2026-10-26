//! Paths between objects: what a corridor, a road or a river follows.

mod carve_paths;

pub use carve_paths::{CarvePaths, CarvePathsConfig, Shape as PathShape};

#[cfg(test)]
mod tests;
