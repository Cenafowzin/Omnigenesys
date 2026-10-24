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
                }
            }
        }
    }
    h
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn island() {
    assert_eq!(fingerprint(ISLAND), 17_127_211_488_029_945_790);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn cave() {
    assert_eq!(fingerprint(CAVE), 9_229_285_801_968_616_189);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn volume() {
    assert_eq!(fingerprint(VOLUME), 16_705_279_568_056_867_519);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn island_from_noise() {
    assert_eq!(fingerprint(ISLAND_NOISE), 12_091_903_640_494_519_404);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn archipelago() {
    assert_eq!(fingerprint(ARCHIPELAGO), 3_463_316_865_257_611_296);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn isaac_floor() {
    assert_eq!(fingerprint(ISAAC), 2_971_879_752_968_153_115);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn classic_dungeon() {
    assert_eq!(fingerprint(DUNGEON), 926_442_635_239_489_141);
}
