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
        (0..self.steps.len()).try_for_each(|i| self.run_step(i, grid, salt, seed))
    }

    /// Runs only step `i` (of the enabled steps, in order). Running them
    /// all one by one, in order, is exactly [`run`](Self::run): each step's
    /// randomness depends on its id, not on what ran before. For an editor
    /// that shows the map after each step.
    ///
    /// # Panics
    ///
    /// If `i` is not below [`len`](Self::len).
    pub fn run_step(
        &self,
        i: usize,
        grid: &mut Grid,
        salt: &str,
        seed: u64,
    ) -> Result<(), StepError<OpError>> {
        self.run_step_in(i, grid, salt, seed, 0)
    }

    /// [`run_step`](Self::run_step) in attempt `attempt` of the map, each
    /// attempt with its own randomness ([`StepRng::with_attempt`]).
    ///
    /// # Panics
    ///
    /// If `i` is not below [`len`](Self::len).
    pub fn run_step_in(
        &self,
        i: usize,
        grid: &mut Grid,
        salt: &str,
        seed: u64,
        attempt: u32,
    ) -> Result<(), StepError<OpError>> {
        let step = &self.steps[i];
        let rng = StepRng::with_attempt(salt, seed, &step.id, attempt);
        let mut ctx = crate::operator::Context::new(grid, &step.access, rng);
        step.operator.execute(&mut ctx).map_err(|error| StepError {
            step_id: step.id.clone(),
            error,
        })
    }

    /// Makes the map from `empty` (the grid as built), up to `attempts`
    /// times. A step that fails as a map can fail — a `Check`, a step with
    /// nothing to pick ([`OpError::Failed`]) — throws the attempt away, and
    /// the next starts again from `empty` with its own randomness. Any
    /// other error is a bug in the pipeline and stops at once.
    pub fn run_attempts(
        &self,
        empty: &Grid,
        salt: &str,
        seed: u64,
        attempts: u32,
    ) -> Result<Made, RunError> {
        let mut failures = Vec::new();
        for attempt in 0..attempts {
            let mut grid = empty.clone();
            match (0..self.len())
                .try_for_each(|i| self.run_step_in(i, &mut grid, salt, seed, attempt))
            {
                Ok(()) => {
                    return Ok(Made {
                        grid,
                        attempt,
                        failures,
                    });
                }
                Err(e) if retries(&e.error) => failures.push(e),
                Err(e) => return Err(RunError::Step(e)),
            }
        }
        Err(RunError::GaveUp { failures })
    }

    /// Enabled steps, the ones that run.
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// The id of step `i`, in run order.
    pub fn step_id(&self, i: usize) -> Option<&str> {
        self.steps.get(i).map(|s| s.id.as_str())
    }

    /// What a step reads and writes, as built.
    pub fn access(&self, step_id: &str) -> Option<&Access> {
        self.steps
            .iter()
            .find(|s| s.id == step_id)
            .map(|s| &s.access)
    }
}

/// Whether a step error is the map failing (worth another attempt) rather
/// than the pipeline being wrong.
pub fn retries(e: &OpError) -> bool {
    matches!(e, OpError::Failed { .. })
}

/// A map made by [`Pipeline::run_attempts`].
#[derive(Debug)]
pub struct Made {
    pub grid: Grid,
    /// The attempt that made it, from 0.
    pub attempt: u32,
    /// Why each attempt before it was thrown away.
    pub failures: Vec<StepError<OpError>>,
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum RunError {
    /// Every attempt failed; why, attempt by attempt.
    #[error("{}", gave_up(.failures))]
    GaveUp { failures: Vec<StepError<OpError>> },
    #[error(transparent)]
    Step(StepError<OpError>),
}

/// "every one of the 50 attempts failed (cota 31 times, chefe 19 times);
/// the last: step 'cota': ...".
fn gave_up(failures: &[StepError<OpError>]) -> String {
    let mut times: Vec<(&str, usize)> = Vec::new();
    for f in failures {
        match times.iter_mut().find(|(id, _)| *id == f.step_id) {
            Some((_, n)) => *n += 1,
            None => times.push((&f.step_id, 1)),
        }
    }
    times.sort_by(|a, b| b.1.cmp(&a.1));
    let counts: Vec<String> = times
        .iter()
        .map(|(id, n)| format!("{id} {n} time{}", if *n == 1 { "" } else { "s" }))
        .collect();
    match failures.last() {
        Some(last) if failures.len() == 1 => format!("the map failed: {last}"),
        Some(last) => format!(
            "every one of the {} attempts failed ({}); the last: {last}",
            failures.len(),
            counts.join(", ")
        ),
        None => "no attempt was made".to_string(),
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
    let mut bcx = BuildContext::new(grid, tiles, conditions, entry.scope);
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
