//! KeepLargestRegion and FillSmallRegions, against a breadth-first search
//! written the obvious way.

use std::collections::VecDeque;

use serde_json::{Value, json};

use crate::{
    access::Reach,
    bounds::Bounds,
    condition::Condition,
    coords::{Coord, GridSize},
    grid::Grid,
    layer::LayerKind,
    operator::{BuildError, OpError, Registry, Scope},
    pipeline::{Pipeline, StepError, StepSpec},
    tile::{TileId, TileRegistry},
};

fn tiles() -> TileRegistry {
    TileRegistry::new(["floor", "wall", "water", "door"]).unwrap()
}

struct World {
    grid: Grid,
    tiles: TileRegistry,
}

fn world(origin: Coord, size: GridSize) -> World {
    let mut grid = Grid::new(Bounds::new(origin, size));
    grid.add_layer("terrain", LayerKind::Tiles).unwrap();
    World {
        grid,
        tiles: tiles(),
    }
}

impl World {
    fn id(&self, name: &str) -> TileId {
        self.tiles.id(name).unwrap()
    }

    fn cells(&self) -> Vec<TileId> {
        let id = self.grid.layer_id("terrain").unwrap();
        self.grid.tiles(id).unwrap().as_slice().to_vec()
    }

    fn paint(&mut self, cells: &[TileId]) {
        let id = self.grid.layer_id("terrain").unwrap();
        self.grid
            .tiles_mut(id)
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(cells);
    }

    /// The terrain from rows of `.` (floor), `#` (wall), `~` (water).
    fn draw(&mut self, rows: &[&str]) {
        let cells: Vec<TileId> = rows
            .iter()
            .flat_map(|r| r.chars())
            .map(|ch| match ch {
                '.' => self.id("floor"),
                '#' => self.id("wall"),
                '~' => self.id("water"),
                _ => panic!("{ch}"),
            })
            .collect();
        self.paint(&cells);
    }

    fn picture(&self) -> Vec<String> {
        let w = self.grid.bounds().size.width as usize;
        let (floor, wall) = (self.id("floor"), self.id("wall"));
        self.cells()
            .chunks(w)
            .map(|row| {
                row.iter()
                    .map(|&t| match t {
                        t if t == floor => '.',
                        t if t == wall => '#',
                        _ => '~',
                    })
                    .collect()
            })
            .collect()
    }

    fn try_run(&mut self, steps: &[StepSpec]) -> Result<(), StepError<OpError>> {
        Pipeline::build(&Registry::with_builtins(), &self.grid, &self.tiles, steps)
            .unwrap()
            .run(&mut self.grid, "", 1)
    }

    fn run(&mut self, steps: &[StepSpec]) {
        self.try_run(steps).unwrap();
    }

    fn build_error(&self, steps: &[StepSpec]) -> BuildError {
        Pipeline::build(&Registry::with_builtins(), &self.grid, &self.tiles, steps)
            .err()
            .expect("the build fails")
            .remove(0)
            .error
    }
}

fn step(operator: &str, config: Value) -> StepSpec {
    StepSpec {
        id: "regions".to_string(),
        operator: operator.to_string(),
        enabled: true,
        config,
        conditions: Vec::new(),
    }
}

fn keep(region: Value, connectivity: &str) -> StepSpec {
    step(
        "KeepLargestRegion",
        json!({"layer": "terrain", "region": region, "fill": "wall",
               "connectivity": connectivity}),
    )
}

fn fill_small(region: Value, connectivity: &str, below: u32) -> StepSpec {
    step(
        "FillSmallRegions",
        json!({"layer": "terrain", "region": region, "fill": "wall",
               "connectivity": connectivity, "below": below}),
    )
}

