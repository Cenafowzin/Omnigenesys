//! Typed params and `{"$param": "name"}` substitution (PLANNING §2.1, items
//! 5 and 6).

use std::collections::HashSet;
use std::fmt;

use indexmap::IndexMap;
use serde::Deserialize;
use serde::de::value::MapAccessDeserializer;
use serde::de::{Deserializer, MapAccess, Visitor};
use serde_json::Value;

use super::source::{push_index, push_key};
use super::{Diagnostic, deserialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParamType {
    Number,
    Integer,
    Boolean,
    String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParamValue {
    Number(f64),
    Integer(i64),
    Boolean(bool),
    String(String),
}

/// A param as declared. `min`, `max` and `step` only on numbers and
/// integers; `choices` (`enum` in the file) only on strings.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub default: ParamValue,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    pub choices: Option<Vec<String>>,
    pub description: Option<String>,
}

impl ParamValue {
    pub fn kind(&self) -> ParamType {
        match self {
            ParamValue::Number(_) => ParamType::Number,
            ParamValue::Integer(_) => ParamType::Integer,
            ParamValue::Boolean(_) => ParamType::Boolean,
            ParamValue::String(_) => ParamType::String,
        }
    }

    fn to_json(&self) -> Value {
        match self {
            ParamValue::Number(n) => Value::from(*n),
            ParamValue::Integer(n) => Value::from(*n),
            ParamValue::Boolean(b) => Value::Bool(*b),
            ParamValue::String(s) => Value::String(s.clone()),
        }
    }
}

/// Where a param was substituted: the pointer of the `{"$param": ...}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Site {
    pub pointer: String,
    pub param: String,
}

/// The file's `params`, checked.
#[derive(Debug, Default)]
pub(super) struct Declarations {
    pub params: IndexMap<String, Param>,
    /// Declared with an error (already reported): their uses stay quiet
    /// instead of adding "not declared" for the same mistake.
    pub broken: HashSet<String>,
}

pub(super) fn declarations(params: Option<&Value>, errors: &mut Vec<Diagnostic>) -> Declarations {
    let mut out = Declarations::default();
    let Some(params) = params else {
        return out;
    };
    // With serde_json's preserve_order, Map keeps the file's order.
    let raw: serde_json::Map<String, Value> = match deserialize(params.clone(), "/params") {
        Ok(raw) => raw,
        Err(d) => {
            errors.push(d);
            return out;
        }
    };
    for (name, value) in raw {
        let at = param_pointer(&name);
        let declared = deserialize::<Declared>(value, &at).and_then(|d| match d {
            Declared::Short(default) => {
                short(default).map_err(|m| Diagnostic::new(&at, format!("param '{name}': {m}")))
            }
            Declared::Full(decl) => full(decl).map_err(|(field, m)| {
                Diagnostic::new(format!("{at}/{field}"), format!("param '{name}': {m}"))
            }),
        });
        match declared {
            Ok(param) => {
                out.params.insert(name, param);
            }
            Err(d) => {
                errors.push(d);
                out.broken.insert(name);
            }
        }
    }
    out
}

pub(super) fn param_pointer(name: &str) -> String {
    let mut p = String::from("/params");
    push_key(&mut p, name);
    p
}

