use super::*;
use crate::coords::GridSize;

/// Origin (10, 20, 5), size (4, 3, 2): 24 cells, non-zero origin on every axis.
fn sample_bounds() -> Bounds {
    Bounds::new(Coord::new(10, 20, 5), GridSize::new(4, 3, 2))
}

// new / bounds

#[test]
fn new_allocates_one_cell_per_bounds_cell() {
    let layer = Layer::new(sample_bounds(), 0u16).unwrap();
    assert_eq!(layer.cells.len(), sample_bounds().size.len());
}

#[test]
fn new_fills_every_cell_with_the_given_value() {
    let layer = Layer::new(sample_bounds(), 7u16).unwrap();
    for c in sample_bounds().iter() {
        assert_eq!(layer.get(c), Some(&7), "cell {c:?}");
    }
}

#[test]
fn bounds_returns_the_creation_bounds() {
    let layer = Layer::new(sample_bounds(), 0u16).unwrap();
    assert_eq!(layer.bounds(), sample_bounds());
}

// index

#[test]
fn index_follows_bounds_iter_order() {
    let layer = Layer::new(sample_bounds(), 0u16).unwrap();
    for (expected, c) in sample_bounds().iter().enumerate() {
        assert_eq!(layer.index(c), Some(expected), "cell {c:?}");
    }
}

#[test]
fn index_matches_hand_computed_value() {
    // local (1, 1, 1) -> x + width * (y + height * z) = 1 + 4 * (1 + 3 * 1) = 17
    let layer = Layer::new(sample_bounds(), 0u16).unwrap();
    assert_eq!(layer.index(Coord::new(11, 21, 6)), Some(17));
}

#[test]
fn index_outside_bounds_is_none() {
    let layer = Layer::new(sample_bounds(), 0u16).unwrap();
    // Below the origin the local coord is negative; casting it to usize
    // before the bounds check would produce a huge index instead of None.
    assert_eq!(layer.index(Coord::new(9, 20, 5)), None);
    assert_eq!(layer.index(Coord::new(14, 20, 5)), None);
    assert_eq!(layer.index(Coord::new(10, 20, 7)), None);
}

// get / set

#[test]
fn get_outside_bounds_is_none() {
    let layer = Layer::new(sample_bounds(), 0u16).unwrap();
    assert_eq!(layer.get(Coord::new(9, 20, 5)), None);
}

#[test]
fn set_then_get_returns_the_new_value() {
    let mut layer = Layer::new(sample_bounds(), 0u16).unwrap();
    let c = Coord::new(12, 21, 6);
    layer.set(c, 9).unwrap();
    assert_eq!(layer.get(c), Some(&9));
}

#[test]
fn set_changes_only_the_target_cell() {
    let mut layer = Layer::new(sample_bounds(), 0u16).unwrap();
    let target = Coord::new(12, 21, 6);
    layer.set(target, 9).unwrap();
    for c in sample_bounds().iter() {
        let expected: u16 = if c == target { 9 } else { 0 };
        assert_eq!(layer.get(c), Some(&expected), "cell {c:?}");
    }
}

#[test]
fn set_outside_bounds_returns_out_of_bounds() {
    let mut layer = Layer::new(sample_bounds(), 0u16).unwrap();
    let outside = Coord::new(14, 21, 6);
    let expected = OutOfBounds {
        coord: outside,
        bounds: sample_bounds(),
    };
    assert_eq!(layer.set(outside, 9), Err(expected));
}

#[test]
fn failed_set_leaves_layer_unchanged() {
    let mut layer = Layer::new(sample_bounds(), 0u16).unwrap();
    let before = layer.clone();
    assert!(layer.set(Coord::new(14, 21, 6), 9).is_err());
    assert_eq!(layer, before);
}

// edge cases

#[test]
fn empty_layer_has_no_cells_and_rejects_access() {
    let bounds = Bounds::new(Coord::new(10, 20, 5), GridSize::new(0, 3, 2));
    let mut layer = Layer::new(bounds, 0u16).unwrap();
    assert!(layer.cells.is_empty());
    assert_eq!(layer.get(bounds.origin), None);
    assert!(layer.set(bounds.origin, 9).is_err());
}

#[test]
fn layer_works_with_non_copy_elements() {
    let mut layer = Layer::new(sample_bounds(), String::from("floor")).unwrap();
    let origin = sample_bounds().origin;
    let c = Coord::new(12, 21, 6);
    layer.set(c, String::from("wall")).unwrap();
    assert_eq!(layer.get(c).map(String::as_str), Some("wall"));
    assert_eq!(layer.get(origin).map(String::as_str), Some("floor"));
}

// AnyLayer

fn sample_tiles_layer() -> Layer<TileId> {
    Layer::new(sample_bounds(), TileId(0)).unwrap()
}

