use super::*;
use crate::coords::GridSize;
use std::{cmp::Reverse, collections::BinaryHeap};

/// A map drawn as rows: `.` costs 1, a digit costs that much, `#` is
/// blocked. Origin (-3, 5, 2), so local and world coordinates differ.
fn map(rows: &[&str]) -> (Bounds, Vec<u32>) {
    let region = Bounds::new(
        Coord::new(-3, 5, 2),
        GridSize::new(rows[0].len() as u32, rows.len() as u32, 1),
    );
    let cost = rows
        .iter()
        .flat_map(|r| r.chars())
        .map(|ch| match ch {
            '.' => 1,
            '#' => BLOCKED,
            d => d.to_digit(10).unwrap(),
        })
        .collect();
    (region, cost)
}

fn at(x: i32, y: i32) -> Coord {
    Coord::new(x - 3, y + 5, 2)
}

/// The path drawn over the map as `*`.
fn draw(rows: &[&str], path: &[Coord]) -> Vec<String> {
    let mut out: Vec<Vec<char>> = rows.iter().map(|r| r.chars().collect()).collect();
    for c in path {
        out[(c.y - 5) as usize][(c.x + 3) as usize] = '*';
    }
    out.into_iter().map(|r| r.into_iter().collect()).collect()
}

/// Cells where the walk changes direction.
fn turns(path: &[Coord]) -> usize {
    path.windows(3)
        .filter(|w| (w[1].x - w[0].x, w[1].y - w[0].y) != (w[2].x - w[1].x, w[2].y - w[1].y))
        .count()
}

fn steps_are_side_by_side(path: &[Coord]) {
    for w in path.windows(2) {
        assert_eq!(
            w[0].x.abs_diff(w[1].x) + w[0].y.abs_diff(w[1].y),
            1,
            "{path:?}"
        );
    }
}

#[test]
fn a_straight_walk_on_an_even_map() {
    let rows = [".......", ".......", "......."];
    let (region, cost) = map(&rows);
    let path = cheapest_path(region, &cost, 0, at(0, 1), at(6, 1)).unwrap();
    assert_eq!(draw(&rows, &path), [".......", "*******", "......."]);
}

#[test]
fn it_goes_around_what_is_blocked() {
    let rows = ["...#...", "...#...", "......."];
    let (region, cost) = map(&rows);
    let path = cheapest_path(region, &cost, 0, at(0, 0), at(6, 0)).unwrap();
    steps_are_side_by_side(&path);
    assert_eq!(path.len(), 11, "down two, across six, up two");
    assert!(
        path.iter()
            .all(|c| cost[(c.x + 3 + 7 * (c.y - 5)) as usize] != BLOCKED)
    );
}

#[test]
fn an_expensive_band_is_crossed_once_not_walked_along() {
    // A wall of 9s between the ends: the cheapest walk crosses one 9.
    let rows = [
        "........", //
        "99999999", //
        "........",
    ];
    let (region, cost) = map(&rows);
    for turn in [0, 3] {
        let path = cheapest_path(region, &cost, turn, at(0, 0), at(7, 2)).unwrap();
        steps_are_side_by_side(&path);
        let nines = path.iter().filter(|c| c.y == 6).count();
        assert_eq!(nines, 1, "turn {turn}: {:?}", draw(&rows, &path));
    }
}

#[test]
fn a_turn_cost_makes_the_walk_straighter() {
    let rows = ["..........", "..........", "..........", ".........."];
    let (region, cost) = map(&rows);
    let straight = cheapest_path(region, &cost, 5, at(0, 0), at(9, 3)).unwrap();
    steps_are_side_by_side(&straight);
    assert_eq!(straight.len(), 13, "still one of the shortest");
    assert_eq!(turns(&straight), 1, "an L");
    // Turns that save more than they cost are still taken: around a block.
    let rows = ["....#.....", "....#.....", "..........", "....#....."];
    let (region, cost) = map(&rows);
    let around = cheapest_path(region, &cost, 5, at(0, 0), at(9, 0)).unwrap();
    assert_eq!(around.len(), 14, "down two, across nine, up two");
    assert_eq!(turns(&around), 2);
}

