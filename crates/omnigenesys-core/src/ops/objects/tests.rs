//! The object operators through pipeline files as a user writes them.

use serde_json::{Value as Json, json};

use crate::{
    grid::Grid,
    object::{Object, Value},
    operator::Registry,
    pipeline::RunError,
    spec::{Built, Diagnostic, PipelineSpec},
};

/// A 9 × 8 lattice of 15 × 9 slots on "salas" (Isaac's), "conexoes" free.
fn pipeline(steps: Json) -> Json {
    json!({
        "version": 2, "seed": 512, "size": {"width": 135, "height": 72},
        "layers": ["terreno",
            {"name": "salas", "kind": "objects", "lattice": {"columns": 9, "rows": 8, "cell": {"width": 15, "height": 9}}},
            {"name": "conexoes", "kind": "objects"},
            {"name": "solta", "kind": "objects"}],
        "tiles": ["chao", "parede", "porta"],
        "steps": steps,
    })
}

fn build(p: &Json) -> Result<Built, Vec<Diagnostic>> {
    PipelineSpec::parse(&p.to_string())?
        .spec
        .build(&Registry::with_builtins())
}

fn run_seed(p: &Json, seed: u64) -> Result<Grid, String> {
    let Built {
        mut grid, pipeline, ..
    } = build(p).unwrap_or_else(|e| panic!("{e:#?}"));
    pipeline
        .run(&mut grid, "", seed)
        .map_err(|e| e.to_string())?;
    Ok(grid)
}

fn run(p: &Json) -> Grid {
    run_seed(p, 512).unwrap()
}

fn objects(g: &Grid, layer: &str) -> Vec<Object> {
    g.objects(g.layer_id(layer).unwrap())
        .unwrap()
        .iter()
        .cloned()
        .collect()
}

/// Column and row of an object on the 15 × 9 lattice.
fn slot(o: &Object) -> (i32, i32) {
    (o.bbox.origin.x / 15, o.bbox.origin.y / 9)
}

fn role(o: &Object) -> Option<&str> {
    o.prop("role").and_then(Value::as_str)
}

fn start() -> Json {
    json!({"id": "inicio", "operator": "PlaceObjects", "config": {
        "layer": "salas", "kind": "sala", "where": [{"type": "slot", "x": 4, "y": 4}],
        "props": {"role": "inicio"}}})
}

fn isaac_growth(count: Json) -> Json {
    json!({"id": "crescer", "operator": "GrowObjects", "config": {
        "layer": "salas", "count": count, "chance": 0.5,
        "where": [{"type": "neighbours", "max": 1}]}})
}

fn neighbours(rooms: &[Object], a: &Object) -> usize {
    rooms
        .iter()
        .filter(|b| crate::filter::touching(a.bbox, b.bbox))
        .count()
}

#[test]
fn place_takes_the_slot_the_filter_names() {
    let g = run(&pipeline(json!([start()])));
    let rooms = objects(&g, "salas");
    assert_eq!(rooms.len(), 1);
    assert_eq!(slot(&rooms[0]), (4, 4));
    assert_eq!(rooms[0].kind, "sala");
    assert_eq!(role(&rooms[0]), Some("inicio"));
    assert_eq!(rooms[0].bbox.size.width, 15);
    assert_eq!(rooms[0].bbox.size.height, 9);
}

#[test]
fn place_looks_again_after_each_object() {
    // Three objects, each where no object is around yet: each pick sees
    // the ones placed before it in the same step.
    let g = run(&pipeline(
        json!([{"id": "a", "operator": "PlaceObjects", "config": {
        "layer": "salas", "kind": "x", "count": 30,
        "where": [{"type": "neighbours", "max": 0}]}}]),
    ));
    let rooms = objects(&g, "salas");
    assert_eq!(rooms.len(), 30);
    for r in &rooms {
        assert_eq!(neighbours(&rooms, r), 0, "{:?}", slot(r));
    }
    // Without an order, slots go row by row: a checkerboard from (0, 0).
    assert_eq!(slot(&rooms[0]), (0, 0));
    assert_eq!(slot(&rooms[1]), (2, 0));
}