#[test]
fn any_layer_reports_its_kind() {
    let any = AnyLayer::Tiles(sample_tiles_layer());
    assert_eq!(any.kind(), LayerKind::Tiles);
}

#[test]
fn any_layer_exposes_the_bounds_of_the_inner_layer() {
    let any = AnyLayer::Tiles(sample_tiles_layer());
    assert_eq!(any.bounds(), sample_bounds());
}

#[test]
fn as_tiles_returns_the_inner_layer() {
    let any = AnyLayer::Tiles(sample_tiles_layer());
    let layer = any.as_tiles().expect("layer is Tiles");
    assert_eq!(layer.get(sample_bounds().origin), Some(&TileId(0)));
}

#[test]
fn as_tiles_mut_writes_through_the_enum() {
    let mut any = AnyLayer::Tiles(sample_tiles_layer());
    let c = Coord::new(12, 21, 6);
    any.as_tiles_mut()
        .expect("layer is Tiles")
        .set(c, TileId(3))
        .unwrap();
    let written = any.as_tiles().expect("layer is Tiles").get(c);
    assert_eq!(written, Some(&TileId(3)));
}

// cells / cells_mut / iter

/// Gives every cell a different value, so a shifted pairing cannot pass.
fn layer_numbered_in_bounds_order() -> Layer<u16> {
    let mut layer = Layer::new(sample_bounds(), 0u16).unwrap();
    for (i, c) in sample_bounds().iter().enumerate() {
        layer.set(c, i as u16).unwrap();
    }
    layer
}

#[test]
fn cells_yields_one_value_per_cell_in_memory_order() {
    let layer = layer_numbered_in_bounds_order();
    let values: Vec<u16> = layer.cells().copied().collect();
    let expected: Vec<u16> = (0..sample_bounds().size.len() as u16).collect();
    assert_eq!(values, expected);
}

#[test]
fn cells_mut_writes_every_cell() {
    let mut layer = Layer::new(sample_bounds(), 0u16).unwrap();
    for cell in layer.cells_mut() {
        *cell = 5;
    }
    for c in sample_bounds().iter() {
        assert_eq!(layer.get(c), Some(&5), "cell {c:?}");
    }
}

#[test]
fn iter_visits_every_coord_in_bounds_order() {
    let layer = Layer::new(sample_bounds(), 0u16).unwrap();
    let coords: Vec<Coord> = layer.iter().map(|(c, _)| c).collect();
    let expected: Vec<Coord> = sample_bounds().iter().collect();
    assert_eq!(coords, expected);
}

#[test]
fn iter_pairs_each_coord_with_the_value_get_returns() {
    let layer = layer_numbered_in_bounds_order();
    for (c, value) in layer.iter() {
        assert_eq!(Some(value), layer.get(c), "cell {c:?}");
    }
}

#[test]
fn empty_layer_yields_no_cells_and_no_pairs() {
    let bounds = Bounds::new(Coord::new(10, 20, 5), GridSize::new(0, 3, 2));
    let layer = Layer::new(bounds, 0u16).unwrap();
    assert_eq!(layer.cells().count(), 0);
    assert_eq!(layer.iter().count(), 0);
}

// size validation

#[test]
fn new_rejects_a_size_that_overflows_usize() {
    let size = GridSize::new(u32::MAX, u32::MAX, u32::MAX);
    let bounds = Bounds::new(Coord::ZERO, size);
    // Must fail before trying to allocate anything.
    assert_eq!(Layer::new(bounds, 0u16), Err(TooLarge { size }));
}

// error messages
//
// Messages that embed a Debug-formatted value are checked with `contains`:
// the exact Debug layout of a derive is not a stability promise.

#[test]
fn out_of_bounds_message_names_the_coord() {
    let err = OutOfBounds {
        coord: Coord::new(14, 21, 6),
        bounds: sample_bounds(),
    };
    let msg = err.to_string();
    assert!(msg.starts_with("coord "), "{msg}");
    assert!(msg.contains("is outside bounds"), "{msg}");
    assert!(msg.contains("x: 14"), "{msg}");
}

#[test]
fn too_large_message_explains_the_problem() {
    let err = TooLarge {
        size: GridSize::new(u32::MAX, u32::MAX, u32::MAX),
    };
    assert!(err.to_string().contains("has too many cells to index"));
}

#[test]
fn layer_kind_displays_the_json_name() {
    // Same word the pipeline JSON uses in `kind` (PLANNING 8.8.1).
    assert_eq!(LayerKind::Tiles.to_string(), "tiles");
}

#[test]
fn layer_errors_are_std_errors() {
    // Compiles only if both implement std::error::Error.
    let errors: Vec<Box<dyn std::error::Error>> = vec![
        Box::new(OutOfBounds {
            coord: Coord::ZERO,
            bounds: sample_bounds(),
        }),
        Box::new(TooLarge {
            size: GridSize::new(1, 1, 1),
        }),
    ];
    assert_eq!(errors.len(), 2);
}
