use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Coord {
    pub x: i32,
    pub y: i32,
    /// Absent in 2D pipelines — defaults to 0.
    #[serde(default)]
    pub z: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GridSize {
    pub width: u32,
    pub height: u32,
    /// Absent means 1 (a 2D map) — PLANNING §2.1.
    #[serde(default = "default_depth")]
    pub depth: u32,
}

fn default_depth() -> u32 {
    1
}

impl Coord {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    pub fn is_2d(&self) -> bool {
        self.z == 0
    }
}

impl GridSize {
    pub fn is_2d(&self) -> bool {
        self.depth == 1
    }

    pub fn volume(&self) -> usize {
        self.width as usize * self.height as usize * self.depth as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_defaults_to_one() {
        let size: GridSize = serde_json::from_str(r#"{ "width": 80, "height": 50 }"#).unwrap();
        assert_eq!(size.depth, 1);
        assert!(size.is_2d());
    }

    #[test]
    fn unknown_field_is_an_error() {
        let result: Result<GridSize, _> =
            serde_json::from_str(r#"{ "width": 80, "height": 50, "widht": 3 }"#);
        assert!(result.is_err());
    }

    #[test]
    fn volume_does_not_overflow_u32_math() {
        let size = GridSize { width: 100_000, height: 100_000, depth: 1 };
        assert_eq!(size.volume(), 10_000_000_000);
    }
}
