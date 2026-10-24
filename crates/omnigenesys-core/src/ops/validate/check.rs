use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    condition::CompiledCondition,
    operator::{
        BuildContext, BuildError, Context, Dimensions, OpError, Operator, OperatorType, Scope,
    },
};

/// Counts the cells where the step's conditions hold and fails the step
/// when the count is below `min` or above `max` (both counted): "the
/// largest cave has at least 300 cells", "under 5% of the map is lava".
/// Changes nothing.
///
/// The rule a game uses to throw a map away is a step of its own, in sight
/// and editable, never a hidden option of the operator that made the map
/// (PLANNING §2.14). Retrying with another seed when it fails belongs to
/// the pipeline (decided, not built yet).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CheckConfig {
    /// Fewest cells allowed.
    #[serde(default)]
    pub min: Option<u64>,
    /// Most cells allowed.
    #[serde(default)]
    pub max: Option<u64>,
    /// What to tell the user when it fails, instead of the count.
    #[serde(default)]
    pub message: Option<String>,
}

pub struct Check {
    min: u64,
    max: u64,
    message: Option<String>,
    when: CompiledCondition,
}

impl OperatorType for Check {
    const NAME: &'static str = "Check";
    const DIMENSIONS: Dimensions = Dimensions::Any;
    // A count over the whole map: a chunk sees only its part.
    const SCOPE: Scope = Scope::Global;
    type Config = CheckConfig;

    fn build(c: CheckConfig, bcx: &mut BuildContext<'_>) -> Result<Check, BuildError> {
        let invalid = |field: &str, message: String| BuildError::Invalid {
            field: field.to_string(),
            message,
        };
        let (min, max) = match (c.min, c.max) {
            (None, None) => return Err(invalid("min", "needs `min`, `max` or both".into())),
            (Some(min), Some(max)) if min > max => {
                return Err(invalid("min", format!("{min} is above max {max}")));
            }
            (min, max) => (min.unwrap_or(0), max.unwrap_or(u64::MAX)),
        };
        let when = bcx.step_conditions().ok_or_else(|| {
            invalid(
                "conditions",
                "say which cells to count with the step's conditions".into(),
            )
        })?;
        Ok(Check {
            min,
            max,
            message: c.message,
            when,
        })
    }
}

impl Operator for Check {
    fn execute(&self, ctx: &mut Context<'_>) -> Result<(), OpError> {
        let parts = ctx.parts();
        let grid = parts.grid;
        let count = grid.prepare(&self.when)?.eval(grid.region()).count_ones() as u64;
        if (self.min..=self.max).contains(&count) {
            return Ok(());
        }
        let message = match &self.message {
            Some(m) => format!("{m} ({count} cells)"),
            None if count < self.min => format!("{count} cells match, fewer than min {}", self.min),
            None => format!("{count} cells match, more than max {}", self.max),
        };
        Err(OpError::Failed { message })
    }
}

#[cfg(test)]
mod tests;
