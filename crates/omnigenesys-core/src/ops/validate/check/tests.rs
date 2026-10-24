use serde_json::{Value, json};

use crate::{
    operator::Registry,
    spec::{Built, PipelineSpec},
};

/// A 10 × 4 map, all sand: no cell is more than 1 from the edge, so
/// `edge_distance ≤ 2` takes every one (40 cells).
fn run(check: Value) -> Result<(), String> {
    let p = json!({
        "version": 2, "seed": 1, "size": {"width": 10, "height": 4},
        "layers": ["terreno"], "tiles": ["grama", "areia"],
        "steps": [
            {"id": "base", "operator": "Fill", "config": {"layer": "terreno", "tile": "grama"}},
            {"id": "praia", "operator": "Fill", "config": {"layer": "terreno", "tile": "areia"},
             "conditions": [{"type": "edge_distance", "max": 2}, {"type": "tile_is", "layer": "terreno", "tile": "grama"}]},
            check,
        ],
    });
    let Built {
        mut grid, pipeline, ..
    } = PipelineSpec::parse(&p.to_string())
        .unwrap()
        .spec
        .build(&Registry::with_builtins())
        .map_err(|e| e[0].message.clone())?;
    pipeline
        .run(&mut grid, "", 1)
        .map_err(|e| format!("{}: {}", e.step_id, e.error))
}

fn check(config: Value) -> Value {
    json!({"id": "areia", "operator": "Check", "config": config,
           "conditions": [{"type": "tile_is", "layer": "terreno", "tile": "areia"}]})
}

#[test]
fn passes_inside_the_range_with_both_ends_counted() {
    // `edge_distance ≤ 2` on a 10 × 4 map is every cell: all sand.
    assert_eq!(run(check(json!({"min": 40, "max": 40}))), Ok(()));
    assert_eq!(run(check(json!({"min": 1}))), Ok(()));
    assert_eq!(run(check(json!({"max": 40}))), Ok(()));
}

#[test]
fn fails_outside_it_and_says_why() {
    assert_eq!(
        run(check(json!({"min": 41}))),
        Err("areia: 40 cells match, fewer than min 41".to_string())
    );
    assert_eq!(
        run(check(json!({"max": 39}))),
        Err("areia: 40 cells match, more than max 39".to_string())
    );
    assert_eq!(
        run(check(json!({"max": 10, "message": "praia demais"}))),
        Err("areia: praia demais (40 cells)".to_string())
    );
}

#[test]
fn bad_checks_fail_the_build() {
    let e = run(check(json!({}))).unwrap_err();
    assert!(e.contains("needs `min`, `max` or both"), "{e}");
    let e = run(check(json!({"min": 5, "max": 4}))).unwrap_err();
    assert!(e.contains("above max"), "{e}");
    let e = run(json!({"id": "tudo", "operator": "Check", "config": {"min": 1}})).unwrap_err();
    assert!(e.contains("which cells to count"), "{e}");
}
