use crate::coords::{Coord, GridSize};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Hash)]
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
        let lx = c.x as i64 - self.origin.x as i64;
        let ly = c.y as i64 - self.origin.y as i64;
        let lz = c.z as i64 - self.origin.z as i64;
        0 <= lx
            && lx < self.size.width as i64
            && 0 <= ly
            && ly < self.size.height as i64
            && 0 <= lz
            && lz < self.size.depth as i64
    }

    #[must_use = "expand returns a new Bounds; it does not modify self"]
    pub fn expand(self, x: u32, y: u32, z: u32) -> Bounds {
        debug_assert!(x <= i32::MAX as u32, "padding must fit in i32");
        debug_assert!(y <= i32::MAX as u32, "padding must fit in i32");
        debug_assert!(z <= i32::MAX as u32, "padding must fit in i32");
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
