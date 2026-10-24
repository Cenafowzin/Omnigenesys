//! What the web app asks of the engine, in plain Rust: the wasm-bindgen
//! layer in `lib.rs` only moves it across, so all of this is tested
//! natively too.
//!
//! JSON goes out as text (the app calls `JSON.parse`): serde_json is in the
//! binary anyway, and the download needs the text. Positions in diagnostics
//! are UTF-16 offsets, the unit of JavaScript strings and of web editors.

use omnigenesys_core::{
    access::Reach,
    export::{self, Encoding, Provenance},
    grid::Grid,
    operator::{Dimensions, Registry, Scope},
    spec::{self, Built, Diagnostic, PipelineSpec, Span},
    tile::{TileId, TileRegistry},
};
use schemars::generate::SchemaSettings;
use serde_json::{Map as JsonMap, Value, json};

/// The JSON Schema of a whole pipeline file, for the editor.
pub fn schema(registry: &Registry) -> String {
    spec::schema(registry).to_string()
}

/// Every operator: name, dimensions, scope and the schema of its `config`,
/// for forms. The schemas share `definitions` (conditions, tile lists), and
/// are plain: a form fills values, it does not need `{"$param": ...}`.
pub fn operators(registry: &Registry) -> String {
    let mut generator = SchemaSettings::draft07().into_generator();
    let mut list = Vec::new();
    for name in registry.names() {
        let Some(entry) = registry.get(name) else {
            continue;
        };
        list.push(json!({
            "name": name,
            "dimensions": match entry.dimensions {
                Dimensions::TwoD => "2d",
                Dimensions::ThreeD => "3d",
                Dimensions::Any => "any",
            },
            "scope": match entry.scope {
                Scope::Local => "local",
                Scope::Global => "global",
            },
            "config": (entry.schema)(&mut generator).to_value(),
        }));
    }
    let definitions: JsonMap<String, Value> =
        generator.take_definitions(true).into_iter().collect();
    json!({"operators": list, "definitions": definitions}).to_string()
}

/// Checks a pipeline without running it. Always answers (problems are
/// data, not failures): `{"ok", "errors", "warnings", "steps"}`, `steps`
/// only when it builds, with what each step reads and writes, for the
/// graph view.
pub fn validate(registry: &Registry, text: &str, params: &[(String, String)]) -> String {
    let parsed = match PipelineSpec::parse_with(text, params) {
        Ok(p) => p,
        Err(errors) => return report(text, &errors, &[], None),
    };
    match parsed.spec.build(registry) {
        Ok(built) => report(text, &[], &parsed.warnings, Some((&parsed.spec, &built))),
        Err(errors) => report(text, &errors, &parsed.warnings, None),
    }
}

fn report(
    text: &str,
    errors: &[Diagnostic],
    warnings: &[Diagnostic],
    built: Option<(&PipelineSpec, &Built)>,
) -> String {
    let list = |ds: &[Diagnostic]| ds.iter().map(|d| diagnostic(text, d)).collect::<Vec<_>>();
    let mut out = json!({
        "ok": errors.is_empty(),
        "errors": list(errors),
        "warnings": list(warnings),
    });
    if let Some((spec, built)) = built {
        out["steps"] = steps(spec, built);
    }
    out.to_string()
}

fn diagnostic(text: &str, d: &Diagnostic) -> Value {
    let note = d
        .note
        .as_ref()
        .map(|n| json!({"message": n.message, "pointer": n.pointer, "span": span(text, n.span)}));
    json!({
        "message": d.message,
        "step_id": d.step_id,
        "pointer": d.pointer,
        "span": span(text, d.span),
        "note": note,
    })
}

/// `start`/`end` in UTF-16 code units; line and column as the parser gave
/// them (1-based, column in characters).
fn span(text: &str, span: Option<Span>) -> Value {
    let Some(s) = span else {
        return Value::Null;
    };
    let utf16 = |byte: usize| text.get(..byte).map_or(0, |t| t.encode_utf16().count());
    json!({"start": utf16(s.start), "end": utf16(s.end), "line": s.line, "column": s.column})
}

