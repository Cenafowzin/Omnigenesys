//! The published schema of output v2 (`schema/omap-2.schema.json`) holds
//! for what the engine writes: every recipe in `tests/pipelines`, in every
//! encoding. An adapter written against the schema reads these files.

use omnigenesys_core::export::{self, Encoding};
use omnigenesys_core::operator::Registry;
use omnigenesys_core::spec::{Built, PipelineSpec};
use serde_json::Value;

const SCHEMA: &str = include_str!("../schema/omap-2.schema.json");

fn validator() -> jsonschema::Validator {
    let schema: Value = serde_json::from_str(SCHEMA).unwrap();
    jsonschema::draft202012::new(&schema).unwrap()
}

fn output(text: &str, encoding: Encoding) -> Value {
    let spec = PipelineSpec::parse(text).unwrap().spec;
    let Built {
        grid,
        tiles,
        pipeline,
    } = spec.build(&Registry::with_builtins()).unwrap();
    let seed = spec.seed.unwrap_or(1);
    let run = pipeline
        .run_attempts(&grid, &spec.salt, seed, spec.attempts)
        .unwrap_or_else(|e| panic!("{e}"));
    let mut provenance = spec.provenance(seed, &spec.salt);
    provenance.attempt = run.attempt;
    let mut out = Vec::new();
    export::write_json(&mut out, &run.grid, &tiles, &provenance, encoding).unwrap();
    serde_json::from_slice(&out).unwrap()
}

#[test]
fn every_recipe_writes_a_valid_map_in_every_encoding() {
    let v = validator();
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/pipelines");
    let mut seen = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        for encoding in [Encoding::Auto, Encoding::Rle, Encoding::Dense] {
            let map = output(&text, encoding);
            let errors: Vec<String> = v
                .iter_errors(&map)
                .map(|e| format!("{} at {}", e, e.instance_path()))
                .take(5)
                .collect();
            assert!(errors.is_empty(), "{}: {errors:#?}", path.display());
        }
        seen += 1;
    }
    assert!(seen >= 8, "only {seen} recipes found");
}

#[test]
fn the_schema_refuses_what_a_reader_must_refuse() {
    let v = validator();
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/pipelines/cave.json"
    ))
    .unwrap();
    let good = output(&text, Encoding::Auto);
    assert!(v.is_valid(&good));
    let bad = |f: &dyn Fn(&mut Value)| {
        let mut m = good.clone();
        f(&mut m);
        assert!(!v.is_valid(&m), "{m:#}");
    };
    // Another major version.
    bad(&|m| m["version"] = 3.into());
    // A kind or an encoding it does not know.
    bad(&|m| m["layers"][0]["kind"] = "voxels".into());
    bad(&|m| m["layers"][0]["encoding"] = "zstd".into());
    // tiles[0] is always "empty".
    bad(&|m| m["tiles"][0] = "floor".into());
    // A prop whose value is not its type.
    bad(&|m| {
        m["objects"] = serde_json::json!([{"id": 1, "type": "t", "layer": "l",
            "bbox": [0, 0, 0, 1, 1, 1], "props": {"hp": {"type": "int", "value": 2.5}}}])
    });
    // A field from a later minor is read past.
    let mut newer = good.clone();
    newer["minor"] = 7.into();
    newer["chunks"] = serde_json::json!([]);
    assert!(v.is_valid(&newer));
}