#[test]
fn too_few_fails_and_a_range_or_all_does_not() {
    let place = |count: Json| {
        pipeline(json!([{"id": "a", "operator": "PlaceObjects", "config": {
            "layer": "salas", "kind": "x", "count": count,
            "where": [{"type": "slot", "y": 0}]}}]))
    };
    let e = run_seed(&place(json!(10)), 1).unwrap_err();
    assert!(e.contains("placed 9, fewer than the 10 asked for"), "{e}");
    assert_eq!(
        objects(&run(&place(json!({"min": 2, "max": 50}))), "salas").len(),
        9
    );
    assert_eq!(objects(&run(&place(json!("all"))), "salas").len(), 9);
}

#[test]
fn fallbacks_are_tried_in_order() {
    let g = run(&pipeline(
        json!([start(), {"id": "a", "operator": "PlaceObjects", "config": {
        "layer": "salas", "kind": "x",
        "where": [{"type": "neighbours", "min": 3}],
        "fallback": [[{"type": "slot", "x": 8, "y": 7}], [{"type": "slot", "x": 0, "y": 0}]]}}]),
    ));
    let rooms = objects(&g, "salas");
    assert_eq!(slot(&rooms[1]), (8, 7));
}

#[test]
fn order_by_sorts_by_keys_with_ties_in_order() {
    // Rooms in a row; the most neighbours first: the middle ones.
    let row = json!({"id": "fila", "operator": "PlaceObjects", "config": {
        "layer": "salas", "kind": "x", "count": 5, "where": [{"type": "slot", "y": 0, "x": {"max": 4}}]}});
    let tag = |order: Json| {
        pipeline(
            json!([row, {"id": "t", "operator": "TagObjects", "config": {
            "layer": "salas", "order_by": order, "count": 2, "set": {"role": "x"}}}]),
        )
    };
    let picked = |p: &Json| -> Vec<(i32, i32)> {
        objects(&run(p), "salas")
            .iter()
            .filter(|o| role(o).is_some())
            .map(slot)
            .collect()
    };
    assert_eq!(
        picked(&tag(json!([{"by": "neighbours", "desc": true}]))),
        [(1, 0), (2, 0)]
    );
    assert_eq!(
        picked(&tag(json!([{"by": "neighbours"}]))),
        [(0, 0), (4, 0)]
    );
    // A score: here, the slot column, from `neighbours` alone is not
    // enough; `random` makes it differ between seeds.
    let scored = tag(json!([{"by": "score", "expr": "neighbours * 10 + random", "desc": true}]));
    let p = picked(&scored);
    assert_eq!(p.len(), 2);
    assert!(p.iter().all(|s| (1..=3).contains(&s.0)), "{p:?}");
    let random: std::collections::HashSet<Vec<(i32, i32)>> = (0..20)
        .map(|seed| {
            objects(
                &run_seed(&tag(json!([{"by": "random"}])), seed).unwrap(),
                "salas",
            )
            .iter()
            .filter(|o| role(o).is_some())
            .map(slot)
            .collect()
        })
        .collect();
    assert!(random.len() > 3, "{random:?}");
}

#[test]
fn a_score_must_be_a_number_over_known_names() {
    for (expr, words) in [
        ("neighbours > 1", "is a boolean, not a number"),
        ("vizinhos * 2", "vizinhos"),
        ("1 +", "score `1 +`"),
    ] {
        let p = pipeline(json!([{"id": "t", "operator": "TagObjects", "config": {
            "layer": "salas", "order_by": [{"by": "score", "expr": expr}], "set": {"a": 1}}}]));
        let e = build(&p).err().unwrap();
        assert!(e[0].message.contains(words), "{expr}: {}", e[0].message);
        assert_eq!(e[0].pointer, "/steps/0/config/order_by");
    }
}

