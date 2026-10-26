use serde_json::json;

use super::*;
use crate::tile::TileId;

/// A small valid file; tests change one thing in it.
fn base() -> Value {
    json!({
        "version": 2,
        "size": {"width": 8, "height": 4},
        "layers": ["terrain"],
        "tiles": ["floor", "wall"],
        "steps": [
            {"id": "base", "operator": "Fill", "config": {"layer": "terrain", "tile": "floor"}}
        ]
    })
}

fn with(change: impl FnOnce(&mut Value)) -> String {
    let mut v = base();
    change(&mut v);
    v.to_string()
}

fn parse(text: &str) -> Parsed {
    PipelineSpec::parse(text).unwrap_or_else(|e| panic!("{e:#?}"))
}

/// (pointer, message) of every error.
fn errors(text: &str) -> Vec<(String, String)> {
    PipelineSpec::parse(text)
        .err()
        .unwrap_or_else(|| panic!("expected errors in {text}"))
        .into_iter()
        .map(|d| (d.pointer, d.message))
        .collect()
}

fn one_error(text: &str) -> (String, String) {
    let mut e = errors(text);
    assert_eq!(e.len(), 1, "{e:?}");
    e.remove(0)
}

/// Line and column of the first `needle` in an ASCII `text`.
fn position_of(text: &str, needle: &str) -> (usize, usize) {
    let i = text.find(needle).unwrap();
    let line = text[..i].matches('\n').count() + 1;
    let column = i - text[..i].rfind('\n').map_or(0, |n| n + 1) + 1;
    (line, column)
}

fn position(d: &Diagnostic) -> (usize, usize) {
    let s = d.span.unwrap_or_else(|| panic!("{d:?} is not located"));
    (s.line, s.column)
}

#[test]
fn reads_a_minimal_file_with_its_defaults() {
    let spec = parse(&base().to_string()).spec;
    assert_eq!(spec.seed, None);
    assert_eq!(spec.salt, "");
    assert_eq!(spec.size, GridSize::new(8, 4, 1));
    assert_eq!(
        spec.layers,
        [LayerSpec {
            name: "terrain".to_string(),
            kind: LayerKind::Tiles
        }]
    );
    assert_eq!(spec.tiles, ["floor", "wall"]);
    let step = &spec.steps[0];
    assert_eq!((step.id.as_str(), step.operator.as_str()), ("base", "Fill"));
    assert!(step.enabled);
    assert!(step.conditions.is_empty());
}

#[test]
fn a_step_without_config_gets_an_empty_one() {
    let text = with(|v| v["steps"] = json!([{"id": "a", "operator": "Nop"}]));
    assert_eq!(parse(&text).spec.steps[0].config, json!({}));
}

#[test]
fn optional_fields_are_read() {
    let text = with(|v| {
        v["$schema"] = json!("./pipeline.schema.json");
        v["description"] = json!("a test map");
        v["salt"] = json!("lost-fields");
        v["seed"] = json!(512);
        v["size"]["depth"] = json!(3);
        v["steps"][0]["enabled"] = json!(false);
        v["steps"][0]["description"] = json!("the ground");
    });
    let spec = parse(&text).spec;
    assert_eq!(spec.description.as_deref(), Some("a test map"));
    assert_eq!(spec.salt, "lost-fields");
    assert_eq!(spec.seed, Some(512));
    assert_eq!(spec.size.depth, 3);
    assert!(!spec.steps[0].enabled);
}

#[test]
fn version_is_checked_before_anything_else() {
    let newer = with(|v| {
        v["version"] = json!(3);
        v["rivers"] = json!(true);
    });
    assert_eq!(
        one_error(&newer),
        (
            "/version".to_string(),
            "format version 3 is newer than this omnigenesys reads (2); update omnigenesys"
                .to_string()
        )
    );
    let older = with(|v| v["version"] = json!(1));
    assert!(one_error(&older).1.contains("version 1 is not supported"));
    let text = with(|v| v["version"] = json!("2"));
    assert_eq!(one_error(&text).1, "`version` must be an integer");
    let missing = with(|v| {
        v.as_object_mut().unwrap().remove("version");
    });
    assert_eq!(
        one_error(&missing).1,
        "missing `version` (this is format version 2)"
    );
    assert_eq!(one_error("[]").1, "a pipeline is a JSON object");
}

