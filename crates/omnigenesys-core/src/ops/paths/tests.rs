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
