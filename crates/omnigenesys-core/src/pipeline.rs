//! Two-phase validation (PLANNING §2.3): `Pipeline::build` reports every
//! structural error at once (with `step_id`); `run` handles runtime
//! failures per step according to its `on_fail` policy.

use std::collections::HashSet;

use crate::context::Context;
use crate::error::{OpError, PipelineError};
use crate::registry::{Operator, OperatorRegistry};
use crate::rng::derive_step_rng;
use crate::spec::{resolve_params, OnFail, PipelineSpec, FORMAT_VERSION};

pub struct Pipeline {
    seed: u64,
    salt: String,
    size: crate::coords::GridSize,
    layers: Vec<String>,
    steps: Vec<BuiltStep>,
}

struct BuiltStep {
    id: String,
    on_fail: OnFail,
    enabled: bool,
    operator: Box<dyn Operator>,
}

/// Result of a run: the final context plus warnings from `on_fail: warn`
/// steps. `warn` records, `skip` stays silent — but both are explicit
/// choices in the pipeline JSON, never defaults.
pub struct RunOutcome {
    pub context: Context,
    pub warnings: Vec<StepWarning>,
}

pub struct StepWarning {
    pub step_id: String,
    pub error: OpError,
}

impl Pipeline {
    /// Validates the whole spec and constructs every operator. Collects all
    /// structural errors instead of stopping at the first one, so a user
    /// fixes the pipeline in one pass instead of playing whack-a-mole.
    pub fn build(spec: PipelineSpec, registry: &OperatorRegistry) -> Result<Self, Vec<PipelineError>> {
        if spec.version != FORMAT_VERSION {
            // Field semantics may differ across versions; validating the
            // rest would produce misleading errors.
            return Err(vec![PipelineError::UnsupportedVersion {
                found: spec.version,
                supported: FORMAT_VERSION,
            }]);
        }

        let mut errors = Vec::new();
        let mut seen_ids = HashSet::new();
        let mut steps = Vec::with_capacity(spec.steps.len());

        for (index, step) in spec.steps.into_iter().enumerate() {
            if step.id.is_empty() {
                errors.push(PipelineError::EmptyStepId { index });
                continue;
            }
            if !seen_ids.insert(step.id.clone()) {
                errors.push(PipelineError::DuplicateStepId { step_id: step.id });
                continue;
            }

            let Some(entry) = registry.get(&step.operator) else {
                errors.push(PipelineError::UnknownOperator {
                    step_id: step.id,
                    operator: step.operator,
                });
                continue;
            };

            if !entry.dimensions.supports_depth(spec.size.depth) {
                errors.push(PipelineError::DimensionMismatch {
                    step_id: step.id,
                    operator: step.operator,
                    required: entry.dimensions,
                    depth: spec.size.depth,
                });
                continue;
            }

            let config = match resolve_params(&step.id, &step.config, &spec.params) {
                Ok(config) => config,
                Err(error) => {
                    errors.push(error);
                    continue;
                }
            };

            // Disabled steps are validated and constructed too: a toggle in
            // the editor must never hide a broken step.
            match (entry.factory)(config) {
                Ok(operator) => steps.push(BuiltStep {
                    id: step.id,
                    on_fail: step.on_fail,
                    enabled: step.enabled,
                    operator,
                }),
                Err(source) => errors.push(PipelineError::BuildFailed { step_id: step.id, source }),
            }
        }

        if errors.is_empty() {
            Ok(Self {
                seed: spec.seed,
                salt: spec.salt,
                size: spec.size,
                layers: spec.layers,
                steps,
            })
        } else {
            Err(errors)
        }
    }

    /// Executes every enabled step. `salt_override` supports the
    /// generation-time salt (CLI `--salt`, FFI parameter) without editing
    /// the pipeline JSON — PLANNING §2.1.
    pub fn run(&self, salt_override: Option<&str>) -> Result<RunOutcome, PipelineError> {
        let salt = salt_override.unwrap_or(&self.salt);
        let mut context = Context::new(self.size, self.seed, salt, &self.layers);
        let mut warnings = Vec::new();

        for step in &self.steps {
            if !step.enabled {
                continue;
            }
            context.rng = derive_step_rng(salt, self.seed, &step.id);
            if let Err(error) = step.operator.execute(&mut context) {
                match step.on_fail {
                    OnFail::Error => {
                        return Err(PipelineError::StepFailed { step_id: step.id.clone(), source: error });
                    }
                    OnFail::Warn => warnings.push(StepWarning { step_id: step.id.clone(), error }),
                    OnFail::Skip => {}
                }
            }
        }

        Ok(RunOutcome { context, warnings })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{Dimensions, OperatorEntry};
    use schemars::JsonSchema;
    use serde::Deserialize;
    use serde_json::json;

    /// Test operator: writes `tile` at (0,0,0) of `layer`, or fails.
    #[derive(Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    struct StampConfig {
        layer: String,
        tile: String,
        #[serde(default)]
        fail: bool,
    }

    struct Stamp(StampConfig);

    impl Operator for Stamp {
        fn execute(&self, ctx: &mut Context) -> Result<(), OpError> {
            if self.0.fail {
                return Err(OpError::PlacementFailed("forced by test".into()));
            }
            let tile = ctx.tiles.get_or_insert(&self.0.tile);
            let layer = ctx
                .grid
                .layer_mut(&self.0.layer)
                .ok_or_else(|| OpError::UnknownLayer(self.0.layer.clone()))?;
            layer.set(0, 0, 0, tile);
            Ok(())
        }
    }

