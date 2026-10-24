//! What the room operators share: drawing a room, telling its walls.

use crate::{
    bounds::Bounds,
    coords::{Coord, GridSize},
    layer::Layer,
    tile::TileId,
};

/// A room's box: `bbox` includes its wall ring, one cell thick. Cells
/// outside the layer are skipped.
pub(super) fn draw_room(tiles: &mut Layer<TileId>, bbox: Bounds, wall: TileId, floor: TileId) {
    for c in bbox.iter() {
        let tile = if on_ring(bbox, c) { wall } else { floor };
        let _ = tiles.set(c, tile);
    }
}

/// Whether `c` is on the outer ring of `bbox` (in x and y).
pub(super) fn on_ring(bbox: Bounds, c: Coord) -> bool {
    let (x0, y0) = (bbox.origin.x, bbox.origin.y);
    let (x1, y1) = (
        x0 + bbox.size.width as i32 - 1,
        y0 + bbox.size.height as i32 - 1,
    );
    bbox.contains(c) && (c.x == x0 || c.x == x1 || c.y == y0 || c.y == y1)
}

/// The inside of a room, without its wall ring.
pub(super) fn interior(bbox: Bounds) -> Bounds {
    let shrink = |n: u32| n.saturating_sub(2);
    Bounds::new(
        bbox.origin + Coord::new(1, 1, 0),
        GridSize::new(
            shrink(bbox.size.width),
            shrink(bbox.size.height),
            bbox.size.depth,
        ),
    )
}

/// The cell at the middle of a box's interior.
pub(super) fn center(bbox: Bounds) -> Coord {
    Coord::new(
        bbox.origin.x + (bbox.size.width / 2) as i32,
        bbox.origin.y + (bbox.size.height / 2) as i32,
        bbox.origin.z,
    )
}

/// Whether two boxes, each grown by `gap` cells, overlap.
pub(super) fn too_close(a: Bounds, b: Bounds, gap: u32) -> bool {
    let g = gap as i64;
    let span = |o: i32, n: u32| (i64::from(o) - g, i64::from(o) + i64::from(n) + g);
    let (ax0, ax1) = span(a.origin.x, a.size.width);
    let (ay0, ay1) = span(a.origin.y, a.size.height);
    let (bx0, bx1) = (
        i64::from(b.origin.x),
        i64::from(b.origin.x) + i64::from(b.size.width),
    );
    let (by0, by1) = (
        i64::from(b.origin.y),
        i64::from(b.origin.y) + i64::from(b.size.height),
    );
    ax0 < bx1 && bx0 < ax1 && ay0 < by1 && by0 < ay1
}
