//! PlaceRooms, ConnectRooms and AssignRooms, through pipeline
//! files as a user writes them.

use std::collections::VecDeque;

use serde_json::{Value as Json, json};

use crate::{
    grid::Grid,
    object::{Object, ObjectId, Value},
    operator::Registry,
    spec::{Built, Diagnostic, PipelineSpec},
    tile::TileRegistry,
};

const TILES: [&str; 8] = [
    "chao", "parede", "porta", "secreta", "chefe", "inicio", "tesouro", "pedra",
];

fn pipeline(size: (u32, u32), steps: Json) -> Json {
    json!({
        "version": 2, "seed": 512, "size": {"width": size.0, "height": size.1},
        "layers": ["terreno", {"name": "salas", "kind": "objects"}, {"name": "conexoes", "kind": "objects"}],
        "tiles": TILES,
        "steps": steps,
    })
}

fn build(p: &Json) -> Result<Built, Vec<Diagnostic>> {
    PipelineSpec::parse(&p.to_string())
        .unwrap()
        .spec
        .build(&Registry::with_builtins())
}

fn run_seed(p: &Json, seed: u64) -> Result<(Grid, TileRegistry), String> {
    let Built {
        mut grid,
        tiles,
        pipeline,
    } = build(p).unwrap_or_else(|e| panic!("{e:#?}"));
    pipeline
        .run(&mut grid, "", seed)
        .map_err(|e| e.to_string())?;
    Ok((grid, tiles))
}

fn run(p: &Json) -> (Grid, TileRegistry) {
    run_seed(p, 512).unwrap()
}

fn objects(g: &Grid, layer: &str) -> Vec<Object> {
    g.objects(g.layer_id(layer).unwrap())
        .unwrap()
        .iter()
        .cloned()
        .collect()
}

fn place(tries: u32) -> Json {
    json!({"id": "salas", "operator": "PlaceRooms", "config": {
        "layer": "terreno", "rooms": "salas", "floor": "chao", "wall": "parede", "tries": tries}})
}

fn connect(strategy: &str, extra: Json) -> Json {
    let mut config = json!({"rooms": "salas", "connections": "conexoes", "layer": "terreno",
        "floor": "chao", "door": "porta", "secret_door": "secreta", "strategy": strategy});
    for (k, v) in extra.as_object().unwrap() {
        config[k] = v.clone();
    }
    json!({"id": "ligar", "operator": "ConnectRooms", "config": config})
}

fn links(g: &Grid) -> Vec<(ObjectId, ObjectId)> {
    objects(g, "conexoes")
        .iter()
        .map(|c| {
            (
                c.prop("from").unwrap().as_object().unwrap(),
                c.prop("to").unwrap().as_object().unwrap(),
            )
        })
        .collect()
}

/// Rooms reachable from the first one through the connections.
fn reachable(rooms: &[Object], links: &[(ObjectId, ObjectId)]) -> usize {
    let mut seen = vec![rooms[0].id];
    let mut queue = VecDeque::from([rooms[0].id]);
    while let Some(id) = queue.pop_front() {
        for &(a, b) in links {
            let other = if a == id {
                b
            } else if b == id {
                a
            } else {
                continue;
            };
            if !seen.contains(&other) {
                seen.push(other);
                queue.push_back(other);
            }
        }
    }
    seen.len()
}

#[test]
fn place_rooms_keeps_them_apart_and_inside() {
    let p = pipeline(
        (96, 64),
        json!([{"id": "salas", "operator": "PlaceRooms", "config": {
        "layer": "terreno", "rooms": "salas", "floor": "chao", "wall": "parede", "tries": 60, "gap": 2}}]),
    );
    let (g, _) = run(&p);
    let rooms = objects(&g, "salas");
    assert!(rooms.len() >= 5, "{}", rooms.len());
    for (i, a) in rooms.iter().enumerate() {
        assert!(g.bounds().contains(a.bbox.origin));
        for b in &rooms[i + 1..] {
            assert!(
                !super::common::too_close(a.bbox, b.bbox, 2),
                "{:?} {:?}",
                a.bbox,
                b.bbox
            );
        }
    }
}

#[test]
fn every_strategy_connects_every_room() {
    let place = json!({"id": "salas", "operator": "PlaceRooms", "config": {
        "layer": "terreno", "rooms": "salas", "floor": "chao", "wall": "parede", "tries": 40}});
    for (strategy, extra) in [
        ("chain", json!({})),
        ("mst", json!({})),
        ("mst", json!({"loops": 1.0, "corridor": "z", "width": 2})),
    ] {
        for seed in 0..8 {
            let p = pipeline(
                (96, 64),
                json!([{"id": "rocha", "operator": "Fill", "config": {"layer": "terreno", "tile": "pedra"}}, place, connect(strategy, extra.clone())]),
            );
            let (g, _) = run_seed(&p, seed).unwrap();
            let rooms = objects(&g, "salas");
            let l = links(&g);
            assert_eq!(
                reachable(&rooms, &l),
                rooms.len(),
                "{strategy} {extra} seed {seed}"
            );
            if extra.get("loops").is_none() {
                assert_eq!(l.len(), rooms.len() - 1, "{strategy}: a tree");
            }
        }
    }
    // Loops add links.
    let count = |loops: f64| {
        let p = pipeline(
            (96, 64),
            json!([place, connect("mst", json!({"loops": loops}))]),
        );
        links(&run(&p).0).len()
    };
    assert!(count(1.0) > count(0.0));
}

