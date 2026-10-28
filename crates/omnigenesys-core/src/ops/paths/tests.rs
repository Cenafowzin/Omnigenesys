//! `CarvePaths` through pipeline files as a user writes them.

use serde_json::{Value as Json, json};

use crate::{
    coords::Coord,
    operator::Registry,
    spec::{Built, Diagnostic, PipelineSpec},
    tile::TileId,
};

/// Two 9 × 7 rooms on a 40 × 20 map, in slots of a 2 × 2 lattice (the
/// first in column 0, row 0; the second in column 1, row `row_b`),
/// joined, and then `carve` (and `more` steps).
fn pipeline(row_b: u32, carve: Json, more: Json) -> Json {
    let steps = vec![
        json!({"id": "rocha", "operator": "Fill", "config": {"layer": "terreno", "tile": "pedra"}}),
        json!({"id": "a", "operator": "PlaceObjects", "config": {
            "layer": "salas", "kind": "sala", "where": [{"type": "slot", "x": 0, "y": 0}]}}),
        json!({"id": "b", "operator": "PlaceObjects", "config": {
            "layer": "salas", "kind": "sala", "where": [{"type": "slot", "x": 1, "y": row_b}]}}),
        json!({"id": "c", "operator": "ConnectObjects", "config": {
            "objects": "salas", "layer": "conexoes", "strategy": "mst"}}),
        json!({"id": "cavar", "operator": "CarvePaths", "config": carve, "conditions": more}),
    ];
    json!({
        "version": 2, "seed": 512, "size": {"width": 40, "height": 20},
        "layers": ["terreno",
            {"name": "salas", "kind": "objects", "lattice": {"columns": 2, "rows": 2, "cell": {"width": 9, "height": 7}, "origin": {"x": 0, "y": 0}}},
            {"name": "conexoes", "kind": "objects"},
            {"name": "outra", "kind": "objects"}],
        "tiles": ["pedra", "chao"],
        "steps": steps,
    })
}

fn carve(extra: Json) -> Json {
    let mut c =
        json!({"connections": "conexoes", "objects": "salas", "layer": "terreno", "tile": "chao"});
    c.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    c
}

fn build(p: &Json) -> Result<Built, Vec<Diagnostic>> {
    PipelineSpec::parse(&p.to_string())?
        .spec
        .build(&Registry::with_builtins())
}

/// The map after the run, as rows of `#` (chao) and `.` (anything else).
fn run(p: &Json) -> Result<Vec<String>, String> {
    let Built {
        mut grid,
        pipeline,
        tiles,
    } = build(p).unwrap_or_else(|e| panic!("{e:#?}"));
    pipeline
        .run(&mut grid, "", 512)
        .map_err(|e| e.to_string())?;
    let chao: TileId = tiles.id("chao").unwrap();
    let layer = grid.tiles(grid.layer_id("terreno").unwrap()).unwrap();
    Ok((0..20)
        .map(|y| {
            (0..40)
                .map(|x| {
                    if *layer.get(Coord::new(x, y, 0)).unwrap() == chao {
                        '#'
                    } else {
                        '.'
                    }
                })
                .collect()
        })
        .collect())
}

#[test]
fn a_straight_path_joins_two_middles_in_a_row() {
    // Middles (4, 3) and (13, 3): the same row, so L and Z are one line.
    for shape in ["l", "z"] {
        let rows = run(&pipeline(0, carve(json!({"shape": shape})), json!([]))).unwrap();
        assert_eq!(&rows[3][..18], "....##########....", "{shape}");
        assert_eq!(
            rows.iter().map(|r| r.matches('#').count()).sum::<usize>(),
            10
        );
    }
}

#[test]
fn a_wider_brush_and_conditions() {
    let rows = run(&pipeline(0, carve(json!({"width": 3})), json!([]))).unwrap();
    for row in &rows[2..=4] {
        assert_eq!(&row[..18], "...############...");
    }
    // Not inside the rooms: only through the two walls that touch.
    let rows = run(&pipeline(
        0,
        carve(json!({})),
        json!([{"type": "not", "condition": {"type": "in_object", "layer": "salas", "part": "inside"}}]),
    ))
    .unwrap();
    assert_eq!(&rows[3][..18], "........##........");
}

