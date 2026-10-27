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
    let Compiled::Count { tiles, .. } = c else {
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
    let Compiled::Count { radius, .. } = c else {
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
    let Compiled::Count { radius, .. } = c else {
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

fn compiled_count_bounds(bounds: &str) -> Result<(u64, u64), ConditionError> {
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
    assert_eq!(compiled_count_bounds(r#", "min": 5"#), Ok((5, u64::MAX)));
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
    assert_eq!(
        ConditionError::TooManyCellsToCount {
            cells: 5_000_000_000
        }
        .to_string(),
        "layer has 5000000000 cells, more than a distance condition can count"
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
fn empty_lists_are_refused() {
    // They would hold never (`any`, no tile) or always (`all`): nobody
    // writes that on purpose. A step with no conditions builds no `all`.
    let w = world_2d();
    assert_eq!(
        compile_json(r#"{"type": "all", "conditions": []}"#, &w),
        Err(ConditionError::NoConditions { what: "all" })
    );
    assert_eq!(
        compile_json(r#"{"type": "any", "conditions": []}"#, &w),
        Err(ConditionError::NoConditions { what: "any" })
    );
    for t in ["tile_is", "near_type", "count"] {
        let json = format!(
            r#"{{"type": "{t}", "layer": "terrain", "tile": [], "distance": 1, "min": 1}}"#
        );
        let json = if t == "tile_is" {
            r#"{"type": "tile_is", "layer": "terrain", "tile": []}"#.to_string()
        } else if t == "near_type" {
            json.replace(r#", "min": 1"#, "")
        } else {
            json
        };
        assert_eq!(compile_json(&json, &w), Err(ConditionError::NoTiles), "{t}");
    }
}

#[test]
fn the_shortcuts_are_counts() {
    let w = world_2d();
    let bounds = |t: &str| match compile_json(
        &format!(r#"{{"type": "{t}", "layer": "terrain", "tile": "wall", "distance": 2}}"#),
        &w,
    ) {
        Ok(Compiled::Count { min, max, .. }) => (min, max),
        other => panic!("{t}: {other:?}"),
    };
    assert_eq!(bounds("near_type"), (1, u64::MAX));
    assert_eq!(bounds("not_near_type"), (0, 0));
    // The whole 5 × 5 box, in 2D.
    assert_eq!(bounds("area_is"), (25, 25));
}

// prepare / test — through the public API

fn prepare_json(json: &str, w: &World) -> PreparedCondition {
    parse(json)
        .unwrap()
        .compile(&w.grid, &w.tiles)
        .unwrap()
        .prepare(&w.grid)
        .unwrap()
}

impl World {
    // A method, not a free fn: `w.paint(w.terrain, ..)` reads `w.terrain`
    // before activating `&mut self` (two-phase borrow). The free-fn form
    // `w.paint(w.terrain, ..)` is rejected by the borrow checker.
    fn paint(&mut self, layer: LayerId, c: Coord, tile: &str) {
        let id = self.tiles.id(tile).unwrap();
        self.grid.tiles_mut(layer).unwrap().set(c, id).unwrap();
    }

    fn randomize(&mut self, layer: LayerId, seed: u64) {
        let names = ["empty", "floor", "wall", "water", "lava"];
        let ids: Vec<TileId> = names.iter().map(|n| self.tiles.id(n).unwrap()).collect();
        let mut rng = XorShift(seed);
        for cell in self.grid.tiles_mut(layer).unwrap().cells_mut() {
            *cell = ids[(rng.next() % ids.len() as u64) as usize];
        }
    }
}

#[test]
fn tile_is_answers_per_cell() {
    let mut w = world_2d();
    let water = Coord::new(12, 21, 5);
    w.paint(w.terrain, water, "water");
    let p = prepare_json(
        r#"{"type": "tile_is", "layer": "terrain", "tile": ["water", "lava"]}"#,
        &w,
    );
    assert!(p.test(water));
    assert!(!p.test(Coord::new(13, 21, 5)));
    assert!(!p.test(Coord::new(-50, 21, 5)), "outside never matches");
}

#[test]
fn tile_is_empty_matches_untouched_cells() {
    let mut w = world_2d();
    let wall = Coord::new(10, 20, 5);
    w.paint(w.objects, wall, "wall");
    let p = prepare_json(
        r#"{"type": "tile_is", "layer": "objects", "tile": "empty"}"#,
        &w,
    );
    assert!(!p.test(wall));
    assert!(p.test(Coord::new(11, 20, 5)));
}

#[test]
fn combinators_follow_boolean_logic() {
    let mut w = world_2d();
    let c = Coord::new(12, 21, 5);
    w.paint(w.terrain, c, "floor");
    let floor = r#"{"type": "tile_is", "layer": "terrain", "tile": "floor"}"#;
    let wall = r#"{"type": "tile_is", "layer": "terrain", "tile": "wall"}"#;
    let any = format!(r#"{{"type": "any", "conditions": [{floor}, {wall}]}}"#);
    let all = format!(r#"{{"type": "all", "conditions": [{floor}, {wall}]}}"#);
    let not_wall = format!(r#"{{"type": "not", "condition": {wall}}}"#);
    assert!(prepare_json(&any, &w).test(c));
    assert!(!prepare_json(&all, &w).test(c));
    assert!(prepare_json(&not_wall, &w).test(c));
}

#[test]
fn prepared_condition_is_a_snapshot_of_the_grid() {
    // PLANNING 2.7: Prepared owns its data. The grid can be written while the
    // prepared condition is alive (no borrow conflict), and the condition keeps
    // answering about the map as it was at prepare time.
    let mut w = world_2d();
    let c = Coord::new(12, 21, 5);
    let p = prepare_json(
        r#"{"type": "tile_is", "layer": "terrain", "tile": "floor"}"#,
        &w,
    );
    w.paint(w.terrain, c, "floor");
    assert!(!p.test(c));
}

// reference check: random map, every cell, against a naive evaluation

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

#[test]
fn tile_conditions_match_a_naive_evaluation_on_a_random_map() {
    let mut w = world_3d();
    w.randomize(w.terrain, 0x9E37_79B9_7F4A_7C15);
    w.randomize(w.objects, 0xD1B5_4A32_D192_ED03);
    let json = r#"{"type": "any", "conditions": [
        {"type": "all", "conditions": [
            {"type": "tile_is", "layer": "terrain", "tile": ["floor", "water"]},
            {"type": "not", "condition": {"type": "tile_is", "layer": "objects", "tile": "wall"}}
        ]},
        {"type": "tile_is", "layer": "objects", "tile": "lava"}
    ]}"#;
    let p = prepare_json(json, &w);

    let terrain = w.grid.tiles(w.terrain).unwrap();
    let objects = w.grid.tiles(w.objects).unwrap();
    let is = |layer: &Layer<TileId>, c: Coord, names: &[&str]| {
        let v = *layer.get(c).unwrap();
        names.iter().any(|n| w.tiles.id(n).unwrap() == v)
    };
    let mut matched = 0;
    for c in w.grid.bounds().iter() {
        let naive = (is(terrain, c, &["floor", "water"]) && !is(objects, c, &["wall"]))
            || is(objects, c, &["lava"]);
        assert_eq!(p.test(c), naive, "cell {c:?}");
        matched += naive as usize;
    }
    // Guard against a vacuous pass: the random map must exercise both answers.
    assert!(matched > 0 && matched < w.grid.bounds().size.len());
    for region in regions_around(w.grid.bounds()) {
        assert_eval_agrees_with_test(&p, region);
    }
}

// eval: test is checked against naive evaluations, so eval only has to agree
// with test, cell by cell, over any region.

fn assert_eval_agrees_with_test(p: &PreparedCondition, region: Bounds) {
    let mask = p.eval(region);
    assert_eq!(mask.bounds(), region);
    for c in region.iter() {
        assert_eq!(mask.get(c), p.test(c), "cell {c:?}, region {region:?}");
    }
}

/// The grid itself (the fast path for tile_is), a larger region, one that
/// overlaps only a corner, and one entirely outside.
fn regions_around(grid: Bounds) -> [Bounds; 4] {
    let o = grid.origin;
    [
        grid,
        grid.expand(2, 1, 1),
        Bounds::new(Coord::new(o.x + 5, o.y - 2, o.z), GridSize::new(6, 4, 1)),
        Bounds::new(Coord::new(o.x - 20, o.y, o.z), GridSize::new(3, 3, 1)),
    ]
}

#[test]
fn eval_of_an_empty_region_is_an_empty_mask() {
    let w = world_2d();
    let region = Bounds::new(Coord::new(10, 20, 5), GridSize::new(0, 4, 1));
    for json in [
        r#"{"type": "tile_is", "layer": "terrain", "tile": "empty"}"#,
        r#"{"type": "not_near_type", "layer": "terrain", "tile": "wall", "distance": 1}"#,
    ] {
        assert_eq!(prepare_json(json, &w).eval(region).count_ones(), 0);
    }
}

#[test]
fn eval_not_keeps_bits_past_the_region_zero() {
    // 8×8×1 = 64 cells fits a word exactly; 3×3×1 = 9 leaves 55 spare bits
    // that a careless NOT would turn on.
    let w = world_2d();
    let p = prepare_json(
        r#"{"type": "not", "condition": {"type": "tile_is", "layer": "terrain", "tile": "wall"}}"#,
        &w,
    );
    let region = Bounds::new(Coord::new(11, 21, 5), GridSize::new(3, 3, 1));
    assert_eq!(p.eval(region).count_ones(), 9);
}

// prepare / test — PrefixSums (the four distance conditions)

fn rad(x: u32, y: u32, z: u32) -> Radius {
    Radius { x, y, z }
}

/// The Go algorithm: visit every cell of the box. Cells outside the layer
/// never match. Only for small radii and centers.
fn naive_count(layer: &Layer<TileId>, tiles: &[TileId], c: Coord, r: Radius) -> u32 {
    let (rx, ry, rz) = (r.x as i32, r.y as i32, r.z as i32);
    let mut n = 0;
    for z in c.z - rz..=c.z + rz {
        for y in c.y - ry..=c.y + ry {
            for x in c.x - rx..=c.x + rx {
                if let Some(t) = layer.get(Coord::new(x, y, z)) {
                    n += u32::from(tiles.contains(t));
                }
            }
        }
    }
    n
}

#[test]
fn prefix_sums_entry_counts_the_box_below_it() {
    // Every cell matches, so entry (x, y, z) must be exactly x * y * (z + 1):
    // x and y have a zero border at 0, z has no border (plane z covers 0..=z).
    let bounds = Bounds::new(Coord::new(-3, 7, 2), GridSize::new(3, 2, 2));
    let layer = Layer::new(bounds, TileId(1)).unwrap();
    let s = PrefixSums::from_layer(&layer, &TileSet::new(&[TileId(1)])).unwrap();
    assert_eq!((s.sw, s.sh, s.sums.len()), (4, 3, 4 * 3 * 2));
    for z in 0..2 {
        for y in 0..3 {
            for x in 0..4 {
                let entry = s.sums[x + 4 * (y + 3 * z)];
                assert_eq!(entry, (x * y * (z + 1)) as u32, "entry ({x}, {y}, {z})");
            }
        }
    }
}

#[test]
fn window_count_matches_a_naive_box_scan_on_a_random_map() {
    let mut w = world_3d();
    w.randomize(w.terrain, 0x2545_F491_4F6C_DD1D);
    let layer = w.grid.tiles(w.terrain).unwrap();
    let tiles = [id(&w, "wall"), id(&w, "water")];
    let s = PrefixSums::from_layer(layer, &TileSet::new(&tiles)).unwrap();
    // Uneven radii catch a swapped axis; 9 is larger than the whole 8×8×4 map.
    let radii = [
        rad(0, 0, 0),
        rad(1, 1, 1),
        rad(2, 2, 2),
        rad(3, 1, 0),
        rad(0, 4, 2),
        rad(9, 9, 9),
    ];
    // Centers up to 3 cells outside the layer, so clipping runs on every side.
    let around = layer.bounds().expand(3, 3, 3);
    for r in radii {
        for c in around.iter() {
            let expected = naive_count(layer, &tiles, c, r);
            assert_eq!(s.count(c, r), expected, "cell {c:?}, {r:?}");
        }
    }
}

#[test]
fn distance_conditions_match_a_naive_evaluation_on_random_maps() {
    for (mut w, seed) in [(world_2d(), 0x9E37_79B9), (world_3d(), 0x7F4A_7C15)] {
        w.randomize(w.terrain, seed);
        // Four of the five random tiles: dense enough that area_is and count
        // are sometimes true.
        let names = r#"["floor", "wall", "water", "lava"]"#;
        let tiles = ["floor", "wall", "water", "lava"].map(|n| id(&w, n));
        let layer = w.grid.tiles(w.terrain).unwrap();
        let around = layer.bounds().expand(2, 2, 2);
        // [condition][answer]: guards against a vacuous pass, every condition
        // must have answered both true and false somewhere.
        let mut seen = [[false; 2]; 4];
        for distance in [0, 1, 2, 9] {
            let r = radius(distance, &w.grid);
            let cond = |kind: &str, extra: &str| {
                let json = format!(
                    r#"{{"type": "{kind}", "layer": "terrain", "tile": {names},
                         "distance": {distance}{extra}}}"#
                );
                prepare_json(&json, &w)
            };
            let near = cond("near_type", "");
            let not_near = cond("not_near_type", "");
            let area = cond("area_is", "");
            let count = cond("count", r#", "min": 3, "max": 7"#);
            for c in around.iter() {
                let n = u64::from(naive_count(layer, &tiles, c, r));
                let at = format!("cell {c:?}, distance {distance}");
                assert_eq!(near.test(c), n >= 1, "near_type {at}");
                assert_eq!(not_near.test(c), n == 0, "not_near_type {at}");
                assert_eq!(area.test(c), n == r.volume(), "area_is {at}");
                assert_eq!(count.test(c), (3..=7).contains(&n), "count {at}");
                for (i, p) in [&near, &not_near, &area, &count].into_iter().enumerate() {
                    seen[i][usize::from(p.test(c))] = true;
                }
            }
            for p in [&near, &not_near, &area, &count] {
                for region in regions_around(w.grid.bounds()) {
                    assert_eval_agrees_with_test(p, region);
                }
            }
        }
        assert_eq!(seen, [[true; 2]; 4], "2D: {}", w.grid.bounds().size.is_2d());
    }
}

impl World {
    fn paint_all(&mut self, layer: LayerId, tile: &str) {
        for c in self.grid.bounds().iter() {
            self.paint(layer, c, tile);
        }
    }
}

#[test]
fn area_is_fails_where_the_box_leaves_the_map() {
    // The map is all floor, yet a border cell's box sticks out of the map and
    // the cells out there never match. In 2D there is no z radius, so the
    // single plane does not make every cell a border cell.
    let mut w = world_2d();
    w.paint_all(w.terrain, "floor");
    let p = prepare_json(
        r#"{"type": "area_is", "layer": "terrain", "tile": "floor", "distance": 1}"#,
        &w,
    );
    // Bounds are x 10..18, y 20..28.
    assert!(p.test(Coord::new(11, 21, 5)));
    assert!(p.test(Coord::new(16, 26, 5)));
    assert!(!p.test(Coord::new(10, 23, 5)));
    assert!(!p.test(Coord::new(17, 23, 5)));
    assert!(!p.test(Coord::new(13, 27, 5)));
}

#[test]
fn not_near_type_is_true_once_the_box_misses_the_map() {
    let mut w = world_2d();
    w.paint_all(w.terrain, "floor");
    let p = prepare_json(
        r#"{"type": "not_near_type", "layer": "terrain", "tile": "floor", "distance": 2}"#,
        &w,
    );
    assert!(!p.test(Coord::new(12, 23, 5)));
    // x = 8 still reaches x = 10, the first column; x = 7 does not.
    assert!(!p.test(Coord::new(8, 23, 5)));
    assert!(p.test(Coord::new(7, 23, 5)));
}

#[test]
fn count_bounds_are_inclusive() {
    let mut w = world_2d();
    for (x, y) in [(11, 22), (12, 22), (13, 24)] {
        w.paint(w.terrain, Coord::new(x, y, 5), "wall");
    }
    let count = |range: &str| {
        let json = format!(
            r#"{{"type": "count", "layer": "terrain", "tile": "wall", "distance": 1, {range}}}"#
        );
        prepare_json(&json, &w).test(Coord::new(12, 23, 5))
    };
    assert!(count(r#""min": 3, "max": 3"#));
    assert!(count(r#""min": 3"#));
    assert!(count(r#""max": 3"#));
    assert!(!count(r#""min": 4"#));
    assert!(!count(r#""max": 2"#));
}

#[test]
fn a_box_larger_than_the_map_counts_every_match() {
    let mut w = world_3d();
    w.randomize(w.terrain, 0xA076_1D64_78BD_642F);
    let wall = id(&w, "wall");
    let layer = w.grid.tiles(w.terrain).unwrap();
    let total = layer.cells().filter(|&&t| t == wall).count();
    let json = format!(
        r#"{{"type": "count", "layer": "terrain", "tile": "wall", "distance": 1000,
             "min": {total}, "max": {total}}}"#
    );
    let p = prepare_json(&json, &w);
    for c in w.grid.bounds().expand(5, 5, 5).iter() {
        assert!(p.test(c), "cell {c:?}");
    }
}

#[test]
fn extreme_centers_and_distances_do_not_overflow() {
    // The clipping is done in i64: with i32 a center at i32::MAX plus any
    // radius would overflow (a panic in debug, a wrong box in release).
    let mut w = world_3d();
    w.paint(w.terrain, Coord::new(13, 24, 6), "wall");
    let max = u32::MAX;
    let cond = |kind: &str, extra: &str| {
        let json = format!(
            r#"{{"type": "{kind}", "layer": "terrain", "tile": "wall", "distance": {max}{extra}}}"#
        );
        prepare_json(&json, &w)
    };
    let near = cond("near_type", "");
    let count = cond("count", r#", "min": 1, "max": 1"#);
    let area = cond("area_is", "");
    for c in [
        Coord::new(i32::MIN, i32::MAX, 0),
        Coord::new(i32::MAX, i32::MIN, i32::MAX),
        Coord::new(13, 24, 6),
    ] {
        assert!(near.test(c), "cell {c:?}");
        assert!(count.test(c), "cell {c:?}");
        // The volume saturates at u64::MAX, which no count can reach.
        assert!(!area.test(c), "cell {c:?}");
    }
}

#[test]
fn distance_zero_is_the_cell_itself() {
    let mut w = world_2d();
    let wall = Coord::new(12, 23, 5);
    w.paint(w.terrain, wall, "wall");
    let p = prepare_json(
        r#"{"type": "near_type", "layer": "terrain", "tile": "wall", "distance": 0}"#,
        &w,
    );
    assert!(p.test(wall));
    assert!(!p.test(Coord::new(13, 23, 5)));
    assert!(!p.test(Coord::new(12, 24, 5)));
}

#[test]
fn window_condition_is_a_snapshot_of_the_grid() {
    let mut w = world_2d();
    let c = Coord::new(12, 23, 5);
    let p = prepare_json(
        r#"{"type": "near_type", "layer": "terrain", "tile": "wall", "distance": 1}"#,
        &w,
    );
    w.paint(w.terrain, c, "wall");
    assert!(!p.test(c));
}

#[test]
fn layer_with_no_cells_needs_no_table() {
    // (u32::MAX, 0, 1) has no cells, but a padded table would still have
    // 2^32 × 1 × 2 entries: it must not be allocated.
    for size in [GridSize::new(0, 3, 2), GridSize::new(u32::MAX, 0, 1)] {
        let layer = Layer::new(Bounds::new(Coord::ZERO, size), TileId::EMPTY).unwrap();
        let s = PrefixSums::from_layer(&layer, &TileSet::new(&[TileId::EMPTY])).unwrap();
        assert!(s.sums.is_empty(), "size {size:?}");
        assert_eq!(s.count(Coord::ZERO, rad(5, 5, 5)), 0, "size {size:?}");
    }
}

#[test]
fn radius_volume_is_the_full_box_and_saturates() {
    assert_eq!(rad(0, 0, 0).volume(), 1);
    assert_eq!(rad(1, 1, 0).volume(), 9);
    assert_eq!(rad(2, 1, 3).volume(), 5 * 3 * 7);
    assert_eq!(rad(u32::MAX, u32::MAX, u32::MAX).volume(), u64::MAX);
}

// offset

#[test]
fn offset_looks_at_the_cell_that_far_away() {
    let mut w = world_2d();
    let floor = Coord::new(12, 21, 5);
    w.paint(w.terrain, floor, "floor");
    let left_is_floor = prepare_json(
        r#"{"type": "offset", "x": -1, "condition": {"type": "tile_is", "layer": "terrain", "tile": "floor"}}"#,
        &w,
    );
    assert!(left_is_floor.test(Coord::new(13, 21, 5)));
    assert!(!left_is_floor.test(floor));
    assert!(!left_is_floor.test(Coord::new(13, 22, 5)));
}

#[test]
fn past_the_edge_of_the_map_offset_is_false_and_not_offset_true() {
    let w = world_2d();
    // Every cell is empty, so the condition holds on every cell of the map.
    let empty = r#"{"type": "tile_is", "layer": "terrain", "tile": "empty"}"#;
    let left = format!(r#"{{"type": "offset", "x": -1, "condition": {empty}}}"#);
    let p = prepare_json(&left, &w);
    let edge = Coord::new(10, 23, 5);
    assert!(!p.test(edge), "no cell to the left of the left edge");
    assert!(p.test(Coord::new(11, 23, 5)));
    let not_left = format!(r#"{{"type": "not", "condition": {left}}}"#);
    assert!(prepare_json(&not_left, &w).test(edge));
    // The same with a condition that holds outside the layer too.
    let not_wall =
        r#"{"type": "not", "condition": {"type": "tile_is", "layer": "terrain", "tile": "wall"}}"#;
    let up = format!(r#"{{"type": "offset", "y": -1, "condition": {not_wall}}}"#);
    assert!(!prepare_json(&up, &w).test(Coord::new(12, 20, 5)));
    for region in regions_around(w.grid.bounds()) {
        assert_eval_agrees_with_test(&p, region);
        assert_eval_agrees_with_test(&prepare_json(&up, &w), region);
    }
}

#[test]
fn offset_matches_a_naive_evaluation_on_a_random_map() {
    let mut w = world_3d();
    w.randomize(w.terrain, 0x2545_F491_4F6C_DD1D);
    // Floor two to the right and one up in z, with a wall within 1 of that
    // cell, or no water just below in y.
    let json = r#"{"type": "any", "conditions": [
        {"type": "offset", "x": 2, "z": 1, "condition": {"type": "all", "conditions": [
            {"type": "tile_is", "layer": "terrain", "tile": "floor"},
            {"type": "near_type", "layer": "terrain", "tile": "wall", "distance": 1}]}},
        {"type": "not", "condition": {"type": "offset", "y": 1,
            "condition": {"type": "tile_is", "layer": "terrain", "tile": "water"}}}
    ]}"#;
    let p = prepare_json(json, &w);
    let map = w.grid.bounds();
    let terrain = w.grid.tiles(w.terrain).unwrap();
    let is = |c: Coord, name: &str| {
        map.contains(c) && *terrain.get(c).unwrap() == w.tiles.id(name).unwrap()
    };
    let mut matched = 0;
    for c in map.iter() {
        let a = Coord::new(c.x + 2, c.y, c.z + 1);
        let near_wall = (a.x - 1..=a.x + 1).any(|x| {
            (a.y - 1..=a.y + 1)
                .any(|y| (a.z - 1..=a.z + 1).any(|z| is(Coord::new(x, y, z), "wall")))
        });
        let below = Coord::new(c.x, c.y + 1, c.z);
        let naive = (is(a, "floor") && near_wall) || !is(below, "water");
        assert_eq!(p.test(c), naive, "cell {c:?}");
        matched += usize::from(naive);
    }
    assert!(matched > 0 && matched < map.size.len());
    for region in regions_around(map) {
        assert_eval_agrees_with_test(&p, region);
    }
}

#[test]
fn offset_moves_what_the_inner_condition_reads() {
    let w = world_2d();
    let json = r#"{"type": "offset", "x": -1, "y": 3,
        "condition": {"type": "near_type", "layer": "terrain", "tile": "wall", "distance": 1}}"#;
    let c = parse(json).unwrap().compile(&w.grid, &w.tiles).unwrap();
    // x: one more below, one less above; y: none below (3 - 1 past it),
    // four above.
    let reach = Reach {
        below: [2, 0, 0],
        above: [0, 4, 0],
    };
    assert_eq!(c.reads(), vec![(w.terrain, reach)]);
}

#[test]
fn offset_in_z_on_a_2d_map_is_an_error() {
    let w = world_2d();
    let e = compile_json(
        r#"{"type": "offset", "z": 1, "condition": {"type": "tile_is", "layer": "terrain", "tile": "wall"}}"#,
        &w,
    )
    .unwrap_err();
    assert_eq!(e, ConditionError::OffsetInZOn2d { z: 1 });
    assert!(e.to_string().contains("the map is 2D"));
    assert!(compile_json(
        r#"{"type": "offset", "z": 1, "condition": {"type": "tile_is", "layer": "terrain", "tile": "wall"}}"#,
        &world_3d(),
    )
    .is_ok());
}

// neighbours

#[test]
fn neighbours_counts_the_sides_or_all_around() {
    let mut w = world_2d();
    // Floor left and right of (13, 23), and on its upper-left corner.
    for c in [(12, 23), (14, 23), (12, 22)] {
        w.paint(w.terrain, Coord::new(c.0, c.1, 5), "floor");
    }
    let floor = r#"{"type": "tile_is", "layer": "terrain", "tile": "floor"}"#;
    let with = |shape: &str, range: &str| {
        prepare_json(
            &format!(
                r#"{{"type": "neighbours", "shape": "{shape}", {range}, "condition": {floor}}}"#
            ),
            &w,
        )
    };
    let cell = Coord::new(13, 23, 5);
    assert!(with("sides", r#""min": 2, "max": 2"#).test(cell));
    assert!(!with("sides", r#""min": 3"#).test(cell));
    assert!(with("all", r#""min": 3, "max": 3"#).test(cell));
    // The cell itself is not one of its neighbours:
    // (12, 22) is floor, and only (12, 23) of its neighbours.
    assert!(with("all", r#""min": 1, "max": 1"#).test(Coord::new(12, 22, 5)));
    assert!(with("all", r#""min": 1, "max": 1"#).test(Coord::new(15, 23, 5)));
}

#[test]
fn past_the_edge_of_the_map_a_neighbour_passes_nothing() {
    let w = world_2d();
    let empty = r#"{"type": "tile_is", "layer": "terrain", "tile": "empty"}"#;
    let p = |shape: &str| {
        prepare_json(
            &format!(
                r#"{{"type": "neighbours", "shape": "{shape}", "min": 0, "condition": {empty}}}"#
            ),
            &w,
        )
    };
    let exactly = |shape: &str, n: u32| {
        prepare_json(
            &format!(
                r#"{{"type": "neighbours", "shape": "{shape}", "min": {n}, "max": {n}, "condition": {empty}}}"#
            ),
            &w,
        )
    };
    let corner = Coord::new(10, 20, 5);
    assert!(exactly("sides", 2).test(corner));
    assert!(exactly("all", 3).test(corner));
    assert!(exactly("sides", 3).test(Coord::new(12, 20, 5)));
    assert!(exactly("all", 8).test(Coord::new(12, 22, 5)));
    for region in regions_around(w.grid.bounds()) {
        assert_eval_agrees_with_test(&exactly("sides", 2), region);
        assert_eval_agrees_with_test(&p("all"), region);
    }
}

#[test]
fn neighbours_matches_a_naive_count_on_a_random_map() {
    let mut w = world_3d();
    w.randomize(w.terrain, 0x9E37_79B9_7F4A_7C15);
    let map = w.grid.bounds();
    let terrain = w.grid.tiles(w.terrain).unwrap();
    let is = |c: Coord, name: &str| {
        map.contains(c) && *terrain.get(c).unwrap() == w.tiles.id(name).unwrap()
    };
    for (shape, min, max) in [("sides", 2, 3), ("all", 6, 9)] {
        // Floor next to a wall, counted around the cell.
        let json = format!(
            r#"{{"type": "neighbours", "shape": "{shape}", "min": {min}, "max": {max},
                "condition": {{"type": "any", "conditions": [
                    {{"type": "tile_is", "layer": "terrain", "tile": "floor"}},
                    {{"type": "offset", "x": 1, "condition": {{"type": "tile_is", "layer": "terrain", "tile": "wall"}}}}]}}}}"#
        );
        let p = prepare_json(&json, &w);
        let mut matched = 0;
        for c in map.iter() {
            let mut n = 0;
            for dz in -1i32..=1 {
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let away = dx.abs() + dy.abs() + dz.abs();
                        if away == 0 || (shape == "sides" && away > 1) {
                            continue;
                        }
                        let s = Coord::new(c.x + dx, c.y + dy, c.z + dz);
                        let right = Coord::new(s.x + 1, s.y, s.z);
                        n += u32::from(map.contains(s) && (is(s, "floor") || is(right, "wall")));
                    }
                }
            }
            let naive = (min..=max).contains(&n);
            assert_eq!(p.test(c), naive, "{shape}, cell {c:?}");
            matched += usize::from(naive);
        }
        assert!(
            matched > 0 && matched < map.size.len(),
            "{shape}: {matched}"
        );
        for region in regions_around(map) {
            assert_eval_agrees_with_test(&p, region);
        }
    }
}

#[test]
fn neighbours_reads_one_more_cell_around() {
    let w = world_2d();
    let json = r#"{"type": "neighbours", "min": 1, "condition":
        {"type": "offset", "x": 2, "condition": {"type": "tile_is", "layer": "terrain", "tile": "wall"}}}"#;
    let c = parse(json).unwrap().compile(&w.grid, &w.tiles).unwrap();
    // x: the offset reads 2 above, a neighbour 1 more on each side. A
    // reach always holds the cell itself, so the left neighbour's
    // `[0, 2]` becomes 1 below: one cell more than needed, never less.
    let reach = Reach {
        below: [1, 1, 0],
        above: [3, 1, 0],
    };
    assert_eq!(c.reads(), vec![(w.terrain, reach)]);
}

#[test]
fn neighbours_needs_a_range_it_can_meet() {
    let w = world_2d();
    let json = |range: &str| {
        format!(
            r#"{{"type": "neighbours", {range} "condition": {{"type": "tile_is", "layer": "terrain", "tile": "wall"}}}}"#
        )
    };
    let err = |range: &str| compile_json(&json(range), &w).unwrap_err();
    assert_eq!(err(""), ConditionError::NeighboursWithNoBounds);
    assert_eq!(
        err(r#""min": 3, "max": 2,"#),
        ConditionError::EmptyNeighboursRange { min: 3, max: 2 }
    );
    assert_eq!(
        err(r#""min": 5,"#),
        ConditionError::MoreNeighboursThanThereAre { min: 5, count: 4 }
    );
    assert!(compile_json(&json(r#""shape": "all", "min": 8,"#), &w).is_ok());
    assert!(compile_json(&json(r#""min": 6,"#), &world_3d()).is_ok());
}