/// Per enabled step: id, operator, and per layer what it reads (how far
/// around each cell) and whether it writes.
fn steps(spec: &PipelineSpec, built: &Built) -> Value {
    let reach = |r: Reach| json!({"below": r.below, "above": r.above});
    let list: Vec<Value> = spec
        .steps
        .iter()
        .filter(|s| s.enabled)
        .map(|s| {
            let layers: Vec<Value> = built
                .pipeline
                .access(&s.id)
                .map(|a| {
                    a.layers()
                        .iter()
                        .map(|l| {
                            json!({
                                "layer": built.grid.layer_name(l.layer),
                                "reads": a.read_reach(l.layer).map(reach),
                                "writes": l.write.is_some(),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            json!({"id": s.id, "operator": s.operator, "layers": layers})
        })
        .collect();
    Value::Array(list)
}

/// A map made from a pipeline: the grid, its tiles, and how it was made.
pub struct Map {
    grid: Grid,
    tiles: TileRegistry,
    provenance: Provenance,
}

/// Runs a pipeline. `seed` and `salt` override the file's; a seed is
/// required from one of the two (the app draws one when the file has
/// none, so it can show which). On failure, the JSON of
/// `{"errors": [...]}`, in the shape of [`validate`]'s.
pub fn generate(
    registry: &Registry,
    text: &str,
    params: &[(String, String)],
    seed: Option<u64>,
    salt: Option<&str>,
) -> Result<Map, String> {
    let fail = |errors: &[Diagnostic]| {
        let list: Vec<Value> = errors.iter().map(|d| diagnostic(text, d)).collect();
        json!({ "errors": list }).to_string()
    };
    let parsed = PipelineSpec::parse_with(text, params).map_err(|e| fail(&e))?;
    let spec = parsed.spec;
    let Built {
        mut grid,
        tiles,
        pipeline,
    } = spec.build(registry).map_err(|e| fail(&e))?;
    let message = |pointer: &str, step_id: Option<&str>, message: String| {
        json!({"errors": [{
            "message": message, "step_id": step_id, "pointer": pointer,
            "span": null, "note": null,
        }]})
        .to_string()
    };
    let Some(seed) = seed.or(spec.seed) else {
        return Err(message(
            "/seed",
            None,
            "the pipeline has no seed: give one".to_string(),
        ));
    };
    let salt = salt.map_or_else(|| spec.salt.clone(), str::to_string);
    pipeline.run(&mut grid, &salt, seed).map_err(|e| {
        let at = spec
            .steps
            .iter()
            .position(|s| s.id == e.step_id)
            .map_or_else(String::new, |i| format!("/steps/{i}"));
        message(&at, Some(&e.step_id), e.error.to_string())
    })?;
    let provenance = spec.provenance(seed, &salt);
    Ok(Map {
        grid,
        tiles,
        provenance,
    })
}

impl Map {
    pub fn size(&self) -> (u32, u32, u32) {
        let s = self.grid.bounds().size;
        (s.width, s.height, s.depth)
    }

    pub fn seed(&self) -> u64 {
        self.provenance.seed
    }

    /// Tile names by id: entry 0 is `empty`.
    pub fn tiles(&self) -> Vec<String> {
        self.tiles.names().map(str::to_string).collect()
    }

    /// Layer names, in the order they were declared.
    pub fn layers(&self) -> Vec<String> {
        self.grid.names().map(str::to_string).collect()
    }

    /// One layer's tile ids, x fastest, then y, then z. `None` for a name
    /// the map does not have.
    pub fn layer(&self, name: &str) -> Option<Vec<u16>> {
        let id = self.grid.layer_id(name).ok()?;
        let layer = self.grid.tiles(id).ok()?;
        Some(layer.as_slice().iter().map(|t| t.index()).collect())
    }

    /// Every layer drawn over the one before it: for each cell, the tile of
    /// the last layer that is not empty there (objects over terrain), as
    /// the CLI's preview draws it.
    pub fn composite(&self) -> Vec<u16> {
        let mut out = vec![TileId::EMPTY.index(); self.grid.bounds().size.len()];
        for name in self.grid.names() {
            let Some(layer) = self.layer(name) else {
                continue;
            };
            for (o, t) in out.iter_mut().zip(layer) {
                if t != TileId::EMPTY.index() {
                    *o = t;
                }
            }
        }
        out
    }

    /// The map as output v2 JSON, the file engines read.
    pub fn to_json(&self) -> Result<String, String> {
        let mut out = Vec::new();
        export::write_json(
            &mut out,
            &self.grid,
            &self.tiles,
            &self.provenance,
            Encoding::Auto,
        )
        .map_err(|e| e.to_string())?;
        String::from_utf8(out).map_err(|e| e.to_string())
    }
}

/// `{"name": value}` from the app to the `NAME=VALUE` overrides the parser
/// reads by each param's declared type.
pub fn params(json: &str) -> Result<Vec<(String, String)>, String> {
    if json.trim().is_empty() {
        return Ok(Vec::new());
    }
    let map: JsonMap<String, Value> =
        serde_json::from_str(json).map_err(|e| format!("params: {e}"))?;
    Ok(map
        .into_iter()
        .map(|(name, value)| {
            let text = match value {
                Value::String(s) => s,
                other => other.to_string(),
            };
            (name, text)
        })
        .collect())
}

#[cfg(test)]
mod tests;
