use super::*;
use crate::{
    access::{Reach, WriteHandle},
    bounds::Bounds,
    coords::{Coord, GridSize},
    layer::LayerKind,
    operator::{Context, Dimensions, OperatorType, Scope},
    rng::Chance,
    tile::TileId,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

// Test operators

/// Puts `tile` on each cell with probability `chance`, deciding with rng.at.
struct Sprinkle {
    layer: WriteHandle,
    tile: TileId,
    chance: Chance,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SprinkleConfig {
    layer: String,
    tile: String,
    chance: f64,
}

impl OperatorType for Sprinkle {
    const NAME: &'static str = "Sprinkle";
    const DIMENSIONS: Dimensions = Dimensions::Any;
    const SCOPE: Scope = Scope::Local;
    type Config = SprinkleConfig;

    fn build(c: SprinkleConfig, bcx: &mut BuildContext<'_>) -> Result<Sprinkle, BuildError> {
        Ok(Sprinkle {
            layer: bcx.write(&c.layer)?,
            tile: bcx.tile(&c.tile)?,
            chance: Chance::new(c.chance).ok_or_else(|| BuildError::Invalid {
                field: "chance".to_string(),
                message: format!("{} is not between 0 and 1", c.chance),
            })?,
        })
    }
}

impl Operator for Sprinkle {
    fn execute(&self, ctx: &mut Context<'_>) -> Result<(), OpError> {
        let parts = ctx.parts();
        let mut grid = parts.grid;
        let region = grid.region();
        let layer = grid.tiles_mut(self.layer)?;
        for c in region.iter() {
            if self.chance.hit(parts.rng.at(c)[0]) {
                layer.set(c, self.tile)?;
            }
        }
        Ok(())
    }
}

/// Only for 3D maps.
struct Volume;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct VolumeConfig {}

impl OperatorType for Volume {
    const NAME: &'static str = "Volume";
    const DIMENSIONS: Dimensions = Dimensions::ThreeD;
    const SCOPE: Scope = Scope::Local;
    type Config = VolumeConfig;

    fn build(_: VolumeConfig, _: &mut BuildContext<'_>) -> Result<Volume, BuildError> {
        Ok(Volume)
    }
}

impl Operator for Volume {
    fn execute(&self, _: &mut Context<'_>) -> Result<(), OpError> {
        Ok(())
    }
}

/// Reads the layer one cell around and writes it: a conflict.
struct Smear;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SmearConfig {
    layer: String,
}

impl OperatorType for Smear {
    const NAME: &'static str = "Smear";
    const DIMENSIONS: Dimensions = Dimensions::Any;
    const SCOPE: Scope = Scope::Local;
    type Config = SmearConfig;

    fn build(c: SmearConfig, bcx: &mut BuildContext<'_>) -> Result<Smear, BuildError> {
        bcx.read(&c.layer, Reach::uniform(1))?;
        bcx.write(&c.layer)?;
        Ok(Smear)
    }
}

impl Operator for Smear {
    fn execute(&self, _: &mut Context<'_>) -> Result<(), OpError> {
        Ok(())
    }
}

/// Always fails while running.
struct Broken;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct BrokenConfig {}

impl OperatorType for Broken {
    const NAME: &'static str = "Broken";
    const DIMENSIONS: Dimensions = Dimensions::Any;
    const SCOPE: Scope = Scope::Global;
    type Config = BrokenConfig;

    fn build(_: BrokenConfig, _: &mut BuildContext<'_>) -> Result<Broken, BuildError> {
        Ok(Broken)
    }
}

impl Operator for Broken {
    fn execute(&self, _: &mut Context<'_>) -> Result<(), OpError> {
        Err(OpError::Failed {
            message: "no room left".to_string(),
        })
    }
}

// Helpers

struct World {
    grid: Grid,
    tiles: TileRegistry,
    registry: Registry,
}

fn world(size: GridSize) -> World {
    let mut grid = Grid::new(Bounds::new(Coord::new(-4, 2, 0), size));
    grid.add_layer("terrain", LayerKind::Tiles).unwrap();
    grid.add_layer("objects", LayerKind::Tiles).unwrap();
    let tiles = TileRegistry::new(["floor", "wall", "tree"]).unwrap();
    let mut registry = Registry::with_builtins();
    registry.register::<Sprinkle>().unwrap();
    registry.register::<Volume>().unwrap();
    registry.register::<Smear>().unwrap();
    registry.register::<Broken>().unwrap();
    World {
        grid,
        tiles,
        registry,
    }
}

fn world_2d() -> World {
    world(GridSize::new(16, 16, 1))
}

fn step(id: &str, operator: &str, config: serde_json::Value) -> StepSpec {
    StepSpec {
        id: id.to_string(),
        operator: operator.to_string(),
        enabled: true,
        config,
        conditions: Vec::new(),
    }
}

fn build(w: &World, steps: &[StepSpec]) -> Result<Pipeline, Vec<StepError<BuildError>>> {
    Pipeline::build(&w.registry, &w.grid, &w.tiles, steps)
}

fn sprinkle(id: &str) -> StepSpec {
    step(
        id,
        "Sprinkle",
        json!({"layer": "objects", "tile": "tree", "chance": 0.3}),
    )
}

fn layer_cells(w: &World, name: &str) -> Vec<TileId> {
    let id = w.grid.layer_id(name).unwrap();
    w.grid.tiles(id).unwrap().cells().copied().collect()
}

// Build

#[test]
fn build_reports_every_failing_step_at_once() {
    let w = world_2d();
    let errors = build(
        &w,
        &[
            step("a", "Fil", json!({})),
            step("b", "Fill", json!({"layer": "terrain", "tile": "floor"})),
            step("c", "Fill", json!({"layer": "terrain", "tlie": "floor"})),
            step("d", "Fill", json!({"layer": "terrain", "tile": "lava"})),
        ],
    )
    .err()
    .unwrap();
    let ids: Vec<&str> = errors.iter().map(|e| e.step_id.as_str()).collect();
    assert_eq!(ids, ["a", "c", "d"]);
    assert_eq!(
        errors[0].error,
        BuildError::UnknownOperator {
            name: "Fil".to_string()
        }
    );
    assert!(matches!(errors[1].error, BuildError::Config { .. }));
    assert_eq!(errors[2].to_string(), "step 'd': tile 'lava' is unknown");
}

#[test]
fn a_missing_layer_is_reported_once() {
    let w = world_2d();
    let errors = build(
        &w,
        &[
            step("a", "Fill", json!({"layer": "water", "tile": "floor"})),
            step("b", "Fill", json!({"layer": "water", "tile": "wall"})),
            step("c", "Fill", json!({"layer": "lava", "tile": "wall"})),
        ],
    )
    .err()
    .unwrap();
    let ids: Vec<&str> = errors.iter().map(|e| e.step_id.as_str()).collect();
    assert_eq!(ids, ["a", "c"]);
}

#[test]
fn a_missing_layer_inside_a_condition_counts_as_the_same_missing_layer() {
    let w = world_2d();
    let mut b = step("b", "Fill", json!({"layer": "terrain", "tile": "wall"}));
    b.conditions = vec![
        serde_json::from_str(r#"{"type": "tile_is", "layer": "water", "tile": "floor"}"#).unwrap(),
    ];
    let errors = build(
        &w,
        &[
            step("a", "Fill", json!({"layer": "water", "tile": "floor"})),
            b,
        ],
    )
    .err()
    .unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].step_id, "a");
}

#[test]
fn step_ids_must_be_unique() {
    let w = world_2d();
    let errors = build(&w, &[sprinkle("trees"), sprinkle("trees")])
        .err()
        .unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(
        errors[0].error,
        BuildError::DuplicateStepId {
            id: "trees".to_string()
        }
    );
}

#[test]
fn operators_are_checked_against_the_map_dimensions() {
    let flat = world_2d();
    let errors = build(&flat, &[step("v", "Volume", json!({}))])
        .err()
        .unwrap();
    assert_eq!(
        errors[0].error,
        BuildError::WrongDimensions {
            operator: "Volume".to_string(),
            found: "2D"
        }
    );
    let deep = world(GridSize::new(4, 4, 4));
    assert!(build(&deep, &[step("v", "Volume", json!({}))]).is_ok());
}

#[test]
fn operator_validation_errors_reach_the_user() {
    let w = world_2d();
    let bad = step(
        "trees",
        "Sprinkle",
        json!({"layer": "objects", "tile": "tree", "chance": 1.5}),
    );
    let errors = build(&w, &[bad]).err().unwrap();
    assert_eq!(
        errors[0].to_string(),
        "step 'trees': invalid 'chance': 1.5 is not between 0 and 1"
    );
}

#[test]
fn read_write_conflicts_fail_the_build() {
    let w = world_2d();
    let errors = build(&w, &[step("s", "Smear", json!({"layer": "terrain"}))])
        .err()
        .unwrap();
    assert_eq!(
        errors[0].error,
        BuildError::ReadWriteConflict {
            layer: "terrain".to_string()
        }
    );
}

#[test]
fn disabled_steps_are_not_built_or_run() {
    let mut w = world_2d();
    let mut off = step("off", "Fil", json!({}));
    off.enabled = false;
    let fill = step("fill", "Fill", json!({"layer": "terrain", "tile": "wall"}));
    let p = build(&w, &[off, fill]).unwrap();
    assert!(p.access("off").is_none());
    p.run(&mut w.grid, "", 1).unwrap();
    let wall = w.tiles.id("wall").unwrap();
    assert!(layer_cells(&w, "terrain").iter().all(|&t| t == wall));
}

#[test]
fn the_access_of_each_step_is_kept() {
    let w = world_2d();
    let mut fill = step("fill", "Fill", json!({"layer": "objects", "tile": "tree"}));
    fill.conditions = vec![
        serde_json::from_str(
            r#"{"type": "not_near_type", "layer": "terrain", "tile": "wall", "distance": 2}"#,
        )
        .unwrap(),
    ];
    let p = build(&w, &[fill]).unwrap();
    let access = p.access("fill").unwrap();
    let (terrain, objects) = (
        w.grid.layer_id("terrain").unwrap(),
        w.grid.layer_id("objects").unwrap(),
    );
    assert!(access.writes(objects));
    // 2D map: no z reach.
    assert_eq!(access.read_reach(terrain), Some(Reach::symmetric(2, 2, 0)));
}

// Run

#[test]
fn steps_run_in_order_and_see_earlier_writes() {
    let mut w = world_2d();
    let mut trees = step("trees", "Fill", json!({"layer": "objects", "tile": "tree"}));
    trees.conditions = vec![
        serde_json::from_str(r#"{"type": "tile_is", "layer": "terrain", "tile": "floor"}"#)
            .unwrap(),
    ];
    let p = build(
        &w,
        &[
            step(
                "floor",
                "Fill",
                json!({"layer": "terrain", "tile": "floor"}),
            ),
            trees,
        ],
    )
    .unwrap();
    p.run(&mut w.grid, "", 1).unwrap();
    let tree = w.tiles.id("tree").unwrap();
    assert!(layer_cells(&w, "objects").iter().all(|&t| t == tree));
}

#[test]
fn a_runtime_failure_names_its_step() {
    let mut w = world_2d();
    let p = build(&w, &[sprinkle("trees"), step("rooms", "Broken", json!({}))]).unwrap();
    let err = p.run(&mut w.grid, "", 1).unwrap_err();
    assert_eq!(err.step_id, "rooms");
    assert_eq!(err.to_string(), "step 'rooms': no room left");
}

#[test]
fn same_seed_same_map_and_another_seed_another_map() {
    let run = |seed: u64| {
        let mut w = world_2d();
        build(&w, &[sprinkle("trees")])
            .unwrap()
            .run(&mut w.grid, "", seed)
            .unwrap();
        layer_cells(&w, "objects")
    };
    assert_eq!(run(7), run(7));
    assert_ne!(run(7), run(8));
}

#[test]
fn adding_a_step_does_not_reshuffle_the_others() {
    // The core promise of per-step randomness (PLANNING §2.1): a step's
    // result depends on its id, not on its position in the list.
    let run = |steps: &[StepSpec]| {
        let mut w = world_2d();
        build(&w, steps).unwrap().run(&mut w.grid, "", 7).unwrap();
        layer_cells(&w, "objects")
    };
    let alone = run(&[sprinkle("trees")]);
    let after_another = run(&[
        step(
            "rocks",
            "Sprinkle",
            json!({"layer": "terrain", "tile": "wall", "chance": 0.5}),
        ),
        sprinkle("trees"),
    ]);
    assert_eq!(alone, after_another);
}

#[test]
fn each_step_draws_from_its_own_randomness() {
    // Two identical sprinkles on two layers: with a shared rng they would
    // pick the very same cells. And renaming a step changes what it draws.
    let mut w = world_2d();
    let on = |layer: &str, id: &str| {
        step(
            id,
            "Sprinkle",
            json!({"layer": layer, "tile": "tree", "chance": 0.3}),
        )
    };
    build(&w, &[on("terrain", "a"), on("objects", "b")])
        .unwrap()
        .run(&mut w.grid, "", 7)
        .unwrap();
    assert_ne!(layer_cells(&w, "terrain"), layer_cells(&w, "objects"));

    let run_named = |id: &str| {
        let mut w = world_2d();
        build(&w, &[on("objects", id)])
            .unwrap()
            .run(&mut w.grid, "", 7)
            .unwrap();
        layer_cells(&w, "objects")
    };
    assert_ne!(run_named("trees"), run_named("bushes"));
}

#[test]
fn the_salt_changes_the_map() {
    let run = |salt: &str| {
        let mut w = world_2d();
        build(&w, &[sprinkle("trees")])
            .unwrap()
            .run(&mut w.grid, salt, 7)
            .unwrap();
        layer_cells(&w, "objects")
    };
    assert_ne!(run("game-a"), run("game-b"));
}
