//! The operators that ship with the core, one folder per kind of work.
//! Each operator is one procedural method (CLAUDE.md, PLANNING §2.14);
//! what a map does with them is in the pipeline's steps and conditions.
//!
//! - `terrain`: `Fill`.
//! - `scatter`: `Scatter`.
//! - `field`: scalar layers — `Noise`, `RadialGradient`,
//!   `LinearGradient`, `Combine`, `Curve`.
//! - `shape`: `CellularAutomaton`, `RegionSize`.
//! - `validate`: `Check`.
//! - `objects`: `PlaceObjects`, `GrowObjects`, `MeasureObjects`,
//!   `TagObjects`, `ConnectObjects`.
//! - `paths`: `CarvePaths`.
//!
//! The folders are for reading the code; the public paths stay flat
//! (`ops::Fill`), so moving an operator between them breaks nobody.

mod field;
mod objects;
mod paths;
mod scatter;
mod shape;
mod terrain;
mod validate;

pub use field::{
    Combine, CombineConfig, CombineInput, CombineOp, Curve, CurveConfig, CurveKind, Interpolation,
    LinearGradient, LinearGradientConfig, Noise, NoiseConfig, NoiseKind, RadialGradient,
    RadialGradientConfig, RadiusOf, SmoothOrder,
};
pub use objects::{
    ConnectObjects, ConnectObjectsConfig, Count, CountRange, CountWord, Expand, Frontier,
    GrowObjects, GrowObjectsConfig, Link, Measure, MeasureObjects, MeasureObjectsConfig,
    OnExhausted, OrderKey, PlaceObjects, PlaceObjectsConfig, Retire, Side, Size, TagObjects,
    TagObjectsConfig,
};
pub use paths::{Blocked, CarvePaths, CarvePathsConfig, Cost, CostRule, PathShape};
pub use scatter::{Scatter, ScatterConfig};
pub use shape::{
    CellularAutomaton, CellularAutomatonConfig, Connectivity, Outside, RegionSize, RegionSizeConfig,
};
pub use terrain::{Fill, FillConfig};
pub use validate::{Check, CheckConfig};

use crate::operator::{OperatorType, Registry};

fn add<O: OperatorType>(r: &mut Registry) {
    r.register::<O>()
        .expect("built-in operator names are unique");
}

pub(crate) fn register_builtins(r: &mut Registry) {
    add::<CarvePaths>(r);
    add::<CellularAutomaton>(r);
    add::<Check>(r);
    add::<Combine>(r);
    add::<ConnectObjects>(r);

    add::<Curve>(r);
    add::<Fill>(r);
    add::<GrowObjects>(r);
    add::<LinearGradient>(r);
    add::<MeasureObjects>(r);
    add::<Noise>(r);
    add::<PlaceObjects>(r);

    add::<RadialGradient>(r);
    add::<RegionSize>(r);
    add::<Scatter>(r);
    add::<TagObjects>(r);
}
