use serde_json::json;

use crate::{
    bounds::Bounds,
    condition::Condition,
    coords::{Coord, GridSize},
    grid::Grid,
    layer::LayerKind,
    operator::{BuildError, Registry},
    pipeline::{Pipeline, StepSpec},
    tile::{TileId, TileRegistry},
};

struct World {
    grid: Grid,
    tiles: TileRegistry,
}

fn world() -> World {
    let mut grid = Grid::new(Bounds::new(Coord::new(3, -2, 1), GridSize::new(6, 5, 2)));
    grid.add_layer("terrain", LayerKind::Tiles).unwrap();
    let tiles = TileRegistry::new(["floor", "wall", "water"]).unwrap();
    World { grid, tiles }
}

impl World {
    fn id(&self, name: &str) -> TileId {
        self.tiles.id(name).unwrap()
    }

    fn at(&self, c: Coord) -> TileId {
        let terrain = self.grid.layer_id("terrain").unwrap();
        *self.grid.tiles(terrain).unwrap().get(c).unwrap()
    }

    fn run(&mut self, steps: &[StepSpec]) -> Result<(), Vec<String>> {
        let p = Pipeline::build(&Registry::with_builtins(), &self.grid, &self.tiles, steps)
            .map_err(|es| es.iter().map(|e| e.to_string()).collect::<Vec<_>>())?;
        p.run(&mut self.grid, "", 1)
            .map_err(|e| vec![e.to_string()])
    }
}

fn fill(id: &str, tile: &str, conditions: &[&str]) -> StepSpec {
    StepSpec {
        id: id.to_string(),
        operator: "Fill".to_string(),
        enabled: true,
        config: json!({"layer": "terrain", "tile": tile}),
        conditions: conditions
            .iter()
            .map(|c| serde_json::from_str::<Condition>(c).unwrap())
            .collect(),
    }
}

#[test]
fn fill_sets_every_cell() {
    let mut w = world();
    w.run(&[fill("base", "floor", &[])]).unwrap();
    let floor = w.id("floor");
    for c in w.grid.bounds().iter() {
        assert_eq!(w.at(c), floor, "cell {c:?}");
    }
}

#[test]
fn fill_with_conditions_sets_only_the_cells_where_they_hold() {
    // Base floor, one wall, then water where there is no wall within 1.
    let mut w = world();
    let wall_at = Coord::new(5, 0, 1);
    w.run(&[fill("base", "floor", &[])]).unwrap();
    let terrain = w.grid.layer_id("terrain").unwrap();
    let wall = w.id("wall");
    w.grid
        .tiles_mut(terrain)
        .unwrap()
        .set(wall_at, wall)
        .unwrap();

    w.run(&[fill(
        "lake",
        "water",
        &[r#"{"type": "not_near_type", "layer": "terrain", "tile": "wall", "distance": 1}"#],
    )])
    .unwrap();

    for c in w.grid.bounds().iter() {
        let d = c - wall_at;
        let near_wall = d.x.abs() <= 1 && d.y.abs() <= 1 && d.z.abs() <= 1;
        let expected = match (c == wall_at, near_wall) {
            (true, _) => "wall",
            (false, true) => "floor",
            (false, false) => "water",
        };
        assert_eq!(w.at(c), w.id(expected), "cell {c:?}");
    }
}

#[test]
fn conditions_see_the_layer_as_it_was_before_the_step() {
    // "water where floor is near" over an all-floor map: every cell starts
    // near floor, so every cell becomes water. If the condition saw the
    // step's own writes, cells visited late would no longer qualify.
    let mut w = world();
    w.run(&[fill("base", "floor", &[])]).unwrap();
    w.run(&[fill(
        "flood",
        "water",
        &[r#"{"type": "count", "layer": "terrain", "tile": "floor", "distance": 1, "min": 4}"#],
    )])
    .unwrap();
    let water = w.id("water");
    for c in w.grid.bounds().iter() {
        assert_eq!(w.at(c), water, "cell {c:?}");
    }
}

#[test]
fn fill_config_errors_are_build_errors() {
    let mut w = world();
    let mut typo = fill("base", "floor", &[]);
    typo.config = json!({"layer": "terrain", "tiel": "floor"});
    let errors = w.run(&[typo]).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert!(
        errors[0].starts_with("step 'base': invalid config at '/tiel': unknown field `tiel`"),
        "{}",
        errors[0]
    );
}

#[test]
fn fill_schema_requires_layer_and_tile() {
    let r = Registry::with_builtins();
    let mut generator = schemars::generate::SchemaSettings::draft07().into_generator();
    (r.get("Fill").unwrap().schema)(&mut generator);
    let schema = &generator.definitions()["FillConfig"];
    assert_eq!(schema["additionalProperties"], json!(false));
    let mut required: Vec<&str> = schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    required.sort_unstable();
    assert_eq!(required, ["layer", "tile"]);
}

#[test]
fn unknown_tile_in_fill_is_reported_with_its_name() {
    let w = world();
    let errors = Pipeline::build(
        &Registry::with_builtins(),
        &w.grid,
        &w.tiles,
        &[fill("base", "lava", &[])],
    )
    .err()
    .unwrap();
    assert!(matches!(errors[0].error, BuildError::Tile(_)));
}
