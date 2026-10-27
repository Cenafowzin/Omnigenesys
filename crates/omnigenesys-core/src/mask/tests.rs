use super::*;
use crate::coords::GridSize;

fn row_bounds(width: u32) -> Bounds {
    // A single row with a non-zero origin, so words and bounds both matter.
    Bounds::new(Coord::new(-3, 7, 2), GridSize::new(width, 1, 1))
}

fn row_layer(width: u32) -> Layer<TileId> {
    Layer::new(row_bounds(width), TileId::EMPTY).unwrap()
}

fn set(ids: &[u16]) -> TileSet {
    TileSet::new(&ids.iter().map(|&i| TileId(i)).collect::<Vec<_>>())
}

/// Origin (10, 20, 5), size (5, 3, 2): 30 cells, the last word half used.
fn box_bounds() -> Bounds {
    Bounds::new(Coord::new(10, 20, 5), GridSize::new(5, 3, 2))
}

fn mask_of(bounds: Bounds, on: &[Coord]) -> CellMask {
    let mut w = MaskWriter::new(bounds);
    for c in bounds.iter() {
        w.push(on.contains(&c));
    }
    w.finish()
}

// from_layer / get

#[test]
fn mask_uses_one_word_per_64_cells_rounded_up() {
    for (cells, words) in [(0, 0), (1, 1), (64, 1), (65, 2), (128, 2), (129, 3)] {
        let mask = CellMask::from_layer(&row_layer(cells), &set(&[1]));
        assert_eq!(mask.words.len(), words, "{cells} cells");
    }
}

#[test]
fn mask_bits_are_right_across_word_boundaries() {
    let mut layer = row_layer(130);
    let marked = [0, 1, 62, 63, 64, 65, 127, 128, 129];
    for x in marked {
        layer.set(Coord::new(-3 + x, 7, 2), TileId(2)).unwrap();
    }
    let mask = CellMask::from_layer(&layer, &set(&[2]));
    for x in 0..130 {
        let expected = marked.contains(&x);
        assert_eq!(mask.get(Coord::new(-3 + x, 7, 2)), expected, "x = {x}");
    }
}

#[test]
fn mask_is_false_outside_its_bounds() {
    let mut layer = row_layer(4);
    for c in layer.bounds().iter() {
        layer.set(c, TileId(1)).unwrap();
    }
    let mask = CellMask::from_layer(&layer, &set(&[1]));
    assert!(mask.get(Coord::new(-3, 7, 2)));
    assert!(!mask.get(Coord::new(-4, 7, 2)));
    assert!(!mask.get(Coord::new(1, 7, 2)));
    assert!(!mask.get(Coord::new(-3, 8, 2)));
}

#[test]
fn mask_matches_any_tile_of_the_set() {
    let mut layer = row_layer(3);
    layer.set(Coord::new(-3, 7, 2), TileId(1)).unwrap();
    layer.set(Coord::new(-2, 7, 2), TileId(2)).unwrap();
    layer.set(Coord::new(-1, 7, 2), TileId(3)).unwrap();
    let mask = CellMask::from_layer(&layer, &set(&[1, 3]));
    assert!(mask.get(Coord::new(-3, 7, 2)));
    assert!(!mask.get(Coord::new(-2, 7, 2)));
    assert!(mask.get(Coord::new(-1, 7, 2)));
}

// new / full / tail

#[test]
fn new_is_all_false_and_full_is_all_true() {
    let b = box_bounds();
    let (none, all) = (CellMask::new(b), CellMask::full(b));
    for c in b.iter() {
        assert!(!none.get(c), "cell {c:?}");
        assert!(all.get(c), "cell {c:?}");
    }
}

#[test]
fn bits_past_the_last_cell_stay_zero() {
    // 30 cells in one word: a full mask must count 30, not 64. Equality
    // between masks relies on the same invariant.
    let b = box_bounds();
    assert_eq!(CellMask::full(b).count_ones(), 30);
    assert_eq!(CellMask::full(b), mask_of(b, &b.iter().collect::<Vec<_>>()));
    assert_eq!(!CellMask::full(b), CellMask::new(b));
}

#[test]
fn full_mask_of_an_exact_word_multiple_counts_every_cell() {
    let b = row_bounds(128);
    assert_eq!(CellMask::full(b).count_ones(), 128);
}

#[test]
fn empty_bounds_give_an_empty_mask() {
    let b = row_bounds(0);
    assert_eq!(CellMask::full(b).count_ones(), 0);
    assert_eq!(CellMask::full(b).iter_ones().count(), 0);
}

// writer

#[test]
#[should_panic(expected = "pushed a different number of cells")]
fn writer_rejects_too_few_cells() {
    let mut w = MaskWriter::new(box_bounds());
    w.push(true);
    w.finish();
}

#[test]
#[should_panic(expected = "pushed a different number of cells")]
fn writer_rejects_too_many_cells() {
    // 31 pushes still fit in the one word of a 30-cell mask: the check must
    // count cells, not words.
    let mut w = MaskWriter::new(box_bounds());
    for _ in 0..31 {
        w.push(true);
    }
    w.finish();
}

