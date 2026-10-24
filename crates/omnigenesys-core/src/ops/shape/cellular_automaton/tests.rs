use serde_json::{Value, json};

use crate::{
    access::Reach,
    bounds::Bounds,
    condition::Condition,
    coords::{Coord, GridSize},
    grid::Grid,
    layer::LayerKind,
    operator::{BuildError, Registry},
    pipeline::{Pipeline, StepSpec},
    tile::{TileId, TileRegistry},
};

struct World {
    grid: Grid,
    tiles: TileRegistry,
}

fn world(width: u32, height: u32) -> World {
    let mut grid = Grid::new(Bounds::new(
        Coord::new(-7, 3, 0),
        GridSize::new(width, height, 1),
    ));
    grid.add_layer("terrain", LayerKind::Tiles).unwrap();
    grid.add_layer("zone", LayerKind::Tiles).unwrap();
    World {
        grid,
        tiles: tiles(),
    }
}

/// floor, wall and water, then fillers, so that `deep` and `rock` are 0xff
/// and 0x100: ids that differ in every bit of both bytes but one, so a
/// mask that drops any of them shows.
fn tiles() -> TileRegistry {
    let fillers = (0..251).map(|i| format!("filler{i}"));
    let names = ["floor", "wall", "water"]
        .map(String::from)
        .into_iter()
        .chain(fillers)
        .chain(["deep", "rock"].map(String::from));
    TileRegistry::new(names).unwrap()
}

fn step(config: Value, conditions: &[&str]) -> StepSpec {
    StepSpec {
        id: "caves".to_string(),
        operator: "CellularAutomaton".to_string(),
        enabled: true,
        config,
        conditions: conditions
            .iter()
            .map(|c| serde_json::from_str::<Condition>(c).unwrap())
            .collect(),
    }
}

fn config(birth: &[u8], survive: &[u8], iterations: u32, outside: &str) -> Value {
    config_of("wall", "floor", birth, survive, iterations, outside)
}

fn config_of(
    alive: &str,
    dead: &str,
    birth: &[u8],
    survive: &[u8],
    iterations: u32,
    outside: &str,
) -> Value {
    json!({
        "layer": "terrain", "alive": alive, "dead": dead,
        "birth": birth, "survive": survive,
        "iterations": iterations, "outside": outside,
    })
}

impl World {
    fn id(&self, name: &str) -> TileId {
        self.tiles.id(name).unwrap()
    }

    fn cells(&self, layer: &str) -> Vec<TileId> {
        let id = self.grid.layer_id(layer).unwrap();
        self.grid.tiles(id).unwrap().as_slice().to_vec()
    }

    fn paint(&mut self, layer: &str, cells: &[TileId]) {
        let id = self.grid.layer_id(layer).unwrap();
        self.grid
            .tiles_mut(id)
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(cells);
    }

    /// The terrain from rows of `#` (wall), `.` (floor) and `~` (water).
    fn draw(&mut self, rows: &[&str]) {
        let cells: Vec<TileId> = rows
            .iter()
            .flat_map(|r| r.chars())
            .map(|ch| match ch {
                '#' => self.id("wall"),
                '.' => self.id("floor"),
                '~' => self.id("water"),
                _ => panic!("{ch}"),
            })
            .collect();
        self.paint("terrain", &cells);
    }

    fn picture(&self) -> Vec<String> {
        let w = self.grid.bounds().size.width as usize;
        let (wall, floor) = (self.id("wall"), self.id("floor"));
        self.cells("terrain")
            .chunks(w)
            .map(|row| {
                row.iter()
                    .map(|&t| match t {
                        t if t == wall => '#',
                        t if t == floor => '.',
                        _ => '~',
                    })
                    .collect()
            })
            .collect()
    }

    fn run(&mut self, steps: &[StepSpec]) {
        Pipeline::build(&Registry::with_builtins(), &self.grid, &self.tiles, steps)
            .unwrap()
            .run(&mut self.grid, "", 1)
            .unwrap();
    }

    fn build_error(&self, steps: &[StepSpec]) -> BuildError {
        let errors = Pipeline::build(&Registry::with_builtins(), &self.grid, &self.tiles, steps)
            .err()
            .expect("the build fails");
        errors.into_iter().next().unwrap().error
    }
}

/// The rule written the obvious way, one cell and one neighbour at a time:
/// what the bit-sliced board must agree with.
struct Oracle<'a> {
    width: usize,
    height: usize,
    alive: TileId,
    dead: TileId,
    birth: &'a [u8],
    survive: &'a [u8],
    outside_alive: bool,
    /// Cells allowed to change (the step conditions).
    free: &'a [bool],
}

