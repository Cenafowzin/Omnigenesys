use super::*;

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
