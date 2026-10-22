use serde_json::json;

use super::*;

fn declare(params: Value) -> Result<IndexMap<String, Param>, Vec<Diagnostic>> {
    let mut errors = Vec::new();
    let out = declarations(Some(&params), &mut errors);
    if errors.is_empty() {
        Ok(out.params)
    } else {
        Err(errors)
    }
}

/// The single error for one param `p`: (pointer, message).
fn declare_error(p: Value) -> (String, String) {
    let mut errors = declare(json!({ "p": p })).unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    let d = errors.remove(0);
    (d.pointer, d.message)
}

#[test]
fn the_short_form_infers_the_type() {
    let p = declare(json!({"a": 0.5, "b": 3, "c": -3, "d": true, "e": "forest"})).unwrap();
    let kinds: Vec<_> = p.values().map(|p| p.default.kind()).collect();
    assert_eq!(
        kinds,
        [
            ParamType::Number,
            ParamType::Integer,
            ParamType::Integer,
            ParamType::Boolean,
            ParamType::String
        ]
    );
    assert_eq!(p["a"].default, ParamValue::Number(0.5));
    assert_eq!(p["c"].default, ParamValue::Integer(-3));
}

#[test]
fn params_keep_the_order_they_were_written_in() {
    let p = declare(json!({"zeta": 1, "alpha": 2, "mid": 3})).unwrap();
    assert_eq!(p.keys().collect::<Vec<_>>(), ["zeta", "alpha", "mid"]);
}

#[test]
fn the_full_form_keeps_everything() {
    let p = declare(json!({"density": {
        "type": "number", "default": 0.5, "min": 0, "max": 1, "step": 0.05,
        "description": "how many trees"
    }}))
    .unwrap();
    assert_eq!(
        p["density"],
        Param {
            default: ParamValue::Number(0.5),
            min: Some(0.0),
            max: Some(1.0),
            step: Some(0.05),
            choices: None,
            description: Some("how many trees".to_string()),
        }
    );
    let p = declare(
        json!({"biome": {"type": "string", "default": "forest", "enum": ["forest", "desert"]}}),
    )
    .unwrap();
    assert_eq!(p["biome"].choices.as_deref().map(<[_]>::len), Some(2));
}

#[test]
fn an_integer_param_with_a_float_default_is_an_error() {
    let (at, message) = declare_error(json!({"type": "integer", "default": 2.5}));
    assert_eq!(at, "/params/p/default");
    assert_eq!(message, "param 'p': 2.5 is not an integer");
}

#[test]
fn bad_declarations_name_the_field() {
    for (decl, field, message) in [
        (
            json!({"type": "number", "default": "x"}),
            "default",
            "\"x\" is not a number",
        ),
        (
            json!({"type": "boolean", "default": 1}),
            "default",
            "1 is not a boolean",
        ),
        (
            json!({"type": "number", "default": 1, "min": 2, "max": 0}),
            "min",
            "min 2 is greater than max 0",
        ),
        (
            json!({"type": "number", "default": 5, "max": 1}),
            "default",
            "5 is outside min..=max",
        ),
        (
            json!({"type": "integer", "default": 0, "min": -1, "max": 1}),
            "",
            "",
        ),
        (
            json!({"type": "integer", "default": 1, "step": 0.5}),
            "step",
            "0.5 is not an integer",
        ),
        (
            json!({"type": "number", "default": 1, "step": 0}),
            "step",
            "step 0 must be greater than 0",
        ),
        (
            json!({"type": "boolean", "default": true, "min": 0}),
            "min",
            "boolean params have no min",
        ),
        (
            json!({"type": "number", "default": 1, "enum": ["a"]}),
            "enum",
            "number params have no enum",
        ),
        (
            json!({"type": "string", "default": "c", "enum": ["a", "b"]}),
            "default",
            "\"c\" is not one of the enum",
        ),
    ] {
        if field.is_empty() {
            assert!(declare(json!({ "p": decl })).is_ok(), "{decl}");
            continue;
        }
        let (at, got) = declare_error(decl.clone());
        assert_eq!(at, format!("/params/p/{field}"), "{decl}");
        assert_eq!(got, format!("param 'p': {message}"), "{decl}");
    }
}

