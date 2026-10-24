//! Connected regions of a set of tiles, found by runs: BRTS (Lee,
//! Allegretti, Bolelli, Grana, 2021), the run-based labelling for packed
//! bits (PLANNING §2.9).
//!
//! Each row is packed to bits and cut into runs (maximal stretches of
//! region cells) with word operations. A run is joined, by union-find, to
//! the runs it touches in the rows already seen: the row above, and in 3D
//! the rows of the layer below. Two runs of the same row never touch, being
//! maximal, so that is every contact.
//!
//! The union keeps the smaller label as the root, and labels are handed
//! out in scan order, so a region's root is always its first run: ties
//! between regions resolve to the first one in scan order, whatever order
//! the unions happened in. No randomness, no dependence on the platform.

use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    coords::GridSize,
    tile::{TileId, TileSet},
};

use super::bits::pack;

/// Which neighbours share a region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Connectivity {
    /// Across a side only: 4 neighbours in 2D, 6 in 3D.
    Orthogonal,
    /// Across a side, an edge or a corner: 8 neighbours in 2D, 26 in 3D.
    Diagonal,
}

/// A run of region cells in one row, `start..end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Run {
    start: u32,
    end: u32,
}

impl Run {
    /// Whether two runs of neighbouring rows touch: sharing a column, or
    /// with `diagonal`, also only by a corner.
    #[inline]
    fn touches(self, other: Run, diagonal: bool) -> bool {
        if diagonal {
            self.start <= other.end && other.start <= self.end
        } else {
            self.start < other.end && other.start < self.end
        }
    }
}

/// Every region of a layer: its runs, each with the root of its region,
/// and the size of each region by root.
pub(super) struct Regions {
    width: usize,
    runs: Vec<Run>,
    /// Runs of row `r` (row `y + z * height`): `runs[rows[r]..rows[r + 1]]`.
    rows: Vec<u32>,
    /// Root of each run's region, the index of its first run.
    root: Vec<u32>,
    /// Cells of the region rooted at each run; 0 for runs that are not roots.
    size: Vec<u64>,
}

impl Regions {
    /// # Panics
    ///
    /// If the layer has `u32::MAX` runs or more, or a row is wider than
    /// `u32::MAX` cells.
    pub(super) fn label(
        cells: &[TileId],
        size: GridSize,
        region: &TileSet,
        connectivity: Connectivity,
    ) -> Regions {
        let (w, h, d) = (
            size.width as usize,
            size.height as usize,
            size.depth as usize,
        );
        let diagonal = connectivity == Connectivity::Diagonal;
        let mut runs = Vec::new();
        let mut rows = vec![0u32];
        let mut parent: Vec<u32> = Vec::new();
        let mut words = vec![0u64; w.div_ceil(64)];
        let mut bytes = [0u8; 64];
        let index = |n: usize| u32::try_from(n).expect("fewer than u32::MAX runs");

        for (r, row) in cells.chunks_exact(w.max(1)).take(h * d).enumerate() {
            for (k, chunk) in row.chunks(64).enumerate() {
                bytes.fill(0);
                for (b, &t) in bytes.iter_mut().zip(chunk) {
                    *b = u8::from(region.contains(t));
                }
                words[k] = pack(&bytes);
            }
            let first = runs.len();
            cut(&words, w, &mut runs);
            for i in first..runs.len() {
                parent.push(index(i));
            }
            rows.push(index(runs.len()));

            // The rows before this one that can touch it.
            let (y, z) = (r % h, r / h);
            let this = first..runs.len();
            let mut join = |other: usize, diagonal: bool| {
                let other = rows[other] as usize..rows[other + 1] as usize;
                merge(&runs, this.clone(), other, diagonal, &mut parent);
            };
            if y > 0 {
                join(r - 1, diagonal);
            }
            if z > 0 {
                join(r - h, diagonal);
                if diagonal {
                    if y > 0 {
                        join(r - h - 1, true);
                    }
                    if y + 1 < h {
                        join(r - h + 1, true);
                    }
                }
            }
        }

        // A parent always has a smaller index than its child, so one
        // ascending pass leaves every run pointing at its root.
        for i in 0..parent.len() {
            parent[i] = parent[parent[i] as usize];
        }
        let mut sizes = vec![0u64; runs.len()];
        for (run, &root) in runs.iter().zip(&parent) {
            sizes[root as usize] += u64::from(run.end - run.start);
        }
        Regions {
            width: w,
            runs,
            rows,
            root: parent,
            size: sizes,
        }
    }

    /// Every region: its root and its size, in scan order.
    pub(super) fn roots(&self) -> impl Iterator<Item = (u32, u64)> + '_ {
        self.size
            .iter()
            .enumerate()
            .filter(|(_, s)| **s > 0)
            .map(|(r, s)| (r as u32, *s))
    }

    /// Writes into `cells` (the layer, in its order) the value `value`
    /// gives each region by its root, over the region's cells only.
    pub(super) fn paint<T: Copy>(&self, cells: &mut [T], value: impl Fn(u32) -> Option<T>) {
        for (r, bounds) in self.rows.windows(2).enumerate() {
            let row = r * self.width;
            for i in bounds[0] as usize..bounds[1] as usize {
                if let Some(v) = value(self.root[i]) {
                    let Run { start, end } = self.runs[i];
                    cells[row + start as usize..row + end as usize].fill(v);
                }
            }
        }
    }
}

/// The runs of one packed row, in order. Bits past `width` must be clear.
fn cut(words: &[u64], width: usize, out: &mut Vec<Run>) {
    let mut open: Option<u32> = None;
    for (k, &word) in words.iter().enumerate() {
        let base = (k * 64) as u32;
        let mut from = 0;
        loop {
            // The bits at and after `from` that would end the open run, or
            // start a new one.
            let wanted = if open.is_some() { !word } else { word };
            let rest = wanted & (!0u64 << from);
            if rest == 0 {
                break;
            }
            let at = rest.trailing_zeros();
            match open.take() {
                Some(start) => out.push(Run {
                    start,
                    end: base + at,
                }),
                None => open = Some(base + at),
            }
            from = at;
        }
    }
    if let Some(start) = open {
        out.push(Run {
            start,
            end: u32::try_from(width).expect("rows narrower than u32::MAX"),
        });
    }
}

/// Joins the touching runs of two rows, both in order: walk them together,
/// always moving on from the run that ends first.
fn merge(
    runs: &[Run],
    a: std::ops::Range<usize>,
    b: std::ops::Range<usize>,
    diagonal: bool,
    parent: &mut [u32],
) {
    let (mut i, mut j) = (a.start, b.start);
    while i < a.end && j < b.end {
        if runs[i].touches(runs[j], diagonal) {
            union(parent, i as u32, j as u32);
        }
        if runs[i].end < runs[j].end {
            i += 1;
        } else {
            j += 1;
        }
    }
}

/// The root of `x`, halving the path on the way.
#[inline]
fn find(parent: &mut [u32], mut x: u32) -> u32 {
    while parent[x as usize] != x {
        let up = parent[parent[x as usize] as usize];
        parent[x as usize] = up;
        x = up;
    }
    x
}

/// Joins two regions under the smaller root.
#[inline]
fn union(parent: &mut [u32], a: u32, b: u32) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    match ra.cmp(&rb) {
        std::cmp::Ordering::Less => parent[rb as usize] = ra,
        std::cmp::Ordering::Greater => parent[ra as usize] = rb,
        std::cmp::Ordering::Equal => {}
    }
}

#[cfg(test)]
mod tests;
