use super::*;
use crate::coords::{Coord, GridSize};
use crate::layer::LayerKind;

struct World {
    grid: Grid,
    tiles: TileRegistry,
}

fn world(depth: u32) -> World {
    let mut grid = Grid::new(Bounds::new(Coord::ZERO, GridSize::new(8, 8, depth)));
    grid.add_layer("terrain", LayerKind::Tiles).unwrap();
    grid.add_layer("objects", LayerKind::Tiles).unwrap();
    let tiles = TileRegistry::new(["floor", "wall", "tree"]).unwrap();
    World { grid, tiles }
}

fn condition(json: &str) -> Condition {
    serde_json::from_str(json).unwrap()
}

// Reach

#[test]
fn reach_union_takes_the_larger_side_of_each_axis() {
    let a = Reach {
        below: [1, 0, 3],
        above: [0, 2, 0],
    };
    let b = Reach {
        below: [0, 4, 1],
        above: [5, 0, 0],
    };
    let u = a.union(b);
    assert_eq!(u.below, [1, 4, 3]);
    assert_eq!(u.above, [5, 2, 0]);
    assert!(Reach::ZERO.is_zero());
    assert_eq!(Reach::uniform(2), Reach::symmetric(2, 2, 2));
}

// BuildContext: declarations become the access

#[test]
fn reads_and_writes_are_recorded_with_their_reach() {
    let w = world(4);
    let mut bcx = BuildContext::new(&w.grid, &w.tiles, None, Scope::Local);
    let r = bcx.read("terrain", Reach::uniform(2)).unwrap();
    let _ = bcx.read("terrain", Reach::symmetric(3, 0, 0)).unwrap();
    let o = bcx.write("objects").unwrap();
    let access = bcx.finish("Test").unwrap();

    let terrain = w.grid.layer_id("terrain").unwrap();
    assert_eq!(r.layer, terrain);
    assert_eq!(access.read_reach(terrain), Some(Reach::symmetric(3, 2, 2)));
    assert!(!access.writes(terrain));
    assert!(access.writes(o.layer));
    assert_eq!(access.read_reach(o.layer), None);
}

#[test]
fn a_2d_map_drops_the_z_reach() {
    let w = world(1);
    let mut bcx = BuildContext::new(&w.grid, &w.tiles, None, Scope::Local);
    bcx.read("terrain", Reach::uniform(2)).unwrap();
    let access = bcx.finish("Test").unwrap();
    let terrain = w.grid.layer_id("terrain").unwrap();
    assert_eq!(access.read_reach(terrain), Some(Reach::symmetric(2, 2, 0)));
}

#[test]
fn condition_reads_join_the_access_with_their_distance() {
    let w = world(4);
    let mut bcx = BuildContext::new(&w.grid, &w.tiles, None, Scope::Local);
    bcx.condition(&condition(
        r#"{"type": "all", "conditions": [
            {"type": "tile_is", "layer": "terrain", "tile": "floor"},
            {"type": "near_type", "layer": "objects", "tile": "tree", "distance": 3}
        ]}"#,
    ))
    .unwrap();
    let access = bcx.finish("Test").unwrap();
    let (terrain, objects) = (
        w.grid.layer_id("terrain").unwrap(),
        w.grid.layer_id("objects").unwrap(),
    );
    assert_eq!(access.read_reach(terrain), Some(Reach::ZERO));
    assert_eq!(access.read_reach(objects), Some(Reach::uniform(3)));
}

#[test]
fn unknown_names_are_build_errors() {
    let w = world(1);
    let mut bcx = BuildContext::new(&w.grid, &w.tiles, None, Scope::Local);
    assert_eq!(
        bcx.write("terain").unwrap_err(),
        BuildError::Grid(GridError::LayerNotFound {
            name: "terain".to_string()
        })
    );
    assert_eq!(
        bcx.tile("flor").unwrap_err(),
        BuildError::Tile(TileError::UnknownTile {
            name: "flor".to_string()
        })
    );
}

#[test]
fn reading_around_a_cell_and_writing_the_same_layer_is_a_conflict() {
    let w = world(1);
    let mut bcx = BuildContext::new(&w.grid, &w.tiles, None, Scope::Local);
    bcx.read("terrain", Reach::uniform(1)).unwrap();
    bcx.write("terrain").unwrap();
    assert_eq!(
        bcx.finish("Smear").unwrap_err(),
        BuildError::ReadWriteConflict {
            layer: "terrain".to_string()
        }
    );
}

#[test]
fn reading_the_cell_itself_and_writing_it_is_fine() {
    let w = world(1);
    let mut bcx = BuildContext::new(&w.grid, &w.tiles, None, Scope::Local);
    bcx.read("terrain", Reach::ZERO).unwrap();
    bcx.write("terrain").unwrap();
    assert!(bcx.finish("Swap").is_ok());
}