#[test]
fn an_l_turns_once_and_a_z_turns_twice() {
    // Middles (4, 3) and (13, 10).
    let turns = |shape: &str| {
        let rows = run(&pipeline(1, carve(json!({"shape": shape})), json!([]))).unwrap();
        let open = |x: usize, y: usize| rows[y].as_bytes()[x] == b'#';
        assert!(open(4, 3) && open(13, 10), "{shape}: both middles");
        let cells: usize = rows.iter().map(|r| r.matches('#').count()).sum();
        assert_eq!(cells, 9 + 7 + 1, "{shape}: the shortest walk between them");
        // Corners: a cell with an open neighbour on each axis.
        let mut corners = 0;
        for y in 0..20 {
            for x in 0..40 {
                if !open(x, y) {
                    continue;
                }
                let h = (x > 0 && open(x - 1, y)) || (x < 39 && open(x + 1, y));
                let v = (y > 0 && open(x, y - 1)) || (y < 19 && open(x, y + 1));
                corners += usize::from(h && v);
            }
        }
        corners
    };
    assert_eq!(turns("l"), 1);
    assert!(turns("z") <= 2);
}

#[test]
fn a_connection_must_name_objects_of_objects() {
    let mut p = pipeline(0, carve(json!({})), json!([]));
    // The ends are looked for in a layer without them.
    p["steps"][4]["config"]["objects"] = json!("outra");
    let e = run(&p).unwrap_err();
    assert!(e.contains("which is not in `objects`"), "{e}");
    let mut p = pipeline(0, carve(json!({})), json!([]));
    p["steps"][4]["config"]["objects"] = json!("conexoes");
    let e = build(&p).err().unwrap();
    assert_eq!(e[0].pointer, "/steps/4/config/objects");
    let mut p = pipeline(0, carve(json!({"width": 6})), json!([]));
    p["steps"][4]["conditions"] = json!([]);
    let e = build(&p).err().unwrap();
    assert_eq!(e[0].pointer, "/steps/4/config/width");
}

/// Rooms A and B diagonal from each other: the cheapest walk with walls
/// costing 10 goes through one wall cell of each and never a corner.
#[test]
fn a_cheapest_path_crosses_each_wall_once() {
    let costs =
        json!([{"where": [{"type": "in_object", "layer": "salas", "part": "wall"}], "cost": 10}]);
    for turn in [0, 4] {
        let rows = run(&pipeline(
            1,
            carve(json!({"shape": "cheapest", "costs": costs, "turn": turn})),
            json!([]),
        ))
        .unwrap();
        let open = |x: i32, y: i32| rows[y as usize].as_bytes()[x as usize] == b'#';
        assert!(open(4, 3) && open(13, 10));
        // Room A is x 0..=8, y 0..=6; room B x 9..=17, y 7..=13.
        let ring = |x: i32, y: i32| {
            let on = |x0: i32, y0: i32| {
                (x0..=x0 + 8).contains(&x)
                    && (y0..=y0 + 6).contains(&y)
                    && (x == x0 || x == x0 + 8 || y == y0 || y == y0 + 6)
            };
            on(0, 0) || on(9, 7)
        };
        let walls: Vec<(i32, i32)> = (0..20)
            .flat_map(|y| (0..40).map(move |x| (x, y)))
            .filter(|&(x, y)| open(x, y) && ring(x, y))
            .collect();
        assert_eq!(
            walls.len(),
            2,
            "turn {turn}: {walls:?}\n{}",
            rows.join("\n")
        );
    }
}

/// The costs are worked out again after each path: with the path's tile
/// cheap, the second path walks along the first.
#[test]
fn a_later_path_sees_the_earlier_ones() {
    let mut p = pipeline(
        0,
        carve(json!({"shape": "cheapest", "costs": [
            {"where": [{"type": "tile_is", "layer": "terreno", "tile": "chao"}], "cost": 1},
            {"where": [], "cost": 5}]})),
        json!([]),
    );
    // A 3 × 3 lattice of 9 × 6 cells: A at (0, 0), B at (2, 0), C at (1, 2),
    // joined in the order they were made: A–B, then B–C.
    p["layers"][1]["lattice"] = json!({"columns": 3, "rows": 3, "cell": {"width": 9, "height": 6}, "origin": {"x": 0, "y": 0}});
    p["steps"][2]["config"]["where"] = json!([{"type": "slot", "x": 2, "y": 0}]);
    let c = json!({"id": "c2", "operator": "PlaceObjects", "config": {
        "layer": "salas", "kind": "sala", "where": [{"type": "slot", "x": 1, "y": 2}]}});
    p["steps"].as_array_mut().unwrap().insert(3, c);
    p["steps"][4]["config"]["strategy"] = json!("chain");
    let rows = run(&p).unwrap();
    let open = |x: usize, y: usize| rows[y].as_bytes()[x] == b'#';
    // A–B: the row of the middles, (4, 3) to (22, 3).
    assert!((4..=22).all(|x| open(x, 3)));
    // B–C, (22, 3) to (13, 15): back along A–B, then down.
    assert!((4..=15).all(|y| open(13, y)), "\n{}", rows.join("\n"));
    assert!(!open(22, 4) && !open(14, 15));
    assert_eq!(
        rows.iter().map(|r| r.matches('#').count()).sum::<usize>(),
        19 + 12
    );
}

