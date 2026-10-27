//! Paths between objects: what a corridor, a road or a river follows.

mod carve_paths;

pub use carve_paths::{Blocked, CarvePaths, CarvePathsConfig, Cost, CostRule, Shape as PathShape};

#[cfg(test)]
mod tests;
