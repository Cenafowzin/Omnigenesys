//! Typed params and `{"$param": "name"}` substitution (PLANNING §2.1, items
//! 5 and 6).

use std::collections::{HashMap, HashSet};
use std::fmt;

use indexmap::IndexMap;
use serde::Deserialize;
use serde::de::value::MapAccessDeserializer;
use serde::de::{Deserializer, MapAccess, Visitor};
use serde_json::Value;

use super::source::{push_index, push_key};
use super::{Diagnostic, deserialize};
use crate::expr::{self, Expr};

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
///
/// A param with an `expr` is computed from other params (`"rooms": {"type":
/// "integer", "expr": "min(20, 5 + floor(stage * 10 / 3))"}`): it has no
/// default of its own, and a game cannot set it, only the params it reads.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    /// For a computed param, what it computed for this file and its
    /// overrides.
    pub default: ParamValue,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    pub choices: Option<Vec<String>>,
    pub description: Option<String>,
    /// The expression of a computed param, as written.
    pub expr: Option<String>,
    compiled: Option<Expr>,
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

/// Where a value was substituted: the pointer of the `{"$param": ...}` or
/// `{"$expr": ...}`, the params it read and, for an expression, its text
/// and the value it gave.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Site {
    pub pointer: String,
    pub params: Vec<String>,
    pub expr: Option<String>,
    pub value: ParamValue,
}

/// The file's `params`, checked.
#[derive(Debug, Default)]
pub(super) struct Declarations {
    pub params: IndexMap<String, Param>,
    /// Declared with an error (already reported): their uses stay quiet
    /// instead of adding "not declared" for the same mistake.
    pub broken: HashSet<String>,
    /// Values given from outside the file, used instead of the default.
    pub overrides: HashMap<String, ParamValue>,
    /// What each computed param gave, once [`compute`] ran.
    pub computed: HashMap<String, ParamValue>,
}

impl Declarations {
    /// The value a step gets for `name`: the override, the computed value
    /// or the default.
    pub fn value(&self, name: &str) -> Option<&ParamValue> {
        self.overrides
            .get(name)
            .or_else(|| self.computed.get(name))
            .or_else(|| self.params.get(name).map(|p| &p.default))
    }
}

/// Applies `name=value` overrides: each must name a declared param and
/// fit its declaration. The last one for a name wins.
pub(super) fn apply_overrides(
    declared: &mut Declarations,
    overrides: &[(String, String)],
    errors: &mut Vec<Diagnostic>,
) {
    for (name, text) in overrides {
        if declared.broken.contains(name) {
            continue;
        }
        let Some(param) = declared.params.get(name) else {
            let known: Vec<&str> = declared.params.keys().map(String::as_str).collect();
            let known = if known.is_empty() {
                "the file declares none".to_string()
            } else {
                format!("declared: {}", known.join(", "))
            };
            errors.push(Diagnostic::new(
                "/params",
                format!("override {name}={text}: no param '{name}' ({known})"),
            ));
            continue;
        };
        if let Some(e) = &param.compiled {
            let reads = names_of(e);
            let reads = if reads.is_empty() {
                "the expression".to_string()
            } else {
                reads.join(", ")
            };
            errors.push(Diagnostic::new(
                param_pointer(name),
                format!(
                    "override {name}={text}: '{name}' is computed from its expression; set {reads} instead"
                ),
            ));
            continue;
        }
        match param.parse_value(text) {
            Ok(value) => {
                declared.overrides.insert(name.clone(), value);
            }
            // Placed at the declaration: it says what the param accepts.
            Err(m) => errors.push(Diagnostic::new(
                param_pointer(name),
                format!("override {name}={text}: {m}"),
            )),
        }
    }
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
            errors.push(*d);
            return out;
        }
    };
    for (name, value) in raw {
        let at = param_pointer(&name);
        if expr::RESERVED.contains(&name.as_str()) {
            errors.push(Diagnostic::new(
                &at,
                format!(
                    "param '{name}': '{name}' is a word of the expression language; pick another name"
                ),
            ));
            out.broken.insert(name);
            continue;
        }
        let declared = deserialize::<Declared>(value, &at).and_then(|d| match d {
            Declared::Short(default) => short(default)
                .map_err(|m| Box::new(Diagnostic::new(&at, format!("param '{name}': {m}")))),
            Declared::Full(decl) => full(decl).map_err(|(field, m)| {
                Box::new(Diagnostic::new(
                    format!("{at}/{field}"),
                    format!("param '{name}': {m}"),
                ))
            }),
        });
        let declared = declared.and_then(|mut param| {
            if let Some(text) = &param.expr {
                let e = Expr::parse(text).map_err(|e| Box::new(expr_diagnostic(&name, e)))?;
                param.compiled = Some(e);
            }
            Ok(param)
        });
        match declared {
            Ok(param) => {
                out.params.insert(name, param);
            }
            Err(d) => {
                errors.push(*d);
                out.broken.insert(name);
            }
        }
    }
    out
}