#[test]
fn isaac_growth_makes_a_tree_of_the_count() {
    for seed in 0..30 {
        let p = pipeline(json!([start(), isaac_growth(json!({"min": 0, "max": 11}))]));
        let g = run_seed(&p, seed).unwrap();
        let rooms = objects(&g, "salas");
        assert!(rooms.len() <= 12, "seed {seed}");
        // A new room never touches two: the floor is a tree, and each
        // room's parent is the one room it touched when it was made.
        let id_of = |i: usize| rooms[i].id;
        for (i, r) in rooms.iter().enumerate().skip(1) {
            let parent = r.prop("parent").unwrap().as_object().unwrap();
            let before: Vec<&Object> = rooms[..i]
                .iter()
                .filter(|b| crate::filter::touching(r.bbox, b.bbox))
                .collect();
            assert_eq!(before.len(), 1, "seed {seed}");
            assert_eq!(before[0].id, parent);
            assert_ne!(parent, id_of(i));
        }
        let mut slots: Vec<_> = rooms.iter().map(slot).collect();
        slots.sort_unstable();
        slots.dedup();
        assert_eq!(slots.len(), rooms.len());
    }
}

#[test]
fn growth_short_of_its_count_fails_unless_reseeded() {
    // Half the tries fail: 30 rooms are rarely reached in one sweep.
    let exact = pipeline(json!([start(), isaac_growth(json!(30))]));
    let failed = (0..10).filter(|&s| run_seed(&exact, s).is_err()).count();
    assert!(failed > 0);
    let e = run_seed(
        &exact,
        (0..10).find(|&s| run_seed(&exact, s).is_err()).unwrap(),
    )
    .unwrap_err();
    assert!(
        e.contains("grew") && e.contains("fewer than the 30 asked for"),
        "{e}"
    );
    // Reseeding puts every room back whenever the frontier empties.
    let mut reseed = exact.clone();
    reseed["steps"][1]["config"]["on_exhausted"] = json!("reseed");
    let reached = (0..10).filter(|&s| run_seed(&reseed, s).is_ok()).count();
    assert!(reached > 10 - failed, "{reached} vs {}", 10 - failed);
    // From the start only (Isaac's), only its free sides come back.
    reseed["steps"][1]["config"]["from"] = json!([{"type": "has_prop", "name": "role"}]);
    let from_start = (0..10).filter(|&s| run_seed(&reseed, s).is_ok()).count();
    assert!(from_start <= reached, "{from_start} vs {reached}");
}

#[test]
fn the_frontier_policy_is_the_shape() {
    // From a corner, every slot takes; one neighbour at a time.
    let walk = |pick: &str| {
        pipeline(json!([
            {"id": "a", "operator": "PlaceObjects", "config": {"layer": "salas", "kind": "x",
             "where": [{"type": "slot", "x": 0, "y": 0}]}},
            {"id": "b", "operator": "GrowObjects", "config": {"layer": "salas", "count": "all",
             "pick": pick, "expand": "one", "retire": "exhausted"}}]))
    };
    for pick in ["oldest", "newest", "random"] {
        let rooms = objects(&run(&walk(pick)), "salas");
        assert_eq!(rooms.len(), 72, "{pick} fills the lattice");
    }
    // `newest` is a depth-first walk: each room grows from the one made
    // just before, until it is stuck.
    let rooms = objects(&run(&walk("newest")), "salas");
    let chained = rooms
        .windows(2)
        .filter(|w| w[1].prop("parent").unwrap().as_object() == Some(w[0].id))
        .count();
    assert!(chained > 50, "{chained}");
    // `chance` with `exhausted` could loop forever: refused.
    let mut bad = walk("oldest");
    bad["steps"][1]["config"]["chance"] = json!(0.5);
    let e = build(&bad).err().unwrap();
    assert!(e[0].message.contains("retire: visit"), "{}", e[0].message);
}