#[test]
fn seeds_are_numbers_up_to_2_pow_53_or_strings() {
    let seed = |s: Value| PipelineSpec::parse(&with(|v| v["seed"] = s)).map(|p| p.spec.seed);
    assert_eq!(seed(json!(0)).unwrap(), Some(0));
    assert_eq!(seed(json!(MAX_NUMBER_SEED)).unwrap(), Some(MAX_NUMBER_SEED));
    assert_eq!(seed(json!("18446744073709551615")).unwrap(), Some(u64::MAX));
    assert_eq!(seed(json!("007")).unwrap(), Some(7));
    assert_eq!(seed(json!(null)).unwrap(), None);

    let message = |s: Value| one_error(&with(|v| v["seed"] = s)).1;
    assert_eq!(
        message(json!(MAX_NUMBER_SEED + 1)),
        "seed 9007199254740992 is above 2^53-1, where JavaScript rounds numbers; \
         write it as a string: \"9007199254740992\""
    );
    assert!(message(json!(-1)).starts_with("invalid value: integer `-1`"));
    assert!(message(json!(1.5)).starts_with("invalid type: floating point `1.5`"));
    assert!(message(json!(1e20)).contains("write it as a decimal string"));
    assert!(message(json!("12a")).starts_with("invalid value: string \"12a\""));
    assert!(message(json!("")).starts_with("invalid value: string \"\""));
    assert!(message(json!("-1")).starts_with("invalid value"));
    assert_eq!(
        message(json!("18446744073709551616")),
        "seed \"18446744073709551616\" does not fit in 64 bits"
    );
}

#[test]
fn an_unknown_root_field_is_an_error_at_that_field() {
    let text = with(|v| v["sise"] = json!(1));
    let (at, message) = one_error(&text);
    assert_eq!(at, "/sise");
    assert!(message.starts_with("unknown field `sise`"), "{message}");
}

#[test]
fn layers_are_names_or_objects() {
    let text = with(|v| {
        v["layers"] = json!(["terrain", {"name": "objects"}, {"name": "x", "kind": "tiles"}])
    });
    let spec = parse(&text).spec;
    let names: Vec<_> = spec.layers.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["terrain", "objects", "x"]);
    assert!(spec.layers.iter().all(|l| l.kind == LayerKind::Tiles));

    let text = with(|v| v["layers"][0] = json!({"name": "t", "kind": "scalar"}));
    assert_eq!(parse(&text).spec.layers[0].kind, LayerKind::Scalar);
    let (at, message) = one_error(&with(|v| {
        v["layers"][0] = json!({"name": "t", "kind": "volume"})
    }));
    assert_eq!(at, "/layers/0/kind");
    assert!(message.contains("unknown variant `volume`"), "{message}");
    let (at, message) = one_error(&with(|v| v["layers"][0] = json!({"nme": "t"})));
    assert_eq!(at, "/layers/0/nme");
    assert!(message.starts_with("unknown field `nme`"), "{message}");
}

#[test]
fn layer_and_tile_names_are_checked_all_at_once() {
    let text = with(|v| {
        v["layers"] = json!(["terrain", "", "terrain"]);
        v["tiles"] = json!(["floor", "empty", "floor"]);
        v["size"]["width"] = json!(0);
    });
    let e = PipelineSpec::parse(&text).unwrap_err();
    let got: Vec<_> = e
        .iter()
        .map(|d| (d.pointer.as_str(), d.message.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            ("/size/width", "width must be at least 1"),
            ("/layers/1", "layer name is empty"),
            ("/layers/2", "layer 'terrain' is declared twice"),
            ("/tiles/1", "'empty' is reserved and cannot be declared"),
            ("/tiles/2", "tile 'floor' is declared twice"),
        ]
    );
    assert_eq!(e[2].note.as_ref().unwrap().pointer, "/layers/0");
}

