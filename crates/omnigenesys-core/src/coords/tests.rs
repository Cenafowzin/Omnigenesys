use super::*;

//len
#[test]
fn len_counts_all_cells_in_2d_grid() {
    let size = GridSize::new(80, 50, 1);
    assert_eq!(size.len(), 4000);
}

#[test]
fn len_counts_all_cells_in_3d_grid() {
    let size = GridSize::new(4, 3, 2);
    assert_eq!(size.len(), 24);
}

// 2048³ does not fit in a 32-bit usize (WASM); there `len()` must panic
// instead, which `len_panics_when_size_overflows_usize` covers.
#[cfg(target_pointer_width = "64")]
#[test]
fn len_does_not_overflow_for_large_grid() {
    let size = GridSize::new(2048, 2048, 2048);
    assert_eq!(size.len(), 8_589_934_592);
}

//is_2d
#[test]
fn is_2d_returns_true_for_2d_grid() {
    let size = GridSize::new(80, 50, 1);
    assert!(size.is_2d());
}

#[test]
fn is_2d_returns_false_for_3d_grid() {
    let size = GridSize::new(80, 50, 32);
    assert!(!size.is_2d());
}

#[test]
fn is_2d_returns_false_for_0_depth_grid() {
    let size = GridSize::new(80, 50, 0);
    assert!(!size.is_2d());
}

//sub
#[test]
fn coord_subtraction_componenet_wise() {
    let a = Coord::new(1, 2, 3);
    let b = Coord::new(4, 4, 4);
    let c = a - b;
    assert_eq!(c, Coord::new(-3, -2, -1));
}

//add
#[test]
fn add_works_for_zero_const() {
    let c = Coord::new(1, 2, 3);
    assert_eq!(Coord::ZERO + c, c);
}

#[test]
fn add_sum_componenet_wise() {
    let a = Coord::new(1, 2, 3);
    let b = Coord::new(4, 4, 4);
    let c = a + b;
    assert_eq!(c, Coord::new(5, 6, 7));
}

//is empty
#[test]
fn is_empty_returns_true_for_zero_width() {
    let size = GridSize::new(0, 50, 1);
    assert!(size.is_empty());
}

#[test]
fn is_empty_returns_true_for_zero_height() {
    let size = GridSize::new(80, 0, 1);
    assert!(size.is_empty());
}

#[test]
fn is_empty_returns_true_for_zero_depth() {
    let size = GridSize::new(80, 50, 0);
    assert!(size.is_empty());
}

#[test]
fn is_empty_returns_false_for_non_zero_dimensions() {
    let size = GridSize::new(80, 50, 1);
    assert!(!size.is_empty());
}

//serde
#[test]
fn grid_size_defaults_depth_to_1_when_absent() {
    let json = r#"{"width": 80, "height": 50}"#;
    let size: GridSize = serde_json::from_str(json).unwrap();
    assert_eq!(size, GridSize::new(80, 50, 1));
}

#[test]
fn grid_size_rejects_unknown_field() {
    let json = r#"{"width": 80, "height": 50, "deth": 1}"#;
    let result: Result<GridSize, _> = serde_json::from_str(json);
    assert!(result.is_err());
}

#[test]
fn grid_size_rejects_negative_dimension() {
    let json = r#"{"width": 80, "height": 50, "depth": -1}"#;
    let result: Result<GridSize, _> = serde_json::from_str(json);
    assert!(result.is_err());
}

#[test]
fn coord_serde_roundtrip() {
    let c = Coord::new(-1, 2, i32::MIN);
    let json = serde_json::to_string(&c).unwrap();
    let c2: Coord = serde_json::from_str(&json).unwrap();
    assert_eq!(c, c2);
}

// checked_len

#[test]
fn checked_len_matches_len_for_a_normal_grid() {
    let size = GridSize::new(80, 50, 3);
    assert_eq!(size.checked_len(), Some(12_000));
}

#[test]
fn checked_len_is_none_when_size_overflows_usize() {
    // u32::MAX³ ≈ 7.9e28 does not fit even in a 64-bit usize (max ≈ 1.8e19).
    let size = GridSize::new(u32::MAX, u32::MAX, u32::MAX);
    assert_eq!(size.checked_len(), None);
}

#[test]
#[should_panic(expected = "grid size overflows usize")]
fn len_panics_when_size_overflows_usize() {
    let _ = GridSize::new(u32::MAX, u32::MAX, u32::MAX).len();
}