impl Oracle<'_> {
    fn run(&self, cells: &[TileId], iterations: u32) -> Vec<TileId> {
        let mut cur = cells.to_vec();
        for _ in 0..iterations {
            let mut next = cur.clone();
            for y in 0..self.height {
                for x in 0..self.width {
                    let i = y * self.width + x;
                    let t = cur[i];
                    if !self.free[i] || (t != self.alive && t != self.dead) {
                        continue;
                    }
                    let n = self.neighbours(&cur, x as i64, y as i64);
                    let set = if t == self.alive {
                        self.survive
                    } else {
                        self.birth
                    };
                    next[i] = if set.contains(&n) {
                        self.alive
                    } else {
                        self.dead
                    };
                }
            }
            cur = next;
        }
        cur
    }

    fn neighbours(&self, cells: &[TileId], x: i64, y: i64) -> u8 {
        let mut n = 0;
        for dy in -1..=1 {
            for dx in -1..=1 {
                if (dx, dy) == (0, 0) {
                    continue;
                }
                let (nx, ny) = (x + dx, y + dy);
                let inside =
                    (0..self.width as i64).contains(&nx) && (0..self.height as i64).contains(&ny);
                let alive = if inside {
                    cells[ny as usize * self.width + nx as usize] == self.alive
                } else {
                    self.outside_alive
                };
                n += u8::from(alive);
            }
        }
        n
    }
}

/// splitmix64: test data only, not part of any contract.
struct Dice(u64);

impl Dice {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    /// The counts 0..=8 whose bit is set in a random 9-bit mask.
    fn counts(&mut self) -> Vec<u8> {
        let mask = self.below(512);
        (0..9).filter(|n| mask >> n & 1 == 1).collect()
    }
}

