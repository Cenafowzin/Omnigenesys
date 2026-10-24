use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    access::WriteHandle,
    bounds::Bounds,
    coords::{Coord, GridSize},
    operator::{
        BuildContext, BuildError, Context, Dimensions, OpError, Operator, OperatorType, Scope,
    },
    rng::pick,
    tile::TileId,
};

use super::common::{draw_room, too_close};

/// Rooms of random sizes at random places, each kept only if it stays
/// `gap` cells clear of the rooms before it: the classic way (Rogue; the
/// "simple map" of Herbert Wolverson's Rust roguelike tutorial tries 30
/// rooms of 6 to 10). Fewer rooms than tries come out when the map fills
/// up. Each room is a box with a wall ring and an object (`type: "room"`),
/// in the order placed; `ConnectRooms` joins them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlaceRoomsConfig {
    pub layer: String,
    pub rooms: String,
    pub floor: String,
    pub wall: String,
    /// Rooms tried; each one that fits is kept.
    #[serde(default = "default_tries")]
    #[schemars(range(min = 1, max = 500))]
    pub tries: u32,
    /// Inside size, walls not counted.
    #[serde(default = "default_min_size")]
    #[schemars(range(min = 1))]
    pub min_size: u32,
    #[serde(default = "default_max_size")]
    #[schemars(range(min = 1))]
    pub max_size: u32,
    /// Cells kept clear between two rooms' walls.
    #[serde(default = "default_gap")]
    pub gap: u32,
}

fn default_tries() -> u32 {
    30
}
fn default_min_size() -> u32 {
    4
}
fn default_max_size() -> u32 {
    9
}
fn default_gap() -> u32 {
    1
}

pub struct PlaceRooms {
    layer: WriteHandle,
    rooms: WriteHandle,
    floor: TileId,
    wall: TileId,
    tries: u32,
    sizes: (u32, u32),
    gap: u32,
}

impl OperatorType for PlaceRooms {
    const NAME: &'static str = "PlaceRooms";
    const DIMENSIONS: Dimensions = Dimensions::TwoD;
    const SCOPE: Scope = Scope::Global;
    type Config = PlaceRoomsConfig;

    fn build(c: PlaceRoomsConfig, bcx: &mut BuildContext<'_>) -> Result<PlaceRooms, BuildError> {
        let invalid = |field: &str, message: String| BuildError::Invalid {
            field: field.to_string(),
            message,
        };
        if c.tries == 0 || c.tries > 500 {
            return Err(invalid(
                "tries",
                format!("{} is not between 1 and 500", c.tries),
            ));
        }
        if c.min_size == 0 || c.min_size > c.max_size {
            return Err(invalid(
                "min_size",
                format!(
                    "sizes go from {} to {}: min must be 1 or more, and not above max",
                    c.min_size, c.max_size
                ),
            ));
        }
        Ok(PlaceRooms {
            layer: bcx.write(&c.layer)?,
            rooms: bcx.write_objects(&c.rooms)?,
            floor: bcx.tile(&c.floor)?,
            wall: bcx.tile(&c.wall)?,
            tries: c.tries,
            sizes: (c.min_size, c.max_size),
            gap: c.gap,
        })
    }
}

impl Operator for PlaceRooms {
    fn execute(&self, ctx: &mut Context<'_>) -> Result<(), OpError> {
        let parts = ctx.parts();
        let mut grid = parts.grid;
        let rng = parts.rng;
        let region = grid.region();
        let (lo, hi) = self.sizes;
        let mut placed: Vec<Bounds> = Vec::new();
        for _ in 0..self.tries {
            // Box = inside + wall ring.
            let w = lo + pick(rng.next_u32(), hi - lo + 1) + 2;
            let h = lo + pick(rng.next_u32(), hi - lo + 1) + 2;
            if w > region.size.width || h > region.size.height {
                continue;
            }
            let x = pick(rng.next_u32(), region.size.width - w + 1);
            let y = pick(rng.next_u32(), region.size.height - h + 1);
            let bbox = Bounds::new(
                region.origin + Coord::new(x as i32, y as i32, 0),
                GridSize::new(w, h, 1),
            );
            if placed.iter().all(|p| !too_close(bbox, *p, self.gap)) {
                placed.push(bbox);
            }
        }

        let mut split = grid.split(&[], &[self.layer, self.rooms])?;
        let [tiles, rooms] = split.writes.as_mut_slice() else {
            unreachable!("two layers asked for");
        };
        let tiles = tiles.as_tiles_mut().expect("checked when built");
        let rooms = rooms.as_objects_mut().expect("checked when built");
        for bbox in placed {
            draw_room(tiles, bbox, self.wall, self.floor);
            rooms.push(&mut *split.ids, "room", bbox, Vec::new());
        }
        Ok(())
    }
}
