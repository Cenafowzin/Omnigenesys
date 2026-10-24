use serde_json::{Value, json};

use super::*;
use crate::spec::ParamValue;
use crate::{
    bounds::Bounds,
    coords::{Coord, GridSize},
    layer::LayerKind,
};

fn tiles() -> TileRegistry {
    TileRegistry::new(["floor", "wall"]).unwrap()
}

/// A 4×2 map: floor with two walls at the end.
fn small() -> Grid {
    let mut g = Grid::new(Bounds::new(Coord::ZERO, GridSize::new(4, 2, 1)));
    let t = g.add_layer("terrain", LayerKind::Tiles).unwrap();
    let layer = g.tiles_mut(t).unwrap();
    for c in Bounds::new(Coord::ZERO, GridSize::new(4, 2, 1)).iter() {
        layer.set(c, TileId(1)).unwrap();
    }
    layer.set(Coord::new(2, 1, 0), TileId(2)).unwrap();
    layer.set(Coord::new(3, 1, 0), TileId(2)).unwrap();
    g
}

const SEED: Provenance = Provenance {
    seed: 512,
    salted: false,
    params: Vec::new(),
};

fn write(grid: &Grid, provenance: Provenance, encoding: Encoding) -> String {
    let mut out = Vec::new();
    write_json(&mut out, grid, &tiles(), &provenance, encoding).unwrap();
    String::from_utf8(out).unwrap()
}

/// Reads the output back: every layer's cells as plain ids.
fn decode(text: &str) -> (Value, Vec<(String, Vec<u16>)>) {
    let v: Value = serde_json::from_str(text).unwrap();
    let layers = v["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            let data = l["data"].as_array().unwrap();
            let cells = match l["encoding"].as_str().unwrap() {
                "dense" => data.iter().map(|t| t.as_u64().unwrap() as u16).collect(),
                "rle" => data
                    .iter()
                    .flat_map(|run| {
                        let n = run[1].as_u64().unwrap() as usize;
                        std::iter::repeat_n(run[0].as_u64().unwrap() as u16, n)
                    })
                    .collect(),
                other => panic!("unknown encoding {other}"),
            };
            (l["name"].as_str().unwrap().to_string(), cells)
        })
        .collect();
    (v, layers)
}

#[test]
fn writes_the_documented_format_byte_for_byte() {
    let expected = format!(
        r#"{{
  "format": "omnigenesys-map",
  "version": 2,
  "engine_version": "{ENGINE_VERSION}",
  "seed": "512",
  "salted": false,
  "params": {{}},
  "origin": [0, 0, 0],
  "size": [4, 2, 1],
  "tiles": ["empty", "floor", "wall"],
  "layers": [
    {{"name": "terrain", "kind": "tiles", "encoding": "rle", "data": [[1,6],[2,2]]}}
  ],
  "objects": [],
  "markers": []
}}
"#
    );
    assert_eq!(write(&small(), SEED, Encoding::Auto), expected);
}

