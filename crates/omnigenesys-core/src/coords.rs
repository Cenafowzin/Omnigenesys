use serde::{Deserialize, Serialize};
use std::ops::{Add, Sub};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Coord {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GridSize {
    pub width: u32,
    pub height: u32,
    #[serde(default = "default_depth")]
    pub depth: u32,
}

impl Add for Coord {
    type Output = Coord;

    #[inline]
    fn add(self, rhs: Coord) -> Coord {
        Coord {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
            z: self.z + rhs.z,
        }
    }
}

impl Sub for Coord {
    type Output = Coord;

    #[inline]
    fn sub(self, rhs: Coord) -> Coord {
        Coord {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
            z: self.z - rhs.z,
        }
    }
}

impl Coord {
    pub const ZERO: Coord = Coord { x: 0, y: 0, z: 0 };

    #[inline]
    pub const fn new(x: i32, y: i32, z: i32) -> Coord {
        Coord { x, y, z }
    }
}

fn default_depth() -> u32 {
    1
}

impl GridSize {
    #[inline]
    pub const fn new(width: u32, height: u32, depth: u32) -> GridSize {
        GridSize {
            width,
            height,
            depth,
        }
    }

    #[inline]
    pub fn checked_len(&self) -> Option<usize> {
        (self.width as usize)
            .checked_mul(self.height as usize)?
            .checked_mul(self.depth as usize)
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.checked_len().expect("grid size overflows usize")
    }

    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0 || self.depth == 0
    }

    #[inline]
    pub const fn is_2d(&self) -> bool {
        self.depth == 1
    }
}

#[cfg(test)]
mod tests;