#[test]
fn agrees_with_the_obvious_rule_on_random_maps() {
    // Widths around word boundaries; every policy, random rules, a third
    // tile that must stay put, and with and without conditions. The tiles
    // take turns: the usual ones, `empty` as the live one, and two with ids
    // 0xff and 0x100.
    let mut dice = Dice(7);
    let widths = [1, 2, 5, 63, 64, 65, 127, 128, 129, 200];
    let heights = [1, 2, 3, 9, 66];
    let mut cases = 0;
    for &w in &widths {
        for &h in &heights {
            for outside in ["alive", "dead"] {
                for with_conditions in [false, true] {
                    let mut world = world(w, h);
                    let pairs = [("wall", "floor"), ("empty", "wall"), ("rock", "deep")];
                    let (alive_name, dead_name) = pairs[cases % 3];
                    let (alive, dead, water) =
                        (world.id(alive_name), world.id(dead_name), world.id("water"));
                    let (floor, wall) = (world.id("floor"), world.id("wall"));
                    let n = (w * h) as usize;
                    let percent = dice.below(101);
                    let terrain: Vec<TileId> = (0..n)
                        .map(|_| match dice.below(100) {
                            0..4 => water,
                            r if r < percent => alive,
                            _ => dead,
                        })
                        .collect();
                    let zone: Vec<TileId> = (0..n)
                        .map(|_| if dice.below(4) == 0 { wall } else { floor })
                        .collect();
                    world.paint("terrain", &terrain);
                    world.paint("zone", &zone);

                    let (birth, survive) = (dice.counts(), dice.counts());
                    let iterations = 1 + dice.below(6) as u32;
                    let conditions: &[&str] = if with_conditions {
                        &[r#"{"type": "tile_is", "layer": "zone", "tile": "floor"}"#]
                    } else {
                        &[]
                    };
                    world.run(&[step(
                        config_of(alive_name, dead_name, &birth, &survive, iterations, outside),
                        conditions,
                    )]);

                    let free: Vec<bool> = zone
                        .iter()
                        .map(|&z| !with_conditions || z == floor)
                        .collect();
                    let oracle = Oracle {
                        width: w as usize,
                        height: h as usize,
                        alive,
                        dead,
                        birth: &birth,
                        survive: &survive,
                        outside_alive: outside == "alive",
                        free: &free,
                    };
                    assert_eq!(
                        world.cells("terrain"),
                        oracle.run(&terrain, iterations),
                        "{w}x{h} outside {outside}, B{birth:?}/S{survive:?}, \
                         {iterations} passes, conditions {with_conditions}, {alive_name}/{dead_name}"
                    );
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 200);
}

#[test]
fn a_blinker_turns_under_the_game_of_life() {
    // B3/S23 with nothing outside: the textbook check that births and
    // survivals are read the right way round and the update is synchronous.
    let mut w = world(5, 5);
    w.draw(&[".....", "..#..", "..#..", "..#..", "....."]);
    w.run(&[step(config(&[3], &[2, 3], 1, "dead"), &[])]);
    assert_eq!(w.picture(), [".....", ".....", ".###.", ".....", "....."]);
    w.run(&[step(config(&[3], &[2, 3], 1, "dead"), &[])]);
    assert_eq!(w.picture(), [".....", "..#..", "..#..", "..#..", "....."]);
}

#[test]
fn outside_decides_what_the_edge_sees() {
    // An open map with the 4-5 rule: walls grow in from an outside that is
    // alive, and nothing happens when it is dead.
    let open = [".....", ".....", ".....", ".....", "....."];
    let mut alive = world(5, 5);
    alive.draw(&open);
    alive.run(&[step(
        config(&[5, 6, 7, 8], &[4, 5, 6, 7, 8], 1, "alive"),
        &[],
    )]);
    // A corner sees 5 outside cells, an edge cell 3.
    assert_eq!(
        alive.picture(),
        ["#...#", ".....", ".....", ".....", "#...#"]
    );

    let mut dead = world(5, 5);
    dead.draw(&open);
    dead.run(&[step(
        config(&[5, 6, 7, 8], &[4, 5, 6, 7, 8], 1, "dead"),
        &[],
    )]);
    assert_eq!(dead.picture(), open);
}

#[test]
fn other_tiles_count_as_dead_and_stay() {
    // The water would be born a wall (8 walls around it) if it were floor.
    let mut w = world(3, 3);
    w.draw(&["###", "#~#", "###"]);
    w.run(&[step(
        config(&[1, 2, 3, 4, 5, 6, 7, 8], &[0], 3, "alive"),
        &[],
    )]);
    // Every wall had a live neighbour, so none survives (survive is [0]).
    assert_eq!(w.picture(), ["...", ".~.", "..."]);
}

#[test]
fn stops_early_once_stable_with_the_same_result() {
    let mut dice = Dice(11);
    let mut a = world(70, 40);
    let (wall, floor) = (a.id("wall"), a.id("floor"));
    let cells: Vec<TileId> = (0..70 * 40)
        .map(|_| if dice.below(100) < 45 { wall } else { floor })
        .collect();
    a.paint("terrain", &cells);
    let mut b = world(70, 40);
    b.paint("terrain", &cells);
    let rule = (&[5, 6, 7, 8][..], &[4, 5, 6, 7, 8][..]);
    a.run(&[step(config(rule.0, rule.1, 50, "alive"), &[])]);
    b.run(&[step(config(rule.0, rule.1, 5000, "alive"), &[])]);
    assert_eq!(a.cells("terrain"), b.cells("terrain"));
}

#[test]
fn declares_a_synchronous_update_reaching_one_cell_per_pass() {
    let w = world(8, 8);
    let p = Pipeline::build(
        &Registry::with_builtins(),
        &w.grid,
        &w.tiles,
        &[step(config(&[5], &[4], 6, "alive"), &[])],
    )
    .unwrap();
    let access = p.access("caves").unwrap();
    let terrain = w.grid.layer_id("terrain").unwrap();
    assert_eq!(access.read_reach(terrain), Some(Reach::symmetric(6, 6, 0)));
    assert!(access.writes(terrain));
}

#[test]
fn bad_configs_fail_the_build() {
    let w = world(8, 8);
    // The field each one names; "" for a config that does not deserialize.
    let cases = [
        (config(&[5], &[4], 0, "alive"), "iterations"),
        (config(&[9], &[4], 1, "alive"), "birth"),
        (config(&[5], &[4, 12], 1, "alive"), "survive"),
        (
            json!({"layer": "terrain", "alive": "wall", "dead": "wall", "birth": [5],
                   "survive": [4], "iterations": 1, "outside": "alive"}),
            "dead",
        ),
        (
            json!({"layer": "terrain", "alive": "wall", "dead": "floor", "birth": [5],
                   "survive": [4], "iterations": 1}),
            "",
        ),
    ];
    for (config, field) in cases {
        let e = w.build_error(&[step(config.clone(), &[])]);
        match &e {
            BuildError::Invalid { field: f, .. } => assert_eq!(f, field, "{config}: {e}"),
            BuildError::Config { message, .. } => {
                assert!(
                    field.is_empty() && message.contains("outside"),
                    "{config}: {e}"
                );
            }
            _ => panic!("{config}: {e}"),
        }
    }
}

#[test]
fn a_3d_map_is_refused() {
    let mut grid = Grid::new(Bounds::new(Coord::ZERO, GridSize::new(4, 4, 2)));
    grid.add_layer("terrain", LayerKind::Tiles).unwrap();
    grid.add_layer("zone", LayerKind::Tiles).unwrap();
    let w = World {
        grid,
        tiles: tiles(),
    };
    assert!(matches!(
        w.build_error(&[step(config(&[5], &[4], 1, "alive"), &[])]),
        BuildError::WrongDimensions { .. }
    ));
}
