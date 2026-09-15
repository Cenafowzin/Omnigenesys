use super::*;

/// Origin (10, 20, 5), size (4, 3, 2): x in 10..14, y in 20..23, z in 5..7.
/// Non-zero on every axis so a missing origin subtraction cannot pass.
fn sample() -> Bounds {
    Bounds::new(Coord::new(10, 20, 5), GridSize::new(4, 3, 2))
}

// expand

#[test]
fn expand_moves_origin_back_and_grows_on_both_sides() {
    let b = Bounds::new(Coord::new(10, 20, 0), GridSize::new(4, 4, 1));
    let expected = Bounds::new(Coord::new(8, 18, 0), GridSize::new(8, 8, 1));
    assert_eq!(b.expand(2, 2, 0), expected);
}

#[test]
fn expand_applies_each_argument_to_its_own_axis() {
    let expected = Bounds::new(Coord::new(9, 18, 2), GridSize::new(6, 7, 8));
    assert_eq!(sample().expand(1, 2, 3), expected);
}

#[test]
fn expand_by_zero_is_identity() {
    assert_eq!(sample().expand(0, 0, 0), sample());
}

#[test]
fn expand_adds_exactly_the_padding_ring() {
    let b = sample();
    let padded = b.expand(1, 0, 0);

    let left = Coord::new(9, 20, 5);
    let right = Coord::new(14, 20, 5);
    assert!(!b.contains(left));
    assert!(!b.contains(right));
    assert!(padded.contains(left));
    assert!(padded.contains(right));

    assert!(!padded.contains(Coord::new(8, 20, 5)));
    assert!(!padded.contains(Coord::new(15, 20, 5)));
}

// contains

#[test]
fn contains_interior_cell() {
    assert!(sample().contains(Coord::new(12, 21, 6)));
}

#[test]
fn contains_origin() {
    assert!(sample().contains(Coord::new(10, 20, 5)));
}

#[test]
fn contains_last_cell() {
    assert!(sample().contains(Coord::new(13, 22, 6)));
}

#[test]
fn contains_rejects_upper_edge_x() {
    assert!(!sample().contains(Coord::new(14, 21, 6)));
}

#[test]
fn contains_rejects_upper_edge_y() {
    assert!(!sample().contains(Coord::new(12, 23, 6)));
}

#[test]
fn contains_rejects_upper_edge_z() {
    assert!(!sample().contains(Coord::new(12, 21, 7)));
}

#[test]
fn contains_rejects_below_origin_x() {
    assert!(!sample().contains(Coord::new(9, 21, 6)));
}

#[test]
fn contains_rejects_below_origin_y() {
    assert!(!sample().contains(Coord::new(12, 19, 6)));
}

#[test]
fn contains_rejects_below_origin_z() {
    assert!(!sample().contains(Coord::new(12, 21, 4)));
}

#[test]
fn contains_works_with_negative_origin() {
    let b = Bounds::new(Coord::new(-8, -8, 0), GridSize::new(16, 16, 1));
    assert!(b.contains(Coord::new(-8, -8, 0)));
    assert!(b.contains(Coord::new(7, 7, 0)));
    assert!(!b.contains(Coord::new(-9, -8, 0)));
    assert!(!b.contains(Coord::new(8, 8, 0)));
}

#[test]
fn empty_bounds_does_not_contain_its_origin() {
    let b = Bounds::new(Coord::new(10, 20, 5), GridSize::new(0, 3, 2));
    assert!(!b.contains(b.origin));
}

// iter

#[test]
fn iter_yields_coords_in_memory_order() {
    let b = Bounds::new(Coord::new(10, 20, 5), GridSize::new(2, 2, 2));
    let coords: Vec<Coord> = b.iter().collect();
    let expected = [
        Coord::new(10, 20, 5),
        Coord::new(11, 20, 5),
        Coord::new(10, 21, 5),
        Coord::new(11, 21, 5),
        Coord::new(10, 20, 6),
        Coord::new(11, 20, 6),
        Coord::new(10, 21, 6),
        Coord::new(11, 21, 6),
    ];
    assert_eq!(coords, expected);
}

#[test]
fn iter_starts_at_origin_and_ends_at_last_cell() {
    let b = Bounds::new(Coord::new(10, 20, 5), GridSize::new(3, 4, 5));
    assert_eq!(b.iter().next(), Some(Coord::new(10, 20, 5)));
    assert_eq!(b.iter().last(), Some(Coord::new(12, 23, 9)));
}

#[test]
fn iter_yields_one_coord_per_cell() {
    let b = sample();
    assert_eq!(b.iter().count(), b.size.len());
}

#[test]
fn iter_of_empty_bounds_yields_nothing() {
    let b = Bounds::new(Coord::new(10, 20, 5), GridSize::new(0, 3, 2));
    assert_eq!(b.iter().count(), 0);
}

// serde

#[test]
fn bounds_deserializes_with_nested_depth_default() {
    let json = r#"{"origin": {"x": 1, "y": 2, "z": 3}, "size": {"width": 4, "height": 5}}"#;
    let b: Bounds = serde_json::from_str(json).unwrap();
    assert_eq!(b, Bounds::new(Coord::new(1, 2, 3), GridSize::new(4, 5, 1)));
}

#[test]
fn bounds_rejects_unknown_field() {
    let json =
        r#"{"origin": {"x": 1, "y": 2, "z": 3}, "size": {"width": 4, "height": 5}, "padding": 2}"#;
    let result: Result<Bounds, _> = serde_json::from_str(json);
    assert!(result.is_err());
}
