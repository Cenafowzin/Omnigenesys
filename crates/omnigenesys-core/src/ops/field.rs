//! Scalar fields: noise, distance fields, arithmetic between fields, curves.

mod combine;
mod curve;
mod linear_gradient;
mod noise;
mod radial_gradient;

pub use combine::{Combine, CombineConfig, CombineInput, CombineOp};
pub use curve::{Curve, CurveConfig, CurveKind, Interpolation, SmoothOrder};
pub use linear_gradient::{LinearGradient, LinearGradientConfig};
pub use noise::{Noise, NoiseConfig, NoiseKind};
pub use radial_gradient::{RadialGradient, RadialGradientConfig, RadiusOf};

use crate::{bounds::Bounds, coords::Coord, mask::CellMask};

/// Calls `f` with each cell a field step writes and its place in the
/// layers' slices. Without conditions that is every cell of `region`, in
/// slice order: a plain run, with no coordinate turned into an index and
/// no bounds check per cell (the ilha's field steps cost twice the same
/// arithmetic by hand before it, bench_recipes 2026-10-27). With them, the
/// cells of `mask`, in the same order. Every layer of a step covers
/// `region`, so the place indexes any of them.
fn each_cell(region: Bounds, mask: Option<&CellMask>, mut f: impl FnMut(Coord, usize)) {
    match mask {
        Some(mask) => {
            for c in mask.iter_ones() {
                f(c, region.index_of(c).expect("the mask covers the region"));
            }
        }
        None => {
            let o = region.origin;
            let mut i = 0;
            for z in 0..region.size.depth as i32 {
                for y in 0..region.size.height as i32 {
                    for x in 0..region.size.width as i32 {
                        f(Coord::new(o.x + x, o.y + y, o.z + z), i);
                        i += 1;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
