use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    access::{ReadHandle, WriteHandle},
    bounds::Bounds,
    coords::{Coord, GridSize},
    layer::Layer,
    object::{Object, Value},
    operator::{
        BuildContext, BuildError, Context, Dimensions, OpError, Operator, OperatorType, Scope,
    },
    rng::{Chance, StepRng, pick},
    tile::TileId,
};

use super::common::{center, on_ring};

/// Joins the rooms of an object layer, and writes each connection as an
/// object (`type: "connection"`, `from` and `to` the two rooms).
///
/// - `adjacent`: a door through the shared wall of every two boxes that
///   touch (Isaac's floors, `RoomGrid`).
/// - `chain`: each room to the one made before it (the tutorial way:
///   connected, long, no loops).
/// - `mst`: the shortest tree over the rooms' centres (Prim; TinyKeep and
///   Adonaac's dungeons), plus, with `loops`, some of the short links the
///   tree left out: each room's three nearest, each taken with that chance.
///
/// Corridors are carved from centre to centre, in an L (which leg first is
/// a coin flip) or a Z (Rogue's: the turn at a random spot).
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConnectRoomsConfig {
    /// Object layer with the rooms.
    pub rooms: String,
    /// Object layer that gets one object per connection.
    pub connections: String,
    /// Tile layer to carve.
    pub layer: String,
    /// Corridor floor.
    pub floor: String,
    /// Tile where a corridor or door crosses a room's wall; `floor` if
    /// left out.
    #[serde(default)]
    pub door: Option<String>,
    /// Tile for a door into a room whose `role` is `secret`; `door` if left
    /// out.
    #[serde(default)]
    pub secret_door: Option<String>,
    pub strategy: Strategy,
    /// With `mst`: chance of each left-out short link (0 to 1).
    #[serde(default)]
    #[schemars(range(min = 0, max = 1))]
    pub loops: f64,
    #[serde(default)]
    pub corridor: CorridorShape,
    #[serde(default = "default_width")]
    #[schemars(range(min = 1, max = 5))]
    pub width: u32,
}

