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
    let plane = plane_of(region);
    let z = region.origin.z;
    let start = plane.index_of(Coord::new(from.x, from.y, z))?;
    let goal = plane.index_of(Coord::new(to.x, to.y, z))?;
    let cells = plane.size.len();
    assert!(cost.len() >= cells, "a cost for every cell of the plane");
    let width = plane.size.width as usize;
    let (gx, gy) = (goal % width, goal / width);
    // What is left to `to`, never more than it costs: `d` steps, the last
    // into `to` at its own cost and the others at least the cheapest cell's
    // (1 × the distance made A* open nearly every cell where most cost 2:
    // 1.9 ms against 0.29 ms, pesquisa 2026-10-27-caminhos-organicos, Q4),
    // and a bend when `to` is off both the row and the column. Each step
    // lowers it by no more than the step costs, so the first walk to reach
    // `to` is a cheapest one.
    let cheapest = cheapest_cell(&cost[..cells]);
    let last = match cost[goal] {
        BLOCKED => 1,
        c => u64::from(c),
    };
    let h = |i: usize| {
        let (dx, dy) = ((i % width).abs_diff(gx), (i / width).abs_diff(gy));
        let d = (dx + dy) as u64;
        if d == 0 {
            return 0;
        }
        let bend = if dx > 0 && dy > 0 { u64::from(turn) } else { 0 };
        u64::from(cheapest) * (d - 1) + last + bend
    };
    search(plane, cost, turn, start, |i| i == goal, h)
}

/// The cheapest walk from `from` to whichever cell of `goals` (one per cell
/// of the plane, in its row order) is cheapest to reach, as
/// [`cheapest_path`] walks to one: a branch to the nearest path, the
/// roads that join a network of Mizdal & Pozzer (SBGames 2018, alg. 3) and
/// the least-cost paths "to the nearest existing road" of R's `roads`
/// (pesquisa 2026-10-27-caminhos-organicos, Q3). Any goal can be walked
/// into, blocked or not. `None` when there is no goal, when blocked cells
/// cut every one off, or `from` is outside the region.
///
/// # Panics
///
/// If `cost` or `goals` is shorter than the plane.
pub fn cheapest_path_to(
    region: Bounds,
    cost: &[u32],
    turn: u32,
    from: Coord,
    goals: &[bool],
) -> Option<Vec<Coord>> {
    let plane = plane_of(region);
    let start = plane.index_of(Coord::new(from.x, from.y, region.origin.z))?;
    let cells = plane.size.len();
    assert!(
        cost.len() >= cells && goals.len() >= cells,
        "every cell of the plane"
    );
    let near = to_nearest(plane, &goals[..cells])?;
    // `d` steps at least to the nearest goal (one pass each way gives the
    // exact distance in cells, obstacles left out); every step costs the
    // cheapest cell's or more, but the last may enter a blocked goal at 1.
    let cheapest = u64::from(cheapest_cell(&cost[..cells]));
    let h = |i: usize| match near[i] {
        0 => 0,
        d => cheapest * u64::from(d - 1) + 1,
    };
    search(plane, cost, turn, start, |i| goals[i], h)
}

/// The first z plane of `region`.
fn plane_of(region: Bounds) -> Bounds {
    Bounds::new(
        region.origin,
        crate::coords::GridSize::new(region.size.width, region.size.height, 1),
    )
}

/// The cost of the cheapest cell that can be walked into (1 if none can).
fn cheapest_cell(cost: &[u32]) -> u32 {
    cost.iter()
        .copied()
        .filter(|&c| c != BLOCKED)
        .min()
        .unwrap_or(1)
}

/// For each cell, the steps to the nearest goal, side to side and over
/// anything: a pass down and right, then one up and left. `None` when there
/// is no goal.
fn to_nearest(plane: Bounds, goals: &[bool]) -> Option<Vec<u32>> {
    if !goals.iter().any(|&g| g) {
        return None;
    }
    let (w, h) = (plane.size.width as usize, plane.size.height as usize);
    let mut d: Vec<u32> = goals
        .iter()
        .map(|&g| if g { 0 } else { u32::MAX })
        .collect();
    let step = |v: u32| v.saturating_add(1);
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if x > 0 {
                d[i] = d[i].min(step(d[i - 1]));
            }
            if y > 0 {
                d[i] = d[i].min(step(d[i - w]));
            }
        }
    }
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            let i = y * w + x;
            if x + 1 < w {
                d[i] = d[i].min(step(d[i + 1]));
            }
            if y + 1 < h {
                d[i] = d[i].min(step(d[i + w]));
            }
        }
    }
    Some(d)
}

/// A* over `plane` from `start` until a cell where `is_goal` holds is
/// taken, with `h` (by cell index) never above what is left and lowered by
/// no more than each step costs.
fn search(
    plane: Bounds,
    cost: &[u32],
    turn: u32,
    start: usize,
    is_goal: impl Fn(usize) -> bool,
    h: impl Fn(usize) -> u64,
) -> Option<Vec<Coord>> {
    let z = plane.origin.z;
    let cells = plane.size.len();
    let width = plane.size.width as usize;
    let at = |i: usize| {
        Coord::new(
            plane.origin.x + (i % width) as i32,
            plane.origin.y + (i / width) as i32,
            z,
        )
    };
    // With a turn cost, a state is a cell and the direction it was
    // entered in; without one, the direction does not matter.
    let dirs = if turn > 0 { STEPS.len() } else { 1 };
    let mut best = vec![u64::MAX; cells * dirs];
    let mut came = vec![usize::MAX; cells * dirs];
    let mut open = BinaryHeap::new();
    let h0 = h(start);
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
        if is_goal(cell) {
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
                BLOCKED if is_goal(n) => 1,
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
                let hn = h(n);
                open.push(Reverse((ng + hn, hn, ns)));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests;
