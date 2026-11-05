//! The browser makes the same maps as the command line: this file runs
//! natively (`cargo test`) and compiled to wasm in Node
//! (`cargo test --target wasm32-unknown-unknown`, runner in
//! `.cargo/config.toml`), against the same hashes. If they ever disagree,
//! "same seed, same map" no longer holds between the two.

use omnigenesys_core::operator::Registry;
use omnigenesys_wasm::api;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen_test::wasm_bindgen_test;

const ISLAND: &str = include_str!("../../omnigenesys-core/tests/pipelines/island.json");
const CAVE: &str = include_str!("../../omnigenesys-core/tests/pipelines/cave.json");
const CAVERNA: &str = include_str!("../../omnigenesys-core/tests/pipelines/caverna.json");
const LOST_FIELDS: &str = include_str!("../../omnigenesys-core/tests/pipelines/lost_fields.json");
// Noise: floating point, the part most likely to drift between targets.
// The file includes every height as written (f32, shortest decimal).
const ISLAND_NOISE: &str = include_str!("../../omnigenesys-core/tests/pipelines/ilha.json");
const ARCHIPELAGO: &str = include_str!("../../omnigenesys-core/tests/pipelines/arquipelago.json");
// Objects: rooms and connections, with ids and typed props, in the file.
const ISAAC: &str = include_str!("../../omnigenesys-core/tests/pipelines/isaac.json");
const DUNGEON: &str = include_str!("../../omnigenesys-core/tests/pipelines/masmorra.json");

/// A 3D map through every operator that runs in 3D.
const VOLUME: &str = r#"{
  "version": 2, "seed": 99, "size": {"width": 40, "height": 24, "depth": 12},
  "layers": ["terrain", {"name": "size", "kind": "scalar"}, {"name": "rank", "kind": "scalar"}],
  "tiles": ["rock", "air"],
  "steps": [
    {"id": "base", "operator": "Fill", "config": {"layer": "terrain", "tile": "rock"}},
    {"id": "holes", "operator": "Scatter", "config": {"layer": "terrain", "tile": "air", "chance": 0.55}},
    {"id": "sizes", "operator": "RegionSize",
     "config": {"from": "terrain", "region": "air", "connectivity": "orthogonal", "layer": "size", "cap": 6}},
    {"id": "specks", "operator": "Fill", "config": {"layer": "terrain", "tile": "rock"},
     "conditions": [{"type": "scalar_in_range", "layer": "size", "min": 1, "max": 6}]},
    {"id": "ranks", "operator": "RegionSize",
     "config": {"from": "terrain", "region": "air", "connectivity": "diagonal", "layer": "size", "rank": "rank"}},
    {"id": "one", "operator": "Fill", "config": {"layer": "terrain", "tile": "rock"},
     "conditions": [{"type": "scalar_in_range", "layer": "rank", "min": 2}]}
  ]
}"#;

/// FNV-1a over the map file and every layer's ids (little-endian).
fn fingerprint(text: &str) -> u64 {
    let map = api::generate(&Registry::with_builtins(), text, &[], None, None).unwrap();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |bytes: &[u8]| {
        for &b in bytes {
            h = (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    eat(map.to_json().unwrap().as_bytes());
    for name in map.layers() {
        match map.layer(&name) {
            Some(ids) => ids.iter().for_each(|id| eat(&id.to_le_bytes())),
            // A scalar layer: its exact bits, not just its text.
            None => {
                if let Some(values) = map.scalar(&name) {
                    values.iter().for_each(|v| eat(&v.to_bits().to_le_bytes()));
                } else if let Some(values) = map.index(&name) {
                    values.iter().for_each(|v| eat(&v.to_le_bytes()));
                }
            }
        }
    }
    h
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn island() {
    assert_eq!(fingerprint(ISLAND), 16_467_742_384_463_902_516);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn cave() {
    assert_eq!(fingerprint(CAVE), 17_425_259_260_481_602_690);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn cave_recipe() {
    assert_eq!(fingerprint(CAVERNA), 12_851_837_730_035_771_666);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn lost_fields() {
    assert_eq!(fingerprint(LOST_FIELDS), 13_101_020_537_107_846_001);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn volume() {
    assert_eq!(fingerprint(VOLUME), 4_915_384_777_481_700_970);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn island_from_noise() {
    assert_eq!(fingerprint(ISLAND_NOISE), 5_884_310_606_179_055_621);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn archipelago() {
    assert_eq!(fingerprint(ARCHIPELAGO), 2_723_703_261_892_711_014);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn isaac_floor() {
    assert_eq!(fingerprint(ISAAC), 9_768_443_335_494_191_360);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn classic_dungeon() {
    assert_eq!(fingerprint(DUNGEON), 542_386_212_072_927_256);
}

/// A deep floor (stage 6: 20 rooms, growth reseeded from the start) made
/// after failed attempts: every attempt's randomness, the same on both.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn retried_attempts() {
    let params = [("estagio".to_string(), "6".to_string())];
    let map = api::generate(&Registry::with_builtins(), ISAAC, &params, Some(2), None).unwrap();
    let failures: serde_json::Value = serde_json::from_str(&map.failures()).unwrap();
    assert_eq!(
        (map.attempt(), failures.as_array().unwrap().len()),
        (ATTEMPT, ATTEMPT as usize)
    );
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in map.to_json().unwrap().bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    assert_eq!(h, RETRIED);
}

const ATTEMPT: u32 = 5;
const RETRIED: u64 = 14_681_728_831_627_922_317;

/// Expressions over params: the same values to the last bit on both. JSON
/// writes each f64 in its shortest exact form, so equal text is equal bits.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn expressions() {
    let text = r#"{
      "version": 2, "size": {"width": 4, "height": 4}, "layers": ["t"], "tiles": ["a"],
      "params": {
        "stage": 3,
        "rooms": {"type": "integer", "expr": "min(20, 5 + floor(stage * 10 / 3))"},
        "third": {"type": "number", "expr": "1 / 3 + sqrt(2) * pow(1.1, stage + 7)"},
        "mix": {"type": "number", "expr": "lerp(0.1, 0.7, third / 10) - div(-7, 2) + mod(-7, 2)"},
        "half": {"type": "integer", "expr": "round(rooms / 2) + trunc(-2.5) + ceil(0.1)"}
      },
      "steps": [{"id": "s", "operator": "Fill", "config": {"layer": "t", "tile": "a"},
        "conditions": [{"type": "count", "layer": "t", "tile": "a", "distance": 1,
          "min": {"$param": "half"}, "max": {"$expr": "rooms + floor(mix + third)"}}]}]
    }"#;
    let out: serde_json::Value =
        serde_json::from_str(&api::validate(&Registry::with_builtins(), text, &[])).unwrap();
    assert_eq!(out["ok"], true, "{out}");
    let values: Vec<String> = out["params"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["value"].to_string())
        .collect();
    // Checked against Python, the same IEEE operations in the same order.
    assert_eq!(
        values,
        ["3", "15", "4.001439097709713", "5.3400863458625825", "7"]
    );
}