#[test]
fn step_ids_follow_the_frozen_rule() {
    let long = "a".repeat(64);
    for good in ["a", "base-floor_2", long.as_str()] {
        let text = with(|v| v["steps"][0]["id"] = json!(good));
        assert!(PipelineSpec::parse(&text).is_ok(), "{good}");
    }
    let too_long = "a".repeat(65);
    for bad in [
        "",
        "Base",
        "1st",
        "_a",
        "a b",
        "a/b",
        "ação",
        too_long.as_str(),
    ] {
        let text = with(|v| v["steps"][0]["id"] = json!(bad));
        let (at, message) = one_error(&text);
        assert_eq!(at, "/steps/0/id", "{bad}");
        assert!(message.contains("must start with a-z"), "{message}");
    }
}

#[test]
fn a_repeated_step_id_points_at_the_first_even_when_disabled() {
    let text = with(|v| {
        v["steps"] = json!([
            {"id": "a", "operator": "Fill", "enabled": false},
            {"id": "b", "operator": "Fill"},
            {"id": "a", "operator": "Fill"}
        ])
    });
    let e = PipelineSpec::parse(&text).unwrap_err();
    assert_eq!(e.len(), 1, "{e:?}");
    assert_eq!(e[0].pointer, "/steps/2/id");
    assert_eq!(e[0].step_id.as_deref(), Some("a"));
    assert_eq!(e[0].note.as_ref().unwrap().pointer, "/steps/0/id");
}

#[test]
fn every_broken_step_is_reported_with_its_id() {
    let text = with(|v| {
        v["steps"] = json!([
            {"id": "a", "operator": "Fill", "enabeld": true},
            {"id": "b", "operator": "Fill"},
            {"operator": "Fill"},
            {"id": "d", "operator": "Fill", "conditions": [{"type": "tile_is"}]}
        ])
    });
    let e = PipelineSpec::parse(&text).unwrap_err();
    let got: Vec<_> = e
        .iter()
        .map(|d| (d.step_id.as_deref(), d.pointer.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            (Some("a"), "/steps/0/enabeld"),
            (None, "/steps/2"),
            (Some("d"), "/steps/3/conditions/0"),
        ]
    );
    assert!(e[1].message.contains("missing field `id`"));
}

#[test]
fn params_reach_config_and_conditions() {
    let text = with(|v| {
        v["params"] = json!({"wall_tile": "wall", "reach": 2});
        v["steps"][0]["config"]["tile"] = json!({"$param": "wall_tile"});
        v["steps"][0]["conditions"] = json!([{"type": "near_type", "layer": "terrain", "tile": "floor", "distance": {"$param": "reach"}}]);
    });
    let parsed = parse(&text);
    assert!(parsed.warnings.is_empty());
    let step = &parsed.spec.steps[0];
    assert_eq!(step.config["tile"], json!("wall"));
    assert_eq!(
        step.conditions,
        [Condition::NearType {
            layer: "terrain".to_string(),
            tile: crate::condition::TileSpec::One("floor".to_string()),
            distance: 2
        }]
    );
    assert_eq!(parsed.spec.params["reach"].default, ParamValue::Integer(2));
}

#[test]
fn an_unused_param_is_a_warning_at_its_declaration() {
    let text = with(|v| v["params"] = json!({"density": 0.5}));
    let parsed = parse(&text);
    assert_eq!(parsed.warnings.len(), 1);
    let w = &parsed.warnings[0];
    assert_eq!(w.message, "param 'density' is never used");
    assert_eq!(w.pointer, "/params/density");
    assert!(w.span.is_some());
}