fn default_width() -> u32 {
    1
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Strategy {
    Adjacent,
    Chain,
    Mst,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CorridorShape {
    #[default]
    L,
    Z,
}

pub struct ConnectRooms {
    rooms: ReadHandle,
    connections: WriteHandle,
    layer: WriteHandle,
    floor: TileId,
    door: TileId,
    secret_door: TileId,
    strategy: Strategy,
    loops: Chance,
    corridor: CorridorShape,
    width: u32,
}

impl OperatorType for ConnectRooms {
    const NAME: &'static str = "ConnectRooms";
    const DIMENSIONS: Dimensions = Dimensions::TwoD;
    const SCOPE: Scope = Scope::Global;
    type Config = ConnectRoomsConfig;

    fn build(
        c: ConnectRoomsConfig,
        bcx: &mut BuildContext<'_>,
    ) -> Result<ConnectRooms, BuildError> {
        let invalid = |field: &str, message: String| BuildError::Invalid {
            field: field.to_string(),
            message,
        };
        let loops = Chance::new(c.loops)
            .ok_or_else(|| invalid("loops", format!("{} is not between 0 and 1", c.loops)))?;
        if !(1..=5).contains(&c.width) {
            return Err(invalid(
                "width",
                format!("{} is not between 1 and 5", c.width),
            ));
        }
        let floor = bcx.tile(&c.floor)?;
        let door = match &c.door {
            Some(t) => bcx.tile(t)?,
            None => floor,
        };
        let secret_door = match &c.secret_door {
            Some(t) => bcx.tile(t)?,
            None => door,
        };
        Ok(ConnectRooms {
            rooms: bcx.read_objects(&c.rooms)?,
            connections: bcx.write_objects(&c.connections)?,
            layer: bcx.write(&c.layer)?,
            floor,
            door,
            secret_door,
            strategy: c.strategy,
            loops,
            corridor: c.corridor,
            width: c.width,
        })
    }
}

/// A link to make: two rooms, by their place in the layer.
type Link = (usize, usize);

impl ConnectRooms {
    fn links(&self, rooms: &[Object], rng: &mut StepRng) -> Vec<Link> {
        let n = rooms.len();
        match self.strategy {
            Strategy::Chain => (1..n).map(|i| (i - 1, i)).collect(),
            Strategy::Adjacent => {
                let mut out = Vec::new();
                for i in 0..n {
                    for j in i + 1..n {
                        if shared_wall(rooms[i].bbox, rooms[j].bbox).is_some() {
                            out.push((i, j));
                        }
                    }
                }
                out
            }
            Strategy::Mst => {
                let dist = |a: usize, b: usize| {
                    let (p, q) = (center(rooms[a].bbox), center(rooms[b].bbox));
                    let (dx, dy) = (i64::from(p.x - q.x), i64::from(p.y - q.y));
                    dx * dx + dy * dy
                };
                // Prim, from the first room; ties go to the lower indices.
                let mut tree = Vec::new();
                let mut inside = vec![false; n];
                if n > 0 {
                    inside[0] = true;
                }
                for _ in 1..n {
                    let mut best: Option<(i64, usize, usize)> = None;
                    for a in (0..n).filter(|&a| inside[a]) {
                        for b in (0..n).filter(|&b| !inside[b]) {
                            let d = (dist(a, b), a.min(b), a.max(b));
                            if best.is_none_or(|x| d < x) {
                                best = Some(d);
                            }
                        }
                    }
                    let (_, a, b) = best.expect("a room is left outside");
                    let newcomer = if inside[a] { b } else { a };
                    inside[newcomer] = true;
                    tree.push((a, b));
                }
                // Loops: each room's three nearest, those the tree left out,
                // shortest first, each kept with the `loops` chance.
                let mut extra: Vec<(i64, usize, usize)> = Vec::new();
                for a in 0..n {
                    let mut near: Vec<(i64, usize)> = (0..n)
                        .filter(|&b| b != a)
                        .map(|b| (dist(a, b), b))
                        .collect();
                    near.sort_unstable();
                    for &(d, b) in near.iter().take(3) {
                        let e = (d, a.min(b), a.max(b));
                        if !tree.contains(&(e.1, e.2)) && !extra.contains(&e) {
                            extra.push(e);
                        }
                    }
                }
                extra.sort_unstable();
                for (_, a, b) in extra {
                    if self.loops.hit(rng.next_u32()) {
                        tree.push((a, b));
                    }
                }
                tree
            }
        }
    }

    /// The cells of a corridor from `a`'s centre to `b`'s.
    fn path(&self, a: Coord, b: Coord, rng: &mut StepRng) -> Vec<Coord> {
        let mut cells = Vec::new();
        let mut line = |from: Coord, to: Coord| {
            let (dx, dy) = ((to.x - from.x).signum(), (to.y - from.y).signum());
            let mut c = from;
            cells.push(c);
            while c != to {
                c = Coord::new(c.x + dx, c.y + dy, c.z);
                cells.push(c);
            }
        };
        match self.corridor {
            CorridorShape::L => {
                let corner = if rng.next_u32() & 1 == 0 {
                    Coord::new(b.x, a.y, a.z)
                } else {
                    Coord::new(a.x, b.y, a.z)
                };
                line(a, corner);
                line(corner, b);
            }
            CorridorShape::Z => {
                if (b.x - a.x).abs() >= (b.y - a.y).abs() {
                    let span = (b.x - a.x).unsigned_abs();
                    let turn =
                        a.x + (b.x - a.x).signum() * pick(rng.next_u32(), span.max(1)) as i32;
                    line(a, Coord::new(turn, a.y, a.z));
                    line(Coord::new(turn, a.y, a.z), Coord::new(turn, b.y, a.z));
                    line(Coord::new(turn, b.y, a.z), b);
                } else {
                    let span = (b.y - a.y).unsigned_abs();
                    let turn =
                        a.y + (b.y - a.y).signum() * pick(rng.next_u32(), span.max(1)) as i32;
                    line(a, Coord::new(a.x, turn, a.z));
                    line(Coord::new(a.x, turn, a.z), Coord::new(b.x, turn, a.z));
                    line(Coord::new(b.x, turn, a.z), b);
                }
            }
        }
        cells
    }

    /// Carves `cells` (a `width` brush around each): a room's wall becomes
    /// a door, a room's inside stays as it is, anything else becomes
    /// corridor floor.
    fn carve(
        &self,
        tiles: &mut Layer<TileId>,
        rooms: &[Object],
        cells: &[Coord],
        door: TileId,
    ) -> Vec<Coord> {
        let w = self.width as i32;
        let lo = -(w - 1) / 2;
        let mut out = Vec::new();
        for &c in cells {
            for dy in lo..lo + w {
                for dx in lo..lo + w {
                    let p = Coord::new(c.x + dx, c.y + dy, c.z);
                    if !tiles.bounds().contains(p) {
                        continue;
                    }
                    let tile = if rooms.iter().any(|r| on_ring(r.bbox, p)) {
                        door
                    } else if rooms.iter().any(|r| r.bbox.contains(p)) {
                        continue;
                    } else {
                        self.floor
                    };
                    let _ = tiles.set(p, tile);
                    out.push(p);
                }
            }
        }
        out
    }
}

/// The door cells between two boxes that touch side by side, with at least
/// three cells of wall in common (corners are not doors): the middle of the
/// shared stretch, on both walls.
fn shared_wall(a: Bounds, b: Bounds) -> Option<[Coord; 2]> {
    let (ax1, ay1) = (
        a.origin.x + a.size.width as i32 - 1,
        a.origin.y + a.size.height as i32 - 1,
    );
    let (bx1, by1) = (
        b.origin.x + b.size.width as i32 - 1,
        b.origin.y + b.size.height as i32 - 1,
    );
    let z = a.origin.z;
    // Inside rows (or columns) both share, walls excluded.
    let overlap = |a0: i32, a1: i32, b0: i32, b1: i32| {
        let (lo, hi) = ((a0 + 1).max(b0 + 1), (a1 - 1).min(b1 - 1));
        (hi >= lo).then_some((lo + hi) / 2)
    };
    if ax1 + 1 == b.origin.x || bx1 + 1 == a.origin.x {
        let y = overlap(a.origin.y, ay1, b.origin.y, by1)?;
        let (l, r) = if ax1 + 1 == b.origin.x {
            (ax1, b.origin.x)
        } else {
            (bx1, a.origin.x)
        };
        return Some([Coord::new(l, y, z), Coord::new(r, y, z)]);
    }
    if ay1 + 1 == b.origin.y || by1 + 1 == a.origin.y {
        let x = overlap(a.origin.x, ax1, b.origin.x, bx1)?;
        let (t, d) = if ay1 + 1 == b.origin.y {
            (ay1, b.origin.y)
        } else {
            (by1, a.origin.y)
        };
        return Some([Coord::new(x, t, z), Coord::new(x, d, z)]);
    }
    None
}

fn bbox_of(cells: &[Coord]) -> Bounds {
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for c in cells {
        x0 = x0.min(c.x);
        y0 = y0.min(c.y);
        x1 = x1.max(c.x);
        y1 = y1.max(c.y);
    }
    let z = cells.first().map_or(0, |c| c.z);
    Bounds::new(
        Coord::new(x0, y0, z),
        GridSize::new((x1 - x0 + 1) as u32, (y1 - y0 + 1) as u32, 1),
    )
}

impl Operator for ConnectRooms {
    fn execute(&self, ctx: &mut Context<'_>) -> Result<(), OpError> {
        let parts = ctx.parts();
        let mut grid = parts.grid;
        let rng = parts.rng;
        let mut split = grid.split(&[self.rooms], &[self.layer, self.connections])?;
        let rooms: Vec<Object> = split.reads[0]
            .as_objects()
            .expect("checked when built")
            .iter()
            .cloned()
            .collect();
        let [tiles, conns] = split.writes.as_mut_slice() else {
            unreachable!("two layers asked for");
        };
        let tiles = tiles.as_tiles_mut().expect("checked when built");
        let conns = conns.as_objects_mut().expect("checked when built");

        for (a, b) in self.links(&rooms, rng) {
            let (ra, rb) = (&rooms[a], &rooms[b]);
            let secret = [ra, rb]
                .iter()
                .any(|r| r.prop("role").and_then(Value::as_str) == Some("secret"));
            let door = if secret { self.secret_door } else { self.door };
            let carved = match (self.strategy, shared_wall(ra.bbox, rb.bbox)) {
                (Strategy::Adjacent, Some(cells)) => {
                    // Widen along the wall, centred on the middle.
                    let w = self.width as i32;
                    let lo = -(w - 1) / 2;
                    let along_y = cells[0].x != cells[1].x;
                    let mut out = Vec::new();
                    for c in cells {
                        for k in lo..lo + w {
                            let p = if along_y {
                                Coord::new(c.x, c.y + k, c.z)
                            } else {
                                Coord::new(c.x + k, c.y, c.z)
                            };
                            // Never into a corner of either wall ring.
                            let corner = [ra.bbox, rb.bbox].iter().any(|bb| {
                                let (x0, y0) = (bb.origin.x, bb.origin.y);
                                let (x1, y1) = (
                                    x0 + bb.size.width as i32 - 1,
                                    y0 + bb.size.height as i32 - 1,
                                );
                                (p.x == x0 || p.x == x1) && (p.y == y0 || p.y == y1)
                            });
                            if !corner && tiles.set(p, door).is_ok() {
                                out.push(p);
                            }
                        }
                    }
                    out
                }
                _ => {
                    let cells = self.path(center(ra.bbox), center(rb.bbox), rng);
                    self.carve(tiles, &rooms, &cells, door)
                }
            };
            if carved.is_empty() {
                continue;
            }
            let mut props = vec![
                ("from".to_string(), Value::Object(ra.id)),
                ("to".to_string(), Value::Object(rb.id)),
            ];
            if secret {
                props.push(("secret".to_string(), Value::Bool(true)));
            }
            conns.push(&mut *split.ids, "connection", bbox_of(&carved), props);
        }
        Ok(())
    }
}
