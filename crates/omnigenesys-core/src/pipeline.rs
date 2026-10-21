//! Builds steps into operators and runs them in order (PLANNING §2.3, §2.7).

use std::collections::HashSet;

use thiserror::Error;

use crate::{
    access::Access,
    condition::{Condition, ConditionError},
    grid::{Grid, GridError},
    operator::{BuildContext, BuildError, OpError, Operator, Registry},
    rng::StepRng,
    tile::TileRegistry,
};

/// One step as the pipeline declares it.
#[derive(Debug, Clone)]
pub struct StepSpec {
    pub id: String,
    pub operator: String,
    pub enabled: bool,
    pub config: serde_json::Value,
    /// An implicit `all`, handed to the operator (PLANNING §2.6).
    pub conditions: Vec<Condition>,
}

/// An error of one step, with its id. The operator never has to know its
/// own id for the error to say where it happened (PLANNING §2.7, item 7).
#[derive(Debug, Clone, PartialEq, Error)]
#[error("step '{step_id}': {error}")]
pub struct StepError<E> {
    pub step_id: String,
    pub error: E,
}

struct CompiledStep {
    id: String,
    operator: Box<dyn Operator>,
    access: Access,
}

/// Steps built and ready to run on any grid declaring the same layers, in
/// the same order, as the one they were built against.
pub struct Pipeline {
    steps: Vec<CompiledStep>,
}

impl Pipeline {
    /// Builds every enabled step. Does not stop at the first error: returns
    /// all of them, each with its step id. A missing layer is reported once,
    /// at the first step that names it, not again by every later step.
    pub fn build(
        registry: &Registry,
        grid: &Grid,
        tiles: &TileRegistry,
        specs: &[StepSpec],
    ) -> Result<Pipeline, Vec<StepError<BuildError>>> {
        let mut steps = Vec::new();
        let mut errors = Vec::new();
        let mut ids = HashSet::new();
        let mut missing_layers = HashSet::new();

        for spec in specs.iter().filter(|s| s.enabled) {
            let built = if ids.insert(spec.id.as_str()) {
                build_step(registry, grid, tiles, spec)
            } else {
                Err(BuildError::DuplicateStepId {
                    id: spec.id.clone(),
                })
            };
            match built {
                Ok(step) => steps.push(step),
                Err(error) => {
                    let repeated = missing_layer(&error)
                        .is_some_and(|name| !missing_layers.insert(name.to_string()));
                    if !repeated {
                        errors.push(StepError {
                            step_id: spec.id.clone(),
                            error,
                        });
                    }
                }
            }
        }
        if errors.is_empty() {
            Ok(Pipeline { steps })
        } else {
            Err(errors)
        }
    }

    /// Runs every step in order on `grid`, each with its own randomness.
    /// Stops at the first failing step.
    pub fn run(&self, grid: &mut Grid, salt: &str, seed: u64) -> Result<(), StepError<OpError>> {
        for step in &self.steps {
            let rng = StepRng::new(salt, seed, &step.id);
            let mut ctx = crate::operator::Context::new(grid, &step.access, rng);
            step.operator.execute(&mut ctx).map_err(|error| StepError {
                step_id: step.id.clone(),
                error,
            })?;
        }
        Ok(())
    }

    /// What a step reads and writes, as built.
    pub fn access(&self, step_id: &str) -> Option<&Access> {
        self.steps
            .iter()
            .find(|s| s.id == step_id)
            .map(|s| &s.access)
    }
}

fn build_step(
    registry: &Registry,
    grid: &Grid,
    tiles: &TileRegistry,
    spec: &StepSpec,
) -> Result<CompiledStep, BuildError> {
    let entry = registry
        .get(&spec.operator)
        .ok_or_else(|| BuildError::UnknownOperator {
            name: spec.operator.clone(),
        })?;
    let is_2d = grid.bounds().size.is_2d();
    if !entry.dimensions.accepts(is_2d) {
        return Err(BuildError::WrongDimensions {
            operator: spec.operator.clone(),
            found: if is_2d { "2D" } else { "3D" },
        });
    }
    let conditions = if spec.conditions.is_empty() {
        None
    } else {
        let all = Condition::All {
            conditions: spec.conditions.clone(),
        };
        Some(all.compile(grid, tiles)?)
    };
    let mut bcx = BuildContext::new(grid, tiles, conditions);
    let operator = (entry.factory)(spec.config.clone(), &mut bcx)?;
    let access = bcx.finish(&spec.operator)?;
    Ok(CompiledStep {
        id: spec.id.clone(),
        operator,
        access,
    })
}

/// The layer name, if the error is "this layer does not exist".
fn missing_layer(e: &BuildError) -> Option<&str> {
    match e {
        BuildError::Grid(GridError::LayerNotFound { name })
        | BuildError::Condition(ConditionError::Grid(GridError::LayerNotFound { name })) => {
            Some(name)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests;