#[test]
fn an_error_in_a_substituted_value_points_at_the_use_and_the_declaration() {
    let text = r#"{
  "version": 2,
  "params": {
    "reach": "far"
  },
  "size": {"width": 8, "height": 4},
  "layers": ["terrain"],
  "steps": [
    {"id": "a", "operator": "Fill",
     "conditions": [{"type": "not", "condition":
       {"type": "near_type", "layer": "terrain", "tile": "x", "distance": {"$param": "reach"}}}]}
  ]
}"#;
    let e = PipelineSpec::parse(text).unwrap_err();
    assert_eq!(e.len(), 1, "{e:?}");
    // Inside a tagged enum serde loses the path, so the error lands on the
    // condition; the note only appears when the error is on the value.
    assert_eq!(e[0].pointer, "/steps/0/conditions/0");
    assert_eq!(position(&e[0]), position_of(text, "{\"type\": \"not\""));
}

/// The island of tests/map_snapshots.rs, with a param, as text so positions
/// can be checked.
const ISLAND: &str = r#"{
  "version": 2,
  "seed": 512,
  "size": {"width": 12, "height": 6},
  "layers": ["terrain", "objects"],
  "tiles": ["floor", "wall", "water", "tree"],
  "params": {
    "wall_chance": {"type": "number", "default": 0.1, "min": 0, "max": 1}
  },
  "steps": [
    {"id": "base", "operator": "Fill", "config": {"layer": "terrain", "tile": "floor"}},
    {"id": "walls", "operator": "Scatter",
     "config": {"layer": "terrain", "tile": "wall", "chance": {"$param": "wall_chance"}}},
    {"id": "trees", "operator": "Scatter",
     "config": {"layer": "objects", "tile": "tree", "chance": 0.15},
     "conditions": [{"type": "tile_is", "layer": "terrain", "tile": "floor"}]}
  ]
}"#;

fn build_errors(text: &str) -> Vec<Diagnostic> {
    let spec = parse(text).spec;
    spec.build(&Registry::with_builtins())
        .err()
        .expect("expected build errors")
}

#[test]
fn builds_and_runs() {
    let spec = parse(ISLAND).spec;
    let mut built = spec.build(&Registry::with_builtins()).unwrap();
    built
        .pipeline
        .run(&mut built.grid, &spec.salt, spec.seed.unwrap())
        .unwrap();
    let terrain = built.grid.layer_id("terrain").unwrap();
    let wall = built.tiles.id("wall").unwrap();
    let cells = built.grid.tiles(terrain).unwrap().cells();
    let walls = cells.filter(|t| **t == wall).count();
    assert!(walls > 0 && walls < 12 * 6 / 2, "{walls}");
    assert_eq!(
        built.tiles.id("tree").unwrap(),
        TileId(4),
        "declaration order"
    );
}

#[test]
fn an_unknown_operator_points_at_the_operator() {
    let text = ISLAND.replace("\"Scatter\"", "\"Scater\"");
    let e = build_errors(&text);
    assert_eq!(e.len(), 2);
    assert_eq!(e[0].pointer, "/steps/1/operator");
    assert_eq!(e[0].step_id.as_deref(), Some("walls"));
    let (line, column) = position_of(&text, "\"operator\": \"Scater\"");
    assert_eq!(
        e[0].to_string(),
        format!("{line}:{column}: step 'walls': unknown operator 'Scater'")
    );
}

