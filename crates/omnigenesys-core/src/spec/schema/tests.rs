use serde_json::{Value, json};

use super::*;

const ISLAND: &str = include_str!("../../../tests/pipelines/island.json");

fn validator() -> jsonschema::Validator {
    jsonschema::draft7::new(&schema(&Registry::with_builtins()))
        .expect("the schema is valid draft-07")
}

fn island() -> Value {
    serde_json::from_str(ISLAND).unwrap()
}

/// Whether the island, changed by `change`, passes the schema.
fn accepts(change: impl FnOnce(&mut Value)) -> bool {
    let mut v = island();
    change(&mut v);
    validator().is_valid(&v)
}

#[test]
fn accepts_the_island_and_the_parser_agrees() {
    let errors: Vec<String> = validator()
        .iter_errors(&island())
        .map(|e| format!("{} at {}", e, e.instance_path()))
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
    assert!(crate::spec::PipelineSpec::parse(ISLAND).is_ok());
}

#[test]
fn each_config_is_checked_against_its_operator() {
    // Fill has no `chance`; Scatter needs one.
    assert!(!accepts(|v| v["steps"][0]["config"]["chance"] = json!(0.5)));
    assert!(!accepts(|v| {
        v["steps"][3]["config"]
            .as_object_mut()
            .unwrap()
            .remove("chance");
    }));
    assert!(!accepts(
        |v| v["steps"][0]["config"]["tiel"] = json!("floor")
    ));
    assert!(!accepts(
        |v| v["steps"][3]["config"]["chance"] = json!("lots")
    ));
}

#[test]
fn params_are_accepted_where_the_parser_takes_them() {
    assert!(accepts(
        |v| v["steps"][3]["config"]["chance"] = json!({"$param": "wall_chance"})
    ));
    assert!(accepts(
        |v| v["steps"][3]["conditions"][0]["tile"] = json!({"$param": "x"})
    ));
    assert!(!accepts(
        |v| v["steps"][3]["config"]["chance"] = json!({"$param": "x", "y": 1})
    ));
    assert!(!accepts(|v| v["steps"][3]["id"] = json!({"$param": "x"})));
    assert!(!accepts(|v| v["params"]["$x"] = json!(1)));
}

#[test]
fn expressions_are_accepted_where_the_parser_takes_them() {
    assert!(accepts(
        |v| v["steps"][3]["config"]["chance"] = json!({"$expr": "wall_chance / 2"})
    ));
    assert!(!accepts(
        |v| v["steps"][3]["config"]["chance"] = json!({"$expr": "1", "$param": "x"})
    ));
    assert!(accepts(|v| {
        v["params"]["rooms"] = json!({"type": "integer", "expr": "min(20, 8)"});
    }));
    // A value of its own or an expression, not both and not neither.
    assert!(!accepts(|v| {
        v["params"]["rooms"] = json!({"type": "integer", "expr": "8", "default": 8});
    }));
    assert!(!accepts(
        |v| v["params"]["rooms"] = json!({"type": "integer"})
    ));
}

#[test]
fn the_root_rules_are_in_the_schema() {
    assert!(!accepts(|v| v["version"] = json!(3)));
    assert!(!accepts(|v| v["sise"] = json!(1)));
    assert!(!accepts(|v| v["seed"] = json!(MAX_NUMBER_SEED + 1)));
    assert!(accepts(|v| v["seed"] = json!("18446744073709551615")));
    assert!(!accepts(|v| v["tiles"] = json!(["floor", "empty"])));
    assert!(!accepts(|v| v["size"]["width"] = json!(0)));
    assert!(accepts(
        |v| v["layers"][1] = json!({"name": "objects", "kind": "tiles"})
    ));
    assert!(accepts(|v| v["$schema"] = json!("./pipeline.schema.json")));
}

#[test]
fn steps_are_checked() {
    assert!(!accepts(|v| v["steps"][0]["id"] = json!("Base")));
    assert!(!accepts(|v| v["steps"][0]["operator"] = json!("Scater")));
    assert!(!accepts(|v| v["steps"][0]["enabeld"] = json!(true)));
    assert!(!accepts(
        |v| v["steps"][2]["conditions"][0]["distance"] = json!(-1)
    ));
    assert!(!accepts(
        |v| v["steps"][2]["conditions"][0]["type"] = json!("near")
    ));
    assert!(accepts(|v| v["steps"][0]["enabled"] = json!(false)));
}

#[test]
fn the_dispatch_names_every_operator() {
    let s = schema(&Registry::with_builtins());
    let step = &s["definitions"]["Step"];
    assert_eq!(
        step["properties"]["operator"]["enum"],
        json!([
            "CarvePaths",
            "CellularAutomaton",
            "Check",
            "Combine",
            "ConnectObjects",
            "Curve",
            "Fill",
            "GrowObjects",
            "LinearGradient",
            "MeasureObjects",
            "Noise",
            "PlaceObjects",
            "RadialGradient",
            "RegionSize",
            "Scatter",
            "TagObjects"
        ])
    );
    assert_eq!(step["allOf"].as_array().unwrap().len(), 16);
}

const CAVE: &str = include_str!("../../../tests/pipelines/cave.json");

#[test]
fn accepts_the_cave_and_checks_the_automaton_config() {
    let cave: Value = serde_json::from_str(CAVE).unwrap();
    let errors: Vec<String> = validator()
        .iter_errors(&cave)
        .map(|e| format!("{} at {}", e, e.instance_path()))
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
    assert!(crate::spec::PipelineSpec::parse(CAVE).is_ok());

    // The editor catches what the build would: a count past 8, no
    // `outside`, zero passes.
    let rejects = |change: &dyn Fn(&mut Value)| {
        let mut v = cave.clone();
        change(&mut v);
        !validator().is_valid(&v)
    };
    assert!(rejects(
        &|v| v["steps"][2]["config"]["birth"] = json!([5, 9])
    ));
    assert!(rejects(&|v| {
        v["steps"][2]["config"]
            .as_object_mut()
            .unwrap()
            .remove("outside");
    }));
    assert!(rejects(
        &|v| v["steps"][2]["config"]["outside"] = json!("wall")
    ));
    assert!(rejects(
        &|v| v["steps"][2]["config"]["iterations"] = json!(0)
    ));
    assert!(rejects(&|v| v["steps"][4]["config"]["thickness"] = json!(0)));
    assert!(rejects(
        &|v| v["steps"][3]["config"]["connectivity"] = json!("eight")
    ));
    assert!(rejects(&|v| {
        v["steps"][3]["config"]
            .as_object_mut()
            .unwrap()
            .remove("connectivity");
    }));
    assert!(!rejects(
        &|v| v["steps"][3]["config"]["region"] = json!(["floor", "wall"])
    ));
}
