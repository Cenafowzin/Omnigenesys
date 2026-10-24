use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    access::WriteHandle,
    condition::CompiledCondition,
    mask::CellMask,
    operator::{
        BuildContext, BuildError, Context, Dimensions, OpError, Operator, OperatorType, Scope,
    },
};

/// Writes a distance field into a scalar layer: 0 at the centre of the map
/// (or its left or top edge), 1 at the edges — or the other way round with
/// `invert`. The shapes are Red Blob Games' island distances ("Making maps
/// with noise functions"): `round` = `(x² + y²) / √2`, `square` =
/// `1 − (1 − x²)(1 − y²)`, with x and y from −1 to 1 across the map.
///
/// The map's edges, never a chunk's: every chunk computes the same value
/// for the same cell.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GradientConfig {
    /// A scalar layer.
    pub layer: String,
    pub shape: GradientShape,
    /// 1 at the centre and 0 at the edges, instead.
    #[serde(default)]
    pub invert: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum GradientShape {
    /// A round island in the middle.
    Round,
    /// Fills the map, square with rounded corners.
    Square,
    /// 0 at the left edge, 1 at the right.
    Horizontal,
    /// 0 at the top edge, 1 at the bottom.
    Vertical,
}

pub struct Gradient {
    layer: WriteHandle,
    shape: GradientShape,
    invert: bool,
    when: Option<CompiledCondition>,
}

impl OperatorType for Gradient {
    const NAME: &'static str = "Gradient";
    const DIMENSIONS: Dimensions = Dimensions::Any;
    const SCOPE: Scope = Scope::Local;
    type Config = GradientConfig;

    fn build(c: GradientConfig, bcx: &mut BuildContext<'_>) -> Result<Gradient, BuildError> {
        Ok(Gradient {
            layer: bcx.write_scalar(&c.layer)?,
            shape: c.shape,
            invert: c.invert,
            when: bcx.step_conditions(),
        })
    }
}

impl Operator for Gradient {
    fn execute(&self, ctx: &mut Context<'_>) -> Result<(), OpError> {
        let parts = ctx.parts();
        let mut grid = parts.grid;
        let region = grid.region();
        let mask = match &self.when {
            Some(c) => grid.prepare(c)?.eval(region),
            None => CellMask::full(region),
        };
        let map = grid.map();
        let (w, h) = (f64::from(map.size.width), f64::from(map.size.height));
        let layer = grid.scalars_mut(self.layer)?;
        for c in mask.iter_ones() {
            // At the centre of the cell: 0..1 across the map, and −1..1
            // (in this order of operations: it fixes the last bit).
            let (dx, dy) = (f64::from(c.x - map.origin.x), f64::from(c.y - map.origin.y));
            let (ux, uy) = ((dx + 0.5) / w, (dy + 0.5) / h);
            let nx = (2.0 * (dx + 0.5) / w - 1.0) as f32;
            let ny = (2.0 * (dy + 0.5) / h - 1.0) as f32;
            let d = match self.shape {
                GradientShape::Square => 1.0 - (1.0 - nx * nx) * (1.0 - ny * ny),
                GradientShape::Round => ((nx * nx + ny * ny) / std::f32::consts::SQRT_2).min(1.0),
                GradientShape::Horizontal => ux as f32,
                GradientShape::Vertical => uy as f32,
            };
            layer.set(c, if self.invert { 1.0 - d } else { d })?;
        }
        Ok(())
    }
}