#[test]
fn measure_writes_hops_and_degrees() {
    let p = pipeline(json!([start(), isaac_growth(json!({"min": 0, "max": 14})),
        {"id": "d", "operator": "MeasureObjects", "config": {"layer": "salas", "into": "d",
         "measure": {"type": "distance", "from": [{"type": "prop_is", "name": "role", "value": "inicio"}]}}},
        {"id": "f", "operator": "MeasureObjects", "config": {"layer": "salas", "into": "f",
         "measure": {"type": "distance", "fraction": true, "from": [{"type": "prop_is", "name": "role", "value": "inicio"}]}}},
        {"id": "c", "operator": "ConnectObjects", "config": {"objects": "salas", "layer": "conexoes", "strategy": "parent"}},
        {"id": "g", "operator": "MeasureObjects", "config": {"layer": "salas", "into": "portas",
         "measure": {"type": "degree", "connections": "conexoes"}}},
        {"id": "h", "operator": "MeasureObjects", "config": {"layer": "salas", "into": "h",
         "measure": {"type": "distance", "connections": "conexoes", "from": [{"type": "prop_is", "name": "role", "value": "inicio"}]}}},
        {"id": "v", "operator": "MeasureObjects", "config": {"layer": "salas", "into": "v",
         "measure": {"type": "neighbours"}}}]));
    for seed in 0..10 {
        let rooms = objects(&run_seed(&p, seed).unwrap(), "salas");
        let int = |o: &Object, k: &str| o.prop(k).unwrap().as_int().unwrap();
        let most = rooms.iter().map(|o| int(o, "d")).max().unwrap();
        for o in &rooms {
            // A room's hops are its parent's plus one.
            if let Some(p) = o.prop("parent").and_then(Value::as_object) {
                let parent = rooms.iter().find(|r| r.id == p).unwrap();
                assert_eq!(int(o, "d"), int(parent, "d") + 1);
            }
            // The tree is the adjacency, so both ways give the same hops.
            assert_eq!(int(o, "d"), int(o, "h"));
            assert_eq!(int(o, "portas"), int(o, "v"));
            assert_eq!(int(o, "v") as usize, neighbours(&rooms, o));
            let f = match o.prop("f").unwrap() {
                Value::Float(f) => *f,
                v => panic!("{v:?}"),
            };
            assert_eq!(f, int(o, "d") as f64 / most.max(1) as f64);
        }
    }
}

#[test]
fn unreachable_objects_lose_the_prop() {
    let p = pipeline(json!([start(),
        {"id": "longe", "operator": "PlaceObjects", "config": {"layer": "salas", "kind": "sala",
         "where": [{"type": "slot", "x": 0, "y": 0}], "props": {"d": 99}}},
        {"id": "d", "operator": "MeasureObjects", "config": {"layer": "salas", "into": "d",
         "measure": {"type": "distance", "from": [{"type": "has_prop", "name": "role"}]}}}]));
    let rooms = objects(&run(&p), "salas");
    assert_eq!(rooms[0].prop("d"), Some(&Value::Int(0)));
    assert_eq!(rooms[1].prop("d"), None);
}

#[test]
fn tag_picks_count_with_chance_per_step() {
    let p = |chance: f64, count: Json| {
        pipeline(json!([start(), isaac_growth(json!({"min": 0, "max": 9})),
            {"id": "t", "operator": "TagObjects", "config": {"layer": "salas", "count": count,
             "chance": chance, "where": [{"type": "neighbours", "max": 1}],
             "order_by": [{"by": "random"}], "set": {"role": "x", "n": 2}}}]))
    };
    let tagged = |g: &Grid| {
        objects(g, "salas")
            .iter()
            .filter(|o| role(o) == Some("x"))
            .count()
    };
    let g = run(&p(1.0, json!(2)));
    assert_eq!(tagged(&g), 2);
    let first = objects(&g, "salas")
        .into_iter()
        .find(|o| role(o) == Some("x"))
        .unwrap();
    assert_eq!(first.prop("n"), Some(&Value::Int(2)));
    assert_eq!(
        tagged(&run(&p(0.0, json!(2)))),
        0,
        "a failed roll changes nothing"
    );
    // A step-level chance is rolled once: everything or nothing.
    // Every room may be picked here, and there is always the start.
    let mut half = p(0.5, json!(1));
    half["steps"][2]["config"]["where"] = json!([]);
    let counts: Vec<usize> = (0..40)
        .map(|s| tagged(&run_seed(&half, s).unwrap()))
        .collect();
    assert!(counts.contains(&0) && counts.contains(&1), "{counts:?}");
}