/// The regions of `cells` by breadth-first search from each unvisited
/// cell in scan order: for each cell, the index of its region (regions
/// numbered in the order their first cell appears), and each region's size.
fn bfs(
    cells: &[TileId],
    size: GridSize,
    region: &[TileId],
    diagonal: bool,
) -> (Vec<Option<usize>>, Vec<usize>) {
    let (w, h, d) = (size.width as i64, size.height as i64, size.depth as i64);
    let at = |x: i64, y: i64, z: i64| ((z * h + y) * w + x) as usize;
    let mut label = vec![None; cells.len()];
    let mut sizes = Vec::new();
    for start in 0..cells.len() {
        if label[start].is_some() || !region.contains(&cells[start]) {
            continue;
        }
        let id = sizes.len();
        let mut n = 0;
        let mut queue = VecDeque::from([start]);
        label[start] = Some(id);
        while let Some(i) = queue.pop_front() {
            n += 1;
            let (x, y, z) = (i as i64 % w, i as i64 / w % h, i as i64 / (w * h));
            for dz in -1..=1i64 {
                for dy in -1..=1i64 {
                    for dx in -1..=1i64 {
                        let steps = dx.abs() + dy.abs() + dz.abs();
                        if steps == 0 || (!diagonal && steps > 1) {
                            continue;
                        }
                        let (nx, ny, nz) = (x + dx, y + dy, z + dz);
                        if !(0..w).contains(&nx) || !(0..h).contains(&ny) || !(0..d).contains(&nz) {
                            continue;
                        }
                        let j = at(nx, ny, nz);
                        if label[j].is_none() && region.contains(&cells[j]) {
                            label[j] = Some(id);
                            queue.push_back(j);
                        }
                    }
                }
            }
        }
        sizes.push(n);
    }
    (label, sizes)
}

/// splitmix64: test data only.
struct Dice(u64);

impl Dice {
    fn below(&mut self, n: u64) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        (z ^ (z >> 31)) % n
    }
}

/// Floor, wall, water and door in random proportions.
fn noise(dice: &mut Dice, t: &TileRegistry, n: usize) -> Vec<TileId> {
    let floor_share = 20 + dice.below(60);
    (0..n)
        .map(|_| match dice.below(100) {
            r if r < floor_share => t.id("floor").unwrap(),
            r if r < floor_share + 5 => t.id("door").unwrap(),
            r if r < floor_share + 10 => t.id("water").unwrap(),
            _ => t.id("wall").unwrap(),
        })
        .collect()
}

const SIZES: [(u32, u32, u32); 12] = [
    (1, 1, 1),
    (6, 1, 1),
    (1, 7, 1),
    (63, 9, 1),
    (64, 5, 1),
    (65, 17, 1),
    (130, 40, 1),
    (2, 2, 2),
    (9, 7, 5),
    (17, 6, 4),
    (65, 3, 3),
    (64, 4, 6),
];

