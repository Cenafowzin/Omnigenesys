use serde_json::json;

use super::*;
use crate::coords::{Coord, GridSize};
use crate::object::{CellSize, ObjectIds, Offset};

fn b(x: i32, y: i32, w: u32, h: u32) -> Bounds {
    Bounds::new(Coord::new(x, y, 0), GridSize::new(w, h, 1))
}

fn lattice() -> Lattice {
    Lattice {
        columns: 3,
        rows: 3,
        cell: CellSize {
            width: 5,
            height: 4,
        },
        origin: Offset::default(),
    }
}

/// A 15 × 12 map, a 3 × 3 lattice of 5 × 4 slots on layer "salas", and
/// rooms in the slots given (column, row), with props.
fn world(rooms: &[((u32, u32), serde_json::Value)]) -> (Grid, LayerId) {
    let map = b(0, 0, 15, 12);
    let mut grid = Grid::new(map);
    let id = grid.add_object_layer("salas", lattice()).unwrap();
    grid.add_object_layer("outra", lattice()).unwrap();
    let mut ids = ObjectIds::default();
    let mut split = grid.split(&[], &[id]).unwrap();
    let layer = split.writes[0].as_objects_mut().unwrap();
    for ((c, r), props) in rooms {
        let props = props
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    serde_json::from_value::<Scalar>(v.clone())
                        .unwrap()
                        .to_value(),
                )
            })
            .collect();
        layer.push(&mut ids, "room", lattice().bounds(map, *c, *r), props);
    }
    (grid, id)
}

fn filters(v: serde_json::Value) -> Vec<ObjectFilter> {
    serde_json::from_value(v).unwrap()
}

/// Which objects of "salas" pass, by place.
fn passing(grid: &Grid, id: LayerId, f: serde_json::Value) -> Vec<usize> {
    let f = CompiledFilter::compile(&filters(f), grid, id).unwrap();
    let scene = Scene::new(grid.map(), [(id, grid.objects(id).unwrap())]);
    (0..scene.objects(id).len())
        .filter(|&i| f.test(&scene, id, Subject::Object(i)))
        .collect()
}

#[test]
fn touching_needs_a_shared_edge_not_a_corner() {
    let a = b(0, 0, 5, 4);
    assert!(touching(a, b(5, 0, 5, 4)), "right");
    assert!(touching(a, b(0, 4, 5, 4)), "below");
    assert!(touching(a, b(-2, 3, 2, 9)), "left, sharing one row");
    assert!(!touching(a, b(5, 4, 5, 4)), "corner only");
    assert!(!touching(a, b(6, 0, 5, 4)), "a cell apart");
    assert!(!touching(a, b(2, 1, 5, 4)), "overlapping");
    assert!(!touching(a, a), "itself");
}

#[test]
fn overlapping_grows_the_first_box_by_the_margin() {
    let a = b(0, 0, 5, 4);
    assert!(overlapping(a, b(4, 3, 3, 3), 0), "sharing a cell");
    assert!(!overlapping(a, b(5, 0, 5, 4), 0), "side by side");
    assert!(overlapping(a, b(5, 0, 5, 4), 1), "side by side, margin 1");
    assert!(!overlapping(a, b(6, 0, 5, 4), 1), "one cell of gap");
    assert!(
        overlapping(a, b(6, 0, 5, 4), 2),
        "one cell of gap, margin 2"
    );
    assert!(
        overlapping(a, b(5, 4, 2, 2), 1),
        "a corner counts once grown"
    );
    assert!(overlapping(a, b(-6, -5, 5, 4), 2), "above and to the left");
}

#[test]
fn overlaps_counts_objects_and_candidate_boxes() {
    // Rooms in slots (0,0) and (1,0) touch; (2,2) is far.
    let (grid, id) = world(&[
        ((0, 0), json!({})),
        ((1, 0), json!({})),
        ((2, 2), json!({})),
    ]);
    assert_eq!(
        passing(&grid, id, json!([{"type": "overlaps", "max": 0}])),
        [0, 1, 2],
        "boxes side by side do not overlap"
    );
    assert_eq!(
        passing(
            &grid,
            id,
            json!([{"type": "overlaps", "margin": 1, "min": 1}])
        ),
        [0, 1],
        "with a margin they do"
    );
    // A box no object fills yet, over the first room.
    let f = CompiledFilter::compile(&filters(json!([{"type": "overlaps", "max": 0}])), &grid, id)
        .unwrap();
    let scene = Scene::new(grid.map(), [(id, grid.objects(id).unwrap())]);
    assert!(!f.test(&scene, id, Subject::Box(b(2, 2, 3, 3))));
    assert!(f.test(&scene, id, Subject::Box(b(10, 0, 3, 3))));
}