/// An error inside the expression of computed param `name`.
fn expr_diagnostic(name: &str, e: expr::ExprError) -> Diagnostic {
    let mut d = Diagnostic::new(
        format!("{}/expr", param_pointer(name)),
        format!("param '{name}': {}", e.message),
    );
    d.expr_span = Some((e.span.start, e.span.end));
    d
}

/// The params an expression names, once each, in order.
fn names_of(e: &Expr) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (n, _) in e.names() {
        if !out.iter().any(|o| o == n) {
            out.push(n.to_string());
        }
    }
    out
}

impl ParamValue {
    fn to_expr(&self) -> expr::Value {
        match self {
            ParamValue::Number(n) => expr::Value::Number(*n),
            ParamValue::Integer(n) => expr::Value::Integer(*n),
            ParamValue::Boolean(b) => expr::Value::Boolean(*b),
            ParamValue::String(s) => expr::Value::String(s.clone()),
        }
    }

    fn from_expr(v: expr::Value) -> ParamValue {
        match v {
            expr::Value::Number(n) => ParamValue::Number(n),
            expr::Value::Integer(n) => ParamValue::Integer(n),
            expr::Value::Boolean(b) => ParamValue::Boolean(b),
            expr::Value::String(s) => ParamValue::String(s),
        }
    }
}

fn expr_type(kind: ParamType) -> expr::Type {
    match kind {
        ParamType::Number => expr::Type::Number,
        ParamType::Integer => expr::Type::Integer,
        ParamType::Boolean => expr::Type::Boolean,
        ParamType::String => expr::Type::String,
    }
}

impl Declarations {
    /// Every param an expression may name, with its type. A param declared
    /// with an error is left out by the caller's choice (`broken`).
    fn types(&self) -> Vec<(&str, expr::Type)> {
        self.params
            .iter()
            .map(|(n, p)| (n.as_str(), expr_type(p.kind())))
            .collect()
    }

    /// Checks and evaluates `e`: its errors, or its value. A name that is a
    /// broken param makes it quiet (`Ok(None)`): that mistake is reported.
    fn evaluate(
        &self,
        e: &Expr,
        to_diagnostic: &dyn Fn(expr::ExprError) -> Diagnostic,
        errors: &mut Vec<Diagnostic>,
    ) -> Option<ParamValue> {
        if e.names().iter().any(|(n, _)| self.broken.contains(*n)) {
            return None;
        }
        if let Err(es) = e.check(&self.types()) {
            errors.extend(es.into_iter().map(to_diagnostic));
            return None;
        }
        match e.eval(&|n| self.value(n).map(ParamValue::to_expr)) {
            Ok(v) => Some(ParamValue::from_expr(v)),
            Err(err) => {
                errors.push(to_diagnostic(err));
                None
            }
        }
    }
}

/// Computes every param with an `expr`, after the overrides: each after
/// the ones it reads (a cycle is an error naming it), each checked against
/// its declaration like a default is. One that fails is `broken`.
pub(super) fn compute(declared: &mut Declarations, errors: &mut Vec<Diagnostic>) {
    // Depth-first, in declaration order: `done` once computed, the path
    // in `stack` to find a cycle.
    let names: Vec<String> = declared
        .params
        .iter()
        .filter(|(_, p)| p.compiled.is_some())
        .map(|(n, _)| n.clone())
        .collect();
    let mut done: HashSet<String> = HashSet::new();
    for name in &names {
        let mut stack = Vec::new();
        visit(declared, name, &mut stack, &mut done, errors);
    }
}