#[test]
fn a_missing_layer_or_tile_points_where_it_is_written() {
    let text = ISLAND
        .replace(r#""tile": "wall""#, r#""tile": "wal""#)
        .replace(
            r#"{"type": "tile_is", "layer": "terrain""#,
            r#"{"type": "tile_is", "layer": "terain""#,
        );
    let e = build_errors(&text);
    let got: Vec<_> = e
        .iter()
        .map(|d| (d.pointer.as_str(), d.message.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            ("/steps/1/config/tile", "tile 'wal' is unknown"),
            ("/steps/2/conditions/0/layer", "layer 'terain' not found"),
        ]
    );
    assert_eq!(position(&e[1]), position_of(&text, "\"layer\": \"terain\""));
}

#[test]
fn a_bad_param_value_names_the_param_declaration() {
    let text = ISLAND.replace(
        r#"{"type": "number", "default": 0.1, "min": 0, "max": 1}"#,
        r#""lots""#,
    );
    let e = build_errors(&text);
    assert_eq!(e.len(), 1, "{e:?}");
    assert_eq!(e[0].pointer, "/steps/1/config/chance");
    assert!(
        e[0].message.contains("invalid type: string \"lots\""),
        "{}",
        e[0].message
    );
    let note = e[0].note.as_ref().unwrap();
    assert_eq!(note.message, "the value comes from param 'wall_chance'");
    assert_eq!(
        note.span.map(|s| (s.line, s.column)),
        Some(position_of(&text, "\"wall_chance\":"))
    );
}

#[test]
fn an_invalid_field_points_at_it() {
    let text = ISLAND.replace("\"chance\": 0.15", "\"chance\": 1.5");
    let e = build_errors(&text);
    assert_eq!(e[0].pointer, "/steps/2/config/chance");
    assert_eq!(position(&e[0]), position_of(&text, "\"chance\": 1.5"));
}

#[test]
fn diagnostics_point_at_the_file_even_after_the_steps_change() {
    let text = ISLAND.replace("\"chance\": 0.15", "\"chance\": 1.5");
    let mut spec = parse(&text).spec;
    spec.steps.remove(0);
    let e = spec.build(&Registry::with_builtins()).err().unwrap();
    assert_eq!(e[0].pointer, "/steps/2/config/chance");
}

#[test]
fn display_reads_like_a_compiler_message() {
    let d = Diagnostic {
        message: "boom".to_string(),
        step_id: Some("walls".to_string()),
        pointer: "/steps/1".to_string(),
        span: Some(Span {
            start: 0,
            end: 1,
            line: 12,
            column: 9,
        }),
        note: Some(Box::new(Note {
            message: "from here".to_string(),
            pointer: "/params/x".to_string(),
            span: Some(Span {
                start: 0,
                end: 1,
                line: 4,
                column: 5,
            }),
        })),
        expr_span: None,
    };
    assert_eq!(d.to_string(), "12:9: step 'walls': boom; from here at 4:5");
    let bare = Diagnostic::new("", "the file is empty");
    assert_eq!(bare.to_string(), "the file is empty");
}

fn overrides(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(n, v)| (n.to_string(), v.to_string()))
        .collect()
}

#[test]
fn an_override_replaces_the_default_where_the_param_is_used() {
    let parsed = PipelineSpec::parse_with(ISLAND, &overrides(&[("wall_chance", "0.3")])).unwrap();
    assert_eq!(parsed.spec.steps[1].config["chance"], json!(0.3));
    assert_eq!(
        parsed.spec.params["wall_chance"].default,
        ParamValue::Number(0.1),
        "the declaration stays as written"
    );
    let last = overrides(&[("wall_chance", "0.3"), ("wall_chance", "0.2")]);
    let parsed = PipelineSpec::parse_with(ISLAND, &last).unwrap();
    assert_eq!(
        parsed.spec.steps[1].config["chance"],
        json!(0.2),
        "the last one wins"
    );
}

#[test]
fn a_bad_override_is_an_error_at_the_declaration() {
    let e = PipelineSpec::parse_with(ISLAND, &overrides(&[("wall_chance", "1.5")])).unwrap_err();
    assert_eq!(e.len(), 1, "{e:?}");
    assert_eq!(e[0].pointer, "/params/wall_chance");
    assert_eq!(
        e[0].message,
        "override wall_chance=1.5: 1.5 is outside min..=max"
    );
    assert_eq!(position(&e[0]), position_of(ISLAND, "\"wall_chance\": {"));
}