    fn test_registry() -> OperatorRegistry {
        let mut registry = OperatorRegistry::new();
        registry.register("Stamp", OperatorEntry {
            factory: |config| Ok(Box::new(Stamp(serde_json::from_value(config)?))),
            schema: || schemars::schema_for!(StampConfig),
            dimensions: Dimensions::TwoD,
        });
        registry
    }

    fn spec(steps: serde_json::Value) -> PipelineSpec {
        serde_json::from_value(json!({
            "version": 2, "seed": 7,
            "size": { "width": 4, "height": 4 },
            "layers": ["terrain"],
            "steps": steps
        }))
        .unwrap()
    }

    #[test]
    fn build_and_run_happy_path() {
        let spec = spec(json!([
            { "id": "a", "operator": "Stamp", "config": { "layer": "terrain", "tile": "floor" } }
        ]));
        let pipeline = Pipeline::build(spec, &test_registry()).unwrap();
        let outcome = pipeline.run(None).unwrap();
        let layer = outcome.context.grid.layer("terrain").unwrap();
        assert_ne!(layer.get(0, 0, 0).unwrap(), crate::tile::EMPTY);
        assert!(outcome.warnings.is_empty());
    }

    #[test]
    fn build_collects_all_errors_at_once() {
        let spec = spec(json!([
            { "id": "a", "operator": "Nope", "config": {} },
            { "id": "a", "operator": "Stamp", "config": { "layer": "t", "tile": "x" } },
            { "id": "c", "operator": "Stamp", "config": { "layer": "t", "tile": "x", "typo": 1 } }
        ]));
        let errors = Pipeline::build(spec, &test_registry()).err().unwrap();
        assert_eq!(errors.len(), 3, "expected all errors reported: {errors:?}");
        assert!(matches!(errors[0], PipelineError::UnknownOperator { .. }));
        assert!(matches!(errors[1], PipelineError::DuplicateStepId { .. }));
        assert!(matches!(errors[2], PipelineError::BuildFailed { .. }));
    }

    #[test]
    fn unsupported_version_is_rejected() {
        let mut spec = spec(json!([]));
        spec.version = 1;
        let errors = Pipeline::build(spec, &test_registry()).err().unwrap();
        assert!(matches!(errors[0], PipelineError::UnsupportedVersion { found: 1, .. }));
    }

    #[test]
    fn dimension_mismatch_is_a_build_error() {
        let mut spec = spec(json!([
            { "id": "a", "operator": "Stamp", "config": { "layer": "terrain", "tile": "floor" } }
        ]));
        spec.size.depth = 32;
        let errors = Pipeline::build(spec, &test_registry()).err().unwrap();
        assert!(matches!(errors[0], PipelineError::DimensionMismatch { .. }));
    }

    #[test]
    fn on_fail_policies() {
        let steps = |policy: &str| {
            spec(json!([{
                "id": "a", "operator": "Stamp", "on_fail": policy,
                "config": { "layer": "terrain", "tile": "floor", "fail": true }
            }]))
        };
        let registry = test_registry();

        let err = Pipeline::build(steps("error"), &registry).unwrap().run(None).err().unwrap();
        assert!(matches!(err, PipelineError::StepFailed { .. }));

        let outcome = Pipeline::build(steps("warn"), &registry).unwrap().run(None).unwrap();
        assert_eq!(outcome.warnings.len(), 1);
        assert_eq!(outcome.warnings[0].step_id, "a");

        let outcome = Pipeline::build(steps("skip"), &registry).unwrap().run(None).unwrap();
        assert!(outcome.warnings.is_empty());
    }

    #[test]
    fn disabled_step_is_validated_but_not_executed() {
        let broken_disabled = spec(json!([
            { "id": "a", "operator": "Nope", "enabled": false, "config": {} }
        ]));
        assert!(Pipeline::build(broken_disabled, &test_registry()).is_err());

        let valid_disabled = spec(json!([{
            "id": "a", "operator": "Stamp", "enabled": false,
            "config": { "layer": "terrain", "tile": "floor" }
        }]));
        let outcome = Pipeline::build(valid_disabled, &test_registry()).unwrap().run(None).unwrap();
        let layer = outcome.context.grid.layer("terrain").unwrap();
        assert_eq!(layer.get(0, 0, 0).unwrap(), crate::tile::EMPTY);
    }

    #[test]
    fn param_resolution_feeds_operator_config() {
        let spec: PipelineSpec = serde_json::from_value(json!({
            "version": 2, "seed": 7,
            "size": { "width": 4, "height": 4 },
            "layers": ["terrain"],
            "params": { "ground": "floor" },
            "steps": [{
                "id": "a", "operator": "Stamp",
                "config": { "layer": "terrain", "tile": { "$param": "ground" } }
            }]
        }))
        .unwrap();
        let outcome = Pipeline::build(spec, &test_registry()).unwrap().run(None).unwrap();
        let context = outcome.context;
        let tile = context.grid.layer("terrain").unwrap().get(0, 0, 0).unwrap();
        assert_eq!(context.tiles.name(tile), Some("floor"));
    }

    #[test]
    fn salt_override_changes_step_rng_stream() {
        // Indirect check via derivation: same pipeline, different salt,
        // different stream for the same step id.
        use rand::RngCore;
        let mut a = derive_step_rng("", 7, "a");
        let mut b = derive_step_rng("other", 7, "a");
        assert_ne!(a.next_u64(), b.next_u64());
    }
}
