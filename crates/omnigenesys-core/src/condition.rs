use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged, expecting = "a tile name or a list of tile names")]
pub enum TileSpec {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Condition {
    TileIs {
        layer: String,
        tile: TileSpec,
    },
    NearType {
        layer: String,
        tile: TileSpec,
        distance: u32,
    },
    NotNearType {
        layer: String,
        tile: TileSpec,
        distance: u32,
    },
    AreaIs {
        layer: String,
        tile: TileSpec,
        distance: u32,
    },
    Count {
        layer: String,
        tile: TileSpec,
        distance: u32,
        min: Option<u32>,
        max: Option<u32>,
    },
    Any {
        conditions: Vec<Condition>,
    },
    All {
        conditions: Vec<Condition>,
    },
    Not {
        condition: Box<Condition>,
    },
}

#[cfg(test)]
mod tests;
