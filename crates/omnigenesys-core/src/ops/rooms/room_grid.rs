use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    access::WriteHandle,
    bounds::Bounds,
    coords::{Coord, GridSize},
    object::Value,
    operator::{
        BuildContext, BuildError, Context, Dimensions, OpError, Operator, OperatorType, Scope,
    },
    rng::{StepRng, pick},
    tile::TileId,
};

use super::common::draw_room;

/// Rooms on a grid, laid out as The Binding of Isaac lays out a floor
/// (Florian Himsl's rules, as Boris the Brave describes them, 2020): from
/// the middle cell, each room may open a neighbour cell, half the time, if
/// that cell has no more than one occupied neighbour (no blobs), until the
/// count is reached. A layout with the wrong count or too few dead ends is
/// thrown away and made again — inside the step, from the step's own
/// random stream, so the result is the same on every run.
///
/// Each room is a box with its own wall ring; neighbouring boxes touch, so
/// `ConnectRooms` with `adjacent` opens doors between them. Every room
/// becomes an object (`type: "room"`, `cell` = its grid cell); the first is
/// the starting one. With `secret`, one more room goes where Isaac puts a
/// secret room: the empty cell with the most rooms around it, never next to
/// a dead end.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RoomGridConfig {
    /// Tile layer to draw the rooms on.
    pub layer: String,
    /// Object layer that gets one object per room.
    pub rooms: String,
    pub floor: String,
    pub wall: String,
    #[serde(default = "default_columns")]
    #[schemars(range(min = 1, max = 32))]
    pub columns: u32,
    #[serde(default = "default_rows")]
    #[schemars(range(min = 1, max = 32))]
    pub rows: u32,
    /// Inside of a room, without walls: Isaac's is 13 × 7.
    #[serde(default = "default_room_width")]
    #[schemars(range(min = 1))]
    pub room_width: u32,
    #[serde(default = "default_room_height")]
    #[schemars(range(min = 1))]
    pub room_height: u32,
    /// Rooms to lay out (the secret room not counted).
    #[serde(default = "default_count")]
    #[schemars(range(min = 1))]
    pub count: u32,
    /// Fewest dead ends (rooms with one door) a layout may have.
    #[serde(default = "default_dead_ends")]
    pub dead_ends: u32,
    #[serde(default)]
    pub secret: bool,
}

fn default_columns() -> u32 {
    9
}
fn default_rows() -> u32 {
    8
}
fn default_room_width() -> u32 {
    13
}
fn default_room_height() -> u32 {
    7
}
fn default_count() -> u32 {
    10
}
fn default_dead_ends() -> u32 {
    3
}

/// Layouts tried before giving up.
const ATTEMPTS: u32 = 500;

pub struct RoomGrid {
    layer: WriteHandle,
    rooms: WriteHandle,
    floor: TileId,
    wall: TileId,
    columns: u32,
    rows: u32,
    room: (u32, u32),
    count: u32,
    dead_ends: u32,
    secret: bool,
}

impl OperatorType for RoomGrid {
    const NAME: &'static str = "RoomGrid";
    const DIMENSIONS: Dimensions = Dimensions::TwoD;
    const SCOPE: Scope = Scope::Global;
    type Config = RoomGridConfig;

    fn build(c: RoomGridConfig, bcx: &mut BuildContext<'_>) -> Result<RoomGrid, BuildError> {
        let invalid = |field: &str, message: String| BuildError::Invalid {
            field: field.to_string(),
            message,
        };
        if c.columns == 0 || c.rows == 0 || c.columns > 32 || c.rows > 32 {
            return Err(invalid(
                "columns",
                "columns and rows go from 1 to 32".to_string(),
            ));
        }
        if c.room_width == 0 || c.room_height == 0 {
            return Err(invalid(
                "room_width",
                "a room needs at least 1 × 1 inside".to_string(),
            ));
        }
        if c.count == 0 || c.count > c.columns * c.rows {
            return Err(invalid(
                "count",
                format!(
                    "{} rooms do not fit a {} × {} grid",
                    c.count, c.columns, c.rows
                ),
            ));
        }
        let size = bcx.bounds().size;
        let (need_w, need_h) = (c.columns * (c.room_width + 2), c.rows * (c.room_height + 2));
        if need_w > size.width || need_h > size.height {
            return Err(invalid(
                "columns",
                format!(
                    "{} × {} rooms of {} × {} (walls included) need a map of {need_w} × {need_h}; this one is {} × {}",
                    c.columns,
                    c.rows,
                    c.room_width + 2,
                    c.room_height + 2,
                    size.width,
                    size.height
                ),
            ));
        }
        Ok(RoomGrid {
            layer: bcx.write(&c.layer)?,
            rooms: bcx.write_objects(&c.rooms)?,
            floor: bcx.tile(&c.floor)?,
            wall: bcx.tile(&c.wall)?,
            columns: c.columns,
            rows: c.rows,
            room: (c.room_width, c.room_height),
            count: c.count,
            dead_ends: c.dead_ends,
            secret: c.secret,
        })
    }
}

/// A floor plan: which grid cells hold a room, in the order they were
/// opened (the first is the start).
struct Plan {
    columns: u32,
    rows: u32,
    order: Vec<(u32, u32)>,
    taken: Vec<bool>,
}

impl Plan {
    fn new(columns: u32, rows: u32) -> Plan {
        Plan {
            columns,
            rows,
            order: Vec::new(),
            taken: vec![false; (columns * rows) as usize],
        }
    }