#[test]
fn a_typo_in_the_full_form_is_reported_as_one() {
    let (at, message) = declare_error(json!({"type": "number", "defualt": 1}));
    assert_eq!(at, "/params/p/defualt");
    assert!(message.starts_with("unknown field `defualt`"), "{message}");
    let (at, message) = declare_error(json!({"type": "float", "default": 1}));
    assert_eq!(at, "/params/p/type");
    assert!(message.contains("unknown variant `float`"), "{message}");
}

#[test]
fn only_scalars_can_be_params() {
    for bad in [json!([1, 2]), json!(null)] {
        let (at, message) = declare_error(bad);
        assert_eq!(at, "/params/p");
        assert!(
            message.contains("expected a number, a boolean, a string"),
            "{message}"
        );
    }
    let (_, message) = declare_error(json!(u64::MAX));
    assert!(
        message.contains("too large for an integer param"),
        "{message}"
    );
}

#[test]
fn every_bad_param_is_reported() {
    let errors = declare(json!({"a": [], "b": 1, "c": {"type": "x"}})).unwrap_err();
    let pointers: Vec<_> = errors.iter().map(|d| d.pointer.as_str()).collect();
    assert_eq!(pointers, ["/params/a", "/params/c/type"]);
}

fn substituted(mut doc: Value, params: Value) -> (Value, Vec<Site>, Vec<(String, String)>) {
    let mut errors = Vec::new();
    let params = declarations(Some(&params), &mut errors);
    let sites = substitute(&mut doc, &params, &mut errors);
    let errors = errors.into_iter().map(|d| (d.pointer, d.message)).collect();
    (doc, sites, errors)
}

#[test]
fn references_in_config_and_conditions_are_replaced() {
    let doc = json!({"steps": [{
        "id": "a",
        "config": {"chance": {"$param": "density"}, "list": [1, {"$param": "tile"}]},
        "conditions": [{"type": "near_type", "distance": {"$param": "r"}}]
    }]});
    let (doc, sites, errors) = substituted(doc, json!({"density": 0.25, "tile": "wall", "r": 2}));
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(
        doc["steps"][0]["config"],
        json!({"chance": 0.25, "list": [1, "wall"]})
    );
    assert_eq!(doc["steps"][0]["conditions"][0]["distance"], json!(2));
    let sites: Vec<_> = sites
        .iter()
        .map(|s| (s.pointer.as_str(), s.param.as_str()))
        .collect();
    assert_eq!(
        sites,
        [
            ("/steps/0/config/chance", "density"),
            ("/steps/0/config/list/1", "tile"),
            ("/steps/0/conditions/0/distance", "r"),
        ]
    );
}

#[test]
fn an_undeclared_param_is_an_error_at_its_use() {
    let doc = json!({"steps": [{"config": {"chance": {"$param": "densty"}}}]});
    let (_, sites, errors) = substituted(doc, json!({"density": 0.25}));
    assert!(sites.is_empty());
    assert_eq!(
        errors,
        [(
            "/steps/0/config/chance".to_string(),
            "param 'densty' is not declared in `params`".to_string()
        )]
    );
}

#[test]
fn a_reference_is_exactly_one_key() {
    let doc = json!({"steps": [{"config": {
        "a": {"$param": "x", "min": 0},
        "b": {"$param": 3}
    }}]});
    let (_, _, errors) = substituted(doc, json!({"x": 1}));
    let pointers: Vec<_> = errors.iter().map(|e| e.0.as_str()).collect();
    assert_eq!(pointers, ["/steps/0/config/a", "/steps/0/config/b"]);
    assert!(errors[0].1.starts_with("a param reference is exactly"));
}