/// Rooms A and B in a row, their walls blocked but for a gap in the
/// facing ones (the wall cells `gap` from the map's edge: rows `gap` of
/// x 8 and 9). A path 3 wide fits through a gap 3 tall and paints no
/// blocked cell; through a gap 1 tall it does not fit at all.
#[test]
fn a_wide_cheapest_path_fits_through_gaps_as_wide_as_itself() {
    let wall = json!({"type": "in_object", "layer": "salas", "part": "wall"});
    let costs = |gap: Json| {
        json!([
            {"where": [wall, {"type": "edge_distance", "min": gap["min"], "max": gap["max"]}], "cost": 1},
            {"where": [wall], "cost": "blocked"}])
    };
    let carve3 = |gap: Json| {
        pipeline(
            0,
            carve(json!({"shape": "cheapest", "width": 3, "costs": costs(gap)})),
            json!([]),
        )
    };
    let rows = run(&carve3(json!({"min": 2, "max": 4}))).unwrap();
    let open = |x: usize, y: usize| rows[y].as_bytes()[x] == b'#';
    for x in [8, 9] {
        let painted: Vec<usize> = (0..7).filter(|&y| open(x, y)).collect();
        assert_eq!(
            painted,
            [2, 3, 4],
            "x {x}
{}",
            rows.join(
                "
"
            )
        );
    }
    let e = run(&carve3(json!({"min": 3, "max": 3}))).unwrap_err();
    assert!(
        e.contains("blocked cells cut (4, 3) off from (13, 3)"),
        "{e}"
    );
}

#[test]
fn cheapest_fails_when_blocked_cells_cut_the_ends_apart() {
    // Rooms that touch only at a corner, and nothing outside them walkable.
    let costs = json!([{"where": [{"type": "not", "condition": {"type": "in_object", "layer": "salas"}}], "cost": "blocked"}]);
    let e = run(&pipeline(
        1,
        carve(json!({"shape": "cheapest", "costs": costs})),
        json!([]),
    ))
    .unwrap_err();
    assert!(
        e.contains("blocked cells cut (4, 3) off from (13, 10)"),
        "{e}"
    );
}

#[test]
fn costs_and_turns_belong_to_cheapest_paths() {
    let pointer = |extra: Json| {
        let p = pipeline(0, carve(extra), json!([]));
        build(&p).err().unwrap()[0].pointer.clone()
    };
    let wall = json!([{"where": [], "cost": 3}]);
    assert_eq!(pointer(json!({"costs": wall})), "/steps/4/config/costs");
    assert_eq!(
        pointer(json!({"shape": "z", "turn": 2})),
        "/steps/4/config/turn"
    );
    assert_eq!(
        pointer(json!({"shape": "cheapest", "turn": 1001})),
        "/steps/4/config/turn"
    );
    let p = pipeline(
        0,
        carve(json!({"shape": "cheapest", "costs": [{"where": [], "cost": 0}]})),
        json!([]),
    );
    let e = &build(&p).err().unwrap()[0];
    assert_eq!(e.pointer, "/steps/4/config/costs");
    assert!(e.message.contains("rule 0: a cost of 0"), "{}", e.message);
    let p = pipeline(
        0,
        carve(json!({"shape": "cheapest", "costs": [{"where": [], "cost": "blockd"}]})),
        json!([]),
    );
    let e = &build(&p).err().unwrap()[0];
    assert!(e.message.contains("or \"blocked\""), "{}", e.message);
}

// add: terms summed on the first-match cost

