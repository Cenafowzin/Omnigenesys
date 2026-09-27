use super::*;
use crate::coords::{Coord, GridSize};

fn sample_bounds() -> Bounds {
    Bounds::new(Coord::new(10, 20, 5), GridSize::new(4, 3, 2))
}

fn sample_grid() -> Grid {
    Grid::new(sample_bounds())
}

// new / add_layer

#[test]
fn new_grid_has_the_given_bounds_and_no_layers() {
    let grid = sample_grid();
    assert_eq!(grid.bounds(), sample_bounds());
    assert_eq!(grid.names().count(), 0);
}

#[test]
fn added_layer_covers_the_grid_bounds() {
    let mut grid = sample_grid();
    let terrain = grid.add_layer("terrain", LayerKind::Tiles).unwrap();
    assert_eq!(grid.tiles(terrain).unwrap().bounds(), sample_bounds());
}

#[test]
fn added_layer_starts_empty() {
    let mut grid = sample_grid();
    let terrain = grid.add_layer("terrain", LayerKind::Tiles).unwrap();
    let layer = grid.tiles(terrain).unwrap();
    for c in sample_bounds().iter() {
        assert_eq!(layer.get(c), Some(&TileId::EMPTY), "cell {c:?}");
    }
}

#[test]
fn add_layer_accepts_both_str_and_string() {
    let mut grid = sample_grid();
    grid.add_layer("terrain", LayerKind::Tiles).unwrap();
    grid.add_layer(String::from("vegetation"), LayerKind::Tiles)
        .unwrap();
    assert_eq!(grid.names().count(), 2);
}

#[test]
fn add_layer_rejects_a_duplicate_name_without_touching_the_existing_one() {
    let mut grid = sample_grid();
    let c = Coord::new(12, 21, 6);
    let terrain = grid.add_layer("terrain", LayerKind::Tiles).unwrap();
    grid.tiles_mut(terrain).unwrap().set(c, TileId(7)).unwrap();

    let err = grid.add_layer("terrain", LayerKind::Tiles).unwrap_err();
    assert_eq!(
        err,
        GridError::DuplicateLayer {
            name: "terrain".to_string()
        }
    );
    assert_eq!(grid.tiles(terrain).unwrap().get(c), Some(&TileId(7)));
    assert_eq!(grid.names().count(), 1);
}

#[test]
fn names_follow_insertion_order() {
    // Not alphabetical on purpose: a sorted map would reorder these.
    let mut grid = sample_grid();
    for name in ["terrain", "vegetation", "height"] {
        grid.add_layer(name, LayerKind::Tiles).unwrap();
    }
    let names: Vec<&str> = grid.names().collect();
    assert_eq!(names, ["terrain", "vegetation", "height"]);
}

// layer_id

#[test]
fn add_layer_and_layer_id_agree() {
    let mut grid = sample_grid();
    let terrain = grid.add_layer("terrain", LayerKind::Tiles).unwrap();
    let vegetation = grid.add_layer("vegetation", LayerKind::Tiles).unwrap();
    assert_eq!(grid.layer_id("terrain"), Ok(terrain));
    assert_eq!(grid.layer_id("vegetation"), Ok(vegetation));
    assert_ne!(terrain, vegetation);
}

#[test]
fn layer_id_reports_a_missing_name() {
    let grid = sample_grid();
    assert_eq!(
        grid.layer_id("nope"),
        Err(GridError::LayerNotFound {
            name: "nope".to_string()
        })
    );
}

#[test]
fn a_layer_id_is_valid_across_grids_declared_in_the_same_order() {
    // Chunked generation (Part 2): every chunk is its own Grid built from the
    // same layer list, so an id resolved once at build works on all of them.
    let chunk = |origin: Coord| {
        let mut grid = Grid::new(Bounds::new(origin, GridSize::new(4, 3, 2)));
        grid.add_layer("terrain", LayerKind::Tiles).unwrap();
        grid.add_layer("vegetation", LayerKind::Tiles).unwrap();
        grid
    };
    let a = chunk(Coord::new(0, 0, 0));
    let mut b = chunk(Coord::new(4, 0, 0));

    let vegetation = a.layer_id("vegetation").unwrap();
    let c = Coord::new(5, 1, 0);
    b.tiles_mut(vegetation).unwrap().set(c, TileId(3)).unwrap();
    assert_eq!(b.layer_id("vegetation"), Ok(vegetation));
    assert_eq!(b.tiles(vegetation).unwrap().get(c), Some(&TileId(3)));
}