#[test]
fn connections_join_neighbours_or_the_growth_tree() {
    let p = |strategy: &str| {
        pipeline(json!([start(), isaac_growth(json!({"min": 0, "max": 11})),
            {"id": "s", "operator": "PlaceObjects", "config": {"layer": "salas", "kind": "s",
             "where": [{"type": "neighbours", "min": 2}], "props": {"role": "secreta"}}},
            {"id": "c", "operator": "ConnectObjects", "config": {"objects": "salas",
             "layer": "conexoes", "strategy": strategy, "kind": "porta"}}]))
    };
    for seed in 0..10 {
        let g = run_seed(&p("neighbours"), seed).unwrap();
        let rooms = objects(&g, "salas");
        let links = objects(&g, "conexoes");
        let pairs: usize = rooms.iter().map(|r| neighbours(&rooms, r)).sum::<usize>() / 2;
        assert_eq!(links.len(), pairs, "seed {seed}");
        let tree = objects(&run_seed(&p("parent"), seed).unwrap(), "conexoes");
        // Only grown rooms have a parent: the start and the secret do not.
        assert_eq!(tree.len(), rooms.len() - 2, "seed {seed}");
        for l in &links {
            assert_eq!(l.kind, "porta");
            let end = |k: &str| {
                let id = l.prop(k).unwrap().as_object().unwrap();
                rooms.iter().find(|r| r.id == id).unwrap().clone()
            };
            let (a, b) = (end("from"), end("to"));
            assert!(crate::filter::touching(a.bbox, b.bbox));
            // The box spans the centres: one cell thick, across both walls.
            let thin = l.bbox.size.width == 1 || l.bbox.size.height == 1;
            assert!(
                thin && (l.bbox.size.width == 16 || l.bbox.size.height == 10),
                "{:?}",
                l.bbox
            );
        }
    }
    let e = build(&pipeline(
        json!([{"id": "c", "operator": "ConnectObjects", "config": {
        "objects": "salas", "layer": "salas", "strategy": "neighbours"}}]),
    ))
    .err()
    .unwrap();
    assert!(
        e[0].message.contains("layer of their own"),
        "{}",
        e[0].message
    );
}

#[test]
fn in_object_paints_walls_insides_and_doors() {
    let p = pipeline(json!([start(), isaac_growth(json!({"min": 0, "max": 3})),
        {"id": "c", "operator": "ConnectObjects", "config": {"objects": "salas", "layer": "conexoes", "strategy": "neighbours"}},
        {"id": "w", "operator": "Fill", "config": {"layer": "terreno", "tile": "parede"},
         "conditions": [{"type": "in_object", "layer": "salas", "part": "wall"}]},
        {"id": "f", "operator": "Fill", "config": {"layer": "terreno", "tile": "chao"},
         "conditions": [{"type": "in_object", "layer": "salas", "part": "inside"}]},
        {"id": "d", "operator": "Fill", "config": {"layer": "terreno", "tile": "porta"},
         "conditions": [{"type": "in_object", "layer": "conexoes"},
                        {"type": "tile_is", "layer": "terreno", "tile": "parede"}]}]));
    let Built {
        mut grid,
        tiles,
        pipeline,
    } = build(&p).unwrap();
    pipeline.run(&mut grid, "", 3).unwrap();
    let t = grid.tiles(grid.layer_id("terreno").unwrap()).unwrap();
    let rooms = objects(&grid, "salas");
    let links = objects(&grid, "conexoes");
    let (chao, parede, porta) = (
        tiles.id("chao").unwrap(),
        tiles.id("parede").unwrap(),
        tiles.id("porta").unwrap(),
    );
    let count = |id| t.as_slice().iter().filter(|&&x| x == id).count();
    assert_eq!(count(chao), rooms.len() * 13 * 7);
    assert_eq!(count(porta), links.len() * 2, "two door cells per link");
    assert_eq!(
        count(parede) + count(porta),
        rooms.len() * (15 * 9 - 13 * 7)
    );
    let _ = parede;
}