#[test]
fn an_added_rule_is_the_same_as_its_sum_in_costs() {
    let wall = json!({"type": "in_object", "layer": "salas", "part": "wall"});
    for turn in [0, 4] {
        let first = run(&pipeline(
            1,
            carve(json!({"shape": "cheapest", "turn": turn, "costs": [{"where": [wall], "cost": 10}]})),
            json!([]),
        ))
        .unwrap();
        let added = run(&pipeline(
            1,
            carve(
                json!({"shape": "cheapest", "turn": turn, "add": [{"where": [wall], "cost": 9}]}),
            ),
            json!([]),
        ))
        .unwrap();
        assert_eq!(first, added, "turn {turn}");
    }
}

/// Rooms A and B in a row, the straight path between their middles along
/// y 3; a hill of cost in a field halfway pushes the path off that row.
#[test]
fn a_field_term_makes_the_path_go_round_its_hills() {
    let rows_with = |weight: f64| {
        let mut p = pipeline(
            0,
            carve(json!({"shape": "cheapest", "add": [{"field": "morro", "weight": weight}]})),
            json!([]),
        );
        p["layers"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name": "morro", "kind": "scalar"}));
        // 1 at (8, 3)…(9, 3), falling to 0 three cells away; a Curve
        // inverted so the hill is high in the middle.
        let hill = [
            json!({"id": "morro", "operator": "RadialGradient", "config": {
                "layer": "morro", "x": 0.225, "y": 0.175, "radius_x": 0.15, "radius_y": 0.15}}),
            json!({"id": "alto", "operator": "Curve", "config": {
                "from": "morro", "layer": "morro", "curve": {"kind": "linear"}, "invert": true}}),
        ];
        let steps = p["steps"].as_array_mut().unwrap();
        let at = steps.len() - 1;
        for (k, s) in hill.into_iter().enumerate() {
            steps.insert(at + k, s);
        }
        run(&p).unwrap()
    };
    let open = |rows: &[String], x: usize, y: usize| rows[y].as_bytes()[x] == b'#';
    let flat = rows_with(0.0);
    assert!((4..=13).all(|x| open(&flat, x, 3)), "\n{}", flat.join("\n"));
    let hilly = rows_with(40.0);
    assert!(
        !(4..=13).all(|x| open(&hilly, x, 3)),
        "\n{}",
        hilly.join("\n")
    );
    assert!(open(&hilly, 4, 3) && open(&hilly, 13, 3));
}

#[test]
fn added_terms_keep_costs_from_1_and_blocked_cells_blocked() {
    // A field taking everything below 1, and the rooms' walls blocked but
    // the middle cell of the facing ones: the path still goes through it.
    let mut p = pipeline(
        0,
        carve(json!({"shape": "cheapest", "costs": [
            {"where": [{"type": "in_object", "layer": "salas", "part": "wall"},
                       {"type": "not", "condition": {"type": "edge_distance", "min": 3, "max": 3}}], "cost": "blocked"}],
            "add": [{"field": "todo", "weight": -500}]})),
        json!([]),
    );
    p["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name": "todo", "kind": "scalar"}));
    let steps = p["steps"].as_array_mut().unwrap();
    let at = steps.len() - 1;
    steps.insert(at, json!({"id": "todo", "operator": "LinearGradient", "config": {"layer": "todo", "start": 0, "end": 0.01}}));
    let rows = run(&p).unwrap();
    let open = |x: usize, y: usize| rows[y].as_bytes()[x] == b'#';
    assert!(open(8, 3) && open(9, 3), "\n{}", rows.join("\n"));
    assert!(!open(8, 2) && !open(9, 4));
}

#[test]
fn add_is_checked_when_built() {
    let bad = |extra: Json| {
        let p = pipeline(0, carve(extra), json!([]));
        build(&p).err().unwrap()[0].message.clone()
    };
    let wall = json!({"type": "in_object", "layer": "salas", "part": "wall"});
    assert!(bad(json!({"add": [{"where": [wall], "cost": 3}]})).contains("only a `cheapest` path"));
    assert!(
        bad(json!({"shape": "cheapest", "add": [{"where": [wall], "cost": 0}]}))
            .contains("term 0: a cost of 0")
    );
    let e = bad(json!({"shape": "cheapest", "add": [{"field": "nada", "weight": 2}]}));
    assert!(e.contains("nada"), "{e}");
    let e = bad(json!({"shape": "cheapest", "add": [{"field": "x", "weight": 20000}]}));
    assert!(e.contains("not found") || e.contains("20000"), "{e}");
    let e = bad(json!({"shape": "cheapest", "add": [{"wehre": [], "cost": 3}]}));
    assert!(e.contains("\"field\": layer, \"weight\": w"), "{e}");
}

// to_cells: from loose spots to the nearest path

