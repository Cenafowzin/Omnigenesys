//! Pipeline format v2 (PLANNING §2.1) — the serde/schemars source of truth.
//! `pipeline.schema.json` is generated from these types, so the schema can
//! never drift from the code.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::coords::GridSize;
use crate::error::PipelineError;

/// The only pipeline format version this build understands.
pub const FORMAT_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PipelineSpec {
    /// Must equal [`FORMAT_VERSION`]; unknown versions are rejected at build.
    pub version: u32,
    pub seed: u64,
    /// Differentiates games sharing similar pipelines: same seed + same
    /// pipeline + different salt = different maps. Overridable at
    /// generation time (CLI flag / FFI parameter), so a publicly shared
    /// pipeline does not leak the game's salt.
    #[serde(default)]
    pub salt: String,
    pub size: GridSize,
    pub layers: Vec<String>,
    /// Named values referenced from step configs via `{"$param": "name"}`.
    /// Resolved as a preprocessing pass before operator dispatch.
    #[serde(default)]
    pub params: BTreeMap<String, Value>,
    pub steps: Vec<StepSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StepSpec {
    /// Unique within the pipeline. Used for error messages, the web editor
    /// and per-step RNG derivation — renaming a step changes its RNG stream.
    pub id: String,
    pub operator: String,
    /// Editor toggle: disabled steps are still validated but not executed.
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub on_fail: OnFail,
    /// Operator-specific config, deserialized by the operator's own factory.
    #[serde(default)]
    pub config: Value,
}

fn default_true() -> bool {
    true
}

/// Per-step failure policy (PLANNING §2.3). Placement failures should not
/// abort the whole pipeline by default in-game — but `error` stays the
/// default because silent misbehavior is the worse failure mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum OnFail {
    #[default]
    Error,
    /// Continue and surface the failure as a warning in the run report.
    Warn,
    /// Continue silently. Explicit opt-in, never the default.
    Skip,
}

/// Generates the JSON Schema for the pipeline format from the Rust types.
pub fn pipeline_schema() -> schemars::Schema {
    schemars::schema_for!(PipelineSpec)
}

/// Replaces every `{"$param": "name"}` object in `config` with the matching
/// value from `params`, recursively. An object containing a `$param` key
/// alongside other keys, or with a non-string value, is an error — typos
/// must never pass silently (PLANNING §1, bug #2 of the Go version).
pub fn resolve_params(
    step_id: &str,
    config: &Value,
    params: &BTreeMap<String, Value>,
) -> Result<Value, PipelineError> {
    match config {
        Value::Object(map) => {
            if map.contains_key("$param") {
                if map.len() != 1 {
                    return Err(PipelineError::InvalidParamRef { step_id: step_id.to_string() });
                }
                match &map["$param"] {
                    Value::String(name) => params.get(name).cloned().ok_or_else(|| {
                        PipelineError::UnknownParam {
                            step_id: step_id.to_string(),
                            param: name.clone(),
                        }
                    }),
                    _ => Err(PipelineError::InvalidParamRef { step_id: step_id.to_string() }),
                }
            } else {
                let mut resolved = serde_json::Map::with_capacity(map.len());
                for (key, value) in map {
                    resolved.insert(key.clone(), resolve_params(step_id, value, params)?);
                }
                Ok(Value::Object(resolved))
            }
        }
        Value::Array(items) => items
            .iter()
            .map(|item| resolve_params(step_id, item, params))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        other => Ok(other.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The example pipeline from PLANNING §2.1 must parse as-is.
    #[test]
    fn planning_example_parses() {
        let spec: PipelineSpec = serde_json::from_value(json!({
            "version": 2,
            "seed": 512,
            "salt": "lost-fields",
            "size": { "width": 80, "height": 50, "depth": 1 },
            "layers": ["terrain", "vegetation", "structures", "entities"],
            "params": { "vegetation_density": 0.52, "path_organic": 4.0 },
            "steps": [
                {
                    "id": "base_floor",
                    "operator": "Fill",
                    "config": { "layer": "terrain", "tile": "floor" }
                },
                {
                    "id": "veg",
                    "operator": "NoiseScatter",
                    "enabled": true,
                    "config": {
                        "layer": "vegetation",
                        "tile": "foliage",
                        "threshold": { "$param": "vegetation_density" },
                        "noise": { "type": "perlin", "scale": 0.18 }
                    }
                }
            ]
        }))
        .unwrap();

        assert_eq!(spec.version, 2);
        assert_eq!(spec.steps.len(), 2);
        assert!(spec.steps[0].enabled, "enabled must default to true");
        assert_eq!(spec.steps[0].on_fail, OnFail::Error, "on_fail must default to error");
    }

    #[test]
    fn salt_defaults_to_empty() {
        let spec: PipelineSpec = serde_json::from_value(json!({
            "version": 2, "seed": 1,
            "size": { "width": 4, "height": 4 },
            "layers": ["terrain"], "steps": []
        }))
        .unwrap();
        assert_eq!(spec.salt, "");
    }

    #[test]
    fn typo_in_step_field_is_an_error() {
        let result: Result<PipelineSpec, _> = serde_json::from_value(json!({
            "version": 2, "seed": 1,
            "size": { "width": 4, "height": 4 },
            "layers": ["terrain"],
            "steps": [{ "id": "a", "operator": "Fill", "enbaled": false }]
        }));
        assert!(result.is_err());
    }

    #[test]
    fn resolve_params_replaces_nested_refs() {
        let params = BTreeMap::from([("density".to_string(), json!(0.52))]);
        let config = json!({
            "layer": "veg",
            "noise": { "threshold": { "$param": "density" }, "scale": 0.18 },
            "list": [{ "$param": "density" }, 1]
        });
        let resolved = resolve_params("veg", &config, &params).unwrap();
        assert_eq!(resolved["noise"]["threshold"], json!(0.52));
        assert_eq!(resolved["list"][0], json!(0.52));
        assert_eq!(resolved["noise"]["scale"], json!(0.18));
    }

    #[test]
    fn unknown_param_is_an_error() {
        let err = resolve_params("veg", &json!({ "$param": "nope" }), &BTreeMap::new())
            .unwrap_err();
        assert!(matches!(err, PipelineError::UnknownParam { .. }));
        assert!(err.to_string().contains("veg"), "error must name the step");
    }

    #[test]
    fn param_ref_with_extra_keys_is_an_error() {
        let params = BTreeMap::from([("d".to_string(), json!(1))]);
        let err = resolve_params("s", &json!({ "$param": "d", "extra": 1 }), &params)
            .unwrap_err();
        assert!(matches!(err, PipelineError::InvalidParamRef { .. }));
    }

    #[test]
    fn schema_generates() {
        let schema = serde_json::to_value(pipeline_schema()).unwrap();
        let text = schema.to_string();
        assert!(text.contains("version"));
        assert!(text.contains("steps"));
    }
}
