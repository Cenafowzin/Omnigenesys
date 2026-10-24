//! The chunk promise for `Local` operators (PLANNING §8.7): run on one
//! chunk of a map, with the padding the steps declare, every cell inside
//! the chunk comes out as on the whole map. If an operator depends on
//! something a chunk cannot see — the map's size, cells past its declared
//! reach — this fails, before chunked generation exists to show a seam.

use omnigenesys_core::bounds::Bounds;
use omnigenesys_core::coords::{Coord, GridSize};
use omnigenesys_core::grid::Grid;
use omnigenesys_core::layer::{AnyLayer, LayerKind};
use omnigenesys_core::operator::{Registry, Scope};
use omnigenesys_core::pipeline::{Pipeline, StepSpec};
use omnigenesys_core::tile::TileRegistry;
use serde_json::{Value, json};

const MAP: Bounds = Bounds::new(Coord::new(-7, 3, 0), GridSize::new(60, 40, 1));

fn step(id: &str, op: &str, config: Value, conditions: Value) -> StepSpec {
    StepSpec {
        id: id.to_string(),
        operator: op.to_string(),
        enabled: true,
        config,
        conditions: serde_json::from_value(conditions).unwrap(),
    }
}

fn grid(bounds: Bounds) -> Grid {
    let mut g = Grid::chunk(bounds, MAP);
    g.add_layer("terreno", LayerKind::Tiles).unwrap();
    g.add_layer("altura", LayerKind::Scalar).unwrap();
    g
}

/// Runs `steps` on the whole map and on chunks of it, each grown by
/// `padding` (clipped to the map), and compares every layer inside each
/// chunk.
fn agrees(name: &str, steps: &[StepSpec], padding: u32) {
    let registry = Registry::with_builtins();
    let tiles = TileRegistry::new(["chao", "parede", "agua", "areia"]).unwrap();
    for s in steps {
        assert_eq!(
            registry.get(&s.operator).unwrap().scope,
            Scope::Local,
            "{name}: {} is not Local",
            s.operator
        );
    }
    let whole_grid = grid(MAP);
    let pipeline = Pipeline::build(&registry, &whole_grid, &tiles, steps).unwrap();
    let mut whole = whole_grid;
    pipeline.run(&mut whole, "", 99).unwrap();

    for (x, y, w, h) in [
        (0, 0, 20, 15),
        (25, 12, 18, 14),
        (44, 30, 16, 10),
        (10, 20, 30, 8),
    ] {
        let inner = Bounds::new(MAP.origin + Coord::new(x, y, 0), GridSize::new(w, h, 1));
        let p = padding as i32;
        let lo = |v: i32, o: i32| (v - p).max(o);
        let hi = |v: i32, n: u32, o: i32, len: u32| (v + n as i32 + p).min(o + len as i32);
        let origin = Coord::new(
            lo(inner.origin.x, MAP.origin.x),
            lo(inner.origin.y, MAP.origin.y),
            0,
        );
        let end = (
            hi(inner.origin.x, w, MAP.origin.x, MAP.size.width),
            hi(inner.origin.y, h, MAP.origin.y, MAP.size.height),
        );
        let region = Bounds::new(
            origin,
            GridSize::new((end.0 - origin.x) as u32, (end.1 - origin.y) as u32, 1),
        );
        let mut part = grid(region);
        pipeline.run(&mut part, "", 99).unwrap();
        for layer in ["terreno", "altura"] {
            let (a, b) = (
                whole.layer_id(layer).unwrap(),
                part.layer_id(layer).unwrap(),
            );
            for c in inner.iter() {
                match layer {
                    "terreno" => assert_eq!(
                        whole.tiles(a).unwrap().get(c),
                        part.tiles(b).unwrap().get(c),
                        "{name}: {layer} at {c:?}"
                    ),
                    _ => assert_eq!(
                        whole.scalars(a).unwrap().get(c).map(|v| v.to_bits()),
                        part.scalars(b).unwrap().get(c).map(|v| v.to_bits()),
                        "{name}: {layer} at {c:?}"
                    ),
                }
            }
        }
    }
    let _ = AnyLayer::kind;
}

#[test]
fn scatter_and_fill() {
    agrees(
        "scatter",
        &[
            step(
                "base",
                "Fill",
                json!({"layer": "terreno", "tile": "chao"}),
                json!([]),
            ),
            step(
                "ruido",
                "Scatter",
                json!({"layer": "terreno", "tile": "parede", "chance": 0.45}),
                json!([]),
            ),
        ],
        0,
    );
}

#[test]
fn border_uses_the_maps_edge() {
    agrees(
        "border",
        &[step(
            "borda",
            "FillBorder",
            json!({"layer": "terreno", "tile": "parede", "thickness": 3}),
            json!([]),
        )],
        0,
    );
}

#[test]
fn noise_and_falloff_use_world_coordinates_and_the_maps_size() {
    agrees(
        "island",
        &[
            step(
                "relevo",
                "Noise",
                json!({"layer": "altura", "wavelength": 12}),
                json!([]),
            ),
            step(
                "costa",
                "Falloff",
                json!({"layer": "altura", "shape": "round", "mix": 0.6}),
                json!([]),
            ),
            step(
                "mar",
                "Fill",
                json!({"layer": "terreno", "tile": "agua"}),
                json!([{"type": "scalar_in_range", "layer": "altura", "max": 0.45}]),
            ),
        ],
        0,
    );
}

#[test]
fn the_automaton_within_its_reach() {
    // 4 passes read 4 cells around; the padding covers it.
    agrees(
        "automaton",
        &[
            step(
                "ruido",
                "Scatter",
                json!({"layer": "terreno", "tile": "parede", "chance": 0.45}),
                json!([]),
            ),
            step(
                "alisa",
                "CellularAutomaton",
                json!({"layer": "terreno", "alive": "parede", "dead": "empty",
                "birth": [5, 6, 7, 8], "survive": [4, 5, 6, 7, 8], "iterations": 4, "outside": "alive"}),
                json!([]),
            ),
        ],
        4,
    );
}

#[test]
fn small_regions_within_their_reach() {
    agrees(
        "small regions",
        &[
            step(
                "ruido",
                "Scatter",
                json!({"layer": "terreno", "tile": "parede", "chance": 0.5}),
                json!([]),
            ),
            step(
                "bolsoes",
                "FillSmallRegions",
                json!({"layer": "terreno", "region": "empty", "fill": "parede",
                "connectivity": "orthogonal", "below": 6}),
                json!([]),
            ),
        ],
        5,
    );
}

#[test]
fn conditions_within_their_reach() {
    agrees(
        "conditions",
        &[
            step(
                "ruido",
                "Scatter",
                json!({"layer": "terreno", "tile": "parede", "chance": 0.3}),
                json!([]),
            ),
            step(
                "longe",
                "Fill",
                json!({"layer": "terreno", "tile": "areia"}),
                json!([{"type": "not_near_type", "layer": "terreno", "tile": "parede", "distance": 3}]),
            ),
        ],
        3,
    );
}
