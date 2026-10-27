use super::*;
use crate::coords::GridSize;

/// A map drawn as rows: `.` costs 1, a digit costs that much, `#` is
/// blocked. Origin (-3, 5, 2), so local and world coordinates differ.
fn map(rows: &[&str]) -> (Bounds, Vec<u32>) {
    let region = Bounds::new(
        Coord::new(-3, 5, 2),
        GridSize::new(rows[0].len() as u32, rows.len() as u32, 1),
    );
    let cost = rows
        .iter()
        .flat_map(|r| r.chars())
        .map(|ch| match ch {
            '.' => 1,
            '#' => BLOCKED,
            d => d.to_digit(10).unwrap(),
        })
        .collect();
    (region, cost)
}

fn at(x: i32, y: i32) -> Coord {
    Coord::new(x - 3, y + 5, 2)
}

/// The path drawn over the map as `*`.
fn draw(rows: &[&str], path: &[Coord]) -> Vec<String> {
    let mut out: Vec<Vec<char>> = rows.iter().map(|r| r.chars().collect()).collect();
    for c in path {
        out[(c.y - 5) as usize][(c.x + 3) as usize] = '*';
    }
    out.into_iter().map(|r| r.into_iter().collect()).collect()
}

/// Cells where the walk changes direction.
fn turns(path: &[Coord]) -> usize {
    path.windows(3)
        .filter(|w| (w[1].x - w[0].x, w[1].y - w[0].y) != (w[2].x - w[1].x, w[2].y - w[1].y))
        .count()
}

fn steps_are_side_by_side(path: &[Coord]) {
    for w in path.windows(2) {
        assert_eq!(
            w[0].x.abs_diff(w[1].x) + w[0].y.abs_diff(w[1].y),
            1,
            "{path:?}"
        );
    }
}

#[test]
fn a_straight_walk_on_an_even_map() {
    let rows = [".......", ".......", "......."];
    let (region, cost) = map(&rows);
    let path = cheapest_path(region, &cost, 0, at(0, 1), at(6, 1)).unwrap();
    assert_eq!(draw(&rows, &path), [".......", "*******", "......."]);
}

#[test]
fn it_goes_around_what_is_blocked() {
    let rows = ["...#...", "...#...", "......."];
    let (region, cost) = map(&rows);
    let path = cheapest_path(region, &cost, 0, at(0, 0), at(6, 0)).unwrap();
    steps_are_side_by_side(&path);
    assert_eq!(path.len(), 11, "down two, across six, up two");
    assert!(
        path.iter()
            .all(|c| cost[(c.x + 3 + 7 * (c.y - 5)) as usize] != BLOCKED)
    );
}

#[test]
fn an_expensive_band_is_crossed_once_not_walked_along() {
    // A wall of 9s between the ends: the cheapest walk crosses one 9.
    let rows = [
        "........", //
        "99999999", //
        "........",
    ];
    let (region, cost) = map(&rows);
    for turn in [0, 3] {
        let path = cheapest_path(region, &cost, turn, at(0, 0), at(7, 2)).unwrap();
        steps_are_side_by_side(&path);
        let nines = path.iter().filter(|c| c.y == 6).count();
        assert_eq!(nines, 1, "turn {turn}: {:?}", draw(&rows, &path));
    }
}

#[test]
fn a_turn_cost_makes_the_walk_straighter() {
    let rows = ["..........", "..........", "..........", ".........."];
    let (region, cost) = map(&rows);
    let straight = cheapest_path(region, &cost, 5, at(0, 0), at(9, 3)).unwrap();
    steps_are_side_by_side(&straight);
    assert_eq!(straight.len(), 13, "still one of the shortest");
    assert_eq!(turns(&straight), 1, "an L");
    // Turns that save more than they cost are still taken: around a block.
    let rows = ["....#.....", "....#.....", "..........", "....#....."];
    let (region, cost) = map(&rows);
    let around = cheapest_path(region, &cost, 5, at(0, 0), at(9, 0)).unwrap();
    assert_eq!(around.len(), 14, "down two, across nine, up two");
    assert_eq!(turns(&around), 2);
}

#[test]
fn a_blocked_end_can_still_be_reached_but_a_walled_in_one_cannot() {
    let rows = ["....#", "....."];
    let (region, cost) = map(&rows);
    let path = cheapest_path(region, &cost, 0, at(0, 0), at(4, 0)).unwrap();
    assert_eq!(path.last(), Some(&at(4, 0)));
    assert_eq!(path.len(), 5);
    let rows = ["..#..", "..#..", "..#.."];
    let (region, cost) = map(&rows);
    assert_eq!(cheapest_path(region, &cost, 0, at(0, 1), at(4, 1)), None);
    // An end outside the region.
    assert_eq!(cheapest_path(region, &cost, 0, at(0, 1), at(9, 1)), None);
}

#[test]
fn the_same_ends_give_the_same_walk() {
    let rows = ["..2..3..", ".#..2...", "...1..#.", "..3....."];
    let (region, cost) = map(&rows);
    for turn in [0, 1, 4] {
        let a = cheapest_path(region, &cost, turn, at(0, 0), at(7, 3)).unwrap();
        let b = cheapest_path(region, &cost, turn, at(0, 0), at(7, 3)).unwrap();
        assert_eq!(a, b);
        assert_eq!((a[0], *a.last().unwrap()), (at(0, 0), at(7, 3)));
    }
}
