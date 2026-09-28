use super::*;
use crate::bounds::Bounds;
use crate::coords::{Coord, GridSize};
use crate::layer::LayerKind;

fn parse(json: &str) -> Result<Condition, serde_json::Error> {
    serde_json::from_str(json)
}

fn one(name: &str) -> TileSpec {
    TileSpec::One(name.to_string())
}

// TileSpec

#[test]
fn tile_spec_accepts_a_single_name() {
    let spec: TileSpec = serde_json::from_str(r#""water""#).unwrap();
    assert_eq!(spec, one("water"));
}

#[test]
fn tile_spec_accepts_a_list_of_names() {
    let spec: TileSpec = serde_json::from_str(r#"["water", "lava"]"#).unwrap();
    assert_eq!(
        spec,
        TileSpec::Many(vec!["water".to_string(), "lava".to_string()])
    );
}

#[test]
fn tile_spec_error_says_what_was_expected() {
    // `expecting` replaces serde's generic "data did not match any variant".
    let err = serde_json::from_str::<TileSpec>("42").unwrap_err();
    assert!(
        err.to_string()
            .contains("a tile name or a list of tile names"),
        "{err}"
    );
}

// variants

#[test]
fn parses_tile_is() {
    let c = parse(r#"{"type": "tile_is", "layer": "terrain", "tile": "floor"}"#).unwrap();
    assert_eq!(
        c,
        Condition::TileIs {
            layer: "terrain".to_string(),
            tile: one("floor"),
        }
    );
}

#[test]
fn parses_the_distance_variants() {
    let json = |kind: &str| {
        format!(r#"{{"type": "{kind}", "layer": "objects", "tile": "wall", "distance": 2}}"#)
    };
    let (layer, tile, distance) = ("objects".to_string(), one("wall"), 2);
    assert_eq!(
        parse(&json("near_type")).unwrap(),
        Condition::NearType {
            layer: layer.clone(),
            tile: tile.clone(),
            distance
        }
    );
    assert_eq!(
        parse(&json("not_near_type")).unwrap(),
        Condition::NotNearType {
            layer: layer.clone(),
            tile: tile.clone(),
            distance
        }
    );
    assert_eq!(
        parse(&json("area_is")).unwrap(),
        Condition::AreaIs {
            layer,
            tile,
            distance
        }
    );
}

#[test]
fn parses_count_with_min_and_max() {
    let c = parse(
        r#"{"type": "count", "layer": "terrain", "tile": "wall", "distance": 1, "min": 5, "max": 8}"#,
    )
    .unwrap();
    assert_eq!(
        c,
        Condition::Count {
            layer: "terrain".to_string(),
            tile: one("wall"),
            distance: 1,
            min: Some(5),
            max: Some(8),
        }
    );
}

#[test]
fn count_min_and_max_are_optional_in_the_format() {
    // Requiring at least one of them is a compile-time rule (part 2), where the
    // error can name the step. The format itself accepts both missing.
    let c =
        parse(r#"{"type": "count", "layer": "terrain", "tile": "wall", "distance": 1}"#).unwrap();
    assert_eq!(
        c,
        Condition::Count {
            layer: "terrain".to_string(),
            tile: one("wall"),
            distance: 1,
            min: None,
            max: None,
        }
    );
}

#[test]
fn parses_nested_combinators() {
    let json = r#"{
        "type": "all",
        "conditions": [
            {"type": "tile_is", "layer": "terrain", "tile": "floor"},
            {"type": "not", "condition":
                {"type": "near_type", "layer": "objects", "tile": ["wall", "door"], "distance": 2}},
            {"type": "any", "conditions": []}
        ]
    }"#;
    let expected = Condition::All {
        conditions: vec![
            Condition::TileIs {
                layer: "terrain".to_string(),
                tile: one("floor"),
            },
            Condition::Not {
                condition: Box::new(Condition::NearType {
                    layer: "objects".to_string(),
                    tile: TileSpec::Many(vec!["wall".to_string(), "door".to_string()]),
                    distance: 2,
                }),
            },
            Condition::Any { conditions: vec![] },
        ],
    };
    assert_eq!(parse(json).unwrap(), expected);
}

// errors: typo = error, never silence (PLANNING §1, defect 2)

#[test]
fn unknown_type_is_an_error() {
    // The old Go name. It became `area_is` with the "empty" tile (§2.6).
    assert!(parse(r#"{"type": "layer_clear", "layer": "terrain", "distance": 2}"#).is_err());
}

#[test]
fn misspelled_field_is_an_error() {
    let json =
        r#"{"type": "near_type", "layer": "objects", "tile": "wall", "distance": 2, "distanc": 3}"#;
    assert!(parse(json).is_err());
}

#[test]
fn missing_field_is_an_error() {
    assert!(parse(r#"{"type": "near_type", "layer": "objects", "tile": "wall"}"#).is_err());
}

#[test]
fn missing_type_is_an_error() {
    assert!(parse(r#"{"layer": "terrain", "tile": "floor"}"#).is_err());
}

#[test]
fn not_takes_one_condition_not_a_list() {
    let json = r#"{"type": "not", "condition": [{"type": "tile_is", "layer": "t", "tile": "a"}]}"#;
    assert!(parse(json).is_err());
}

// serialize

#[test]
fn serializes_with_snake_case_type_tag() {
    let c = Condition::NotNearType {
        layer: "objects".to_string(),
        tile: one("wall"),
        distance: 2,
    };
    let json = serde_json::to_string(&c).unwrap();
    assert!(json.contains(r#""type":"not_near_type""#), "{json}");
    assert!(json.contains(r#""tile":"wall""#), "{json}");
}

#[test]
fn serde_round_trip_preserves_nested_conditions() {
    let c = Condition::Any {
        conditions: vec![
            Condition::Count {
                layer: "terrain".to_string(),
                tile: TileSpec::Many(vec!["wall".to_string(), "rock".to_string()]),
                distance: 1,
                min: Some(5),
                max: None,
            },
            Condition::Not {
                condition: Box::new(Condition::AreaIs {
                    layer: "objects".to_string(),
                    tile: one("empty"),
                    distance: 3,
                }),
            },
        ],
    };
    let json = serde_json::to_string(&c).unwrap();
    assert_eq!(parse(&json).unwrap(), c);
}

// compile
//
// Tests are a child module, so they can look inside the opaque
// `CompiledCondition` (its `.0`) and the private `Compiled` enum.

struct World {
    grid: Grid,
    tiles: TileRegistry,
    terrain: LayerId,
    objects: LayerId,
}

fn world(size: GridSize) -> World {
    let mut grid = Grid::new(Bounds::new(Coord::new(10, 20, 5), size));
    let terrain = grid.add_layer("terrain", LayerKind::Tiles).unwrap();
    let objects = grid.add_layer("objects", LayerKind::Tiles).unwrap();
    let tiles = TileRegistry::new(["floor", "wall", "water", "lava"]).unwrap();
    World {
        grid,
        tiles,
        terrain,
        objects,
    }
}

fn world_2d() -> World {
    world(GridSize::new(8, 8, 1))
}

fn world_3d() -> World {
    world(GridSize::new(8, 8, 4))
}

fn compile_json(json: &str, w: &World) -> Result<Compiled, ConditionError> {
    parse(json).unwrap().compile(&w.grid, &w.tiles).map(|c| c.0)
}

fn id(w: &World, name: &str) -> TileId {
    w.tiles.id(name).unwrap()
}

#[test]
fn compiles_names_to_ids() {
    let w = world_2d();
    let c = compile_json(
        r#"{"type": "tile_is", "layer": "objects", "tile": "wall"}"#,
        &w,
    )
    .unwrap();
    assert_eq!(
        c,
        Compiled::TileIs {
            layer: w.objects,
            tiles: vec![id(&w, "wall")],
        }
    );
}

#[test]
fn tile_list_is_sorted_and_deduplicated() {
    let w = world_2d();
    let c = compile_json(
        r#"{"type": "tile_is", "layer": "terrain", "tile": ["water", "floor", "water"]}"#,
        &w,
    )
    .unwrap();
    // floor was declared before water, so it has the smaller id.
    assert_eq!(
        c,
        Compiled::TileIs {
            layer: w.terrain,
            tiles: vec![id(&w, "floor"), id(&w, "water")],
        }
    );
}

#[test]
fn empty_resolves_to_the_reserved_tile() {
    // `area_is` with "empty" is the Go LayerClear (PLANNING 8.10).
    let w = world_2d();
    let c = compile_json(
        r#"{"type": "area_is", "layer": "objects", "tile": "empty", "distance": 1}"#,
        &w,
    )
    .unwrap();
    let Compiled::AreaIs { tiles, .. } = c else {
        panic!("expected AreaIs, got {c:?}");
    };
    assert_eq!(tiles, vec![TileId::EMPTY]);
}

#[test]
fn distance_has_no_z_radius_in_a_2d_map() {
    // A (2N+1)^3 box never fits in depth 1: AreaIs would always fail in 2D.
    let w = world_2d();
    let c = compile_json(
        r#"{"type": "near_type", "layer": "objects", "tile": "wall", "distance": 3}"#,
        &w,
    )
    .unwrap();
    let Compiled::NearType { radius, .. } = c else {
        panic!("expected NearType, got {c:?}");
    };
    assert_eq!(radius, Radius { x: 3, y: 3, z: 0 });
}

#[test]
fn distance_applies_to_every_axis_in_a_3d_map() {
    let w = world_3d();
    let c = compile_json(
        r#"{"type": "not_near_type", "layer": "objects", "tile": "wall", "distance": 3}"#,
        &w,
    )
    .unwrap();
    let Compiled::NotNearType { radius, .. } = c else {
        panic!("expected NotNearType, got {c:?}");
    };
    assert_eq!(radius, Radius { x: 3, y: 3, z: 3 });
}

#[test]
fn unknown_layer_is_an_error() {
    let w = world_2d();
    let err = compile_json(
        r#"{"type": "tile_is", "layer": "terain", "tile": "floor"}"#,
        &w,
    )
    .unwrap_err();
    assert_eq!(
        err,
        ConditionError::Grid(GridError::LayerNotFound {
            name: "terain".to_string()
        })
    );
}

#[test]
fn unknown_tile_is_an_error_with_the_registry_message() {
    let w = world_2d();
    let err = compile_json(
        r#"{"type": "tile_is", "layer": "terrain", "tile": ["floor", "flor"]}"#,
        &w,
    )
    .unwrap_err();
    assert_eq!(
        err,
        ConditionError::Tile(TileError::UnknownTile {
            name: "flor".to_string()
        })
    );
    // #[error(transparent)] keeps the inner message as is.
    assert_eq!(err.to_string(), "tile 'flor' is unknown");
}

// count

fn count_json(bounds: &str) -> String {
    format!(r#"{{"type": "count", "layer": "terrain", "tile": "wall", "distance": 1{bounds}}}"#)
}

fn compiled_count_bounds(bounds: &str) -> Result<(u32, u32), ConditionError> {
    let w = world_2d();
    compile_json(&count_json(bounds), &w).map(|c| match c {
        Compiled::Count { min, max, .. } => (min, max),
        other => panic!("expected Count, got {other:?}"),
    })
}

#[test]
fn count_without_min_or_max_is_an_error() {
    assert_eq!(
        compiled_count_bounds(""),
        Err(ConditionError::CountWithNoBounds)
    );
}

#[test]
fn count_with_min_above_max_is_an_error() {
    assert_eq!(
        compiled_count_bounds(r#", "min": 8, "max": 5"#),
        Err(ConditionError::EmptyCountRange { min: 8, max: 5 })
    );
}

#[test]
fn count_fills_the_missing_bound_with_its_neutral_value() {
    assert_eq!(compiled_count_bounds(r#", "min": 5"#), Ok((5, u32::MAX)));
    assert_eq!(compiled_count_bounds(r#", "max": 3"#), Ok((0, 3)));
    assert_eq!(compiled_count_bounds(r#", "min": 4, "max": 4"#), Ok((4, 4)));
}

#[test]
fn condition_error_messages() {
    assert_eq!(
        ConditionError::CountWithNoBounds.to_string(),
        "count needs at least one of `min` or `max`"
    );
    assert_eq!(
        ConditionError::EmptyCountRange { min: 8, max: 5 }.to_string(),
        "count range is empty: min 8 > max 5"
    );
}

// combinators

#[test]
fn compiles_nested_combinators() {
    let w = world_2d();
    let c = compile_json(
        r#"{"type": "all", "conditions": [
            {"type": "tile_is", "layer": "terrain", "tile": "floor"},
            {"type": "not", "condition": {"type": "tile_is", "layer": "objects", "tile": "wall"}}
        ]}"#,
        &w,
    )
    .unwrap();
    let expected = Compiled::All(vec![
        Compiled::TileIs {
            layer: w.terrain,
            tiles: vec![id(&w, "floor")],
        },
        Compiled::Not(Box::new(Compiled::TileIs {
            layer: w.objects,
            tiles: vec![id(&w, "wall")],
        })),
    ]);
    assert_eq!(c, expected);
}

#[test]
fn an_error_deep_inside_a_combinator_surfaces() {
    let w = world_2d();
    let err = compile_json(
        r#"{"type": "any", "conditions": [
            {"type": "tile_is", "layer": "terrain", "tile": "floor"},
            {"type": "not", "condition": {"type": "tile_is", "layer": "terrain", "tile": "lavaa"}}
        ]}"#,
        &w,
    )
    .unwrap_err();
    assert_eq!(
        err,
        ConditionError::Tile(TileError::UnknownTile {
            name: "lavaa".to_string()
        })
    );
}

#[test]
fn empty_combinators_compile() {
    // A step with no conditions is an implicit empty All and must be valid.
    let w = world_2d();
    assert_eq!(
        compile_json(r#"{"type": "all", "conditions": []}"#, &w),
        Ok(Compiled::All(vec![]))
    );
    assert_eq!(
        compile_json(r#"{"type": "any", "conditions": []}"#, &w),
        Ok(Compiled::Any(vec![]))
    );
}
