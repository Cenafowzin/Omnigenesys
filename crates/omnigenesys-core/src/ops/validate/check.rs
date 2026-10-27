use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    access::ReadHandle,
    condition::CompiledCondition,
    filter::{CompiledFilter, ObjectFilter, Scene, Subject},
    operator::{
        BuildContext, BuildError, Context, Dimensions, OpError, Operator, OperatorType, Scope,
    },
};

/// Counts the cells where the step's conditions hold — or, with
/// `objects`, the objects of that layer that pass `where` — and fails the
/// step when the count is below `min` or above `max` (both counted): "the
/// largest cave has at least 300 cells", "under 5% of the map is lava", "a
/// floor has at least 5 dead ends". Changes nothing.
///
/// The rule a game uses to throw a map away is a step of its own, in sight
/// and editable, never a hidden option of the operator that made the map
/// (PLANNING §2.14). When it fails, a pipeline with `attempts` makes the
/// map again.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CheckConfig {
    /// Count objects of this object layer instead of cells.
    #[serde(default)]
    pub objects: Option<String>,
    /// With `objects`: which of them count (all, if left out).
    #[serde(rename = "where", default)]
    pub filter: Vec<ObjectFilter>,
    /// Fewest allowed.
    #[serde(default)]
    pub min: Option<u64>,
    /// Most allowed.
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
    what: What,
}

enum What {
    Cells(CompiledCondition),
    Objects {
        layer: ReadHandle,
        reads: Vec<ReadHandle>,
        filter: CompiledFilter,
    },
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
        let what = match &c.objects {
            Some(name) => {
                let layer = bcx.read_objects(name)?;
                let filter = bcx.filter("where", &c.filter, layer.layer())?;
                let mut reads = vec![layer];
                for l in filter.layers() {
                    if reads.iter().all(|r| r.layer() != l) {
                        reads.push(ReadHandle { layer: l });
                    }
                }
                What::Objects {
                    layer,
                    reads,
                    filter,
                }
            }
            None => {
                if !c.filter.is_empty() {
                    return Err(invalid(
                        "where",
                        "`where` picks objects: say which layer with `objects`".into(),
                    ));
                }
                What::Cells(bcx.step_conditions().ok_or_else(|| {
                    invalid(
                        "conditions",
                        "say which cells to count with the step's conditions, \
                         or which objects with `objects`"
                            .into(),
                    )
                })?)
            }
        };
        Ok(Check {
            min,
            max,
            message: c.message,
            what,
        })
    }
}

impl Operator for Check {
    fn execute(&self, ctx: &mut Context<'_>) -> Result<(), OpError> {
        let parts = ctx.parts();
        let mut grid = parts.grid;
        let (count, what) = match &self.what {
            What::Cells(when) => (
                grid.prepare(when)?.eval(grid.region()).count_ones() as u64,
                "cells",
            ),
            What::Objects {
                layer,
                reads,
                filter,
            } => {
                let map = grid.map();
                let filter = grid.prepare_filter(filter)?;
                let split = grid.split(reads, &[])?;
                let scene = Scene::new(
                    map,
                    reads
                        .iter()
                        .zip(&split.reads)
                        .filter_map(|(h, l)| Some((h.layer(), l.as_objects()?))),
                );
                let id = layer.layer();
                let n = (0..scene.objects(id).len())
                    .filter(|&i| filter.test(&scene, id, Subject::Object(i)))
                    .count();
                (n as u64, "objects")
            }
        };
        if (self.min..=self.max).contains(&count) {
            return Ok(());
        }
        let message = match &self.message {
            Some(m) => format!("{m} ({count} {what})"),
            None if count < self.min => {
                format!("{count} {what} match, fewer than min {}", self.min)
            }
            None => format!("{count} {what} match, more than max {}", self.max),
        };
        Err(OpError::Failed { message })
    }
}

#[cfg(test)]
mod tests;
