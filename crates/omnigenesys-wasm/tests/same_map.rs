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

/// A 3D map through every operator that runs in 3D.
const VOLUME: &str = r#"{
  "version": 2, "seed": 99, "size": {"width": 40, "height": 24, "depth": 12},
  "layers": ["terrain"], "tiles": ["rock", "air"],
  "steps": [
    {"id": "base", "operator": "Fill", "config": {"layer": "terrain", "tile": "rock"}},
    {"id": "holes", "operator": "Scatter", "config": {"layer": "terrain", "tile": "air", "chance": 0.55}},
    {"id": "specks", "operator": "FillSmallRegions",
     "config": {"layer": "terrain", "region": "air", "fill": "rock", "connectivity": "orthogonal", "below": 6}},
    {"id": "one", "operator": "KeepLargestRegion",
     "config": {"layer": "terrain", "region": "air", "fill": "rock", "connectivity": "diagonal"}}
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
        for id in map.layer(&name).unwrap() {
            eat(&id.to_le_bytes());
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
    assert_eq!(fingerprint(CAVE), 4_926_128_203_489_471_246);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn volume() {
    assert_eq!(fingerprint(VOLUME), 5_762_542_940_651_055_300);
}
