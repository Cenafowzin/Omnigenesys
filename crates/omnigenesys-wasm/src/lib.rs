//! Omnigenesys in the browser (PLANNING §2.10). Everything runs on the
//! visitor's machine: the server only hands out files.
//!
//! ```js
//! import init, { validate, generate } from "./omnigenesys_wasm.js";
//! await init();
//! const check = JSON.parse(validate(text, "{}"));   // {ok, errors, warnings, steps}
//! const map = generate(text, "{}", 512n, undefined); // throws {errors} on failure
//! const [w, h, d] = map.size();
//! const cells = map.composite();                     // Uint16Array, x fastest
//! ```
//!
//! Run it in a Web Worker for big maps: a call blocks the thread it is on.

use std::sync::OnceLock;

use omnigenesys_core::operator::Registry;
use wasm_bindgen::prelude::*;

pub mod api;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
fn start() {
    // A Rust panic shows in the console with its message, not as
    // "unreachable executed".
    console_error_panic_hook::set_once();
}

fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(Registry::with_builtins)
}

/// The JSON Schema of a pipeline file (text). Same for the whole session.
#[wasm_bindgen]
pub fn schema() -> String {
    static SCHEMA: OnceLock<String> = OnceLock::new();
    SCHEMA.get_or_init(|| api::schema(registry())).clone()
}

/// Every operator with the schema of its config (text):
/// `{"operators": [{name, dimensions, scope, config}], "definitions"}`.
#[wasm_bindgen]
pub fn operators() -> String {
    static OPERATORS: OnceLock<String> = OnceLock::new();
    OPERATORS.get_or_init(|| api::operators(registry())).clone()
}

/// Checks a pipeline (text) with param values (`{"name": value}`, text).
/// Never throws for a bad pipeline: the answer says what is wrong.
#[wasm_bindgen]
pub fn validate(text: &str, params: &str) -> Result<String, JsError> {
    let params = api::params(params).map_err(|e| JsError::new(&e))?;
    Ok(api::validate(registry(), text, &params))
}

/// Runs a pipeline. `seed` (a BigInt) and `salt` override the file's.
/// Throws `{errors: [...]}` when the pipeline cannot run.
#[wasm_bindgen]
pub fn generate(
    text: &str,
    params: &str,
    seed: Option<u64>,
    salt: Option<String>,
) -> Result<Map, JsValue> {
    let params = api::params(params).map_err(|e| JsValue::from(JsError::new(&e)))?;
    api::generate(registry(), text, &params, seed, salt.as_deref())
        .map(Map)
        .map_err(|json| js_sys::JSON::parse(&json).unwrap_or_else(|_| JsValue::from_str(&json)))
}

/// A generated map. Arrays come out as copies, safe to keep and to send
/// to another thread. `GeneratedMap` in JavaScript, where `Map` is taken.
#[wasm_bindgen(js_name = GeneratedMap)]
pub struct Map(api::Map);

#[wasm_bindgen(js_class = GeneratedMap)]
impl Map {
    /// `[width, height, depth]`.
    pub fn size(&self) -> Vec<u32> {
        let (w, h, d) = self.0.size();
        vec![w, h, d]
    }

    pub fn seed(&self) -> u64 {
        self.0.seed()
    }

    /// Tile names by id: entry 0 is `empty`.
    pub fn tiles(&self) -> Vec<String> {
        self.0.tiles()
    }

    pub fn layers(&self) -> Vec<String> {
        self.0.layers()
    }

    /// One layer's tile ids, x fastest, then y, then z.
    pub fn layer(&self, name: &str) -> Result<Vec<u16>, JsError> {
        self.0
            .layer(name)
            .ok_or_else(|| JsError::new(&format!("no layer '{name}'")))
    }

    /// Every layer drawn over the one before it.
    pub fn composite(&self) -> Vec<u16> {
        self.0.composite()
    }

    /// The map as output v2 JSON (text), the file engines read.
    #[wasm_bindgen(js_name = toJson)]
    pub fn to_json(&self) -> Result<String, JsError> {
        self.0.to_json().map_err(|e| JsError::new(&e))
    }
}