// tiles / tiles_mut
//
// GridError::WrongLayerKind cannot be reached yet: AnyLayer has a single
// variant, so as_tiles never returns None. It gets a test with Scalar.

#[test]
fn tiles_reports_an_unknown_layer_id() {
    // An id from a grid with more layers does not exist in this one.
    let mut other = sample_grid();
    other.add_layer("terrain", LayerKind::Tiles).unwrap();
    let vegetation = other.add_layer("vegetation", LayerKind::Tiles).unwrap();

    let mut grid = sample_grid();
    grid.add_layer("terrain", LayerKind::Tiles).unwrap();
    let expected = GridError::UnknownLayerId { id: vegetation };
    assert_eq!(grid.tiles(vegetation).unwrap_err(), expected);
    assert_eq!(grid.tiles_mut(vegetation).unwrap_err(), expected);
}

#[test]
fn writes_through_tiles_mut_are_visible_through_tiles() {
    let mut grid = sample_grid();
    let c = Coord::new(12, 21, 6);
    let terrain = grid.add_layer("terrain", LayerKind::Tiles).unwrap();
    grid.tiles_mut(terrain).unwrap().set(c, TileId(3)).unwrap();
    assert_eq!(grid.tiles(terrain).unwrap().get(c), Some(&TileId(3)));
}

#[test]
fn layers_are_independent() {
    let mut grid = sample_grid();
    let c = Coord::new(12, 21, 6);
    let terrain = grid.add_layer("terrain", LayerKind::Tiles).unwrap();
    let vegetation = grid.add_layer("vegetation", LayerKind::Tiles).unwrap();
    grid.tiles_mut(terrain).unwrap().set(c, TileId(3)).unwrap();
    assert_eq!(grid.tiles(vegetation).unwrap().get(c), Some(&TileId::EMPTY));
}

#[test]
fn add_layer_reports_a_layer_too_large_and_adds_nothing() {
    let size = GridSize::new(u32::MAX, u32::MAX, u32::MAX);
    let mut grid = Grid::new(Bounds::new(Coord::ZERO, size));
    let err = grid.add_layer("terrain", LayerKind::Tiles).unwrap_err();
    assert_eq!(
        err,
        GridError::LayerTooLarge {
            name: "terrain".to_string(),
            size,
        }
    );
    assert_eq!(grid.names().count(), 0);
}

// error messages

#[test]
fn grid_error_messages() {
    let cases = [
        (
            GridError::LayerNotFound {
                name: "terrain".to_string(),
            },
            "layer 'terrain' not found",
        ),
        (
            GridError::DuplicateLayer {
                name: "terrain".to_string(),
            },
            "layer 'terrain' already exists",
        ),
        (
            // Unreachable through the API while AnyLayer has one variant,
            // but the message is already part of the contract.
            GridError::WrongLayerKind {
                name: "terrain".to_string(),
                expected: LayerKind::Tiles,
                found: LayerKind::Tiles,
            },
            "layer 'terrain' is tiles, expected tiles",
        ),
        (
            GridError::UnknownLayerId { id: LayerId(7) },
            "unknown layer id LayerId(7)",
        ),
    ];
    for (err, expected) in cases {
        assert_eq!(err.to_string(), expected);
    }
}

#[test]
fn layer_too_large_message_names_the_layer() {
    let err = GridError::LayerTooLarge {
        name: "terrain".to_string(),
        size: GridSize::new(u32::MAX, 1, 1),
    };
    assert!(
        err.to_string()
            .starts_with("layer 'terrain' is too large: ")
    );
}

#[test]
fn grid_error_is_a_std_error() {
    let err: Box<dyn std::error::Error> = Box::new(GridError::LayerNotFound {
        name: "terrain".to_string(),
    });
    assert_eq!(err.to_string(), "layer 'terrain' not found");
}

#[test]
fn grid_debug_lists_its_layers() {
    let mut grid = sample_grid();
    grid.add_layer("terrain", LayerKind::Tiles).unwrap();
    assert!(format!("{grid:?}").contains("terrain"));
}