#[test]
fn check_counts_objects_and_attempts_make_the_map_again() {
    let mut p = pipeline(json!([start(), isaac_growth(json!({"min": 0, "max": 9})),
        {"id": "becos", "operator": "Check", "config": {"objects": "salas",
         "where": [{"type": "neighbours", "max": 1}], "min": 4, "message": "poucos becos"}}]));
    let Built { grid, pipeline, .. } = build(&p).unwrap();
    // One attempt: some seeds fail, with the message.
    let failing = (0..20)
        .find(|&s| pipeline.run_attempts(&grid, "", s, 1).is_err())
        .expect("4 dead ends of 10 rooms is not always there");
    let e = pipeline.run_attempts(&grid, "", failing, 1).unwrap_err();
    assert!(e.to_string().contains("poucos becos"), "{e}");
    // More attempts: the same seed makes it, and says which attempt did.
    let made = pipeline.run_attempts(&grid, "", failing, 200).unwrap();
    assert!(made.attempt > 0);
    assert_eq!(made.failures.len() as u32, made.attempt);
    assert!(made.failures.iter().all(|f| f.step_id == "becos"));
    let dead = objects(&made.grid, "salas");
    let dead = dead.iter().filter(|r| neighbours(&dead, r) <= 1).count();
    assert!(dead >= 4);
    // Deterministic: the same seed, the same attempt and map.
    let again = pipeline.run_attempts(&grid, "", failing, 200).unwrap();
    assert_eq!(again.attempt, made.attempt);
    assert_eq!(objects(&again.grid, "salas"), objects(&made.grid, "salas"));
    // Attempt 0 is the map `run` makes.
    let ok = (0..20)
        .find(|&s| pipeline.run_attempts(&grid, "", s, 1).is_ok())
        .unwrap();
    let mut plain = grid.clone();
    pipeline.run(&mut plain, "", ok).unwrap();
    assert_eq!(
        objects(
            &pipeline.run_attempts(&grid, "", ok, 5).unwrap().grid,
            "salas"
        ),
        objects(&plain, "salas")
    );
    // Giving up says how often each step failed.
    p["steps"][2]["config"]["min"] = json!(40);
    let Built { grid, pipeline, .. } = build(&p).unwrap();
    let RunError::GaveUp { failures } = pipeline.run_attempts(&grid, "", 1, 3).unwrap_err() else {
        panic!("gave up expected");
    };
    assert_eq!(failures.len(), 3);
    let e = pipeline
        .run_attempts(&grid, "", 1, 3)
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("every one of the 3 attempts failed (becos 3 times)"),
        "{e}"
    );
}

#[test]
fn a_bug_is_not_retried() {
    // A config error is caught when built; at run time only `Failed`
    // retries. An `Undeclared` error is a bug: it must not be retried.
    let layer = run(&pipeline(json!([]))).layer_id("salas").unwrap();
    assert!(!crate::pipeline::retries(
        &crate::operator::OpError::Undeclared { layer }
    ));
    assert!(crate::pipeline::retries(
        &crate::operator::OpError::Failed {
            message: String::new()
        }
    ));
}

#[test]
fn layers_without_a_lattice_say_so() {
    for op in ["PlaceObjects", "GrowObjects"] {
        let mut config = json!({"layer": "solta", "count": 1});
        if op == "PlaceObjects" {
            config["kind"] = json!("x");
        }
        let e = build(&pipeline(
            json!([{"id": "a", "operator": op, "config": config}]),
        ))
        .err()
        .unwrap();
        assert!(
            e[0].message.contains("has no lattice"),
            "{op}: {}",
            e[0].message
        );
        assert_eq!(e[0].pointer, "/steps/0/config/layer");
    }
    // A lattice on a tile layer, in the file.
    let mut p = pipeline(json!([]));
    p["layers"][0] = json!({"name": "terreno", "lattice": {"columns": 2, "rows": 2, "cell": {"width": 3, "height": 3}}});
    let e = PipelineSpec::parse(&p.to_string()).err().unwrap();
    assert!(
        e[0].message.contains("only an object layer has a lattice"),
        "{}",
        e[0].message
    );
    assert_eq!(e[0].pointer, "/layers/0/lattice");
}

#[test]
fn a_bad_filter_points_at_its_field() {
    let e = build(&pipeline(json!([{"id": "t", "operator": "TagObjects", "config": {
        "layer": "salas", "where": [{"type": "neighbours", "min": 4, "max": 1}], "set": {"a": 1}}}])))
    .err()
    .unwrap();
    assert_eq!(e[0].pointer, "/steps/0/config/where");
    assert!(
        e[0].message.contains("min 4 is above max 1"),
        "{}",
        e[0].message
    );
}