#[test]
fn a_footprint_is_the_slots_a_box_overlaps() {
    let l = lattice();
    let map = b(0, 0, 15, 12);
    assert_eq!(l.footprint(map, l.bounds(map, 1, 2)), [l.index(1, 2)]);
    // Two slots wide, one tall; and a box reaching past the lattice.
    assert_eq!(l.footprint(map, b(3, 0, 5, 4)), [0, 1]);
    assert_eq!(l.footprint(map, b(12, 10, 10, 10)), [8]);
    assert_eq!(l.footprint(map, b(-9, -9, 2, 2)), Vec::<usize>::new());
    // The lattice follows the map's origin and its own offset.
    let moved = Lattice {
        origin: Offset { x: 1, y: 2 },
        ..l
    };
    let map2 = b(10, 10, 15, 12);
    assert_eq!(moved.bounds(map2, 0, 0).origin, Coord::new(11, 12, 0));
    assert_eq!(moved.footprint(map2, b(16, 16, 1, 1)), [moved.index(1, 1)]);
}

#[test]
fn a_lattice_says_what_is_wrong_with_it() {
    let ok = lattice();
    assert_eq!(ok.problem(), None);
    for (bad, words) in [
        (Lattice { columns: 0, ..ok }, "at least 1 column"),
        (
            Lattice {
                cell: CellSize {
                    width: 0,
                    height: 3,
                },
                ..ok
            },
            "at least 1 × 1",
        ),
        (
            Lattice {
                columns: 100,
                rows: 100,
                ..ok
            },
            "at most 4096",
        ),
    ] {
        let p = bad.problem().unwrap();
        assert!(p.contains(words), "{p}");
    }
}

#[test]
fn props_kinds_and_values_filter_objects() {
    let (g, id) = world(&[
        ((0, 0), json!({"role": "inicio", "d": 0})),
        ((1, 0), json!({"role": "chefe", "d": 4})),
        ((2, 0), json!({"d": 2.5})),
        ((2, 1), json!({"secret": true})),
    ]);
    let pass = |f| passing(&g, id, f);
    assert_eq!(
        pass(json!([{"type": "kind_is", "kind": "room"}])),
        [0, 1, 2, 3]
    );
    assert_eq!(
        pass(json!([{"type": "kind_is", "kind": ["x", "y"]}])),
        Vec::<usize>::new()
    );
    assert_eq!(
        pass(json!([{"type": "prop_is", "name": "role", "value": "chefe"}])),
        [1]
    );
    assert_eq!(
        pass(json!([{"type": "prop_is", "name": "role", "value": ["chefe", "inicio"]}])),
        [0, 1]
    );
    // 4 and 4.0 are the same number; true is not the text "true".
    assert_eq!(
        pass(json!([{"type": "prop_is", "name": "d", "value": 4.0}])),
        [1]
    );
    assert_eq!(
        pass(json!([{"type": "prop_is", "name": "secret", "value": true}])),
        [3]
    );
    assert_eq!(
        pass(json!([{"type": "prop_is", "name": "secret", "value": "true"}])),
        Vec::<usize>::new()
    );
    assert_eq!(pass(json!([{"type": "has_prop", "name": "role"}])), [0, 1]);
    assert_eq!(
        pass(json!([{"type": "not", "filter": {"type": "has_prop", "name": "role"}}])),
        [2, 3]
    );
    assert_eq!(
        pass(json!([{"type": "prop_in_range", "name": "d", "min": 1, "max": 4}])),
        [1, 2]
    );
    assert_eq!(
        pass(json!([{"type": "any", "filters": [
            {"type": "prop_is", "name": "role", "value": "inicio"},
            {"type": "has_prop", "name": "secret"}]}])),
        [0, 3]
    );
    // A list is an implicit `all`.
    assert_eq!(
        pass(
            json!([{"type": "has_prop", "name": "d"}, {"type": "prop_in_range", "name": "d", "max": 1}])
        ),
        [0]
    );
}

#[test]
fn neighbours_count_objects_side_by_side() {
    // . a .
    // b c d
    // . e .
    let (g, id) = world(&[
        ((1, 0), json!({})),
        ((0, 1), json!({"role": "x"})),
        ((1, 1), json!({})),
        ((2, 1), json!({})),
        ((1, 2), json!({})),
    ]);
    let pass = |f| passing(&g, id, f);
    assert_eq!(
        pass(json!([{"type": "neighbours", "max": 1}])),
        [0, 1, 3, 4]
    );
    assert_eq!(pass(json!([{"type": "neighbours", "min": 4}])), [2]);
    assert_eq!(
        pass(
            json!([{"type": "neighbours", "min": 1, "where": [{"type": "has_prop", "name": "role"}]}])
        ),
        [2]
    );
    // Empty slots count the objects around them too.
    let f = CompiledFilter::compile(&filters(json!([{"type": "neighbours", "min": 2}])), &g, id)
        .unwrap();
    let scene = Scene::new(g.map(), [(id, g.objects(id).unwrap())]);
    let corners: Vec<usize> = scene
        .empty_slots(id)
        .into_iter()
        .filter(|&s| f.test(&scene, id, Subject::Slot(s)))
        .collect();
    assert_eq!(corners, [0, 2, 6, 8]);
}