/// How a param is written: a bare value (type inferred) or the full form.
enum Declared {
    Short(Value),
    Full(Decl),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Decl {
    #[serde(rename = "type")]
    kind: ParamType,
    default: Value,
    min: Option<f64>,
    max: Option<f64>,
    step: Option<f64>,
    #[serde(rename = "enum")]
    choices: Option<Vec<String>>,
    description: Option<String>,
}

// Written by hand rather than `#[serde(untagged)]`: untagged tries each
// form and, when all fail, says only "did not match any variant". Here a
// typo inside the full form is reported as the typo it is.
impl<'de> Deserialize<'de> for Declared {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Declared, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Declared;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a number, a boolean, a string, or {\"type\": ..., \"default\": ...}")
            }
            fn visit_bool<E>(self, v: bool) -> Result<Declared, E> {
                Ok(Declared::Short(Value::Bool(v)))
            }
            fn visit_i64<E>(self, v: i64) -> Result<Declared, E> {
                Ok(Declared::Short(Value::from(v)))
            }
            fn visit_u64<E>(self, v: u64) -> Result<Declared, E> {
                Ok(Declared::Short(Value::from(v)))
            }
            fn visit_f64<E>(self, v: f64) -> Result<Declared, E> {
                Ok(Declared::Short(Value::from(v)))
            }
            fn visit_str<E>(self, v: &str) -> Result<Declared, E> {
                Ok(Declared::Short(Value::from(v)))
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Declared, A::Error> {
                Decl::deserialize(MapAccessDeserializer::new(map)).map(Declared::Full)
            }
        }
        d.deserialize_any(V)
    }
}

/// `"density": 0.5`: a number with a fraction or exponent is a `number`,
/// one without is an `integer`.
fn short(default: Value) -> Result<Param, String> {
    let default = match default {
        Value::Bool(b) => ParamValue::Boolean(b),
        Value::String(s) => ParamValue::String(s),
        Value::Number(n) if n.is_f64() => ParamValue::Number(n.as_f64().unwrap_or_default()),
        Value::Number(n) => ParamValue::Integer(
            n.as_i64()
                .ok_or_else(|| format!("{n} is too large for an integer param"))?,
        ),
        // The visitor only produces the four above.
        other => return Err(format!("{other} is not a param value")),
    };
    Ok(Param {
        default,
        min: None,
        max: None,
        step: None,
        choices: None,
        description: None,
    })
}

/// Checks the full form. The error names the field it is about.
fn full(d: Decl) -> Result<Param, (&'static str, String)> {
    let numeric = matches!(d.kind, ParamType::Number | ParamType::Integer);
    let default = match (d.kind, &d.default) {
        (ParamType::Number, Value::Number(n)) => n.as_f64().map(ParamValue::Number),
        (ParamType::Integer, Value::Number(n)) => n.as_i64().map(ParamValue::Integer),
        (ParamType::Boolean, Value::Bool(b)) => Some(ParamValue::Boolean(*b)),
        (ParamType::String, Value::String(s)) => Some(ParamValue::String(s.clone())),
        _ => None,
    }
    .ok_or_else(|| ("default", format!("{} is not {}", d.default, a(d.kind))))?;

    for (field, value) in [("min", d.min), ("max", d.max), ("step", d.step)] {
        let Some(v) = value else { continue };
        if !numeric {
            return Err((field, format!("{} params have no {field}", d.kind)));
        }
        if d.kind == ParamType::Integer && v.fract() != 0.0 {
            return Err((field, format!("{v} is not an integer")));
        }
    }
    if let (Some(min), Some(max)) = (d.min, d.max)
        && min > max
    {
        return Err(("min", format!("min {min} is greater than max {max}")));
    }
    if let Some(step) = d.step
        && step <= 0.0
    {
        return Err(("step", format!("step {step} must be greater than 0")));
    }
    let n = match default {
        ParamValue::Number(n) => Some(n),
        ParamValue::Integer(n) => Some(n as f64),
        _ => None,
    };
    if let Some(n) = n
        && (d.min.is_some_and(|min| n < min) || d.max.is_some_and(|max| n > max))
    {
        return Err(("default", format!("{n} is outside min..=max")));
    }
    if let Some(choices) = &d.choices {
        let ParamValue::String(s) = &default else {
            return Err(("enum", format!("{} params have no enum", d.kind)));
        };
        if !choices.contains(s) {
            return Err(("default", format!("\"{s}\" is not one of the enum")));
        }
    }
    Ok(Param {
        default,
        min: d.min,
        max: d.max,
        step: d.step,
        choices: d.choices,
        description: d.description,
    })
}

