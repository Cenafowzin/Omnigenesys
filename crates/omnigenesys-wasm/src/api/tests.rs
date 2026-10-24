use omnigenesys_core::{
    export::{self, Encoding},
    operator::Registry,
    spec::{Built, PipelineSpec},
};
use serde_json::{Value, json};

use super::*;

const ISLAND: &str = include_str!("../../../omnigenesys-core/tests/pipelines/island.json");
const CAVE: &str = include_str!("../../../omnigenesys-core/tests/pipelines/cave.json");

fn registry() -> Registry {
    Registry::with_builtins()
}

fn parse(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

#[test]
fn the_schema_is_the_whole_file_schema() {
    let s = parse(&schema(&registry()));
    assert_eq!(s["$schema"], "http://json-schema.org/draft-07/schema#");
    assert_eq!(s, spec::schema(&registry()));
}

#[test]
fn operators_list_each_with_a_plain_config_schema() {
    let o = parse(&operators(&registry()));
    let names: Vec<&str> = o["operators"]
        .as_array()
        .unwrap()
        .iter()
        .map(|op| op["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "CellularAutomaton",
            "Fill",
            "FillBorder",
            "FillSmallRegions",
            "KeepLargestRegion",
            "Scatter"
        ]
    );
    let scatter = &o["operators"][5];
    assert_eq!(scatter["dimensions"], "any");
    assert_eq!(scatter["scope"], "local");
    // Plain: `chance` is a number, not "a number or a $param".
    let config = &scatter["config"];
    let target = config["$ref"]
        .as_str()
        .map(|r| &o["definitions"][r.trim_start_matches("#/definitions/")])
        .unwrap_or(config);
    assert_eq!(target["properties"]["chance"]["type"], "number");
    assert_eq!(o["operators"][4]["scope"], "global");
}

#[test]
fn a_good_pipeline_validates_with_its_steps_and_what_they_touch() {
    let v = parse(&validate(&registry(), ISLAND, &[]));
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["errors"], json!([]));
    let steps = v["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 4);
    // `lakes` fills water away from walls: reads terrain 2 cells around
    // (the condition), and writes it.
    assert_eq!(steps[2]["id"], "lakes");
    assert_eq!(
        steps[2]["layers"],
        json!([{"layer": "terrain", "writes": true,
                "reads": {"below": [2, 2, 0], "above": [2, 2, 0]}}])
    );
    // `trees` reads terrain at the cell and writes objects.
    let trees: Vec<(&str, bool, bool)> = steps[3]["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            (
                l["layer"].as_str().unwrap(),
                !l["reads"].is_null(),
                l["writes"].as_bool().unwrap(),
            )
        })
        .collect();
    assert!(trees.contains(&("terrain", true, false)), "{trees:?}");
    assert!(trees.contains(&("objects", false, true)), "{trees:?}");
}

#[test]
fn errors_point_at_the_step_and_in_utf16() {
    // An emoji before the error is 4 bytes, 2 UTF-16 units, 1 character.
    let text = ISLAND
        .replacen(
            "\"description\": \"Floor,",
            "\"description\": \"\u{1F3DD} Floor,",
            1,
        )
        .replacen("\"operator\": \"Scatter\"", "\"operator\": \"Scater\"", 1);
    let v = parse(&validate(&registry(), &text, &[]));
    assert_eq!(v["ok"], false);
    let e = &v["errors"][0];
    assert_eq!(e["step_id"], "walls");
    assert_eq!(e["pointer"], "/steps/1/operator");
    // The span starts at the key of the value it is about.
    let at = text.find("\"operator\": \"Scater\"").unwrap();
    let utf16 = text[..at].encode_utf16().count();
    assert_eq!(e["span"]["start"], utf16, "{e}");
    assert_eq!(utf16, at - 2, "the emoji is 4 bytes but 2 units");
    assert!(v.get("steps").is_none());
}