#[test]
fn slot_is_the_place_on_the_lattice() {
    let (g, id) = world(&[
        ((0, 0), json!({})),
        ((2, 1), json!({})),
        ((1, 2), json!({})),
    ]);
    let pass = |f| passing(&g, id, f);
    assert_eq!(pass(json!([{"type": "slot", "x": 2, "y": 1}])), [1]);
    assert_eq!(pass(json!([{"type": "slot", "y": {"min": 1}}])), [1, 2]);
    // Without a lattice, at build time.
    let mut plain = Grid::new(b(0, 0, 10, 10));
    let p = plain.add_layer("solta", LayerKind::Objects).unwrap();
    let e = CompiledFilter::compile(&filters(json!([{"type": "slot", "x": 0}])), &plain, p)
        .unwrap_err();
    assert!(e.to_string().contains("'solta' has none"), "{e}");
}

#[test]
fn refers_to_looks_into_another_layer() {
    let (mut g, rooms) = world(&[((0, 0), json!({"role": "secreta"})), ((1, 0), json!({}))]);
    let links = g.layer_id("outra").unwrap();
    let ids: Vec<_> = g.objects(rooms).unwrap().iter().map(|o| o.id).collect();
    let mut counter = ObjectIds::default();
    for _ in 0..10 {
        counter.fresh();
    }
    let mut split = g.split(&[], &[links]).unwrap();
    let layer = split.writes[0].as_objects_mut().unwrap();
    layer.push(
        &mut counter,
        "door",
        b(4, 1, 2, 1),
        vec![
            ("from".into(), Value::Object(ids[1])),
            ("to".into(), Value::Object(ids[0])),
        ],
    );
    layer.push(
        &mut counter,
        "door",
        b(0, 0, 1, 1),
        vec![("from".into(), Value::Object(ids[1]))],
    );
    let f = CompiledFilter::compile(
        &filters(
            json!([{"type": "refers_to", "props": ["from", "to"], "layer": "salas",
            "where": [{"type": "prop_is", "name": "role", "value": "secreta"}]}]),
        ),
        &g,
        links,
    )
    .unwrap();
    assert_eq!(f.layers(), [rooms]);
    let scene = Scene::new(
        g.map(),
        [
            (links, g.objects(links).unwrap()),
            (rooms, g.objects(rooms).unwrap()),
        ],
    );
    assert!(f.test(&scene, links, Subject::Object(0)));
    assert!(!f.test(&scene, links, Subject::Object(1)));
}

#[test]
fn bad_filters_fail_when_compiled() {
    let (g, id) = world(&[]);
    for (f, words) in [
        (
            json!([{"type": "neighbours"}]),
            "needs at least one of `min` or `max`",
        ),
        (
            json!([{"type": "neighbours", "min": 3, "max": 1}]),
            "min 3 is above max 1",
        ),
        (
            json!([{"type": "prop_is", "name": "a", "value": []}]),
            "at least one value",
        ),
        (
            json!([{"type": "any", "filters": []}]),
            "at least one filter",
        ),
        (
            json!([{"type": "refers_to", "props": "a", "layer": "nada"}]),
            "'nada' not found",
        ),
    ] {
        let e = CompiledFilter::compile(&filters(f.clone()), &g, id).unwrap_err();
        assert!(e.to_string().contains(words), "{f}: {e}");
    }
    // A typo is an error, never a filter that passes everything.
    let e = serde_json::from_value::<Vec<ObjectFilter>>(json!([{"type": "neighbors", "max": 1}]))
        .unwrap_err();
    assert!(e.to_string().contains("neighbors"), "{e}");
    let e = serde_json::from_value::<Vec<ObjectFilter>>(json!([{"type": "neighbours", "mx": 1}]))
        .unwrap_err();
    assert!(e.to_string().contains("mx"), "{e}");
}

#[test]
fn a_scene_sees_objects_added_while_a_step_runs() {
    let (g, id) = world(&[((1, 1), json!({}))]);
    let mut scene = Scene::new(g.map(), [(id, g.objects(id).unwrap())]);
    assert_eq!(scene.empty_slots(id).len(), 8);
    assert_eq!(scene.in_slot(id, 4), Some(0));
    let map = scene.map();
    let i = scene.push(
        id,
        Object {
            id: ObjectIds::default().fresh(),
            kind: "room".into(),
            bbox: lattice().bounds(map, 2, 1),
            props: Vec::new(),
        },
    );
    assert_eq!(i, 1);
    assert_eq!(scene.in_slot(id, 5), Some(1));
    assert_eq!(
        scene.neighbours(id, Subject::Object(0)).collect::<Vec<_>>(),
        [1]
    );
    assert_eq!(scene.slot_of(id, Subject::Object(1)), Some((2, 1)));
}
