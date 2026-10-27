use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    access::WriteHandle,
    condition::CompiledCondition,
    mask::CellMask,
    noise::{Fractal, FractalKind},
    operator::{
        BuildContext, BuildError, Context, Dimensions, OpError, Operator, OperatorType, Scope,
    },
};

/// Writes coherent noise into a scalar layer: values from 0 to 1 that
/// change smoothly, the base of terrain (PLANNING §2.11).
///
/// OpenSimplex2 summed in octaves (fBm, or ridged for sharp crests).
/// Sampled at world coordinates: a cell's value depends only on the seed,
/// the step and the cell, so chunks of one map agree. Normalized by the
/// sum of the amplitudes, never by this map's own minimum and maximum
/// (which would change with the map's size).
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoiseConfig {
    /// A scalar layer.
    pub layer: String,
    /// Size of a feature, in cells: 32 makes hills about 32 cells across.
    #[schemars(range(min = 1))]
    pub wavelength: f64,
    /// Layers of detail, each half the size of the one before.
    #[serde(default = "default_octaves")]
    #[schemars(range(min = 1, max = 12))]
    pub octaves: u32,
    /// How much each octave counts against the one before (0 to 1).
    #[serde(default = "default_persistence")]
    #[schemars(range(min = 0, max = 1))]
    pub persistence: f64,
    /// How much smaller each octave is (usually 2).
    #[serde(default = "default_lacunarity")]
    #[schemars(range(min = 1, max = 4))]
    pub lacunarity: f64,
    #[serde(default)]
    pub kind: NoiseKind,
}

fn default_octaves() -> u32 {
    4
}
fn default_persistence() -> f64 {
    0.5
}
fn default_lacunarity() -> f64 {
    2.0
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum NoiseKind {
    /// Rolling hills, smooth coasts.
    #[default]
    Fbm,
    /// Sharp crests and broken coasts: archipelagos, mountain ridges.
    Ridged,
}

pub struct Noise {
    layer: WriteHandle,
    frequency: f64,
    fractal: Fractal,
    when: Option<CompiledCondition>,
}

impl OperatorType for Noise {
    const NAME: &'static str = "Noise";
    const DIMENSIONS: Dimensions = Dimensions::TwoD;
    const SCOPE: Scope = Scope::Local;
    type Config = NoiseConfig;

    fn build(c: NoiseConfig, bcx: &mut BuildContext<'_>) -> Result<Noise, BuildError> {
        let invalid = |field: &str, message: String| BuildError::Invalid {
            field: field.to_string(),
            message,
        };
        if !(c.wavelength.is_finite() && c.wavelength >= 1.0) {
            return Err(invalid(
                "wavelength",
                format!("{} is not 1 or more", c.wavelength),
            ));
        }
        if !(1..=12).contains(&c.octaves) {
            return Err(invalid(
                "octaves",
                format!("{} is not between 1 and 12", c.octaves),
            ));
        }
        if !(0.0..=1.0).contains(&c.persistence) {
            return Err(invalid(
                "persistence",
                format!("{} is not between 0 and 1", c.persistence),
            ));
        }
        if !(1.0..=4.0).contains(&c.lacunarity) {
            return Err(invalid(
                "lacunarity",
                format!("{} is not between 1 and 4", c.lacunarity),
            ));
        }
        let kind = match c.kind {
            NoiseKind::Fbm => FractalKind::Fbm,
            NoiseKind::Ridged => FractalKind::Ridged,
        };
        Ok(Noise {
            layer: bcx.write_scalar(&c.layer)?,
            frequency: 1.0 / c.wavelength,
            fractal: Fractal::new(c.octaves, c.lacunarity, c.persistence as f32, kind),
            when: bcx.step_conditions(),
        })
    }
}

impl Operator for Noise {
    fn execute(&self, ctx: &mut Context<'_>) -> Result<(), OpError> {
        let parts = ctx.parts();
        let mut grid = parts.grid;
        // One draw per step, the same for every cell and every chunk: the
        // field's seed, not a per-cell choice.
        let seed = parts.rng.next_u64() as i64;
        let region = grid.region();
        let mask = grid
            .mask(self.when.as_ref())?
            .unwrap_or_else(|| CellMask::full(region));
        let layer = grid.scalars_mut(self.layer)?;
        let f = self.frequency;
        for c in mask.iter_ones() {
            let v = self
                .fractal
                .sample2(seed, f64::from(c.x) * f, f64::from(c.y) * f);
            layer.set(c, ((v + 1.0) * 0.5).clamp(0.0, 1.0))?;
        }
        Ok(())
    }
}
