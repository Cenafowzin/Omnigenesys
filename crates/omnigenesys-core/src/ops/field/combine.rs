use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    access::{Reach, ReadHandle, WriteHandle},
    condition::CompiledCondition,
    mask::CellMask,
    operator::{
        BuildContext, BuildError, Context, Dimensions, OpError, Operator, OperatorType, Scope,
    },
};

/// Arithmetic between two scalar layers, cell by cell, into a third (which
/// may be one of them): mixing noise with a gradient makes an island,
/// multiplying two noises makes patches, `min`/`max` carve and raise.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CombineConfig {
    /// First scalar layer.
    pub a: String,
    /// Second scalar layer.
    pub b: String,
    pub op: CombineOp,
    /// For `lerp`: 0 is `a`, 1 is `b`.
    #[serde(default)]
    #[schemars(range(min = 0, max = 1))]
    pub mix: Option<f64>,
    /// The scalar layer written; may be `a` or `b`.
    pub layer: String,
    /// Keep the result between 0 and 1.
    #[serde(default)]
    pub clamp: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CombineOp {
    /// `a + (b − a) · mix`.
    Lerp,
    Add,
    Subtract,
    Multiply,
    Min,
    Max,
}

pub struct Combine {
    a: ReadHandle,
    b: ReadHandle,
    out: WriteHandle,
    op: CombineOp,
    mix: f32,
    clamp: bool,
    when: Option<CompiledCondition>,
}

impl OperatorType for Combine {
    const NAME: &'static str = "Combine";
    const DIMENSIONS: Dimensions = Dimensions::Any;
    const SCOPE: Scope = Scope::Local;
    type Config = CombineConfig;

    fn build(c: CombineConfig, bcx: &mut BuildContext<'_>) -> Result<Combine, BuildError> {
        let invalid = |field: &str, message: String| BuildError::Invalid {
            field: field.to_string(),
            message,
        };
        let mix = match (c.op, c.mix) {
            (CombineOp::Lerp, Some(m)) if (0.0..=1.0).contains(&m) => m as f32,
            (CombineOp::Lerp, Some(m)) => {
                return Err(invalid("mix", format!("{m} is not between 0 and 1")));
            }
            (CombineOp::Lerp, None) => return Err(invalid("mix", "lerp needs mix".to_string())),
            (_, Some(_)) => return Err(invalid("mix", "only lerp takes mix".to_string())),
            (_, None) => 0.0,
        };
        // The layers read and the one written may be the same layer: each
        // cell is read before it is written, at the cell only.
        Ok(Combine {
            a: bcx.read_scalar(&c.a, Reach::ZERO)?,
            b: bcx.read_scalar(&c.b, Reach::ZERO)?,
            out: bcx.write_scalar(&c.layer)?,
            op: c.op,
            mix,
            clamp: c.clamp,
            when: bcx.step_conditions(),
        })
    }
}

impl Operator for Combine {
    fn execute(&self, ctx: &mut Context<'_>) -> Result<(), OpError> {
        let parts = ctx.parts();
        let mut grid = parts.grid;
        let region = grid.region();
        let mask = match &self.when {
            Some(c) => grid.prepare(c)?.eval(region),
            None => CellMask::full(region),
        };
        // Copies of the inputs: the output may be one of them.
        let a = grid.scalars(self.a)?.clone();
        let b = grid.scalars(self.b)?.clone();
        let out = grid.scalars_mut(self.out)?;
        for c in mask.iter_ones() {
            let (x, y) = (*a.get(c).unwrap_or(&0.0), *b.get(c).unwrap_or(&0.0));
            let v = match self.op {
                CombineOp::Lerp => x + (y - x) * self.mix,
                CombineOp::Add => x + y,
                CombineOp::Subtract => x - y,
                CombineOp::Multiply => x * y,
                CombineOp::Min => x.min(y),
                CombineOp::Max => x.max(y),
            };
            out.set(c, if self.clamp { v.clamp(0.0, 1.0) } else { v })?;
        }
        Ok(())
    }
}
