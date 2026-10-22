//! JSON Schema of the whole pipeline file, from a registry (PLANNING §2.1,
//! item 8): what editors use to complete and check a file as it is typed.
//!
//! Draft-07, because VS Code supports it fully and 2020-12 only in part.
//! Each step's `config` is checked against its operator's schema through
//! `if`/`then` on `operator`. The parser stays the authority: the schema
//! cannot express every rule (unique names, params of the right type), it
//! only catches most mistakes earlier.

use schemars::generate::SchemaSettings;
use serde_json::{Map, Value, json};

use crate::{condition::Condition, operator::Registry};

use super::{FORMAT_VERSION, MAX_NUMBER_SEED};

/// Where `{"$param": "name"}` is defined in the schema.
const PARAM_REF: &str = "#/definitions/ParamRef";

/// The schema of a pipeline file whose steps use `registry`'s operators.
pub fn schema(registry: &Registry) -> Value {
    let mut generator = SchemaSettings::draft07().into_generator();
    let mut dispatch = Vec::new();
    for name in registry.names() {
        let Some(entry) = registry.get(name) else {
            continue;
        };
        let mut config = (entry.schema)(&mut generator).to_value();
        allow_params(&mut config);
        dispatch.push(json!({
            "if": {"properties": {"operator": {"const": name}}, "required": ["operator"]},
            "then": {"properties": {"config": config}}
        }));
    }
    let mut condition = generator.subschema_for::<Condition>().to_value();
    allow_params(&mut condition);

    let mut definitions = generator.take_definitions(true);
    for definition in definitions.values_mut() {
        allow_params(definition);
    }
    definitions.insert("ParamRef".to_string(), param_ref());
    definitions.insert("Param".to_string(), param());
    definitions.insert("Step".to_string(), step(registry, condition, dispatch));

    json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "title": "Omnigenesys pipeline",
        "type": "object",
        "additionalProperties": false,
        "required": ["version", "size", "layers", "steps"],
        "properties": {
            "$schema": {"type": "string"},
            "version": {"const": FORMAT_VERSION},
            "description": {"type": "string"},
            "seed": {
                "description": "Above 2^53-1, write it as a string.",
                "oneOf": [
                    {"type": "integer", "minimum": 0, "maximum": MAX_NUMBER_SEED},
                    {"type": "string", "pattern": "^[0-9]+$"},
                    {"type": "null"}
                ]
            },
            "salt": {"type": "string"},
            "size": {
                "type": "object",
                "additionalProperties": false,
                "required": ["width", "height"],
                "properties": {
                    "width": {"type": "integer", "minimum": 1},
                    "height": {"type": "integer", "minimum": 1},
                    "depth": {"type": "integer", "minimum": 1, "default": 1}
                }
            },
            "layers": {
                "type": "array",
                "items": {
                    "anyOf": [
                        {"type": "string", "minLength": 1},
                        {
                            "type": "object",
                            "additionalProperties": false,
                            "required": ["name"],
                            "properties": {
                                "name": {"type": "string", "minLength": 1},
                                "kind": {"enum": ["tiles"]}
                            }
                        }
                    ]
                }
            },
            "tiles": {
                "description": "\"empty\" is tile 0 and cannot be declared.",
                "type": "array",
                "uniqueItems": true,
                "items": {"type": "string", "minLength": 1, "not": {"const": "empty"}}
            },
            "params": {
                "type": "object",
                "propertyNames": {"pattern": "^[^$]"},
                "additionalProperties": {"$ref": "#/definitions/Param"}
            },
            "steps": {"type": "array", "items": {"$ref": "#/definitions/Step"}}
        },
        "definitions": definitions
    })
}

fn step(registry: &Registry, condition: Value, dispatch: Vec<Value>) -> Value {
    let operators: Vec<&str> = registry.names().collect();
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["id", "operator"],
        "properties": {
            "id": {
                "description": "Seeds the step's randomness: renaming a step changes its map.",
                "type": "string",
                "pattern": "^[a-z][a-z0-9_-]{0,63}$"
            },
            "operator": {"enum": operators},
            "enabled": {"type": "boolean", "default": true},
            "description": {"type": "string"},
            "config": {"type": "object"},
            "conditions": {"type": "array", "items": condition}
        },
        "allOf": dispatch
    })
}

fn param_ref() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["$param"],
        "properties": {"$param": {"type": "string"}}
    })
}

fn param() -> Value {
    json!({
        "anyOf": [
            {"type": ["number", "boolean", "string"]},
            {
                "type": "object",
                "additionalProperties": false,
                "required": ["type", "default"],
                "properties": {
                    "type": {"enum": ["number", "integer", "boolean", "string"]},
                    "default": {"type": ["number", "boolean", "string"]},
                    "min": {"type": "number"},
                    "max": {"type": "number"},
                    "step": {"type": "number", "exclusiveMinimum": 0},
                    "enum": {"type": "array", "items": {"type": "string"}},
                    "description": {"type": "string"}
                }
            }
        ]
    })
}

/// Lets every value inside `schema` also be a `{"$param": ...}`, as the
/// parser allows anywhere in `config` and `conditions`.
fn allow_params(schema: &mut Value) {
    let Value::Object(map) = schema else {
        return;
    };
    if let Some(Value::Object(properties)) = map.get_mut("properties") {
        for property in properties.values_mut() {
            allow_params(property);
            or_param(property);
        }
    }
    if let Some(items) = map.get_mut("items")
        && items.is_object()
    {
        allow_params(items);
        or_param(items);
    }
    for key in ["anyOf", "oneOf", "allOf"] {
        if let Some(Value::Array(alternatives)) = map.get_mut(key) {
            alternatives.iter_mut().for_each(allow_params);
        }
    }
}

fn or_param(schema: &mut Value) {
    let inner = std::mem::replace(schema, Value::Object(Map::new()));
    *schema = json!({"anyOf": [inner, {"$ref": PARAM_REF}]});
}

#[cfg(test)]
mod tests;
