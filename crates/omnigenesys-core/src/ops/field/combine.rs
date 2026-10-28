use schemars::JsonSchema;
use serde::Deserialize;

use super::each_cell;
use crate::{
    access::{Reach, ReadHandle, WriteHandle},
    condition::CompiledCondition,
    operator::{
        BuildContext, BuildError, Context, Dimensions, OpError, Operator, OperatorType, Scope,
    },
};

/// Arithmetic over two or more scalar layers, cell by cell, into one (which
/// may be one of them): a weighted sum of noise and a gradient makes an
/// island, multiplying two noises makes patches, `min`/`max` carve and raise.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CombineConfig {
    /// The scalar layers combined, in order; at least two.
    pub inputs: Vec<CombineInput>,
    pub op: CombineOp,
    /// The scalar layer written; may be one of the inputs.
    pub layer: String,
    /// Keep the result between 0 and 1.
    #[serde(default)]
    pub clamp: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CombineInput {
    pub layer: String,
    /// Only for `sum`; 1 when left out. Weights summing to 1 mix the inputs
    /// and keep the result between them; −1 subtracts.
    #[serde(default)]
    pub weight: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CombineOp {
    /// `Σ weight · value`.
    Sum,
    Multiply,
    Min,
    Max,
}

pub struct Combine {
    inputs: Vec<(ReadHandle, f32)>,
    out: WriteHandle,
    op: CombineOp,
    clamp: bool,
    when: Option<CompiledCondition>,
}

impl OperatorType for Combine {
    const NAME: &'static str = "Combine";
    const DIMENSIONS: Dimensions = Dimensions::Any;
    const SCOPE: Scope = Scope::Local;
    type Config = CombineConfig;

    fn build(c: CombineConfig, bcx: &mut BuildContext<'_>) -> Result<Combine, BuildError> {
        let invalid = |field: String, message: String| BuildError::Invalid { field, message };
        if c.inputs.len() < 2 {
            return Err(invalid(
                "inputs".to_string(),
                format!("needs at least two layers, got {}", c.inputs.len()),
            ));
        }
        let mut inputs = Vec::with_capacity(c.inputs.len());
        for (i, input) in c.inputs.iter().enumerate() {
            if c.inputs[..i].iter().any(|seen| seen.layer == input.layer) {
                return Err(invalid(
                    format!("inputs/{i}/layer"),
                    format!("`{}` is listed twice", input.layer),
                ));
            }
            let weight = match (c.op, input.weight) {
                (CombineOp::Sum, Some(w)) if w.is_finite() => w as f32,
                (CombineOp::Sum, Some(w)) => {
                    return Err(invalid(
                        format!("inputs/{i}/weight"),
                        format!("{w} is not a number"),
                    ));
                }
                (CombineOp::Sum, None) => 1.0,
                (_, Some(_)) => {
                    return Err(invalid(
                        format!("inputs/{i}/weight"),
                        "only sum takes weights".to_string(),
                    ));
                }
                (_, None) => 1.0,
            };
            inputs.push((bcx.read_scalar(&input.layer, Reach::ZERO)?, weight));
        }
        // The layers read and the one written may be the same layer: each
        // cell is read before it is written, at the cell only.
        Ok(Combine {
            inputs,
            out: bcx.write_scalar(&c.layer)?,
            op: c.op,
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
        let mask = grid.mask(self.when.as_ref())?;
        // An input that is the output is read from it: each cell is read
        // before it is written, and no other cell is read.
        let others: Vec<ReadHandle> = self
            .inputs
            .iter()
            .map(|&(h, _)| h)
            .filter(|h| h.layer() != self.out.layer())
            .collect();
        let mut split = grid.split(&others, &[self.out])?;
        let mut reads = split.reads.iter().map(|l| l.as_scalar());
        // `None` for the output.
        let inputs: Vec<(Option<&[f32]>, f32)> = self
            .inputs
            .iter()
            .map(|&(h, w)| {
                let l = (h.layer() != self.out.layer()).then(|| {
                    reads
                        .next()
                        .flatten()
                        .expect("checked when built")
                        .as_slice()
                });
                (l, w)
            })
            .collect();
        let out = split.writes[0]
            .as_scalar_mut()
            .expect("checked when built")
            .as_mut_slice();
        each_cell(region, mask.as_ref(), |_, i| {
            let here = out[i];
            let values = inputs.iter().map(|(l, w)| (l.map_or(here, |l| l[i]), *w));
            // There are always two or more inputs, so the infinities never
            // come out.
            let v = match self.op {
                CombineOp::Sum => values.map(|(x, w)| w * x).sum(),
                CombineOp::Multiply => values.map(|(x, _)| x).product(),
                CombineOp::Min => values.map(|(x, _)| x).fold(f32::INFINITY, f32::min),
                CombineOp::Max => values.map(|(x, _)| x).fold(f32::NEG_INFINITY, f32::max),
            };
            out[i] = if self.clamp { v.clamp(0.0, 1.0) } else { v };
        });
        Ok(())
    }
}