#[test]
fn roles_follow_the_graph() {
    let p = pipeline(
        (135, 72),
        json!([place(40), connect("mst", json!({})),
        {"id": "papeis", "operator": "AssignRooms", "config": {"rooms": "salas", "connections": "conexoes",
         "layer": "terreno", "floor": "chao", "roles": [
            {"role": "start", "pick": "first", "tile": "inicio"},
            {"role": "boss", "pick": "farthest", "dead_end": true, "tile": "chefe"},
            {"role": "treasure", "pick": "random", "dead_end": true, "tile": "tesouro"}]}}]),
    );
    for seed in 0..10 {
        let (g, t) = run_seed(&p, seed).unwrap();
        let rooms = objects(&g, "salas");
        let l = links(&g);
        let degree = |id: ObjectId| l.iter().filter(|(a, b)| *a == id || *b == id).count();
        let depth = |r: &Object| r.prop("depth").unwrap().as_int().unwrap();
        let role = |r: &Object| r.prop("role").and_then(Value::as_str).map(str::to_string);
        assert_eq!(role(&rooms[0]).as_deref(), Some("start"));
        assert_eq!(depth(&rooms[0]), 0);
        let boss = rooms
            .iter()
            .find(|r| role(r).as_deref() == Some("boss"))
            .unwrap();
        assert_eq!(degree(boss.id), 1, "the boss is in a dead end");
        let deepest_dead_end = rooms[1..]
            .iter()
            .filter(|r| degree(r.id) == 1)
            .map(depth)
            .max()
            .unwrap();
        assert_eq!(depth(boss), deepest_dead_end, "seed {seed}");
        let treasure = rooms
            .iter()
            .find(|r| role(r).as_deref() == Some("treasure"))
            .unwrap();
        assert_ne!(treasure.id, boss.id);
        // The boss floor is painted, its walls are not.
        let terrain = g.tiles(g.layer_id("terreno").unwrap()).unwrap();
        let chefe = t.id("chefe").unwrap();
        let inside = super::common::interior(boss.bbox);
        assert!(inside.iter().any(|c| terrain.get(c) == Some(&chefe)));
        assert!(
            boss.bbox
                .iter()
                .filter(|c| !inside.contains(*c))
                .all(|c| terrain.get(c) != Some(&chefe))
        );
    }
}

#[test]
fn failures_say_what_to_change() {
    // More roles than dead ends: a chain has two.
    let p = pipeline(
        (96, 64),
        json!([place(40), connect("chain", json!({})),
        {"id": "papeis", "operator": "AssignRooms", "config": {"rooms": "salas", "connections": "conexoes",
         "layer": "terreno", "floor": "chao", "roles": [
            {"role": "a", "pick": "random", "dead_end": true}, {"role": "b", "pick": "random", "dead_end": true},
            {"role": "c", "pick": "random", "dead_end": true}]}}]),
    );
    let e = run_seed(&p, 1).unwrap_err();
    assert!(
        e.contains("no room left for the role 'c' among the dead ends"),
        "{e}"
    );
    // Rooms written to a tile layer.
    let mut bad = place(5);
    bad["config"]["rooms"] = json!("terreno");
    let e = build(&pipeline((96, 64), json!([bad]))).err().unwrap();
    assert!(
        e[0].message.contains("expected objects"),
        "{}",
        e[0].message
    );
}

#[test]
fn rooms_are_exported_with_typed_props() {
    let p = pipeline((96, 64), json!([place(40), connect("chain", json!({}))]));
    let (g, t) = run(&p);
    let rooms = objects(&g, "salas").len();
    let mut out = Vec::new();
    let prov = crate::export::Provenance {
        seed: 512,
        salted: false,
        attempt: 0,
        params: Vec::new(),
    };
    crate::export::write_json(&mut out, &g, &t, &prov, crate::export::Encoding::Auto).unwrap();
    let v: Json = serde_json::from_slice(&out).unwrap();
    let objects = v["objects"].as_array().unwrap();
    assert_eq!(objects.len(), rooms + rooms - 1);
    assert_eq!(objects[0]["type"], "room");
    assert_eq!(objects[0]["layer"], "salas");
    let c = objects.iter().find(|o| o["type"] == "connection").unwrap();
    assert_eq!(c["props"]["from"]["type"], "object");
    assert!(c["bbox"].as_array().unwrap().len() == 6);
    // Ids are unique across both layers.
    let mut ids: Vec<u64> = objects.iter().map(|o| o["id"].as_u64().unwrap()).collect();
    ids.dedup();
    assert_eq!(ids.len(), 2 * rooms - 1);
}