fn visit(
    declared: &mut Declarations,
    name: &str,
    stack: &mut Vec<String>,
    done: &mut HashSet<String>,
    errors: &mut Vec<Diagnostic>,
) {
    if done.contains(name) || declared.broken.contains(name) {
        return;
    }
    let Some(e) = declared.params.get(name).and_then(|p| p.compiled.clone()) else {
        return;
    };
    if let Some(at) = stack.iter().position(|s| s == name) {
        let mut path: Vec<&str> = stack[at..].iter().map(String::as_str).collect();
        path.push(name);
        errors.push(Diagnostic::new(
            format!("{}/expr", param_pointer(name)),
            format!(
                "params compute from each other in a circle: {}",
                path.join(" → ")
            ),
        ));
        for n in &stack[at..] {
            declared.broken.insert(n.clone());
        }
        return;
    }
    stack.push(name.to_string());
    for dep in names_of(&e) {
        visit(declared, &dep, stack, done, errors);
    }
    stack.pop();
    if declared.broken.contains(name) {
        return;
    }
    let at = name.to_string();
    let value = declared.evaluate(&e, &|err| expr_diagnostic(&at, err), errors);
    let param = &declared.params[name];
    let value = value.and_then(|v| {
        // An integer is a number too; the other way is a mistake to say.
        let v = match (param.kind(), v) {
            (ParamType::Number, ParamValue::Integer(n)) => ParamValue::Number(n as f64),
            (ParamType::Integer, ParamValue::Number(_)) => {
                errors.push(Diagnostic::new(
                    format!("{}/expr", param_pointer(name)),
                    format!(
                        "param '{name}' is an integer, and its expression gives a number: \
                         wrap it in floor(), round() or ceil()"
                    ),
                ));
                return None;
            }
            (_, v) => v,
        };
        if let Err(m) = param.check(&v) {
            errors.push(Diagnostic::new(
                format!("{}/expr", param_pointer(name)),
                format!("param '{name}' computed {v}: {m}"),
            ));
            return None;
        }
        Some(v)
    });
    match value {
        Some(v) => {
            declared.computed.insert(name.to_string(), v.clone());
            if let Some(p) = declared.params.get_mut(name) {
                p.default = v;
            }
            done.insert(name.to_string());
        }
        None => {
            declared.broken.insert(name.to_string());
        }
    }
}

/// Params a step's value comes from, directly or through computed params
/// that read them.
pub(super) fn used(params: &IndexMap<String, Param>, sites: &[Site]) -> HashSet<String> {
    let mut used = HashSet::new();
    let mut todo: Vec<String> = sites.iter().flat_map(|s| s.params.clone()).collect();
    while let Some(n) = todo.pop() {
        if used.insert(n.clone())
            && let Some(e) = params.get(&n).and_then(|p| p.compiled.as_ref())
        {
            todo.extend(names_of(e));
        }
    }
    used
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
    default: Option<Value>,
    expr: Option<String>,
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
        expr: None,
        compiled: None,
    })
}

/// Checks the full form. The error names the field it is about.
fn full(d: Decl) -> Result<Param, (&'static str, String)> {
    let numeric = matches!(d.kind, ParamType::Number | ParamType::Integer);
    let default = match (&d.default, &d.expr) {
        (Some(_), Some(_)) => {
            return Err((
                "default",
                "a param with `expr` has no default: its value is computed".into(),
            ));
        }
        (None, None) => return Err(("default", "needs a `default` (or an `expr`)".into())),
        // A stand-in of the declared type until `compute` runs.
        // A stand-in of the declared type until `compute` runs; the
        // expression is parsed by `declarations`, whose error can say where.
        (None, Some(_)) => placeholder(d.kind),
        (Some(v), None) => match (d.kind, v) {
            (ParamType::Number, Value::Number(n)) => n.as_f64().map(ParamValue::Number),
            (ParamType::Integer, Value::Number(n)) => n.as_i64().map(ParamValue::Integer),
            (ParamType::Boolean, Value::Bool(b)) => Some(ParamValue::Boolean(*b)),
            (ParamType::String, Value::String(s)) => Some(ParamValue::String(s.clone())),
            _ => None,
        }
        .ok_or_else(|| ("default", format!("{v} is not {}", a(d.kind))))?,
    };

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
    if d.choices.is_some() && d.kind != ParamType::String {
        return Err(("enum", format!("{} params have no enum", d.kind)));
    }
    let param = Param {
        default,
        min: d.min,
        max: d.max,
        step: d.step,
        choices: d.choices,
        description: d.description,
        expr: d.expr,
        compiled: None,
    };
    if param.expr.is_none() {
        param.check(&param.default).map_err(|m| ("default", m))?;
    }
    Ok(param)
}