#[test]
fn both_agree_with_a_breadth_first_search_on_random_maps() {
    let mut dice = Dice(3);
    let mut cases = 0;
    for &(w, h, d) in &SIZES {
        let size = GridSize::new(w, h, d);
        for connectivity in ["orthogonal", "diagonal"] {
            for (spec, names) in [
                (json!("floor"), &["floor"][..]),
                (json!(["floor", "door"]), &["floor", "door"][..]),
            ] {
                for _ in 0..3 {
                    let mut a = world(Coord::new(-5, 2, 7), size);
                    let start = noise(&mut dice, &a.tiles, size.len());
                    let region: Vec<TileId> = names.iter().map(|n| a.id(n)).collect();
                    let wall = a.id("wall");
                    let (label, sizes) = bfs(&start, size, &region, connectivity == "diagonal");
                    let context = format!("{w}x{h}x{d} {connectivity} {names:?}");

                    // Largest: the first of the largest in scan order stays.
                    a.paint(&start);
                    let result = a.try_run(&[keep(spec.clone(), connectivity)]);
                    match sizes.iter().max() {
                        None => assert!(result.is_err(), "{context}: no region"),
                        Some(&max) => {
                            result.unwrap();
                            let winner = sizes.iter().position(|&s| s == max).unwrap();
                            let expected: Vec<TileId> = start
                                .iter()
                                .zip(&label)
                                .map(|(&t, l)| match l {
                                    Some(l) if *l != winner => wall,
                                    _ => t,
                                })
                                .collect();
                            assert_eq!(a.cells(), expected, "{context}: keep largest");
                        }
                    }

                    // Small: every region under `below` goes.
                    let below = 1 + dice.below(30) as u32;
                    let mut b = world(Coord::new(3, -4, 0), size);
                    b.paint(&start);
                    b.run(&[fill_small(spec.clone(), connectivity, below)]);
                    let expected: Vec<TileId> = start
                        .iter()
                        .zip(&label)
                        .map(|(&t, l)| match l {
                            Some(l) if sizes[*l] < below as usize => wall,
                            _ => t,
                        })
                        .collect();
                    assert_eq!(b.cells(), expected, "{context}: below {below}");
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 144);
}

#[test]
fn a_tie_keeps_the_first_region_in_scan_order() {
    let mut w = world(Coord::ZERO, GridSize::new(7, 3, 1));
    w.draw(&["#..#..#", "#..#..#", "#######"]);
    w.run(&[keep(json!("floor"), "orthogonal")]);
    assert_eq!(w.picture(), ["#..####", "#..####", "#######"]);

    // Three cells each, and the right region's first cell comes first in
    // scan order here.
    let mut w = world(Coord::ZERO, GridSize::new(7, 3, 1));
    w.draw(&["####..#", "#..##.#", "#.#####"]);
    w.run(&[keep(json!("floor"), "orthogonal")]);
    assert_eq!(w.picture(), ["####..#", "#####.#", "#######"]);
}

#[test]
fn diagonal_joins_corners_and_orthogonal_does_not() {
    let rows = [".#.", "#.#", "..#"];
    let mut w = world(Coord::ZERO, GridSize::new(3, 3, 1));
    w.draw(&rows);
    w.run(&[keep(json!("floor"), "orthogonal")]);
    // Orthogonal: three regions; the bottom one (3 cells) wins.
    assert_eq!(w.picture(), ["###", "#.#", "..#"]);

    let mut w = world(Coord::ZERO, GridSize::new(3, 3, 1));
    w.draw(&rows);
    w.run(&[keep(json!("floor"), "diagonal")]);
    assert_eq!(w.picture(), rows);
}

#[test]
fn diagonal_in_3d_reaches_across_a_corner_of_the_cube() {
    // Two cells touching only at a corner, one layer apart.
    let mut w = world(Coord::ZERO, GridSize::new(2, 2, 2));
    let (floor, wall) = (w.id("floor"), w.id("wall"));
    let mut cells = vec![wall; 8];
    cells[0] = floor; // (0, 0, 0)
    cells[7] = floor; // (1, 1, 1)
    w.paint(&cells);
    w.run(&[fill_small(json!("floor"), "diagonal", 2)]);
    assert_eq!(w.cells(), cells, "one region of 2: kept");
    w.run(&[fill_small(json!("floor"), "orthogonal", 2)]);
    assert_eq!(w.cells(), vec![wall; 8], "two regions of 1: filled");
}

#[test]
fn small_regions_decide_the_same_in_any_window() {
    // The promise of Local: run on a window of the map, padded by the
    // declared reach, and every cell inside the window comes out as on the
    // whole map.
    let mut dice = Dice(19);
    for (size, below) in [
        (GridSize::new(70, 50, 1), 6),
        (GridSize::new(70, 50, 1), 1),
        (GridSize::new(20, 18, 12), 5),
    ] {
        for connectivity in ["orthogonal", "diagonal"] {
            let tiles = tiles();
            let start = noise(&mut dice, &tiles, size.len());
            let mut whole = world(Coord::ZERO, size);
            whole.paint(&start);
            whole.run(&[fill_small(json!("floor"), connectivity, below)]);
            let at = |c: Coord| {
                let i = (c.z as u32 * size.height + c.y as u32) * size.width + c.x as u32;
                i as usize
            };

            let pad = below - 1;
            let is_2d = size.depth == 1;
            for _ in 0..12 {
                let mut pick = |n: u32| dice.below(u64::from(n)) as i32;
                let inner = Bounds::new(
                    Coord::new(pick(size.width), pick(size.height), pick(size.depth)),
                    GridSize::new(8, 7, if is_2d { 1 } else { 4 }),
                );
                // The window: the inner box grown by the reach, cut to the map.
                let lo = |v: i32, p: u32| (v - p as i32).max(0);
                let hi =
                    |v: i32, n: u32, p: u32, len: u32| (v + n as i32 + p as i32).min(len as i32);
                let zp = if is_2d { 0 } else { pad };
                let origin = Coord::new(
                    lo(inner.origin.x, pad),
                    lo(inner.origin.y, pad),
                    lo(inner.origin.z, zp),
                );
                let end = Coord::new(
                    hi(inner.origin.x, inner.size.width, pad, size.width),
                    hi(inner.origin.y, inner.size.height, pad, size.height),
                    hi(inner.origin.z, inner.size.depth, zp, size.depth),
                );
                let window = Bounds::new(
                    origin,
                    GridSize::new(
                        (end.x - origin.x) as u32,
                        (end.y - origin.y) as u32,
                        (end.z - origin.z) as u32,
                    ),
                );
                let cells: Vec<TileId> = window.iter().map(|c| start[at(c)]).collect();
                let mut part = world(window.origin, window.size);
                part.paint(&cells);
                part.run(&[fill_small(json!("floor"), connectivity, below)]);
                let got = part.cells();
                for (i, c) in window.iter().enumerate() {
                    if inner.contains(c) {
                        assert_eq!(
                            got[i],
                            whole.cells()[at(c)],
                            "{size:?} {connectivity} below {below}, cell {c:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn failures_are_step_errors() {
    let mut w = world(Coord::ZERO, GridSize::new(4, 4, 1));
    w.draw(&["####", "#..#", "####", "####"]);
    let e = w
        .try_run(&[keep(json!("water"), "orthogonal")])
        .unwrap_err();
    assert_eq!(
        e.error.to_string(),
        "layer 'terrain' has no cell of the region"
    );

    let e = w
        .try_run(&[step(
            "KeepLargestRegion",
            json!({"layer": "terrain", "region": "floor", "fill": "wall",
                   "connectivity": "orthogonal", "min_cells": 3}),
        )])
        .unwrap_err();
    assert_eq!(
        e.error.to_string(),
        "the largest region has 2 cells, fewer than min_cells (3)"
    );
    // A failed step writes nothing.
    assert_eq!(w.picture(), ["####", "#..#", "####", "####"]);
}

#[test]
fn bad_configs_fail_the_build() {
    let w = world(Coord::ZERO, GridSize::new(4, 4, 1));
    let invalid = |e: BuildError, field: &str| {
        assert!(
            matches!(&e, BuildError::Invalid { field: f, .. } if f == field),
            "{e}"
        );
    };
    invalid(
        w.build_error(&[step(
            "KeepLargestRegion",
            json!({"layer": "terrain", "region": ["floor", "wall"], "fill": "wall",
                   "connectivity": "orthogonal"}),
        )]),
        "fill",
    );
    invalid(
        w.build_error(&[fill_small(json!("floor"), "orthogonal", 0)]),
        "below",
    );
    for config in [
        json!({"layer": "terrain", "region": "floor", "fill": "wall"}),
        json!({"layer": "terrain", "region": "floor", "fill": "wall", "connectivity": "eight"}),
    ] {
        assert!(matches!(
            w.build_error(&[step("KeepLargestRegion", config)]),
            BuildError::Config { .. }
        ));
    }
    let mut with_conditions = keep(json!("floor"), "orthogonal");
    with_conditions.conditions = vec![
        serde_json::from_str::<Condition>(
            r#"{"type": "tile_is", "layer": "terrain", "tile": "floor"}"#,
        )
        .unwrap(),
    ];
    assert!(matches!(
        w.build_error(&[with_conditions]),
        BuildError::ConditionsNotSupported { .. }
    ));
}

#[test]
fn scope_and_reach_are_declared() {
    let r = Registry::with_builtins();
    assert_eq!(r.get("KeepLargestRegion").unwrap().scope, Scope::Global);
    assert_eq!(r.get("FillSmallRegions").unwrap().scope, Scope::Local);

    for (size, reach) in [
        (GridSize::new(8, 8, 1), Reach::symmetric(9, 9, 0)),
        (GridSize::new(8, 8, 3), Reach::uniform(9)),
    ] {
        let w = world(Coord::ZERO, size);
        let p = Pipeline::build(
            &r,
            &w.grid,
            &w.tiles,
            &[fill_small(json!("floor"), "diagonal", 10)],
        )
        .unwrap();
        let terrain = w.grid.layer_id("terrain").unwrap();
        assert_eq!(
            p.access("regions").unwrap().read_reach(terrain),
            Some(reach)
        );
    }
}
