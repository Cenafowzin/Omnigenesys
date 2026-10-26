//! What the web app asks of the engine, in plain Rust: the wasm-bindgen
//! layer in `lib.rs` only moves it across, so all of this is tested
//! natively too.
//!
//! JSON goes out as text (the app calls `JSON.parse`): serde_json is in the
//! binary anyway, and the download needs the text. Positions in diagnostics
//! are UTF-16 offsets, the unit of JavaScript strings and of web editors.

use std::collections::HashSet;

use omnigenesys_core::{
    access::Reach,
    export::{self, Encoding, Provenance},
    grid::Grid,
    operator::{Dimensions, Registry, Scope},
    pipeline::Pipeline,
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
/// data, not failures): `{"ok", "errors", "warnings", "params", "exprs",
/// "steps"}`. `params` and `exprs` once the file reads: the value each
/// param has (computed ones too), and what each `$expr` of the steps gave;
/// `steps` only when it builds, with what each step reads and writes, for
/// the graph view.
pub fn validate(registry: &Registry, text: &str, params: &[(String, String)]) -> String {
    let parsed = match PipelineSpec::parse_with(text, params) {
        Ok(p) => p,
        Err(errors) => return report(text, &errors, &[], None, None),
    };
    let values = param_values(&parsed.spec);
    match parsed.spec.build(registry) {
        Ok(built) => report(
            text,
            &[],
            &parsed.warnings,
            Some(values),
            Some((&parsed.spec, &built)),
        ),
        Err(errors) => report(text, &errors, &parsed.warnings, Some(values), None),
    }
}

/// `{"list": [{name, type, value, expr?, ...}], "exprs": [{pointer,
/// value}]}`: each param as declared with the value it has, in the order of
/// the file, and each `$expr` of the steps with the value it gave.
fn param_values(spec: &PipelineSpec) -> Value {
    let list: Vec<Value> = spec
        .param_values()
        .iter()
        .map(|(name, value)| {
            let mut p = json!({
                "name": name,
                "type": value.kind().to_string(),
                "value": json_of(value),
            });
            if let Some(d) = spec.params.get(name) {
                for (key, field) in [
                    ("expr", d.expr.clone().map(Value::from)),
                    ("description", d.description.clone().map(Value::from)),
                    ("min", d.min.map(Value::from)),
                    ("max", d.max.map(Value::from)),
                    ("step", d.step.map(Value::from)),
                    ("enum", d.choices.clone().map(Value::from)),
                ] {
                    if let Some(f) = field {
                        p[key] = f;
                    }
                }
            }
            p
        })
        .collect();
    let exprs: Vec<Value> = spec
        .expressions()
        .map(|(pointer, _, value)| json!({"pointer": pointer, "value": json_of(value)}))
        .collect();
    json!({"list": list, "exprs": exprs})
}

/// As the file writes it: `0.5`, `3`, `true`, `"forest"`.
fn json_of(value: &spec::ParamValue) -> Value {
    serde_json::from_str(&value.to_string()).unwrap_or(Value::Null)
}

fn report(
    text: &str,
    errors: &[Diagnostic],
    warnings: &[Diagnostic],
    params: Option<Value>,
    built: Option<(&PipelineSpec, &Built)>,
) -> String {
    let list = |ds: &[Diagnostic]| ds.iter().map(|d| diagnostic(text, d)).collect::<Vec<_>>();
    let mut out = json!({
        "ok": errors.is_empty(),
        "errors": list(errors),
        "warnings": list(warnings),
    });
    if let Some(mut params) = params {
        out["params"] = params["list"].take();
        out["exprs"] = params["exprs"].take();
    }
    if let Some((spec, built)) = built {
        out["steps"] = steps(spec, built);
        if let Value::Array(list) = &mut out["warnings"] {
            list.extend(unwritten_reads(spec, built));
        }
    }
    out.to_string()
}

/// A step that reads a layer no earlier step wrote: it sees it empty, which
/// is rarely what was meant (a step moved above the one it depends on).
fn unwritten_reads(spec: &PipelineSpec, built: &Built) -> Vec<Value> {
    let mut written = HashSet::new();
    let mut out = Vec::new();
    for (i, s) in spec.steps.iter().enumerate().filter(|(_, s)| s.enabled) {
        let Some(access) = built.pipeline.access(&s.id) else {
            continue;
        };
        for l in access.layers() {
            if access.read_reach(l.layer).is_some() && !written.contains(&l.layer) {
                let name = built.grid.layer_name(l.layer).unwrap_or("?");
                out.push(json!({
                    "message": format!(
                        "reads layer '{name}', but no step before it writes there: it is still empty"
                    ),
                    "step_id": s.id, "pointer": format!("/steps/{i}"), "span": null, "note": null,
                }));
            }
        }
        written.extend(
            access
                .layers()
                .iter()
                .filter(|l| l.write.is_some())
                .map(|l| l.layer),
        );
    }
    out
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
        // Inside the expression's own text, in UTF-8 bytes (the app has
        // the text and converts; for ASCII, bytes are UTF-16 units too).
        "expr_span": d.expr_span.map(|(start, end)| json!({"start": start, "end": end})),
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

/// A map being made from a pipeline, one step at a time or all at once:
/// the grid as the steps so far left it, its tiles, and how it was made.
pub struct Generation {
    grid: Grid,
    tiles: TileRegistry,
    pipeline: Pipeline,
    provenance: Provenance,
    salt: String,
    /// Every step's id in the file, enabled or not: errors point at
    /// `/steps/<index in the file>`.
    file_ids: Vec<String>,
    /// Steps already run.
    done: usize,
}

/// Parses and builds a pipeline, ready to run step by step. `seed` and
/// `salt` override the file's; a seed is required from one of the two (the
/// app draws one when the file has none, so it can show which). On failure,
/// the JSON of `{"errors": [...]}`, in the shape of [`validate`]'s.
pub fn start(
    registry: &Registry,
    text: &str,
    params: &[(String, String)],
    seed: Option<u64>,
    salt: Option<&str>,
) -> Result<Generation, String> {
    let fail = |errors: &[Diagnostic]| {
        let list: Vec<Value> = errors.iter().map(|d| diagnostic(text, d)).collect();
        json!({ "errors": list }).to_string()
    };
    let parsed = PipelineSpec::parse_with(text, params).map_err(|e| fail(&e))?;
    let spec = parsed.spec;
    let Built {
        grid,
        tiles,
        pipeline,
    } = spec.build(registry).map_err(|e| fail(&e))?;
    let Some(seed) = seed.or(spec.seed) else {
        return Err(error_json(
            "/seed",
            None,
            "the pipeline has no seed: give one",
        ));
    };
    let salt = salt.map_or_else(|| spec.salt.clone(), str::to_string);
    Ok(Generation {
        grid,
        tiles,
        pipeline,
        provenance: spec.provenance(seed, &salt),
        salt,
        file_ids: spec.steps.iter().map(|s| s.id.clone()).collect(),
        done: 0,
    })
}

/// [`start`], then every step.
pub fn generate(
    registry: &Registry,
    text: &str,
    params: &[(String, String)],
    seed: Option<u64>,
    salt: Option<&str>,
) -> Result<Generation, String> {
    let mut g = start(registry, text, params, seed, salt)?;
    while g.run_next()? {}
    Ok(g)
}

fn error_json(pointer: &str, step_id: Option<&str>, message: &str) -> String {
    json!({"errors": [{
        "message": message, "step_id": step_id, "pointer": pointer,
        "span": null, "note": null,
    }]})
    .to_string()
}

impl Generation {
    /// Runs the next step; `Ok(false)` when every step has run. On a
    /// failing step, the JSON of `{"errors": [...]}`, pointing at it.
    pub fn run_next(&mut self) -> Result<bool, String> {
        if self.done == self.pipeline.len() {
            return Ok(false);
        }
        let seed = self.provenance.seed;
        self.pipeline
            .run_step(self.done, &mut self.grid, &self.salt, seed)
            .map_err(|e| {
                let at = self
                    .file_ids
                    .iter()
                    .position(|id| *id == e.step_id)
                    .map_or_else(String::new, |i| format!("/steps/{i}"));
                error_json(&at, Some(&e.step_id), &e.error.to_string())
            })?;
        self.done += 1;
        Ok(true)
    }

    /// Steps that run (the enabled ones).
    pub fn steps(&self) -> usize {
        self.pipeline.len()
    }

    /// Steps already run.
    pub fn done(&self) -> usize {
        self.done
    }

    /// The id of the `i`-th step that runs.
    pub fn step_id(&self, i: usize) -> Option<&str> {
        self.pipeline.step_id(i)
    }

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

    /// Each layer's kind, in the same order: "tiles" or "scalar".
    pub fn kinds(&self) -> Vec<String> {
        self.grid
            .names()
            .filter_map(|n| self.grid.layer_id(n).ok())
            .filter_map(|id| self.grid.kind(id).ok())
            .map(|k| k.to_string())
            .collect()
    }

    /// One tile layer's ids, x fastest, then y, then z. `None` for a name
    /// the map does not have, or a scalar layer.
    pub fn layer(&self, name: &str) -> Option<Vec<u16>> {
        let id = self.grid.layer_id(name).ok()?;
        let layer = self.grid.tiles(id).ok()?;
        Some(layer.as_slice().iter().map(|t| t.index()).collect())
    }

    /// One scalar layer's values, in the same order. `None` for a name the
    /// map does not have, or a tile layer.
    pub fn scalar(&self, name: &str) -> Option<Vec<f32>> {
        let id = self.grid.layer_id(name).ok()?;
        Some(self.grid.scalars(id).ok()?.as_slice().to_vec())
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
