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
    grid.add_layer("terrain", LayerKind::Tiles).unwrap();
    assert_eq!(grid.tiles("terrain").unwrap().bounds(), sample_bounds());
}

#[test]
fn added_layer_starts_filled_with_tile_zero() {
    let mut grid = sample_grid();
    grid.add_layer("terrain", LayerKind::Tiles).unwrap();
    let layer = grid.tiles("terrain").unwrap();
    for c in sample_bounds().iter() {
        assert_eq!(layer.get(c), Some(&TileId(0)), "cell {c:?}");
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
    grid.add_layer("terrain", LayerKind::Tiles).unwrap();
    grid.tiles_mut("terrain")
        .unwrap()
        .set(c, TileId(7))
        .unwrap();

    let err = grid.add_layer("terrain", LayerKind::Tiles).unwrap_err();
    assert_eq!(
        err,
        GridError::DuplicateLayer {
            name: "terrain".to_string()
        }
    );
    assert_eq!(grid.tiles("terrain").unwrap().get(c), Some(&TileId(7)));
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

// tiles / tiles_mut
//
// GridError::WrongLayerKind cannot be reached yet: AnyLayer has a single
// variant, so as_tiles never returns None. It gets a test with Scalar.

#[test]
fn tiles_reports_a_missing_layer() {
    let grid = sample_grid();
    let err = grid.tiles("nope").unwrap_err();
    assert_eq!(
        err,
        GridError::LayerNotFound {
            name: "nope".to_string()
        }
    );
}

#[test]
fn tiles_mut_reports_a_missing_layer() {
    let mut grid = sample_grid();
    let err = grid.tiles_mut("nope").unwrap_err();
    assert_eq!(
        err,
        GridError::LayerNotFound {
            name: "nope".to_string()
        }
    );
}

#[test]
fn writes_through_tiles_mut_are_visible_through_tiles() {
    let mut grid = sample_grid();
    let c = Coord::new(12, 21, 6);
    grid.add_layer("terrain", LayerKind::Tiles).unwrap();
    grid.tiles_mut("terrain")
        .unwrap()
        .set(c, TileId(3))
        .unwrap();
    assert_eq!(grid.tiles("terrain").unwrap().get(c), Some(&TileId(3)));
}

#[test]
fn layers_are_independent() {
    let mut grid = sample_grid();
    let c = Coord::new(12, 21, 6);
    grid.add_layer("terrain", LayerKind::Tiles).unwrap();
    grid.add_layer("vegetation", LayerKind::Tiles).unwrap();
    grid.tiles_mut("terrain")
        .unwrap()
        .set(c, TileId(3))
        .unwrap();
    assert_eq!(grid.tiles("vegetation").unwrap().get(c), Some(&TileId(0)));
}
