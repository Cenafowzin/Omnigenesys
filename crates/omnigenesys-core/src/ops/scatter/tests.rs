use serde_json::json;

use crate::{
    bounds::Bounds,
    condition::Condition,
    coords::{Coord, GridSize},
    grid::Grid,
    layer::LayerKind,
    operator::{BuildError, Registry},
    pipeline::{Pipeline, StepSpec},
    rng::{Chance, StepRng},
    tile::{TileId, TileRegistry},
};

struct World {
    grid: Grid,
    tiles: TileRegistry,
}

fn world() -> World {
    let mut grid = Grid::new(Bounds::new(Coord::new(-10, 4, 0), GridSize::new(64, 64, 1)));
    grid.add_layer("terrain", LayerKind::Tiles).unwrap();
    let tiles = TileRegistry::new(["floor", "wall"]).unwrap();
    World { grid, tiles }
}

fn scatter(chance: f64, conditions: &[&str]) -> StepSpec {
    StepSpec {
        id: "walls".to_string(),
        operator: "Scatter".to_string(),
        enabled: true,
        config: json!({"layer": "terrain", "tile": "wall", "chance": chance}),
        conditions: conditions
            .iter()
            .map(|c| serde_json::from_str::<Condition>(c).unwrap())
            .collect(),
    }
}

impl World {
    fn run(&mut self, steps: &[StepSpec], seed: u64) {
        Pipeline::build(&Registry::with_builtins(), &self.grid, &self.tiles, steps)
            .unwrap()
            .run(&mut self.grid, "", seed)
            .unwrap();
    }

    fn at(&self, c: Coord) -> TileId {
        let terrain = self.grid.layer_id("terrain").unwrap();
        *self.grid.tiles(terrain).unwrap().get(c).unwrap()
    }

    fn walls(&self) -> usize {
        let wall = self.tiles.id("wall").unwrap();
        self.grid
            .bounds()
            .iter()
            .filter(|&c| self.at(c) == wall)
            .count()
    }
}

#[test]
fn each_cell_decides_with_its_own_rng_at() {
    // The whole contract of a Local operator: cell c is a wall exactly when
    // the step's rng.at(c) says so, whatever the other cells do.
    let mut w = world();
    w.run(&[scatter(0.3, &[])], 42);
    let rng = StepRng::new("", 42, "walls");
    let chance = Chance::new(0.3).unwrap();
    let wall = w.tiles.id("wall").unwrap();
    for c in w.grid.bounds().iter() {
        assert_eq!(w.at(c) == wall, chance.hit(rng.at(c)[0]), "cell {c:?}");
    }
}

#[test]
fn chance_zero_places_nothing_and_one_places_everywhere() {
    let mut none = world();
    none.run(&[scatter(0.0, &[])], 1);
    assert_eq!(none.walls(), 0);

    let mut all = world();
    all.run(&[scatter(1.0, &[])], 1);
    assert_eq!(all.walls(), 64 * 64);
}

#[test]
fn about_chance_of_the_cells_are_hit() {
    let mut w = world();
    w.run(&[scatter(0.25, &[])], 3);
    // 4096 cells: standard deviation ~28, so 5 sigma is ~140.
    let n = w.walls() as i64;
    assert!((n - 1024).abs() < 140, "{n} walls");
}

#[test]
fn conditions_limit_where_it_scatters() {
    // Only the left half (x < 22) is a candidate.
    let mut w = world();
    let floor = w.tiles.id("floor").unwrap();
    let terrain = w.grid.layer_id("terrain").unwrap();
    for c in w.grid.bounds().iter().filter(|c| c.x < 22) {
        w.grid.tiles_mut(terrain).unwrap().set(c, floor).unwrap();
    }
    w.run(
        &[scatter(
            1.0,
            &[r#"{"type": "tile_is", "layer": "terrain", "tile": "floor"}"#],
        )],
        5,
    );
    let wall = w.tiles.id("wall").unwrap();
    for c in w.grid.bounds().iter() {
        assert_eq!(w.at(c) == wall, c.x < 22, "cell {c:?}");
    }
}

#[test]
fn chance_outside_zero_to_one_is_a_build_error() {
    let w = world();
    for bad in [-0.5, 1.01] {
        let errors = Pipeline::build(
            &Registry::with_builtins(),
            &w.grid,
            &w.tiles,
            &[scatter(bad, &[])],
        )
        .err()
        .unwrap();
        assert_eq!(
            errors[0].error,
            BuildError::Invalid {
                field: "chance".to_string(),
                message: format!("{bad} is not between 0 and 1"),
            }
        );
    }
}
