//! Operator registry (PLANNING §2.2) — the extensibility core.
//! Built-ins register in `OperatorRegistry::default()`; third-party crates
//! call `register` with their own entries, zero changes to the core.

use std::collections::HashMap;
use std::fmt;

use crate::context::Context;
use crate::error::OpError;

pub trait Operator: Send + Sync {
    fn execute(&self, ctx: &mut Context) -> Result<(), OpError>;
}

/// Builds an operator from the step's (param-resolved) `config` value.
/// Operators deserialize their own config with `#[serde(deny_unknown_fields)]`,
/// so a typo is an error, never silence.
pub type OperatorFactory = fn(serde_json::Value) -> Result<Box<dyn Operator>, OpError>;

/// Which map dimensionality an operator supports. Validated against
/// `size.depth` when the pipeline is built, so a 2D-only operator on a
/// depth-32 map fails with a clear message instead of misbehaving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dimensions {
    TwoD,
    ThreeD,
    Any,
}

impl Dimensions {
    pub fn supports_depth(self, depth: u32) -> bool {
        match self {
            Dimensions::TwoD => depth == 1,
            Dimensions::ThreeD => depth > 1,
            Dimensions::Any => true,
        }
    }
}

impl fmt::Display for Dimensions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Dimensions::TwoD => "2D",
            Dimensions::ThreeD => "3D",
            Dimensions::Any => "2D/3D",
        })
    }
}

pub struct OperatorEntry {
    pub factory: OperatorFactory,
    /// JSON Schema of the operator's config — feeds validation, the web
    /// editor UI and AI tooling. A function (not a static string) so it is
    /// generated from the config type by `schemars` and can never desync.
    pub schema: fn() -> schemars::Schema,
    pub dimensions: Dimensions,
}

#[derive(Default)]
pub struct OperatorRegistry {
    entries: HashMap<String, OperatorEntry>,
}

impl OperatorRegistry {
    /// Empty registry. Use `default()` to get the built-in operators
    /// (none yet — the first one, `Fill`, lands right after Fase 0).
    pub fn new() -> Self {
        Self { entries: HashMap::new() }
    }

    pub fn register(&mut self, name: impl Into<String>, entry: OperatorEntry) {
        self.entries.insert(name.into(), entry);
    }

    pub fn get(&self, name: &str) -> Option<&OperatorEntry> {
        self.entries.get(name)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use schemars::JsonSchema;
    use serde::Deserialize;

    #[derive(Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    struct NoopConfig {
        #[serde(default)]
        _unused: bool,
    }

    struct Noop;

    impl Operator for Noop {
        fn execute(&self, _ctx: &mut Context) -> Result<(), OpError> {
            Ok(())
        }
    }

    fn noop_factory(config: serde_json::Value) -> Result<Box<dyn Operator>, OpError> {
        let _config: NoopConfig = serde_json::from_value(config)?;
        Ok(Box::new(Noop))
    }

    #[test]
    fn register_and_get() {
        let mut registry = OperatorRegistry::new();
        registry.register("Noop", OperatorEntry {
            factory: noop_factory,
            schema: || schemars::schema_for!(NoopConfig),
            dimensions: Dimensions::Any,
        });

        let entry = registry.get("Noop").expect("registered");
        assert!(entry.dimensions.supports_depth(1));
        assert!((entry.factory)(serde_json::json!({})).is_ok());
        assert!(registry.get("Unknown").is_none());
    }

    #[test]
    fn factory_rejects_unknown_config_field() {
        let result = noop_factory(serde_json::json!({ "typo": 1 }));
        assert!(matches!(result, Err(OpError::Config(_))));
    }

    #[test]
    fn dimensions_supports_depth() {
        assert!(Dimensions::TwoD.supports_depth(1));
        assert!(!Dimensions::TwoD.supports_depth(32));
        assert!(!Dimensions::ThreeD.supports_depth(1));
        assert!(Dimensions::ThreeD.supports_depth(32));
        assert!(Dimensions::Any.supports_depth(1));
        assert!(Dimensions::Any.supports_depth(32));
    }
}