// operators

#[test]
fn and_or_not_work_cell_by_cell() {
    let b = box_bounds();
    let (p, q, r) = (
        Coord::new(10, 20, 5),
        Coord::new(14, 22, 6),
        Coord::new(12, 21, 5),
    );
    let left = mask_of(b, &[p, q]);
    let right = mask_of(b, &[q, r]);

    let mut and = left.clone();
    and &= &right;
    assert_eq!(and, mask_of(b, &[q]));

    let mut or = left.clone();
    or |= &right;
    assert_eq!(or, mask_of(b, &[p, q, r]));

    let not = !left;
    assert_eq!(not.count_ones(), 28);
    assert!(!not.get(p) && !not.get(q) && not.get(r));
}

#[test]
#[should_panic(expected = "masks over different bounds")]
fn combining_masks_over_different_bounds_panics() {
    let mut a = CellMask::new(box_bounds());
    a &= &CellMask::new(row_bounds(30));
}

// iter_ones

#[test]
fn iter_ones_yields_the_true_cells_in_bounds_order() {
    let b = box_bounds();
    // Every third cell, so set bits land in every row and plane.
    let on: Vec<Coord> = b.iter().step_by(3).collect();
    let mask = mask_of(b, &on);
    assert_eq!(mask.iter_ones().collect::<Vec<_>>(), on);
    assert_eq!(mask.count_ones(), on.len());
}

#[test]
fn iter_ones_crosses_word_boundaries() {
    let b = row_bounds(130);
    let on: Vec<Coord> = [0, 63, 64, 127, 128, 129]
        .iter()
        .map(|&x| Coord::new(-3 + x, 7, 2))
        .collect();
    assert_eq!(mask_of(b, &on).iter_ones().collect::<Vec<_>>(), on);
}

#[test]
fn iter_ones_matches_a_scan_on_any_shape_and_density() {
    // Reference: every cell of the bounds, in order, filtered by `get`. Covers
    // jumps over several rows and planes (sparse), every cell (full), and the
    // odd shapes where a row is a single cell or a single word spans planes.
    let shapes = [
        GridSize::new(5, 3, 4),
        GridSize::new(1, 7, 3),
        GridSize::new(70, 1, 2),
        GridSize::new(3, 1, 1),
    ];
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    for size in shapes {
        let b = Bounds::new(Coord::new(-4, 9, -2), size);
        // 1 in 40 (sparse), 1 in 2, and every cell.
        for keep_one_in in [40, 2, 1] {
            let on: Vec<Coord> = b
                .iter()
                .filter(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    seed.is_multiple_of(keep_one_in)
                })
                .collect();
            let mask = mask_of(b, &on);
            let got: Vec<Coord> = mask.iter_ones().collect();
            assert_eq!(got, on, "size {size:?}, 1 in {keep_one_in}");
        }
    }
}

/// The distance to the nearest edge of `map`, cell by cell: what
/// `from_edge_distance` writes a row at a time.
fn naive_edge(region: Bounds, map: Bounds, min: u32, max: u32) -> CellMask {
    let flat = map.size.depth <= 1;
    let mut w = MaskWriter::new(region);
    for c in region.iter() {
        let o = map.origin;
        let far = |p: i32, o: i32, n: u32| {
            (i64::from(p) - i64::from(o)).min(i64::from(o) + i64::from(n) - 1 - i64::from(p))
        };
        let mut d = far(c.x, o.x, map.size.width).min(far(c.y, o.y, map.size.height));
        if !flat {
            d = d.min(far(c.z, o.z, map.size.depth));
        }
        w.push(d >= i64::from(min) && d <= i64::from(max));
    }
    w.finish()
}

#[test]
fn edge_distance_by_rows_matches_cell_by_cell() {
    let maps = [
        Bounds::new(Coord::new(-7, 3, 0), GridSize::new(70, 13, 1)),
        Bounds::new(Coord::new(0, 0, 0), GridSize::new(1, 1, 1)),
        Bounds::new(Coord::new(2, -4, -1), GridSize::new(9, 11, 6)),
    ];
    let ranges = [
        (0, 0),
        (0, 2),
        (1, 1),
        (2, 5),
        (3, u32::MAX),
        (0, u32::MAX),
        (40, u32::MAX),
    ];
    for map in maps {
        // The map itself, a chunk inside it, and regions reaching past it
        // (an `offset` or `neighbours` looks there).
        let regions = [
            map,
            map.expand(2, 3, 1),
            Bounds::new(
                Coord::new(map.origin.x + 1, map.origin.y - 2, map.origin.z),
                GridSize::new(65, 4, map.size.depth),
            ),
        ];
        for region in regions {
            for (min, max) in ranges {
                assert_eq!(
                    CellMask::from_edge_distance(region, map, min, max),
                    naive_edge(region, map, min, max),
                    "{map:?} {region:?} {min}..={max}"
                );
            }
        }
    }
}