#[test]
fn an_override_of_an_unknown_param_lists_the_declared_ones() {
    let e = PipelineSpec::parse_with(ISLAND, &overrides(&[("wal_chance", "0.3")])).unwrap_err();
    assert_eq!(
        e[0].message,
        "override wal_chance=0.3: no param 'wal_chance' (declared: wall_chance)"
    );
    let e = PipelineSpec::parse_with(&base().to_string(), &overrides(&[("x", "1")])).unwrap_err();
    assert_eq!(
        e[0].message,
        "override x=1: no param 'x' (the file declares none)"
    );
}

/// `base()` with `params`, and the first step's config `extra` set to `extra`.
fn with_params(params: Value, extra: Value) -> String {
    with(|v| {
        v["params"] = params;
        v["steps"][0]["config"]["extra"] = extra;
    })
}

fn extra_of(text: &str, overrides: &[(&str, &str)]) -> Result<Value, Vec<Diagnostic>> {
    let o: Vec<(String, String)> = overrides
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
    PipelineSpec::parse_with(text, &o).map(|p| p.spec.steps[0].config["extra"].clone())
}

#[test]
fn computed_params_follow_their_inputs() {
    let text = with_params(
        json!({
            "stage": {"type": "integer", "default": 1, "min": 1, "max": 9},
            "rooms": {"type": "integer", "expr": "min(20, 5 + floor(stage * 10 / 3))"}
        }),
        json!({"$param": "rooms"}),
    );
    assert_eq!(extra_of(&text, &[]).unwrap(), json!(8));
    assert_eq!(extra_of(&text, &[("stage", "4")]).unwrap(), json!(18));
    assert_eq!(extra_of(&text, &[("stage", "9")]).unwrap(), json!(20));
    // What the output records: every param with the value used.
    let p = PipelineSpec::parse_with(&text, &[("stage".into(), "2".into())]).unwrap();
    assert_eq!(
        p.spec.param_values(),
        [
            ("stage".to_string(), ParamValue::Integer(2)),
            ("rooms".to_string(), ParamValue::Integer(11)),
        ]
    );
    assert_eq!(p.spec.params["rooms"].default, ParamValue::Integer(11));
    // `stage` is used through `rooms`: no "never used" warning.
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
}

#[test]
fn computed_params_read_each_other_in_any_order() {
    let text = with_params(
        json!({
            "double": {"type": "integer", "expr": "base * 2"},
            "base": {"type": "integer", "expr": "size + 1"},
            "size": 3,
            "ratio": {"type": "number", "expr": "double"}
        }),
        json!({"$param": "ratio"}),
    );
    // A number param takes an integer expression.
    assert_eq!(extra_of(&text, &[]).unwrap(), json!(8.0));
}

#[test]
fn a_circle_of_computed_params_is_named() {
    let text = with_params(
        json!({
            "a": {"type": "integer", "expr": "b + 1"},
            "b": {"type": "integer", "expr": "a + 1"},
            "c": {"type": "integer", "expr": "a"}
        }),
        json!({"$param": "c"}),
    );
    let e = extra_of(&text, &[]).unwrap_err();
    assert_eq!(e.len(), 1, "{e:?}");
    assert_eq!(e[0].pointer, "/params/a/expr");
    assert_eq!(
        e[0].message,
        "params compute from each other in a circle: a → b → a"
    );
}