#[test]
fn a_blocked_end_can_still_be_reached_but_a_walled_in_one_cannot() {
    let rows = ["....#", "....."];
    let (region, cost) = map(&rows);
    let path = cheapest_path(region, &cost, 0, at(0, 0), at(4, 0)).unwrap();
    assert_eq!(path.last(), Some(&at(4, 0)));
    assert_eq!(path.len(), 5);
    let rows = ["..#..", "..#..", "..#.."];
    let (region, cost) = map(&rows);
    assert_eq!(cheapest_path(region, &cost, 0, at(0, 1), at(4, 1)), None);
    // An end outside the region.
    assert_eq!(cheapest_path(region, &cost, 0, at(0, 1), at(9, 1)), None);
}

#[test]
fn the_same_ends_give_the_same_walk() {
    let rows = ["..2..3..", ".#..2...", "...1..#.", "..3....."];
    let (region, cost) = map(&rows);
    for turn in [0, 1, 4] {
        let a = cheapest_path(region, &cost, turn, at(0, 0), at(7, 3)).unwrap();
        let b = cheapest_path(region, &cost, turn, at(0, 0), at(7, 3)).unwrap();
        assert_eq!(a, b);
        assert_eq!((a[0], *a.last().unwrap()), (at(0, 0), at(7, 3)));
    }
}

/// What a walk costs as `cheapest_path` counts it: each cell entered (the
/// goal at 1 if blocked), plus `turn` at each bend.
fn walk_cost(region: Bounds, cost: &[u32], turn: u32, path: &[Coord]) -> u64 {
    let idx = |c: Coord| region.index_of(c).unwrap();
    let goal = idx(*path.last().unwrap());
    let mut total = 0;
    for (k, w) in path.windows(2).enumerate() {
        let n = idx(w[1]);
        total += match cost[n] {
            BLOCKED if n == goal => 1,
            c => u64::from(c),
        };
        if k > 0 {
            let p = path[k];
            let before = (p.x - path[k - 1].x, p.y - path[k - 1].y);
            if (w[1].x - p.x, w[1].y - p.y) != before {
                total += u64::from(turn);
            }
        }
    }
    total
}

/// Dijkstra over (cell, direction entered), the plain way: the cheapest
/// cost from `from` to `to`, or `None`.
fn dijkstra(region: Bounds, cost: &[u32], turn: u32, from: Coord, to: Coord) -> Option<u64> {
    let w = region.size.width as i32;
    let h = region.size.height as i32;
    let cells = (w * h) as usize;
    let goal = region.index_of(to)?;
    let start = region.index_of(from)?;
    // Direction 4: none yet (the start).
    let mut best = vec![u64::MAX; cells * 5];
    let mut open = BinaryHeap::new();
    best[start * 5 + 4] = 0;
    open.push(Reverse((0u64, start * 5 + 4)));
    while let Some(Reverse((g, s))) = open.pop() {
        if g > best[s] {
            continue;
        }
        let (cell, d) = (s / 5, s % 5);
        if cell == goal {
            return Some(g);
        }
        let (x, y) = ((cell as i32) % w, (cell as i32) / w);
        for (k, (dx, dy)) in STEPS.iter().enumerate() {
            let (nx, ny) = (x + dx, y + dy);
            if nx < 0 || ny < 0 || nx >= w || ny >= h {
                continue;
            }
            let n = (ny * w + nx) as usize;
            let step = match cost[n] {
                BLOCKED if n == goal => 1,
                BLOCKED => continue,
                c => u64::from(c),
            };
            let bend = if d != 4 && d != k { u64::from(turn) } else { 0 };
            let ng = g + step + bend;
            if ng < best[n * 5 + k] {
                best[n * 5 + k] = ng;
                open.push(Reverse((ng, n * 5 + k)));
            }
        }
    }
    None
}

