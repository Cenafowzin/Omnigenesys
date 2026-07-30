//! Execution context handed to each operator. The pipeline swaps `rng`
//! before every step (see `rng::derive_step_rng`), so operators never share
//! a random stream — inserting or removing a step does not reshuffle the
//! others (fixes bug #5 of the Go version).

use rand_chacha::ChaCha8Rng;

use crate::coords::GridSize;
use crate::grid::Grid;
use crate::rng::derive_step_rng;
use crate::tile::TileRegistry;

pub struct Context {
    pub grid: Grid,
    pub tiles: TileRegistry,
    /// RNG of the step currently executing. Operators MUST use this and
    /// never construct their own RNG (determinism contract).
    pub rng: ChaCha8Rng,
}

impl Context {
    pub fn new(size: GridSize, seed: u64, salt: &str, layers: &[String]) -> Self {
        let mut grid = Grid::new(size, seed);
        for layer in layers {
            grid.add_layer(layer);
        }
        Self {
            grid,
            tiles: TileRegistry::new(),
            // Placeholder stream; the pipeline re-derives per step.
            rng: derive_step_rng(salt, seed, ""),
        }
    }
}