#[test]
fn a_computed_param_is_checked_like_a_default() {
    let one = |params: Value| {
        let e = extra_of(&with_params(params, json!(0)), &[]).unwrap_err();
        assert_eq!(e.len(), 1, "{e:?}");
        (e[0].pointer.clone(), e[0].message.clone(), e[0].expr_span)
    };
    let (at, m, _) = one(json!({"n": {"type": "integer", "expr": "7 / 2"}}));
    assert_eq!(at, "/params/n/expr");
    assert!(m.contains("wrap it in floor(), round() or ceil()"), "{m}");
    let (_, m, _) = one(json!({"n": {"type": "integer", "expr": "20", "max": 10}}));
    assert_eq!(m, "param 'n' computed 20: 20 is outside min..=max");
    let (_, m, span) = one(json!({"stage": 1, "n": {"type": "integer", "expr": "stagee + 1"}}));
    assert_eq!(
        m,
        "param 'n': no param named 'stagee' (did you mean stage?)"
    );
    assert_eq!(span, Some((0, 6)), "where in the expression");
    let (_, m, span) = one(json!({"n": {"type": "integer", "expr": "2 ^ 3"}}));
    assert!(m.contains("pow(base, exponent)"), "{m}");
    assert_eq!(span, Some((2, 3)));
    let (at, m, _) = one(json!({"n": {"type": "integer", "expr": "1", "default": 1}}));
    assert_eq!(at, "/params/n/default");
    assert!(m.contains("has no default"), "{m}");
    let (at, m, _) = one(json!({"n": {"type": "integer"}}));
    assert_eq!(at, "/params/n/default");
    assert!(m.contains("needs a `default` (or an `expr`)"), "{m}");
    let (at, m, _) = one(json!({"max": 3}));
    assert_eq!(at, "/params/max");
    assert!(m.contains("a word of the expression language"), "{m}");
}

#[test]
fn a_game_sets_the_inputs_not_what_they_compute() {
    let text = with_params(
        json!({
            "stage": 1,
            "rooms": {"type": "integer", "expr": "stage + 4"}
        }),
        json!({"$param": "rooms"}),
    );
    let e = extra_of(&text, &[("rooms", "12")]).unwrap_err();
    assert_eq!(e.len(), 1, "{e:?}");
    assert_eq!(
        e[0].message,
        "override rooms=12: 'rooms' is computed from its expression; set stage instead"
    );
}

#[test]
fn an_expression_can_stand_for_any_value_in_a_step() {
    let text = with_params(
        json!({"stage": 2, "rooms": {"type": "integer", "expr": "stage + 6"}}),
        json!({"min": {"$expr": "rooms"}, "max": {"$expr": "rooms + 1"},
               "half": {"$expr": "rooms / 16"}, "many": {"$expr": "rooms > 7"}}),
    );
    assert_eq!(
        extra_of(&text, &[]).unwrap(),
        json!({"min": 8, "max": 9, "half": 0.5, "many": true})
    );
    // A mistake points into the expression, in the step where it is.
    let text = with_params(json!({"stage": 2}), json!({"$expr": "stage * (1 +"}));
    let e = PipelineSpec::parse(&text).unwrap_err();
    assert_eq!(e.len(), 1, "{e:?}");
    assert_eq!(e[0].pointer, "/steps/0/config/extra/$expr");
    assert_eq!(e[0].message, "the expression ends too soon");
    assert_eq!(e[0].expr_span, Some((12, 12)));
    assert!(e[0].span.is_some(), "located in the file");
    // Only exactly one key, only inside steps.
    let text = with_params(json!({}), json!({"$expr": "1", "x": 2}));
    assert!(errors(&text)[0].1.contains(r#"exactly {"$expr": "text"}"#));
    let text = with(|v| v["description"] = json!({"$expr": "1"}));
    assert!(
        errors(&text)[0]
            .1
            .contains("can only be used inside a step")
    );
}

#[test]
fn an_error_later_says_which_expression_gave_the_value() {
    // Scatter's `chance` must be between 0 and 1; the expression gives 2.
    let text = with(|v| {
        v["params"] = json!({"k": 4});
        v["steps"][0] = json!({"id": "noise", "operator": "Scatter",
            "config": {"layer": "terrain", "tile": "wall", "chance": {"$expr": "k / 2"}}});
    });
    let spec = parse(&text).spec;
    let e = spec.build(&Registry::with_builtins()).err().unwrap();
    let note = e[0].note.as_ref().expect("a note");
    assert_eq!(
        note.message,
        "the value 2.0 comes from the expression `k / 2`"
    );
}
