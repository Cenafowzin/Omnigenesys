use serde_json::json;

use crate::{
    bounds::Bounds,
    condition::Condition,
    coords::{Coord, GridSize},
    grid::Grid,
    layer::LayerKind,
    operator::{BuildError, Registry},
    pipeline::{Pipeline, StepSpec},
    tile::TileRegistry,
};

fn border(thickness: u32, conditions: &[&str]) -> StepSpec {
    StepSpec {
        id: "border".to_string(),
        operator: "FillBorder".to_string(),
        enabled: true,
        config: json!({"layer": "terrain", "tile": "wall", "thickness": thickness}),
        conditions: conditions
            .iter()
            .map(|c| serde_json::from_str::<Condition>(c).unwrap())
            .collect(),
    }
}

/// The terrain after `steps` on a `w`×`h` map, as rows: `#` wall, `.` floor,
/// space for empty.
fn run(w: u32, h: u32, steps: &[StepSpec]) -> Vec<String> {
    let mut grid = Grid::new(Bounds::new(Coord::new(3, -2, 0), GridSize::new(w, h, 1)));
    grid.add_layer("terrain", LayerKind::Tiles).unwrap();
    let tiles = TileRegistry::new(["floor", "wall"]).unwrap();
    Pipeline::build(&Registry::with_builtins(), &grid, &tiles, steps)
        .unwrap()
        .run(&mut grid, "", 1)
        .unwrap();
    let terrain = grid.layer_id("terrain").unwrap();
    let (floor, wall) = (tiles.id("floor").unwrap(), tiles.id("wall").unwrap());
    grid.tiles(terrain)
        .unwrap()
        .as_slice()
        .chunks(w as usize)
        .map(|row| {
            row.iter()
                .map(|&t| match t {
                    t if t == wall => '#',
                    t if t == floor => '.',
                    _ => ' ',
                })
                .collect()
        })
        .collect()
}

fn fill_floor() -> StepSpec {
    StepSpec {
        id: "floor".to_string(),
        operator: "Fill".to_string(),
        enabled: true,
        config: json!({"layer": "terrain", "tile": "floor"}),
        conditions: Vec::new(),
    }
}

#[test]
fn fills_each_edge_to_the_thickness() {
    assert_eq!(
        run(7, 6, &[fill_floor(), border(2, &[])]),
        [
            "#######", "#######", "##...##", "##...##", "#######", "#######"
        ]
    );
    assert_eq!(
        run(4, 3, &[fill_floor(), border(1, &[])]),
        ["####", "#..#", "####"]
    );
}

#[test]
fn a_map_thinner_than_two_borders_is_all_border() {
    assert_eq!(run(3, 9, &[border(2, &[])])[4], "###");
    assert_eq!(run(1, 1, &[border(5, &[])]), ["#"]);
}

#[test]
fn conditions_limit_the_border() {
    // Only where there is floor: on an empty map, nothing.
    let on_floor = r#"{"type": "tile_is", "layer": "terrain", "tile": "floor"}"#;
    assert_eq!(
        run(3, 3, &[fill_floor(), border(1, &[on_floor])]),
        ["###", "#.#", "###"]
    );
    assert_eq!(run(3, 3, &[border(1, &[on_floor])]), ["   ", "   ", "   "]);
}

#[test]
fn zero_thickness_is_a_build_error() {
    let grid = {
        let mut g = Grid::new(Bounds::new(Coord::ZERO, GridSize::new(4, 4, 1)));
        g.add_layer("terrain", LayerKind::Tiles).unwrap();
        g
    };
    let tiles = TileRegistry::new(["floor", "wall"]).unwrap();
    let errors = Pipeline::build(&Registry::with_builtins(), &grid, &tiles, &[border(0, &[])])
        .err()
        .unwrap();
    assert_eq!(
        errors[0].error,
        BuildError::Invalid {
            field: "thickness".to_string(),
            message: "must be at least 1".to_string(),
        }
    );
}