    fn is_taken(&self, (x, y): (u32, u32)) -> bool {
        self.taken[(y * self.columns + x) as usize]
    }

    fn take(&mut self, cell: (u32, u32)) {
        self.taken[(cell.1 * self.columns + cell.0) as usize] = true;
        self.order.push(cell);
    }

    /// The four neighbours inside the grid: left, right, up, down.
    fn neighbours(&self, (x, y): (u32, u32)) -> impl Iterator<Item = (u32, u32)> + use<> {
        let (c, r) = (self.columns, self.rows);
        [
            (x > 0).then(|| (x.wrapping_sub(1), y)),
            (x + 1 < c).then_some((x + 1, y)),
            (y > 0).then(|| (x, y.wrapping_sub(1))),
            (y + 1 < r).then_some((x, y + 1)),
        ]
        .into_iter()
        .flatten()
    }

    fn taken_around(&self, cell: (u32, u32)) -> usize {
        self.neighbours(cell).filter(|n| self.is_taken(*n)).count()
    }

    fn dead_ends(&self) -> usize {
        self.order[1..]
            .iter()
            .filter(|c| self.taken_around(**c) == 1)
            .count()
    }
}

impl RoomGrid {
    /// One try at the floor plan, Isaac's way.
    fn try_plan(&self, rng: &mut StepRng) -> Plan {
        let mut plan = Plan::new(self.columns, self.rows);
        plan.take((self.columns / 2, self.rows / 2));
        let mut queue = std::collections::VecDeque::from([(self.columns / 2, self.rows / 2)]);
        while let Some(cell) = queue.pop_front() {
            let around: Vec<(u32, u32)> = plan.neighbours(cell).collect();
            for n in around {
                if plan.order.len() as u32 >= self.count
                    || plan.is_taken(n)
                    || plan.taken_around(n) > 1
                    || rng.next_u32() & 1 == 0
                {
                    continue;
                }
                plan.take(n);
                queue.push_back(n);
            }
        }
        plan
    }

    /// Isaac's secret room: an empty cell next to the most rooms, never next
    /// to a dead end; ties broken by the stream.
    fn secret_cell(&self, plan: &Plan, rng: &mut StepRng) -> Option<(u32, u32)> {
        let dead_end =
            |c: (u32, u32)| plan.is_taken(c) && plan.order[0] != c && plan.taken_around(c) == 1;
        let mut best: Vec<(u32, u32)> = Vec::new();
        let mut most = 0;
        for y in 0..plan.rows {
            for x in 0..plan.columns {
                let c = (x, y);
                if plan.is_taken(c) || plan.neighbours(c).any(dead_end) {
                    continue;
                }
                let n = plan.taken_around(c);
                if n == 0 || n < most {
                    continue;
                }
                if n > most {
                    most = n;
                    best.clear();
                }
                best.push(c);
            }
        }
        (!best.is_empty()).then(|| best[pick(rng.next_u32(), best.len() as u32) as usize])
    }
}

impl Operator for RoomGrid {
    fn execute(&self, ctx: &mut Context<'_>) -> Result<(), OpError> {
        let parts = ctx.parts();
        let mut grid = parts.grid;
        let rng = parts.rng;
        let region = grid.region();

        let mut plan = None;
        for _ in 0..ATTEMPTS {
            let p = self.try_plan(rng);
            if p.order.len() as u32 == self.count && p.dead_ends() as u32 >= self.dead_ends {
                plan = Some(p);
                break;
            }
        }
        let Some(mut plan) = plan else {
            return Err(OpError::Failed {
                message: format!(
                    "no layout of {} rooms with {} dead ends on a {} × {} grid in {ATTEMPTS} tries: \
                     ask for fewer rooms or dead ends, or a bigger grid",
                    self.count, self.dead_ends, self.columns, self.rows
                ),
            });
        };
        let secret = if self.secret {
            self.secret_cell(&plan, rng)
        } else {
            None
        };
        if let Some(cell) = secret {
            plan.take(cell);
        }

        // The grid of boxes, centred on the map.
        let (bw, bh) = (self.room.0 + 2, self.room.1 + 2);
        let ox = region.origin.x + ((region.size.width - self.columns * bw) / 2) as i32;
        let oy = region.origin.y + ((region.size.height - self.rows * bh) / 2) as i32;

        let mut split = grid.split(&[], &[self.layer, self.rooms])?;
        let [tiles, rooms] = split.writes.as_mut_slice() else {
            unreachable!("two layers asked for");
        };
        let tiles = tiles.as_tiles_mut().expect("checked when built");
        let rooms = rooms.as_objects_mut().expect("checked when built");
        for &(cx, cy) in &plan.order {
            let bbox = Bounds::new(
                Coord::new(
                    ox + (cx * bw) as i32,
                    oy + (cy * bh) as i32,
                    region.origin.z,
                ),
                GridSize::new(bw, bh, 1),
            );
            draw_room(tiles, bbox, self.wall, self.floor);
            let mut props = vec![(
                "cell".to_string(),
                Value::Cell(Coord::new(cx as i32, cy as i32, 0)),
            )];
            if Some((cx, cy)) == secret {
                props.push(("role".to_string(), Value::Str("secret".to_string())));
            }
            rooms.push(&mut *split.ids, "room", bbox, props);
        }
        Ok(())
    }
}
