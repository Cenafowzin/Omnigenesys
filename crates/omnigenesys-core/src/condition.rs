use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    grid::{Grid, GridError, LayerId},
    tile::{TileError, TileId, TileRegistry},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledCondition(Compiled);

#[derive(Debug, Clone, PartialEq, Eq)]
enum Compiled {
    TileIs {
        layer: LayerId,
        tiles: Vec<TileId>,
    },
    NearType {
        layer: LayerId,
        tiles: Vec<TileId>,
        radius: Radius,
    },
    NotNearType {
        layer: LayerId,
        tiles: Vec<TileId>,
        radius: Radius,
    },
    AreaIs {
        layer: LayerId,
        tiles: Vec<TileId>,
        radius: Radius,
    },
    Count {
        layer: LayerId,
        tiles: Vec<TileId>,
        radius: Radius,
        min: u32,
        max: u32,
    },
    Any(Vec<Compiled>),
    All(Vec<Compiled>),
    Not(Box<Compiled>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Radius {
    x: u32,
    y: u32,
    z: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged, expecting = "a tile name or a list of tile names")]
pub enum TileSpec {
    One(String),
    Many(Vec<String>),
}

impl TileSpec {
    pub fn names(&self) -> &[String] {
        match self {
            TileSpec::One(name) => std::slice::from_ref(name),
            TileSpec::Many(names) => names,
        }
    }
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

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ConditionError {
    #[error(transparent)]
    Grid(#[from] GridError),
    #[error(transparent)]
    Tile(#[from] TileError),
    #[error("count needs at least one of `min` or `max`")]
    CountWithNoBounds,
    #[error("count range is empty: min {min} > max {max}")]
    EmptyCountRange { min: u32, max: u32 },
}

impl Condition {
    pub fn compile(
        &self,
        grid: &Grid,
        tiles: &TileRegistry,
    ) -> Result<CompiledCondition, ConditionError> {
        compile(self, grid, tiles).map(CompiledCondition)
    }
}

fn compile(c: &Condition, grid: &Grid, tiles: &TileRegistry) -> Result<Compiled, ConditionError> {
    Ok(match c {
        Condition::TileIs { layer, tile } => Compiled::TileIs {
            layer: grid.layer_id(layer)?,
            tiles: resolve_tiles(tile, tiles)?,
        },
        Condition::NearType {
            layer,
            tile,
            distance,
        } => Compiled::NearType {
            layer: grid.layer_id(layer)?,
            tiles: resolve_tiles(tile, tiles)?,
            radius: radius(*distance, grid),
        },
        Condition::NotNearType {
            layer,
            tile,
            distance,
        } => Compiled::NotNearType {
            layer: grid.layer_id(layer)?,
            tiles: resolve_tiles(tile, tiles)?,
            radius: radius(*distance, grid),
        },
        Condition::AreaIs {
            layer,
            tile,
            distance,
        } => Compiled::AreaIs {
            layer: grid.layer_id(layer)?,
            tiles: resolve_tiles(tile, tiles)?,
            radius: radius(*distance, grid),
        },
        Condition::Count {
            layer,
            tile,
            distance,
            min,
            max,
        } => {
            let (min, max) = match (*min, *max) {
                (None, None) => return Err(ConditionError::CountWithNoBounds),
                (Some(min), Some(max)) if min > max => {
                    return Err(ConditionError::EmptyCountRange { min, max });
                }
                (min, max) => (min.unwrap_or(0), max.unwrap_or(u32::MAX)),
            };
            Compiled::Count {
                layer: grid.layer_id(layer)?,
                tiles: resolve_tiles(tile, tiles)?,
                radius: radius(*distance, grid),
                min,
                max,
            }
        }
        Condition::Any { conditions } => Compiled::Any(compile_all(conditions, grid, tiles)?),
        Condition::All { conditions } => Compiled::All(compile_all(conditions, grid, tiles)?),
        Condition::Not { condition } => Compiled::Not(Box::new(compile(condition, grid, tiles)?)),
    })
}

fn compile_all(
    conditions: &[Condition],
    grid: &Grid,
    tiles: &TileRegistry,
) -> Result<Vec<Compiled>, ConditionError> {
    conditions.iter().map(|c| compile(c, grid, tiles)).collect()
}

fn resolve_tiles(spec: &TileSpec, tiles: &TileRegistry) -> Result<Vec<TileId>, ConditionError> {
    let mut ids = spec
        .names()
        .iter()
        .map(|name| tiles.id(name))
        .collect::<Result<Vec<_>, _>>()?;
    ids.sort_unstable();
    ids.dedup();
    Ok(ids)
}

fn radius(distance: u32, grid: &Grid) -> Radius {
    let z = if grid.bounds().size.is_2d() {
        0
    } else {
        distance
    };
    Radius {
        x: distance,
        y: distance,
        z,
    }
}

#[cfg(test)]
mod tests;