#[test]
fn generate_makes_the_same_map_and_file_as_the_core() {
    let map = generate(&registry(), ISLAND, &[], Some(512), None).unwrap();
    let Built {
        mut grid,
        tiles,
        pipeline,
    } = PipelineSpec::parse(ISLAND)
        .unwrap()
        .spec
        .build(&registry())
        .unwrap();
    pipeline.run(&mut grid, "", 512).unwrap();
    let terrain = grid.tiles(grid.layer_id("terrain").unwrap()).unwrap();
    let expected: Vec<u16> = terrain.as_slice().iter().map(|t| t.index()).collect();
    assert_eq!(map.layer("terrain").unwrap(), expected);
    assert_eq!(map.size(), (48, 16, 1));
    assert_eq!(map.seed(), 512);
    assert_eq!(map.layers(), ["terrain", "objects"]);
    assert_eq!(map.tiles(), ["empty", "floor", "wall", "water", "tree"]);
    assert!(map.layer("nope").is_none());

    let spec = PipelineSpec::parse(ISLAND).unwrap().spec;
    let mut file = Vec::new();
    export::write_json(
        &mut file,
        &grid,
        &tiles,
        &spec.provenance(512, ""),
        Encoding::Auto,
    )
    .unwrap();
    assert_eq!(map.to_json().unwrap(), String::from_utf8(file).unwrap());
}

#[test]
fn composite_puts_objects_over_terrain() {
    let map = generate(&registry(), ISLAND, &[], Some(512), None).unwrap();
    let (terrain, objects) = (map.layer("terrain").unwrap(), map.layer("objects").unwrap());
    let tree = map.tiles().iter().position(|t| t == "tree").unwrap() as u16;
    let composite = map.composite();
    let mut trees = 0;
    for i in 0..composite.len() {
        if objects[i] == tree {
            assert_eq!(composite[i], tree);
            trees += 1;
        } else {
            assert_eq!(composite[i], terrain[i]);
        }
    }
    assert!(trees > 0);
}

#[test]
fn params_seed_and_salt_change_the_map() {
    let base = generate(&registry(), ISLAND, &[], Some(512), None).unwrap();
    let p = params(r#"{"wall_chance": 0.4, "lake_margin": 1}"#).unwrap();
    assert_eq!(
        p,
        [
            ("wall_chance".to_string(), "0.4".to_string()),
            ("lake_margin".to_string(), "1".to_string())
        ]
    );
    let other = generate(&registry(), ISLAND, &p, Some(512), None).unwrap();
    assert_ne!(base.layer("terrain"), other.layer("terrain"));
    assert!(
        other
            .to_json()
            .unwrap()
            .contains(r#""params": {"wall_chance": 0.4, "lake_margin": 1}"#)
    );

    let salted = generate(&registry(), ISLAND, &[], Some(512), Some("x")).unwrap();
    assert_ne!(base.layer("terrain"), salted.layer("terrain"));
    assert!(salted.to_json().unwrap().contains(r#""salted": true"#));

    assert_eq!(params("").unwrap(), []);
    assert!(params("[1]").is_err());
    let bad = parse(&validate(
        &registry(),
        ISLAND,
        &params(r#"{"wall_chance": 2}"#).unwrap(),
    ));
    assert_eq!(bad["ok"], false);
}

#[test]
fn failures_come_back_as_errors_to_show() {
    // No seed in the file and none given.
    let unseeded = ISLAND.replacen("\"seed\": 512,", "", 1);
    let e = parse(
        &generate(&registry(), &unseeded, &[], None, None)
            .err()
            .unwrap(),
    );
    assert_eq!(e["errors"][0]["pointer"], "/seed");

    // A step that fails while running: the largest cave region is smaller
    // than asked.
    let strict = CAVE.replacen("\"min_cells\": 300", "\"min_cells\": 100000", 1);
    let e = parse(
        &generate(&registry(), &strict, &[], None, None)
            .err()
            .unwrap(),
    );
    assert_eq!(e["errors"][0]["step_id"], "one_cave");
    assert_eq!(e["errors"][0]["pointer"], "/steps/3");

    // A file that does not parse.
    let e = parse(
        &generate(&registry(), "{", &[], Some(1), None)
            .err()
            .unwrap(),
    );
    assert!(!e["errors"].as_array().unwrap().is_empty());
}
