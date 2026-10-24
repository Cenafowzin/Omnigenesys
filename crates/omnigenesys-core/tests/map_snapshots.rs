//! Golden maps: a small pipeline run end to end, drawn as ASCII and frozen
//! with insta. Any change to the rng, the conditions, the evaluation or the
//! operators shows up here as a diff of the drawing.
//!
//! Unlike the rng contract (tests/rng_contract.rs, never updated), these may
//! be updated on purpose when an operator's behaviour changes: review the
//! diff with `cargo insta review` and commit the new snapshot with the
//! change that caused it.

use omnigenesys_core::bounds::Bounds;
use omnigenesys_core::condition::Condition;
use omnigenesys_core::coords::{Coord, GridSize};
use omnigenesys_core::export::{self, ENGINE_VERSION, Encoding};
use omnigenesys_core::grid::Grid;
use omnigenesys_core::layer::LayerKind;
use omnigenesys_core::operator::Registry;
use omnigenesys_core::pipeline::{Pipeline, StepSpec};
use omnigenesys_core::spec::{Built, PipelineSpec};
use omnigenesys_core::tile::TileRegistry;
use serde_json::{Value, json};

fn step(id: &str, operator: &str, config: Value, conditions: &[&str]) -> StepSpec {
    StepSpec {
        id: id.to_string(),
        operator: operator.to_string(),
        enabled: true,
        config,
        conditions: conditions
            .iter()
            .map(|c| serde_json::from_str::<Condition>(c).unwrap())
            .collect(),
    }
}

/// Floor, scattered walls, lakes away from walls, trees on the floor:
/// tests/pipelines/island.json, read like any user's file.
const ISLAND: &str = include_str!("pipelines/island.json");

fn generate(size: GridSize, seed: u64) -> String {
    let parsed = PipelineSpec::parse(ISLAND).unwrap();
    let mut spec = parsed.spec;
    spec.size = size;
    let Built {
        mut grid,
        tiles,
        pipeline,
    } = spec.build(&Registry::with_builtins()).unwrap();
    pipeline.run(&mut grid, &spec.salt, seed).unwrap();
    let terrain = grid.layer_id("terrain").unwrap();
    let objects = grid.layer_id("objects").unwrap();

    // An object is drawn over the terrain under it.
    let glyph = |name: &str| match name {
        "floor" => '.',
        "wall" => '#',
        "water" => '~',
        "tree" => 'T',
        _ => ' ',
    };
    let (t, o) = (grid.tiles(terrain).unwrap(), grid.tiles(objects).unwrap());
    let mut out = String::from("legend: . floor  # wall  ~ water  T tree\n");
    for z in 0..size.depth as i32 {
        if size.depth > 1 {
            out.push_str(&format!("z = {z}\n"));
        }
        for y in 0..size.height as i32 {
            for x in 0..size.width as i32 {
                let c = Coord::new(x, y, z);
                let object = tiles.name(*o.get(c).unwrap()).unwrap();
                let name = if object == "empty" {
                    tiles.name(*t.get(c).unwrap()).unwrap()
                } else {
                    object
                };
                out.push(glyph(name));
            }
            out.push('\n');
        }
    }
    out
}

#[test]
fn island_2d() {
    insta::assert_snapshot!(generate(GridSize::new(48, 16, 1), 512));
}

#[test]
fn island_3d() {
    // The same pipeline on a volume: distances reach across z too.
    insta::assert_snapshot!(generate(GridSize::new(16, 8, 3), 512));
}

/// Noise, the cellular automaton and a border: tests/pipelines/cave.json.
const CAVE: &str = include_str!("pipelines/cave.json");

#[test]
fn cave_2d() {
    let spec = PipelineSpec::parse(CAVE).unwrap().spec;
    let Built {
        mut grid,
        tiles,
        pipeline,
    } = spec.build(&Registry::with_builtins()).unwrap();
    pipeline.run(&mut grid, &spec.salt, 512).unwrap();
    let terrain = grid.tiles(grid.layer_id("terrain").unwrap()).unwrap();
    let mut out = String::from("legend: . floor  # wall\n");
    for row in terrain.as_slice().chunks(spec.size.width as usize) {
        for &t in row {
            out.push(match tiles.name(t).unwrap() {
                "floor" => '.',
                "wall" => '#',
                _ => ' ',
            });
        }
        out.push('\n');
    }
    insta::assert_snapshot!(out);
}

#[test]
fn scatter_gives_each_cell_the_same_answer_in_any_map() {
    // Two maps of different size and origin that overlap: in the overlap the
    // walls must be identical. This is what will keep neighbouring chunks
    // from showing seams (PLANNING §2.1): a cell's draw depends on its
    // position and the step, never on the map around it.
    let walls = |bounds: Bounds| {
        let mut grid = Grid::new(bounds);
        let terrain = grid.add_layer("terrain", LayerKind::Tiles).unwrap();
        let tiles = TileRegistry::new(["floor", "wall"]).unwrap();
        let steps = [step(
            "walls",
            "Scatter",
            json!({"layer": "terrain", "tile": "wall", "chance": 0.3}),
            &[],
        )];
        Pipeline::build(&Registry::with_builtins(), &grid, &tiles, &steps)
            .unwrap()
            .run(&mut grid, "", 512)
            .unwrap();
        let wall = tiles.id("wall").unwrap();
        let layer = grid.tiles(terrain).unwrap().clone();
        move |c: Coord| layer.get(c) == Some(&wall)
    };
    let big = walls(Bounds::new(Coord::new(0, 0, 0), GridSize::new(40, 30, 4)));
    let small = walls(Bounds::new(Coord::new(25, -5, 2), GridSize::new(30, 20, 5)));
    // Overlap: x 25..40, y 0..15, z 2..4.
    let overlap = Bounds::new(Coord::new(25, 0, 2), GridSize::new(15, 15, 2));
    let mut walls_seen = 0;
    for c in overlap.iter() {
        assert_eq!(big(c), small(c), "cell {c:?}");
        walls_seen += usize::from(big(c));
    }
    assert!(walls_seen > 0, "the overlap must contain walls to compare");
}

#[test]
fn the_island_file_is_clean() {
    let parsed = PipelineSpec::parse(ISLAND).unwrap();
    assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    assert_eq!(parsed.spec.seed, Some(512));
}

#[test]
fn island_depends_on_the_seed() {
    let size = GridSize::new(48, 16, 1);
    assert_eq!(generate(size, 512), generate(size, 512));
    assert_ne!(generate(size, 512), generate(size, 513));
}

/// The island as output v2, the bytes an engine receives.
fn island_output(seed: u64) -> String {
    let spec = PipelineSpec::parse(ISLAND).unwrap().spec;
    let Built {
        mut grid,
        tiles,
        pipeline,
    } = spec.build(&Registry::with_builtins()).unwrap();
    pipeline.run(&mut grid, &spec.salt, seed).unwrap();
    let provenance = spec.provenance(seed, &spec.salt);
    let mut out = Vec::new();
    export::write_json(&mut out, &grid, &tiles, &provenance, Encoding::Auto).unwrap();
    String::from_utf8(out).unwrap()
}

#[test]
fn island_output_v2() {
    // The version would change the snapshot on every release, not the map.
    let text = island_output(512).replace(
        &format!("\"engine_version\": \"{ENGINE_VERSION}\""),
        "\"engine_version\": \"<engine>\"",
    );
    insta::assert_snapshot!(text);
}

#[test]
fn the_same_seed_gives_the_same_bytes() {
    assert_eq!(island_output(512), island_output(512));
    assert_ne!(island_output(512), island_output(513));
}
