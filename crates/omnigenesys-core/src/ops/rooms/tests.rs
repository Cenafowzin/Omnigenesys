//! RoomGrid, PlaceRooms, ConnectRooms and AssignRooms, through pipeline
//! files as a user writes them.

use std::collections::{HashMap, VecDeque};

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

fn grid_step(count: u32, dead_ends: u32, secret: bool) -> Json {
    json!({"id": "andar", "operator": "RoomGrid", "config": {"layer": "terreno", "rooms": "salas",
        "floor": "chao", "wall": "parede", "count": count, "dead_ends": dead_ends, "secret": secret}})
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

fn cell(o: &Object) -> (i32, i32) {
    let c = o.prop("cell").unwrap().as_cell().unwrap();
    (c.x, c.y)
}

#[test]
fn room_grid_lays_out_a_tree_with_the_quotas() {
    for seed in 0..20 {
        let p = pipeline(
            (135, 72),
            json!([grid_step(12, 4, false), connect("adjacent", json!({}))]),
        );
        let (g, _) = run_seed(&p, seed).unwrap();
        let rooms = objects(&g, "salas");
        assert_eq!(rooms.len(), 12, "seed {seed}");
        // Isaac's rule (a cell opens only with ≤ 1 taken neighbour) makes
        // the floor a tree: every room but the first has one way in.
        let l = links(&g);
        assert_eq!(l.len(), 11, "seed {seed}: a tree has rooms − 1 links");
        assert_eq!(reachable(&rooms, &l), 12);
        let degree = |id: ObjectId| l.iter().filter(|(a, b)| *a == id || *b == id).count();
        let dead = rooms[1..].iter().filter(|r| degree(r.id) == 1).count();
        assert!(dead >= 4, "seed {seed}: {dead} dead ends");
        // The first room is the middle cell; cells are distinct.
        assert_eq!(cell(&rooms[0]), (4, 4));
        let mut cells: Vec<_> = rooms.iter().map(cell).collect();
        cells.sort_unstable();
        cells.dedup();
        assert_eq!(cells.len(), 12);
    }
}

#[test]
fn room_grid_is_the_same_for_a_seed_and_differs_between_seeds() {
    let p = pipeline((135, 72), json!([grid_step(10, 3, true)]));
    let cells = |seed| {
        objects(&run_seed(&p, seed).unwrap().0, "salas")
            .iter()
            .map(cell)
            .collect::<Vec<_>>()
    };
    assert_eq!(cells(7), cells(7));
    assert_ne!(cells(7), cells(8));
}

#[test]
fn doors_open_both_walls_at_the_middle() {
    let (g, t) = run(&pipeline(
        (135, 72),
        json!([grid_step(6, 2, false), connect("adjacent", json!({}))]),
    ));
    let terrain = g.tiles(g.layer_id("terreno").unwrap()).unwrap();
    let rooms = objects(&g, "salas");
    let by_id: HashMap<ObjectId, &Object> = rooms.iter().map(|r| (r.id, r)).collect();
    let door = t.id("porta").unwrap();
    for c in objects(&g, "conexoes") {
        let (a, b) = (
            by_id[&c.prop("from").unwrap().as_object().unwrap()],
            by_id[&c.prop("to").unwrap().as_object().unwrap()],
        );
        // Two cells, one in each wall, at the middle of the shared side.
        assert_eq!(c.bbox.size.len(), 2, "{:?}", c.bbox);
        for p in c.bbox.iter() {
            assert_eq!(terrain.get(p), Some(&door));
            assert!(a.bbox.contains(p) || b.bbox.contains(p));
        }
        let (ca, cb) = (cell(a), cell(b));
        let mid = |o: &Object, horizontal: bool| {
            if horizontal {
                o.bbox.origin.y + 4
            } else {
                o.bbox.origin.x + 7
            }
        };
        let p = c.bbox.origin;
        if ca.1 == cb.1 {
            assert_eq!(p.y, mid(a, true), "door at the middle row");
        } else {
            assert_eq!(p.x, mid(a, false), "door at the middle column");
        }
    }
}

#[test]
fn the_secret_room_sits_by_many_rooms_and_no_dead_end() {
    for seed in 0..15 {
        let p = pipeline(
            (135, 72),
            json!([grid_step(12, 4, true), connect("adjacent", json!({}))]),
        );
        let (g, _) = run_seed(&p, seed).unwrap();
        let rooms = objects(&g, "salas");
        let secret: Vec<&Object> = rooms
            .iter()
            .filter(|r| r.prop("role").and_then(Value::as_str) == Some("secret"))
            .collect();
        assert_eq!(secret.len(), 1, "seed {seed}");
        let s = cell(secret[0]);
        let normal: Vec<(i32, i32)> = rooms
            .iter()
            .filter(|r| r.id != secret[0].id)
            .map(cell)
            .collect();
        let near = |c: (i32, i32)| {
            normal
                .iter()
                .filter(|n| (n.0 - c.0).abs() + (n.1 - c.1).abs() == 1)
                .count()
        };
        assert!(near(s) >= 1, "seed {seed}");
        for n in normal
            .iter()
            .filter(|n| (n.0 - s.0).abs() + (n.1 - s.1).abs() == 1)
        {
            let dead_end = *n != normal[0] && near(*n) == 1;
            assert!(!dead_end, "seed {seed}: next to a dead end");
        }
        // Its doors are secret ones.
        for c in objects(&g, "conexoes") {
            let touches = [c.prop("from"), c.prop("to")]
                .iter()
                .any(|v| v.unwrap().as_object() == Some(secret[0].id));
            assert_eq!(touches, c.prop("secret") == Some(&Value::Bool(true)));
        }
    }
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
        json!([grid_step(12, 4, false), connect("adjacent", json!({})),
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
        let painted = boss
            .bbox
            .iter()
            .filter(|c| terrain.get(*c) == Some(&chefe))
            .count();
        assert_eq!(painted, 13 * 7);
    }
}

#[test]
fn failures_say_what_to_change() {
    // Dead ends a 2-room floor cannot have.
    let p = pipeline((135, 72), json!([grid_step(2, 5, false)]));
    let e = run_seed(&p, 1).unwrap_err();
    assert!(e.contains("dead ends") && e.contains("fewer rooms"), "{e}");
    // A grid too big for the map, at build time.
    let e = build(&pipeline((60, 40), json!([grid_step(5, 1, false)])))
        .err()
        .unwrap();
    assert!(
        e[0].message.contains("need a map of 135 × 72"),
        "{}",
        e[0].message
    );
    // More roles than dead ends.
    let p = pipeline(
        (135, 72),
        json!([grid_step(3, 1, false), connect("adjacent", json!({})),
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
    let mut bad = grid_step(5, 1, false);
    bad["config"]["rooms"] = json!("terreno");
    let e = build(&pipeline((135, 72), json!([bad]))).err().unwrap();
    assert!(
        e[0].message.contains("expected objects"),
        "{}",
        e[0].message
    );
}

#[test]
fn rooms_are_exported_with_typed_props() {
    let p = pipeline(
        (135, 72),
        json!([grid_step(4, 1, false), connect("adjacent", json!({}))]),
    );
    let (g, t) = run(&p);
    let mut out = Vec::new();
    let prov = crate::export::Provenance {
        seed: 512,
        salted: false,
        params: Vec::new(),
    };
    crate::export::write_json(&mut out, &g, &t, &prov, crate::export::Encoding::Auto).unwrap();
    let v: Json = serde_json::from_slice(&out).unwrap();
    let objects = v["objects"].as_array().unwrap();
    assert_eq!(objects.len(), 4 + 3);
    assert_eq!(objects[0]["type"], "room");
    assert_eq!(objects[0]["layer"], "salas");
    assert_eq!(
        objects[0]["props"]["cell"],
        json!({"type": "cell", "value": [4, 4, 0]})
    );
    let c = objects.iter().find(|o| o["type"] == "connection").unwrap();
    assert_eq!(c["props"]["from"]["type"], "object");
    assert!(c["bbox"].as_array().unwrap().len() == 6);
    // Ids are unique across both layers.
    let mut ids: Vec<u64> = objects.iter().map(|o| o["id"].as_u64().unwrap()).collect();
    ids.dedup();
    assert_eq!(ids.len(), 7);
}
