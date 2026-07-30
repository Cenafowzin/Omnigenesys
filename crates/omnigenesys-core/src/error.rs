use thiserror::Error;

use crate::registry::Dimensions;

/// Runtime/build error produced by a single operator. Carries no `step_id`
/// on purpose: the operator does not know where it sits in the pipeline.
/// The pipeline wraps it into a [`PipelineError`] with the step context.
#[derive(Debug, Error)]
pub enum OpError {
    #[error("invalid config: {0}")]
    Config(#[from] serde_json::Error),

    #[error("unknown layer `{0}`")]
    UnknownLayer(String),

    #[error("unknown tile `{0}`")]
    UnknownTile(String),

    #[error("placement failed: {0}")]
    PlacementFailed(String),

    #[error("{0}")]
    Other(String),
}

/// Structural or runtime error of the pipeline as a whole.
/// Every step-scoped variant carries the `step_id` so the message
/// always points at the offending step (PLANNING §2.3).
#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("unsupported pipeline format version {found}; this build supports version {supported}")]
    UnsupportedVersion { found: u32, supported: u32 },

    #[error("step at index {index} has an empty `id`")]
    EmptyStepId { index: usize },

    #[error("duplicate step id `{step_id}`")]
    DuplicateStepId { step_id: String },

    #[error("step `{step_id}`: unknown operator `{operator}`")]
    UnknownOperator { step_id: String, operator: String },

    #[error("step `{step_id}`: operator `{operator}` requires a {required} map, but the map depth is {depth}")]
    DimensionMismatch {
        step_id: String,
        operator: String,
        required: Dimensions,
        depth: u32,
    },

    #[error("step `{step_id}`: unknown param `{param}`")]
    UnknownParam { step_id: String, param: String },

    #[error(
        "step `{step_id}`: a `$param` reference must be an object with a single string value, e.g. {{\"$param\": \"name\"}}"
    )]
    InvalidParamRef { step_id: String },

    #[error("step `{step_id}`: {source}")]
    BuildFailed {
        step_id: String,
        #[source]
        source: OpError,
    },

    #[error("step `{step_id}` failed: {source}")]
    StepFailed {
        step_id: String,
        #[source]
        source: OpError,
    },
}