#[test]
fn conditions_on_the_written_layer_are_fine() {
    // Conditions read a snapshot from the start of the step, so a wide
    // condition on the layer being written does not depend on visit order.
    let w = world(1);
    let c =
        condition(r#"{"type": "near_type", "layer": "terrain", "tile": "wall", "distance": 2}"#);
    let mut bcx = BuildContext::new(&w.grid, &w.tiles, None, Scope::Local);
    bcx.condition(&c).unwrap();
    bcx.write("terrain").unwrap();
    assert!(bcx.finish("Grow").is_ok());
}

#[test]
fn step_conditions_must_be_taken_by_the_operator() {
    let w = world(1);
    let c = condition(r#"{"type": "tile_is", "layer": "terrain", "tile": "floor"}"#)
        .compile(&w.grid, &w.tiles)
        .unwrap();

    let ignored = BuildContext::new(&w.grid, &w.tiles, Some(c.clone()), Scope::Local);
    assert_eq!(
        ignored.finish("Deaf").unwrap_err(),
        BuildError::ConditionsNotSupported {
            operator: "Deaf".to_string()
        }
    );

    let mut taken = BuildContext::new(&w.grid, &w.tiles, Some(c.clone()), Scope::Local);
    assert_eq!(taken.step_conditions(), Some(c));
    let access = taken.finish("Listens").unwrap();
    // The step conditions' reads are part of the access either way.
    let terrain = w.grid.layer_id("terrain").unwrap();
    assert_eq!(access.read_reach(terrain), Some(Reach::ZERO));
}

// GridView: only declared layers

#[test]
fn a_handle_from_another_step_is_refused() {
    let mut w = world(1);
    // Step A declares a write to "terrain"; step B declares nothing.
    let mut a = BuildContext::new(&w.grid, &w.tiles, None, Scope::Local);
    let handle = a.write("terrain").unwrap();
    let read = a.read("objects", Reach::ZERO).unwrap();
    let b_access = BuildContext::new(&w.grid, &w.tiles, None, Scope::Local)
        .finish("B")
        .unwrap();

    let mut ctx = Context::new(&mut w.grid, &b_access, StepRng::new("", 1, "b"));
    let mut parts = ctx.parts();
    let terrain = handle.layer;
    assert_eq!(
        parts.grid.tiles_mut(handle).unwrap_err(),
        OpError::Undeclared { layer: terrain }
    );
    assert!(matches!(
        parts.grid.tiles(read),
        Err(OpError::Undeclared { .. })
    ));
}

// Registry

struct Nop;

#[derive(serde::Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct NopConfig {
    size: u32,
    #[serde(default)]
    label: Option<String>,
}

impl Operator for Nop {
    fn execute(&self, _: &mut Context<'_>) -> Result<(), OpError> {
        Ok(())
    }
}

impl OperatorType for Nop {
    const NAME: &'static str = "Nop";
    const DIMENSIONS: Dimensions = Dimensions::Any;
    const SCOPE: Scope = Scope::Global;
    type Config = NopConfig;

    fn build(_: NopConfig, _: &mut BuildContext<'_>) -> Result<Nop, BuildError> {
        Ok(Nop)
    }
}

fn build_nop(config: serde_json::Value) -> Result<(), BuildError> {
    let w = world(1);
    let mut r = Registry::new();
    r.register::<Nop>().unwrap();
    let mut bcx = BuildContext::new(&w.grid, &w.tiles, None, Scope::Local);
    (r.get("Nop").unwrap().factory)(config, &mut bcx).map(|_| ())
}

#[test]
fn registering_a_name_twice_is_an_error() {
    let mut r = Registry::new();
    r.register::<Nop>().unwrap();
    assert_eq!(
        r.register::<Nop>().unwrap_err(),
        DuplicateOperator {
            name: "Nop".to_string()
        }
    );
}

#[test]
fn registry_entries_carry_the_operator_metadata() {
    let mut r = Registry::new();
    r.register::<Nop>().unwrap();
    let e = r.get("Nop").unwrap();
    assert_eq!(e.dimensions, Dimensions::Any);
    assert_eq!(e.scope, Scope::Global);
    assert_eq!(r.names().collect::<Vec<_>>(), ["Nop"]);
    assert!(r.get("nop").is_none(), "names are case-sensitive");
}

#[test]
fn config_errors_say_where_in_the_config() {
    assert!(build_nop(serde_json::json!({"size": 3})).is_ok());

    let typo = build_nop(serde_json::json!({"size": 3, "lable": "x"})).unwrap_err();
    let BuildError::Config { path, message } = typo else {
        panic!("expected a config error, got {typo:?}");
    };
    // The path points at the misspelled field itself.
    assert_eq!(path, "/lable");
    assert!(message.contains("unknown field `lable`"), "{message}");

    let wrong = build_nop(serde_json::json!({"size": "big"})).unwrap_err();
    let BuildError::Config { path, .. } = wrong else {
        panic!("expected a config error, got {wrong:?}");
    };
    assert_eq!(path, "/size");
}

#[test]
fn the_schema_comes_from_the_config_type() {
    let mut r = Registry::new();
    r.register::<Nop>().unwrap();
    let mut generator = schemars::generate::SchemaSettings::draft07().into_generator();
    let reference = (r.get("Nop").unwrap().schema)(&mut generator);
    assert_eq!(
        serde_json::to_value(reference).unwrap(),
        serde_json::json!({"$ref": "#/definitions/NopConfig"})
    );
    let schema = &generator.definitions()["NopConfig"];
    // deny_unknown_fields becomes additionalProperties: false.
    assert_eq!(schema["additionalProperties"], serde_json::json!(false));
    assert_eq!(schema["required"], serde_json::json!(["size"]));
    assert!(schema["properties"]["label"].is_object());
}

#[test]
fn dimensions_accept_the_right_maps() {
    assert!(Dimensions::TwoD.accepts(true) && !Dimensions::TwoD.accepts(false));
    assert!(Dimensions::ThreeD.accepts(false) && !Dimensions::ThreeD.accepts(true));
    assert!(Dimensions::Any.accepts(true) && Dimensions::Any.accepts(false));
}