fn placeholder(kind: ParamType) -> ParamValue {
    match kind {
        ParamType::Number => ParamValue::Number(0.0),
        ParamType::Integer => ParamValue::Integer(0),
        ParamType::Boolean => ParamValue::Boolean(false),
        ParamType::String => ParamValue::String(String::new()),
    }
}

impl Param {
    pub fn kind(&self) -> ParamType {
        self.default.kind()
    }

    /// Whether `value` fits the declaration: its type, `min..=max` and the
    /// `enum`. The default is checked with this too, so a declaration and
    /// an override follow the same rule.
    pub fn check(&self, value: &ParamValue) -> Result<(), String> {
        if value.kind() != self.kind() {
            return Err(format!("{value} is not {}", a(self.kind())));
        }
        let n = match value {
            ParamValue::Number(n) => Some(*n),
            ParamValue::Integer(n) => Some(*n as f64),
            _ => None,
        };
        if let Some(n) = n
            && (self.min.is_some_and(|min| n < min) || self.max.is_some_and(|max| n > max))
        {
            return Err(format!("{n} is outside min..=max"));
        }
        if let (Some(choices), ParamValue::String(s)) = (&self.choices, value)
            && !choices.contains(s)
        {
            return Err(format!("\"{s}\" is not one of the enum"));
        }
        Ok(())
    }

    /// A value written as text (a command-line override), read as this
    /// param's type and checked.
    pub fn parse_value(&self, text: &str) -> Result<ParamValue, String> {
        let value = match self.kind() {
            ParamType::Number => text
                .parse()
                .ok()
                .filter(|n: &f64| n.is_finite())
                .map(ParamValue::Number),
            ParamType::Integer => text.parse().ok().map(ParamValue::Integer),
            ParamType::Boolean => text.parse().ok().map(ParamValue::Boolean),
            ParamType::String => Some(ParamValue::String(text.to_string())),
        }
        .ok_or_else(|| format!("\"{text}\" is not {}", a(self.kind())))?;
        self.check(&value)?;
        Ok(value)
    }
}

/// As JSON writes it: `0.5`, `3`, `true`, `"forest"`.
impl fmt::Display for ParamValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_json())
    }
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
        if place == Place::Substitutable && value.get("$expr").is_some() {
            self.expression(value, pointer);
            return;
        }
        match value {
            Value::Object(map) => {
                for (key, child) in map.iter_mut() {
                    let len = pointer.len();
                    push_key(pointer, key);
                    if !key.starts_with('$') {
                        self.walk(child, pointer, place.member(key));
                    } else if key == "$param" || key == "$expr" {
                        self.error(
                            pointer,
                            format!(
                                "`{key}` can only be used inside a step's `config` or `conditions`"
                            ),
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
        match self.params.value(name) {
            Some(chosen) if !self.params.broken.contains(name) => {
                self.sites.push(Site {
                    pointer: pointer.to_string(),
                    params: vec![name.to_string()],
                    expr: None,
                    value: chosen.clone(),
                });
                *value = chosen.to_json();
            }
            Some(_) => {}
            None if self.params.broken.contains(name) => {}
            None => self.error(
                pointer,
                format!("param '{name}' is not declared in `params`"),
            ),
        }
    }

    /// `{"$expr": "pow(2, size)"}`: the expression's value, from the params.
    fn expression(&mut self, value: &mut Value, pointer: &str) {
        let text = match value.as_object() {
            Some(map) if map.len() == 1 => map.get("$expr").and_then(Value::as_str),
            _ => None,
        };
        let Some(text) = text.map(str::to_string) else {
            self.error(pointer, r#"an expression is exactly {"$expr": "text"}"#);
            return;
        };
        let at = format!("{pointer}/$expr");
        let to_diagnostic = |e: expr::ExprError| {
            let mut d = Diagnostic::new(&at, e.message);
            d.expr_span = Some((e.span.start, e.span.end));
            d
        };
        let e = match Expr::parse(&text) {
            Ok(e) => e,
            Err(e) => {
                self.errors.push(to_diagnostic(e));
                return;
            }
        };
        if let Some(v) = self.params.evaluate(&e, &to_diagnostic, self.errors) {
            self.sites.push(Site {
                pointer: pointer.to_string(),
                params: names_of(&e),
                expr: Some(text),
                value: v.clone(),
            });
            *value = v.to_json();
        }
    }

    fn error(&mut self, pointer: &str, message: impl Into<String>) {
        self.errors.push(Diagnostic::new(pointer, message));
    }
}

#[cfg(test)]
mod tests;
