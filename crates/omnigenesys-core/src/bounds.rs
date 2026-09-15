use crate::coords::{Coord, GridSize};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Bounds {
    pub origin: Coord,
    pub size: GridSize,
}

impl Bounds {
    pub const fn new(origin: Coord, size: GridSize) -> Bounds {
        Bounds { origin, size }
    }

    pub fn contains(&self, c: Coord) -> bool {
        let local = c - self.origin;
        0 <= local.x
            && local.x < self.size.width as i32
            && 0 <= local.y
            && local.y < self.size.height as i32
            && 0 <= local.z
            && local.z < self.size.depth as i32
    }

    pub fn expand(self, x: u32, y: u32, z: u32) -> Bounds {
        Bounds {
            origin: Coord {
                x: self.origin.x - x as i32,
                y: self.origin.y - y as i32,
                z: self.origin.z - z as i32,
            },
            size: GridSize {
                width: self.size.width + (x * 2),
                height: self.size.height + (y * 2),
                depth: self.size.depth + (z * 2),
            },
        }
    }

    pub fn iter(self) -> impl Iterator<Item = Coord> {
        let Bounds { origin, size } = self;
        (0..size.depth).flat_map(move |z| {
            (0..size.height).flat_map(move |y| {
                (0..size.width).map(move |x| {
                    Coord::new(
                        origin.x + x as i32,
                        origin.y + y as i32,
                        origin.z + z as i32,
                    )
                })
            })
        })
    }
}

#[cfg(test)]
mod tests;
