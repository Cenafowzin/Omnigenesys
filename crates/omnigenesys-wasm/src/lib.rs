//! Omnigenesys in the browser (PLANNING §2.10). Everything runs on the
//! visitor's machine: the server only hands out files.
//!
//! ```js
//! import init, { validate, start, generate } from "./omnigenesys_wasm.js";
//! await init();
//! const check = JSON.parse(validate(text, "{}"));  // {ok, errors, warnings, steps}
//!
//! const g = start(text, "{}", 512n);                // throws {errors}
//! while (g.next()) {                                // one step; throws {errors}
//!   thumbnails.push(g.composite());                 // the map after it
//! }
//! const file = g.toJson();
//! // or, all at once: const g = generate(text, "{}", 512n);
//! ```
//!
//! Run it in a Web Worker for big maps: a call blocks the thread it is on.

use std::sync::OnceLock;

use omnigenesys_core::operator::Registry;
use wasm_bindgen::prelude::*;

pub mod api;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
fn start_up() {
    // A Rust panic shows in the console with its message, not as
    // "unreachable executed".
    console_error_panic_hook::set_once();
}

fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(Registry::with_builtins)
}

/// `{errors: [...]}` as a JavaScript object, from its JSON.
fn errors(json: &str) -> JsValue {
    js_sys::JSON::parse(json).unwrap_or_else(|_| JsValue::from_str(json))
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

/// Builds a pipeline, ready to run step by step with `next()`. `seed` (a
/// BigInt) and `salt` override the file's. Throws `{errors: [...]}` when it
/// cannot be built.
#[wasm_bindgen]
pub fn start(
    text: &str,
    params: &str,
    seed: Option<u64>,
    salt: Option<String>,
) -> Result<Generation, JsValue> {
    let params = api::params(params).map_err(|e| JsValue::from(JsError::new(&e)))?;
    api::start(registry(), text, &params, seed, salt.as_deref())
        .map(Generation)
        .map_err(|e| errors(&e))
}

/// `start`, then every step.
#[wasm_bindgen]
pub fn generate(
    text: &str,
    params: &str,
    seed: Option<u64>,
    salt: Option<String>,
) -> Result<Generation, JsValue> {
    let params = api::params(params).map_err(|e| JsValue::from(JsError::new(&e)))?;
    api::generate(registry(), text, &params, seed, salt.as_deref())
        .map(Generation)
        .map_err(|e| errors(&e))
}

/// A map being made. The map getters show it as the steps run so far left
/// it. Arrays come out as copies, safe to keep and to send to another
/// thread.
#[wasm_bindgen]
pub struct Generation(api::Generation);

#[wasm_bindgen]
impl Generation {
    /// Runs the next step; `false` when all have run. Throws `{errors}`
    /// pointing at a step that fails.
    #[wasm_bindgen(js_name = next)]
    pub fn run_next(&mut self) -> Result<bool, JsValue> {
        self.0.run_next().map_err(|e| errors(&e))
    }

    /// Steps that run (the enabled ones).
    pub fn steps(&self) -> usize {
        self.0.steps()
    }

    /// Steps already run.
    pub fn done(&self) -> usize {
        self.0.done()
    }

    /// The id of the `i`-th step that runs.
    #[wasm_bindgen(js_name = stepId)]
    pub fn step_id(&self, i: usize) -> Option<String> {
        self.0.step_id(i).map(str::to_string)
    }

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

    /// Each layer's kind, in the order of `layers()`: "tiles" or "scalar".
    pub fn kinds(&self) -> Vec<String> {
        self.0.kinds()
    }

    /// One tile layer's ids, x fastest, then y, then z.
    pub fn layer(&self, name: &str) -> Result<Vec<u16>, JsError> {
        self.0
            .layer(name)
            .ok_or_else(|| JsError::new(&format!("no tile layer '{name}'")))
    }

    /// One scalar layer's values, x fastest, then y, then z.
    pub fn scalar(&self, name: &str) -> Result<Vec<f32>, JsError> {
        self.0
            .scalar(name)
            .ok_or_else(|| JsError::new(&format!("no scalar layer '{name}'")))
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