/// Spots on "outra", pinned with `at` at (x, 15) for each x; then a branch
/// from each to the nearest `chao`, after the corridor A–B along y 3.
fn branches(xs: &[i32]) -> Vec<String> {
    let mut p = pipeline(0, carve(json!({})), json!([]));
    let steps = p["steps"].as_array_mut().unwrap();
    for (k, &x) in xs.iter().enumerate() {
        steps.push(
            json!({"id": format!("ponta{k}"), "operator": "PlaceObjects", "config": {
            "layer": "outra", "kind": "ponta", "size": {"width": 1, "height": 1},
            "at": {"x": {"frac": 0, "offset": x}, "y": {"frac": 0, "offset": 15}}}}),
        );
    }
    steps.push(json!({"id": "ramos", "operator": "CarvePaths", "config": {
        "objects": "outra", "layer": "terreno", "tile": "chao", "shape": "cheapest",
        "to_cells": [{"type": "tile_is", "layer": "terreno", "tile": "chao"}]}}));
    run(&p).unwrap()
}

/// The `#` cells joined to (x, y) side by side.
fn joined(rows: &[String], x: usize, y: usize) -> usize {
    let open = |x: usize, y: usize| rows[y].as_bytes()[x] == b'#';
    let mut seen = vec![vec![false; 40]; 20];
    let mut todo = vec![(x, y)];
    seen[y][x] = true;
    let mut n = 0;
    while let Some((x, y)) = todo.pop() {
        n += 1;
        let mut go = |a: usize, b: usize| {
            if a < 40 && b < 20 && !seen[b][a] && open(a, b) {
                seen[b][a] = true;
                todo.push((a, b));
            }
        };
        go(x + 1, y);
        go(x.wrapping_sub(1), y);
        go(x, y + 1);
        go(x, y.wrapping_sub(1));
    }
    n
}

#[test]
fn a_branch_joins_the_spot_to_the_nearest_path() {
    let corridor = 10; // (4, 3) to (13, 3)
    let rows = branches(&[30]);
    let all: usize = rows.iter().map(|r| r.matches('#').count()).sum();
    // From (30, 15) to the corridor's end (13, 3): 17 + 12 steps.
    assert_eq!(all, corridor + 29, "\n{}", rows.join("\n"));
    assert_eq!(joined(&rows, 30, 15), all);
}

#[test]
fn a_later_branch_can_end_on_an_earlier_one() {
    let one: usize = branches(&[30]).iter().map(|r| r.matches('#').count()).sum();
    let rows = branches(&[30, 36]);
    let two: usize = rows.iter().map(|r| r.matches('#').count()).sum();
    // The second spot is 6 from the first branch, far from the corridor.
    assert_eq!(two - one, 6, "\n{}", rows.join("\n"));
    assert_eq!(joined(&rows, 36, 15), two);
}

#[test]
fn to_cells_is_checked_when_built_and_when_run() {
    let bad = |config: Json| {
        let mut p = pipeline(0, carve(json!({})), json!([]));
        p["steps"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id": "ramos", "operator": "CarvePaths", "config": config}));
        build(&p).err().unwrap()[0].message.clone()
    };
    let chao = json!([{"type": "tile_is", "layer": "terreno", "tile": "chao"}]);
    let e = bad(
        json!({"connections": "conexoes", "objects": "salas", "layer": "terreno", "tile": "chao",
                       "shape": "cheapest", "to_cells": chao}),
    );
    assert!(e.contains("not both"), "{e}");
    let e =
        bad(json!({"objects": "salas", "layer": "terreno", "tile": "chao", "shape": "cheapest"}));
    assert!(e.contains("say what the paths join"), "{e}");
    let e = bad(json!({"objects": "salas", "layer": "terreno", "tile": "chao", "to_cells": chao}));
    assert!(e.contains("only a `cheapest` path finds its way"), "{e}");
    // Nothing to reach: the step fails, for `attempts` to retry.
    let mut p = pipeline(0, carve(json!({})), json!([]));
    p["steps"].as_array_mut().unwrap().push(
        json!({"id": "ramos", "operator": "CarvePaths", "config": {
        "objects": "salas", "layer": "terreno", "tile": "chao", "shape": "cheapest",
        "to_cells": [{"type": "tile_is", "layer": "terreno", "tile": "empty"}]}}),
    );
    let e = run(&p).unwrap_err();
    assert!(e.contains("no cell where `to_cells` holds"), "{e}");
}