#[test]
fn dense_writes_one_id_per_cell() {
    let text = write(&small(), SEED, Encoding::Dense);
    assert!(
        text.contains(r#""encoding": "dense", "data": [1,1,1,1,1,1,2,2]}"#),
        "{text}"
    );
}

#[test]
fn every_encoding_reads_back_as_the_grid() {
    let grid = small();
    let expected: Vec<u16> = grid
        .tiles(grid.layer_id("terrain").unwrap())
        .unwrap()
        .cells()
        .map(|t| t.0)
        .collect();
    for encoding in [Encoding::Auto, Encoding::Rle, Encoding::Dense] {
        let (_, layers) = decode(&write(&grid, SEED, encoding));
        assert_eq!(layers, [("terrain".to_string(), expected.clone())]);
    }
}

#[test]
fn auto_picks_dense_for_noisy_layers() {
    let mut g = Grid::new(Bounds::new(Coord::ZERO, GridSize::new(4, 1, 1)));
    let t = g.add_layer("noise", LayerKind::Tiles).unwrap();
    let flat = g.add_layer("flat", LayerKind::Tiles).unwrap();
    for (x, tile) in [1, 2, 1, 2].into_iter().enumerate() {
        g.tiles_mut(t)
            .unwrap()
            .set(Coord::new(x as i32, 0, 0), TileId(tile))
            .unwrap();
    }
    g.tiles_mut(flat)
        .unwrap()
        .set(Coord::ZERO, TileId(1))
        .unwrap();
    let (v, _) = decode(&write(&g, SEED, Encoding::Auto));
    // "1,2,1,2" against "[1,1],[2,1],[1,1],[2,1]".
    assert_eq!(v["layers"][0]["encoding"], "dense");
    // "1,0,0,0" (7 bytes) against "[1,1],[0,3]" (11).
    assert_eq!(v["layers"][1]["encoding"], "dense");
    assert_eq!(v["layers"][1]["data"], json!([1, 0, 0, 0]));
}

#[test]
fn a_3d_map_goes_x_then_y_then_z() {
    let size = GridSize::new(2, 2, 2);
    let origin = Coord::new(-1, 5, 10);
    let mut g = Grid::new(Bounds::new(origin, size));
    let t = g.add_layer("terrain", LayerKind::Tiles).unwrap();
    // Wall only at local (1, 0, 1): index 1 + 2 * (0 + 2 * 1) = 5.
    g.tiles_mut(t)
        .unwrap()
        .set(origin + Coord::new(1, 0, 1), TileId(2))
        .unwrap();
    let (v, layers) = decode(&write(&g, SEED, Encoding::Dense));
    assert_eq!(layers[0].1, [0, 0, 0, 0, 0, 2, 0, 0]);
    assert_eq!(v["origin"], json!([-1, 5, 10]));
    assert_eq!(v["size"], json!([2, 2, 2]));
}

#[test]
fn the_seed_is_a_string_and_the_salt_is_never_written() {
    let p = Provenance {
        seed: u64::MAX,
        salted: true,
        params: Vec::new(),
    };
    let (v, _) = decode(&write(&small(), p, Encoding::Auto));
    assert_eq!(v["seed"], "18446744073709551615");
    assert_eq!(v["salted"], true);
    assert!(v.get("salt").is_none());
}

#[test]
fn a_map_without_layers_is_still_valid_json() {
    let g = Grid::new(Bounds::new(Coord::ZERO, GridSize::new(1, 1, 1)));
    let (v, layers) = decode(&write(&g, SEED, Encoding::Auto));
    assert!(layers.is_empty());
    assert_eq!(v["layers"], json!([]));
}

#[test]
fn names_are_escaped() {
    let mut g = Grid::new(Bounds::new(Coord::ZERO, GridSize::new(1, 1, 1)));
    g.add_layer("a \"b\" \\ c\n", LayerKind::Tiles).unwrap();
    let mut out = Vec::new();
    let odd = TileRegistry::new(["ção", "tab\there"]).unwrap();
    write_json(&mut out, &g, &odd, &SEED, Encoding::Auto).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains(r#""name": "a \"b\" \\ c\u000a""#), "{text}");
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["layers"][0]["name"], "a \"b\" \\ c\n");
    assert_eq!(v["tiles"], json!(["empty", "ção", "tab\there"]));
}

#[test]
fn an_unregistered_tile_fails_before_writing_anything() {
    let mut g = small();
    let t = g.add_layer("objects", LayerKind::Tiles).unwrap();
    g.tiles_mut(t).unwrap().set(Coord::ZERO, TileId(9)).unwrap();
    let mut out = Vec::new();
    let e = write_json(&mut out, &g, &tiles(), &SEED, Encoding::Auto).unwrap_err();
    assert_eq!(
        e.to_string(),
        "layer 'objects' has tile id 9, but only 3 tiles are registered"
    );
    assert!(out.is_empty(), "nothing written");
}

#[test]
fn scan_counts_runs_and_the_largest_id() {
    let ids = |v: &[u16]| v.iter().map(|&t| TileId(t)).collect::<Vec<_>>();
    assert_eq!(scan(&[]), (0, None));
    assert_eq!(scan(&ids(&[3])), (1, Some(TileId(3))));
    assert_eq!(scan(&ids(&[1, 1, 4, 4, 1])), (3, Some(TileId(4))));
}

#[test]
fn noisy_layers_are_settled_without_counting() {
    let t = |n| Some(TileId(n));
    // 6 bytes a run at least, 2 bytes a cell at most: runs >= cells / 3.
    assert!(!rle_may_win(9, 3, t(1)));
    assert!(rle_may_win(9, 2, t(1)));
    // Two-digit ids: 3 bytes a cell, so runs >= cells / 2.
    assert!(rle_may_win(10, 4, t(12)));
    assert!(!rle_may_win(10, 5, t(12)));
    assert!(!rle_may_win(0, 0, None));
}

#[test]
fn a_tie_goes_to_dense() {
    // "1,1,12,12,12" and "[1,2],[12,3]": 12 bytes each.
    assert_eq!(
        data_len(&[1, 1, 12, 12, 12], Encoding::Auto),
        (12, "dense".to_string())
    );
    // One more 12: dense grows by 3 bytes, rle not at all.
    assert_eq!(
        data_len(&[1, 1, 12, 12, 12, 12], Encoding::Auto),
        (12, "rle".to_string())
    );
}

/// Length of the `data` array as written, and the encoding used, for one
/// layer of `pattern`.
fn data_len(pattern: &[u16], encoding: Encoding) -> (usize, String) {
    let size = GridSize::new(pattern.len() as u32, 1, 1);
    let mut g = Grid::new(Bounds::new(Coord::ZERO, size));
    let t = g.add_layer("l", LayerKind::Tiles).unwrap();
    for (x, &tile) in pattern.iter().enumerate() {
        let c = Coord::new(x as i32, 0, 0);
        g.tiles_mut(t).unwrap().set(c, TileId(tile)).unwrap();
    }
    let mut out = Vec::new();
    let many = TileRegistry::new((1..=12).map(|i| format!("t{i}"))).unwrap();
    write_json(&mut out, &g, &many, &SEED, encoding).unwrap();
    let text = String::from_utf8(out).unwrap();
    let start = text.find("\"data\": [").unwrap() + "\"data\": [".len();
    let end = text[start..].find("]}").unwrap() + start;
    let chosen = if text.contains("\"encoding\": \"rle\"") {
        "rle"
    } else {
        "dense"
    };
    (end - start, chosen.to_string())
}

#[test]
fn auto_is_never_larger_than_either_encoding() {
    // Every 0/1 pattern of 10 cells, plus long runs and two-digit ids.
    let mut patterns: Vec<Vec<u16>> = (0..1u32 << 10)
        .map(|bits| (0..10).map(|i| (bits >> i & 1) as u16).collect())
        .collect();
    patterns.push([vec![12; 40], vec![1; 3]].concat());
    patterns.push(vec![2; 1000]);
    patterns.push(vec![1, 1, 12, 12, 12]);
    for p in &patterns {
        let (auto, chosen) = data_len(p, Encoding::Auto);
        let (rle, _) = data_len(p, Encoding::Rle);
        let (dense, _) = data_len(p, Encoding::Dense);
        assert_eq!(auto, rle.min(dense), "{p:?}");
        if rle == dense {
            assert_eq!(chosen, "dense", "a tie goes to dense: {p:?}");
        }
    }
}

#[test]
fn output_larger_than_a_block_is_complete() {
    // 300×300 cells of three tiles in a pattern: several 64 KiB blocks.
    let size = GridSize::new(300, 300, 1);
    let mut g = Grid::new(Bounds::new(Coord::ZERO, size));
    let t = g.add_layer("terrain", LayerKind::Tiles).unwrap();
    let layer = g.tiles_mut(t).unwrap();
    for (i, c) in Bounds::new(Coord::ZERO, size).iter().enumerate() {
        layer.set(c, TileId((i * 7 % 3) as u16)).unwrap();
    }
    let text = write(&g, SEED, Encoding::Dense);
    assert!(text.len() > 2 * BLOCK);
    let (_, layers) = decode(&text);
    let expected: Vec<u16> = (0..300 * 300).map(|i| (i * 7 % 3) as u16).collect();
    assert_eq!(layers[0].1, expected);
}

#[test]
fn params_are_written_with_their_type_and_read_back_exactly() {
    let values = [
        ("chance", ParamValue::Number(0.3)),
        ("whole", ParamValue::Number(1.0)),
        ("tiny", ParamValue::Number(1e-7)),
        ("third", ParamValue::Number(1.0 / 3.0)),
        ("passes", ParamValue::Integer(-4)),
        ("rivers", ParamValue::Boolean(true)),
        ("biome", ParamValue::String("des\"ert".to_string())),
    ];
    let p = Provenance {
        seed: 1,
        salted: false,
        params: values
            .iter()
            .map(|(n, v)| (n.to_string(), v.clone()))
            .collect(),
    };
    let text = write(&small(), p, Encoding::Auto);
    assert!(
        text.contains(
            r#""params": {"chance": 0.3, "whole": 1.0, "tiny": 0.0000001, "third": 0.3333333333333333, "passes": -4, "rivers": true, "biome": "des\"ert"},"#
        ),
        "{text}"
    );
    let (v, _) = decode(&text);
    assert_eq!(
        v["params"]["third"].as_f64(),
        Some(1.0 / 3.0),
        "reads back as the same f64"
    );
    assert_eq!(v["params"]["tiny"].as_f64(), Some(1e-7));
    assert!(
        v["params"]["whole"].is_f64(),
        "a number param stays a float"
    );
    assert!(v["params"]["passes"].is_i64());
}

#[test]
fn a_non_finite_param_fails_before_writing() {
    let p = Provenance {
        seed: 1,
        salted: false,
        params: vec![("x".to_string(), ParamValue::Number(f64::NAN))],
    };
    let mut out = Vec::new();
    let e = write_json(&mut out, &small(), &tiles(), &p, Encoding::Auto).unwrap_err();
    assert_eq!(e.to_string(), "param 'x' is NaN, which JSON cannot hold");
    assert!(out.is_empty());
}
