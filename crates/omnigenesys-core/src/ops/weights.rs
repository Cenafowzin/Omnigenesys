//! Weighted picks, shared by the operators that pick a variant (of a cell,
//! of an object): whole-number weights and one 32-bit draw, no fraction
//! on the way.

use crate::{operator::BuildError, rng::pick};

const MAX_VARIANTS: usize = 256;
const MAX_WEIGHT: u32 = 1_000_000;

/// The weights of a config, checked, as running totals: variant `i` takes
/// the draws below `ends[i]`.
#[derive(Debug, Clone)]
pub(crate) struct Weights {
    ends: Vec<u32>,
}

impl Weights {
    /// From 1 to 256 weights, each from 0 to 1000000, not all 0; errors
    /// name the field `weights`.
    pub(crate) fn new(weights: &[u32]) -> Result<Weights, BuildError> {
        let invalid = |message: String| BuildError::Invalid {
            field: "weights".into(),
            message,
        };
        if !(1..=MAX_VARIANTS).contains(&weights.len()) {
            return Err(invalid(format!(
                "{} weights; from 1 to {MAX_VARIANTS}",
                weights.len()
            )));
        }
        if let Some((i, w)) = weights.iter().enumerate().find(|(_, w)| **w > MAX_WEIGHT) {
            return Err(invalid(format!("weight {i} is {w}, above {MAX_WEIGHT}")));
        }
        let ends: Vec<u32> = weights
            .iter()
            .scan(0, |sum, w| {
                *sum += w;
                Some(*sum)
            })
            .collect();
        if ends.last() == Some(&0) {
            return Err(invalid("every weight is 0: nothing could be picked".into()));
        }
        Ok(Weights { ends })
    }

    /// The variant a draw `r` (uniform over u32) gives.
    #[inline]
    pub(crate) fn pick(&self, r: u32) -> u32 {
        let total = *self.ends.last().expect("checked when built");
        let x = pick(r, total);
        // The first variant whose running total passes the draw; one with
        // weight 0 never does.
        self.ends.partition_point(|&end| end <= x) as u32
    }
}