impl fmt::Display for ParamType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ParamType::Number => "number",
            ParamType::Integer => "integer",
            ParamType::Boolean => "boolean",
            ParamType::String => "string",
        })
    }
}

/// "a number", "an integer".
fn a(kind: ParamType) -> String {
    let article = if kind == ParamType::Integer {
        "an"
    } else {
        "a"
    };
    format!("{article} {kind}")
}

/// Where in the file a key starting with `$` may appear.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    /// The root object: `$schema` is allowed.
    Root,
    Steps,
    Step,
    /// Inside a step's `config` or `conditions`: `$param` is allowed.
    Substitutable,
    Other,
}

impl Place {
    fn member(self, key: &str) -> Place {
        match (self, key) {
            (Place::Root, "steps") => Place::Steps,
            (Place::Step, "config" | "conditions") => Place::Substitutable,
            (Place::Substitutable, _) => Place::Substitutable,
            _ => Place::Other,
        }
    }

    fn element(self) -> Place {
        match self {
            Place::Steps => Place::Step,
            Place::Substitutable => Place::Substitutable,
            _ => Place::Other,
        }
    }
}

/// Replaces every `{"$param": "name"}` with the param's default and returns
/// where. Any other key starting with `$` is reserved and is an error.
pub(super) fn substitute(
    root: &mut Value,
    params: &Declarations,
    errors: &mut Vec<Diagnostic>,
) -> Vec<Site> {
    let mut w = Walker {
        params,
        errors,
        sites: Vec::new(),
    };
    w.walk(root, &mut String::new(), Place::Root);
    w.sites
}

struct Walker<'a> {
    params: &'a Declarations,
    errors: &'a mut Vec<Diagnostic>,
    sites: Vec<Site>,
}

impl Walker<'_> {
    fn walk(&mut self, value: &mut Value, pointer: &mut String, place: Place) {
        if place == Place::Substitutable && value.get("$param").is_some() {
            self.reference(value, pointer);
            return;
        }
        match value {
            Value::Object(map) => {
                for (key, child) in map.iter_mut() {
                    let len = pointer.len();
                    push_key(pointer, key);
                    if !key.starts_with('$') {
                        self.walk(child, pointer, place.member(key));
                    } else if key == "$param" {
                        self.error(
                            pointer,
                            "`$param` can only be used inside a step's `config` or `conditions`",
                        );
                    } else if !(place == Place::Root && key == "$schema") {
                        self.error(
                            pointer,
                            format!("`{key}`: keys starting with `$` are reserved"),
                        );
                    }
                    pointer.truncate(len);
                }
            }
            Value::Array(items) => {
                for (i, item) in items.iter_mut().enumerate() {
                    let len = pointer.len();
                    push_index(pointer, i);
                    self.walk(item, pointer, place.element());
                    pointer.truncate(len);
                }
            }
            _ => {}
        }
    }

    fn reference(&mut self, value: &mut Value, pointer: &str) {
        let name = match value.as_object() {
            Some(map) if map.len() == 1 => map.get("$param").and_then(Value::as_str),
            _ => None,
        };
        let Some(name) = name else {
            self.error(
                pointer,
                r#"a param reference is exactly {"$param": "name"}"#,
            );
            return;
        };
        match self.params.params.get(name) {
            Some(param) => {
                self.sites.push(Site {
                    pointer: pointer.to_string(),
                    param: name.to_string(),
                });
                *value = param.default.to_json();
            }
            None if self.params.broken.contains(name) => {}
            None => self.error(
                pointer,
                format!("param '{name}' is not declared in `params`"),
            ),
        }
    }

    fn error(&mut self, pointer: &str, message: impl Into<String>) {
        self.errors.push(Diagnostic::new(pointer, message));
    }
}

#[cfg(test)]
mod tests;
