//! Cheapest paths over a grid of cell costs: A* with a binary heap and
//! arrays indexed by cell (handoff 2026-07-30, Parte 6). The costs are
//! worked out once, before the search; the Go version ran conditions for
//! every neighbour it looked at (its defect nº 4).

use std::{cmp::Reverse, collections::BinaryHeap};

use crate::{bounds::Bounds, coords::Coord};

/// The cost of a cell that cannot be walked into.
pub const BLOCKED: u32 = u32::MAX;

/// +x, −x, +y, −y: the order neighbours are looked at, part of which
/// path wins a tie.
const STEPS: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

/// The cheapest walk from `from` to `to` in the plane of `region` (its
/// first z), moving to the four side neighbours, as the cells it goes
/// through, both ends included.
///
/// `cost` holds what walking into each cell of the plane costs, at least
/// 1, or [`BLOCKED`]; it is in the region's row order. `turn` is added
/// each time the walk changes direction: 0 takes any of the cheapest walks,
/// more makes it straighter. `to` can always be walked into, blocked or
/// not, and `from` is never paid for. `None` when blocked cells cut `to`
/// off, or an end is outside the region.
///
/// Deterministic: ties go to the walk that is closer to `to`, then to the
/// lower cell index.
///
/// # Panics
///
/// If `cost` is shorter than the plane.
pub fn cheapest_path(
    region: Bounds,
    cost: &[u32],
    turn: u32,
    from: Coord,
    to: Coord,
) -> Option<Vec<Coord>> {
    let plane = Bounds::new(
        region.origin,
        crate::coords::GridSize::new(region.size.width, region.size.height, 1),
    );
    let z = region.origin.z;
    let start = plane.index_of(Coord::new(from.x, from.y, z))?;
    let goal = plane.index_of(Coord::new(to.x, to.y, z))?;
    let cells = plane.size.len();
    assert!(cost.len() >= cells, "a cost for every cell of the plane");
    let width = plane.size.width as usize;
    let at = |i: usize| {
        Coord::new(
            plane.origin.x + (i % width) as i32,
            plane.origin.y + (i / width) as i32,
            z,
        )
    };
    let goal_at = at(goal);
    // Every cell costs at least 1: cells left to `to` never overestimate.
    let h = |c: Coord| u64::from(c.x.abs_diff(goal_at.x)) + u64::from(c.y.abs_diff(goal_at.y));
    // With a turn cost, a state is a cell and the direction it was
    // entered in; without one, the direction does not matter.
    let dirs = if turn > 0 { STEPS.len() } else { 1 };
    let mut best = vec![u64::MAX; cells * dirs];
    let mut came = vec![usize::MAX; cells * dirs];
    let mut open = BinaryHeap::new();
    let h0 = h(at(start));
    for d in 0..dirs {
        let s = start * dirs + d;
        best[s] = 0;
        open.push(Reverse((h0, h0, s)));
    }
    while let Some(Reverse((f, hs, s))) = open.pop() {
        let g = f - hs;
        if g > best[s] {
            continue;
        }
        let (cell, d) = (s / dirs, s % dirs);
        if cell == goal {
            let mut path = vec![at(cell)];
            let mut s = s;
            while came[s] != usize::MAX {
                s = came[s];
                path.push(at(s / dirs));
            }
            path.reverse();
            return Some(path);
        }
        let c = at(cell);
        for (k, (dx, dy)) in STEPS.iter().enumerate() {
            let next = Coord::new(c.x + dx, c.y + dy, z);
            let Some(n) = plane.index_of(next) else {
                continue;
            };
            let step = match cost[n] {
                BLOCKED if n == goal => 1,
                BLOCKED => continue,
                c => u64::from(c),
            };
            let (nd, bend) = if dirs == 1 {
                (0, 0)
            } else {
                (k, if k == d { 0 } else { u64::from(turn) })
            };
            let ns = n * dirs + nd;
            let ng = g + step + bend;
            if ng < best[ns] {
                best[ns] = ng;
                came[ns] = s;
                let hn = h(next);
                open.push(Reverse((ng + hn, hn, ns)));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests;
