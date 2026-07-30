//! Output format v2 (PLANNING §2.4): tile string table + RLE per layer +
//! object instances with bounding boxes + named markers. The rasterized
//! grid still exists (tilemaps need it); `objects`/`markers` are additive
//! so adapters stop flood-filling to reconstruct rectangles.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::coords::GridSize;
use crate::tile::TileId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MapOutput {
    pub version: u32,
    pub seed: u64,
    pub size: GridSize,
    /// String table: layer data stores indices into this vec.
    pub tiles: Vec<String>,
    pub layers: Vec<LayerOutput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<ObjectInstance>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub markers: Vec<Marker>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LayerOutput {
    pub name: String,
    pub encoding: Encoding,
    pub data: Vec<RleRun>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Encoding {
    Rle,
}

/// One RLE run: `[tile_id, count]` in JSON. A tuple struct with two fields
/// serializes as a two-element array — compact and self-describing enough.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RleRun(pub TileId, pub u32);

/// An instance an operator placed knowing its bounding box (PlaceStructures,
/// PlaceRoom, ...). Exported directly so adapters can spawn prefabs without
/// flood-filling the raster (fixes bug #6 of the Go version).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ObjectInstance {
    #[serde(rename = "type")]
    pub kind: String,
    pub x: i32,
    pub y: i32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub z: i32,
    pub w: u32,
    pub h: u32,
    /// Depth of the box; 1 for 2D objects.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub d: u32,
    pub layer: String,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub metadata: Map<String, Value>,
}

/// Named point: spawn, path entrance, POI — replaces the Go-era "magic
/// tile + adapter flag" pattern.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Marker {
    #[serde(rename = "type")]
    pub kind: String,
    pub x: i32,
    pub y: i32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub z: i32,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub metadata: Map<String, Value>,
}

fn is_zero(v: &i32) -> bool {
    *v == 0
}

fn one() -> u32 {
    1
}

fn is_one(v: &u32) -> bool {
    *v == 1
}

pub fn rle_encode(cells: &[TileId]) -> Vec<RleRun> {
    let mut runs: Vec<RleRun> = Vec::new();
    for &cell in cells {
        match runs.last_mut() {
            Some(RleRun(tile, count)) if *tile == cell => *count += 1,
            _ => runs.push(RleRun(cell, 1)),
        }
    }
    runs
}

pub fn rle_decode(runs: &[RleRun]) -> Vec<TileId> {
    let total: usize = runs.iter().map(|RleRun(_, count)| *count as usize).sum();
    let mut cells = Vec::with_capacity(total);
    for &RleRun(tile, count) in runs {
        cells.extend(std::iter::repeat_n(tile, count as usize));
    }
    cells
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rle_round_trip() {
        let cells: Vec<TileId> = vec![1, 1, 1, 2, 2, 0, 1];
        let runs = rle_encode(&cells);
        assert_eq!(runs, vec![RleRun(1, 3), RleRun(2, 2), RleRun(0, 1), RleRun(1, 1)]);
        assert_eq!(rle_decode(&runs), cells);
    }

    #[test]
    fn rle_empty() {
        assert!(rle_encode(&[]).is_empty());
        assert!(rle_decode(&[]).is_empty());
    }

    #[test]
    fn run_serializes_as_pair_array() {
        assert_eq!(serde_json::to_value(RleRun(1, 3200)).unwrap(), json!([1, 3200]));
    }

    /// The example output from PLANNING §2.4 must parse as-is.
    #[test]
    fn planning_example_parses() {
        let output: MapOutput = serde_json::from_value(json!({
            "version": 2,
            "seed": 512,
            "size": { "width": 80, "height": 50, "depth": 1 },
            "tiles": ["empty", "floor", "border", "path", "foliage"],
            "layers": [
                { "name": "terrain", "encoding": "rle", "data": [[1, 3200], [2, 160]] }
            ],
            "objects": [
                { "type": "zone_arena", "x": 12, "y": 30, "w": 9, "h": 9, "layer": "structures" }
            ],
            "markers": [
                { "type": "spawn", "x": 40, "y": 46 }
            ]
        }))
        .unwrap();

        assert_eq!(output.tiles.len(), 5);
        assert_eq!(output.layers[0].data[0], RleRun(1, 3200));
        assert_eq!(output.objects[0].kind, "zone_arena");
        assert_eq!(output.objects[0].d, 1, "object depth must default to 1");
        assert_eq!(output.markers[0].z, 0, "marker z must default to 0");

        // 2D round trip must not reintroduce 3D-only fields.
        let value = serde_json::to_value(&output).unwrap();
        assert!(value["objects"][0].get("z").is_none());
        assert!(value["objects"][0].get("d").is_none());
    }
}