#[test]
fn references_outside_steps_are_errors() {
    let doc = json!({
        "seed": {"$param": "s"},
        "steps": [{"id": {"$param": "s"}, "config": {}}]
    });
    let (doc, sites, errors) = substituted(doc, json!({"s": 1}));
    assert!(sites.is_empty());
    let pointers: Vec<_> = errors.iter().map(|e| e.0.as_str()).collect();
    assert_eq!(pointers, ["/seed/$param", "/steps/0/id/$param"]);
    assert!(
        errors[0]
            .1
            .contains("only be used inside a step's `config` or `conditions`")
    );
    assert_eq!(doc["seed"], json!({"$param": "s"}), "left as it was");
}

#[test]
fn dollar_keys_are_reserved_except_schema_at_the_root() {
    let doc = json!({
        "$schema": "https://example.com/pipeline.json",
        "$ref": "x",
        "size": {"$schema": "y"},
        "steps": [{"config": {"$ref": "z"}}]
    });
    let (_, _, errors) = substituted(doc, json!({}));
    let pointers: Vec<_> = errors.iter().map(|e| e.0.as_str()).collect();
    assert_eq!(pointers, ["/$ref", "/size/$schema", "/steps/0/config/$ref"]);
    assert_eq!(errors[0].1, "`$ref`: keys starting with `$` are reserved");
}

#[test]
fn a_broken_declaration_is_reported_once_not_again_at_each_use() {
    let doc = json!({"steps": [{"config": {"a": {"$param": "x"}, "b": {"$param": "x"}}}]});
    let (doc, sites, errors) = substituted(doc, json!({"x": {"type": "number", "default": "big"}}));
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].0, "/params/x/default");
    assert!(sites.is_empty());
    assert_eq!(doc["steps"][0]["config"]["a"], json!({"$param": "x"}));
}

#[test]
fn values_from_text_are_read_by_the_declared_type() {
    let p = declare(json!({
        "n": {"type": "number", "default": 0.5, "min": 0, "max": 1},
        "i": {"type": "integer", "default": 3},
        "b": true,
        "s": {"type": "string", "default": "forest", "enum": ["forest", "desert"]}
    }))
    .unwrap();
    assert_eq!(p["n"].parse_value("0.25"), Ok(ParamValue::Number(0.25)));
    assert_eq!(p["n"].parse_value("1"), Ok(ParamValue::Number(1.0)));
    assert_eq!(p["i"].parse_value("-7"), Ok(ParamValue::Integer(-7)));
    assert_eq!(p["b"].parse_value("false"), Ok(ParamValue::Boolean(false)));
    assert_eq!(
        p["s"].parse_value("desert"),
        Ok(ParamValue::String("desert".to_string()))
    );

    let err = |name: &str, text: &str| p[name].parse_value(text).unwrap_err();
    assert_eq!(err("n", "1.5"), "1.5 is outside min..=max");
    assert_eq!(err("n", "lots"), "\"lots\" is not a number");
    assert_eq!(err("n", "NaN"), "\"NaN\" is not a number");
    assert_eq!(err("n", "inf"), "\"inf\" is not a number");
    assert_eq!(err("i", "3.5"), "\"3.5\" is not an integer");
    assert_eq!(err("b", "yes"), "\"yes\" is not a boolean");
    assert_eq!(err("s", "tundra"), "\"tundra\" is not one of the enum");
}

#[test]
fn check_is_the_rule_the_default_follows() {
    let p = &declare(json!({"n": {"type": "number", "default": 0.5, "max": 1}})).unwrap()["n"];
    assert_eq!(p.check(&ParamValue::Number(1.0)), Ok(()));
    assert_eq!(
        p.check(&ParamValue::Number(2.0)),
        Err("2 is outside min..=max".to_string())
    );
    assert_eq!(
        p.check(&ParamValue::Integer(1)),
        Err("1 is not a number".to_string())
    );
}