/// On random maps of uneven costs (most cells dear, a few cheap, some
/// blocked), with and without a turn cost, the walk found costs exactly
/// the cheapest: the heuristic scaled by the cheapest cell, with the goal
/// and the bend, never overestimates.
#[test]
fn the_walk_found_is_a_cheapest_one_on_uneven_costs() {
    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let region = Bounds::new(Coord::new(-3, 5, 2), GridSize::new(23, 17, 1));
    let cells = region.size.len();
    for round in 0..300 {
        let base = 1 + (round % 3) as u32;
        let cost: Vec<u32> = (0..cells)
            .map(|_| match next() % 20 {
                0..=2 => BLOCKED,
                3..=5 => base,
                k => base + (k % 7) as u32,
            })
            .collect();
        let pick = |r: u64| {
            let i = (r % cells as u64) as usize;
            Coord::new(
                region.origin.x + (i % 23) as i32,
                region.origin.y + (i / 23) as i32,
                2,
            )
        };
        let (from, to) = (pick(next()), pick(next()));
        for turn in [0, 3] {
            let want = dijkstra(region, &cost, turn, from, to);
            let got = cheapest_path(region, &cost, turn, from, to);
            match (want, got) {
                (None, None) => {}
                (Some(w), Some(path)) => {
                    steps_are_side_by_side(&path);
                    assert_eq!(
                        walk_cost(region, &cost, turn, &path),
                        w,
                        "round {round}, turn {turn}"
                    );
                }
                (w, g) => panic!(
                    "round {round}, turn {turn}: {w:?} vs {:?}",
                    g.map(|p| p.len())
                ),
            }
        }
    }
}

/// To a set of goals: the walk ends on one, and costs the least of the
/// plain Dijkstra to each.
#[test]
fn the_walk_to_the_nearest_goal_is_a_cheapest_one() {
    let mut state = 0x2545_f491_4f6c_dd1du64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let region = Bounds::new(Coord::new(-3, 5, 2), GridSize::new(21, 15, 1));
    let cells = region.size.len();
    let pick = |r: u64| {
        let i = (r % cells as u64) as usize;
        Coord::new(
            region.origin.x + (i % 21) as i32,
            region.origin.y + (i / 21) as i32,
            2,
        )
    };
    for round in 0..200 {
        let base = 1 + (round % 3) as u32;
        let cost: Vec<u32> = (0..cells)
            .map(|_| match next() % 20 {
                0..=2 => BLOCKED,
                k => base + (k % 5) as u32,
            })
            .collect();
        let goals: Vec<Coord> = (0..1 + round % 4).map(|_| pick(next())).collect();
        let mut set = vec![false; cells];
        for g in &goals {
            set[region.index_of(*g).unwrap()] = true;
        }
        let from = pick(next());
        for turn in [0, 2] {
            let want = goals
                .iter()
                .filter_map(|&g| dijkstra(region, &cost, turn, from, g))
                .min();
            let got = cheapest_path_to(region, &cost, turn, from, &set);
            match (want, got) {
                (None, None) => {}
                (Some(w), Some(path)) => {
                    steps_are_side_by_side(&path);
                    assert!(set[region.index_of(*path.last().unwrap()).unwrap()]);
                    assert_eq!(
                        walk_cost(region, &cost, turn, &path),
                        w,
                        "round {round}, turn {turn}"
                    );
                }
                (w, g) => panic!(
                    "round {round}, turn {turn}: {w:?} vs {:?}",
                    g.map(|p| p.len())
                ),
            }
        }
    }
    // No goal at all: no walk.
    assert_eq!(
        cheapest_path_to(region, &vec![1; cells], 0, pick(3), &vec![false; cells]),
        None
    );
}
