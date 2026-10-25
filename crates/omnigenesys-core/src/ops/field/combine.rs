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
                (CombineOp::Sum, Some(w)) => w as f32,
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
        let mask = match &self.when {
            Some(c) => grid.prepare(c)?.eval(region),
            None => CellMask::full(region),
        };
        // Copies of the inputs: the output may be one of them.
        let inputs = self
            .inputs
            .iter()
            .map(|&(h, w)| Ok((grid.scalars(h)?.clone(), w)))
            .collect::<Result<Vec<_>, OpError>>()?;
        let out = grid.scalars_mut(self.out)?;
        for c in mask.iter_ones() {
            let values = inputs
                .iter()
                .map(|(l, w)| (*l.get(c).unwrap_or(&0.0), *w));
            // There are always two or more inputs, so the infinities never
            // come out.
            let v = match self.op {
                CombineOp::Sum => values.map(|(x, w)| w * x).sum(),
                CombineOp::Multiply => values.map(|(x, _)| x).product(),
                CombineOp::Min => values.map(|(x, _)| x).fold(f32::INFINITY, f32::min),
                CombineOp::Max => values.map(|(x, _)| x).fold(f32::NEG_INFINITY, f32::max),
            };
            out.set(c, if self.clamp { v.clamp(0.0, 1.0) } else { v })?;
        }
        Ok(())
    }
}
