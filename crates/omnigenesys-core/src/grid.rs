use crate::bounds::Bounds;
use crate::coords::GridSize;
use crate::layer::{AnyLayer, Layer, LayerKind};
use crate::tile::TileId;
use indexmap::IndexMap;
use thiserror::Error;

#[derive(Debug)]
pub struct Grid {
    bounds: Bounds,
    layers: IndexMap<String, AnyLayer>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LayerId(usize);

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GridError {
    #[error("layer '{name}' not found")]
    LayerNotFound { name: String },
    #[error("layer '{name}' already exists")]
    DuplicateLayer { name: String },
    #[error("layer '{name}' is {found}, expected {expected}")]
    WrongLayerKind {
        name: String,
        expected: LayerKind,
        found: LayerKind,
    },
    #[error("layer '{name}' is too large: {size:?}")]
    LayerTooLarge { name: String, size: GridSize },
    #[error("unknown layer id {id:?}")]
    UnknownLayerId { id: LayerId },
}

impl Grid {
    pub fn new(bounds: Bounds) -> Grid {
        Self {
            bounds,
            layers: IndexMap::new(),
        }
    }

    pub fn bounds(&self) -> Bounds {
        self.bounds
    }

    pub fn layer_id(&self, name: &str) -> Result<LayerId, GridError> {
        self.layers
            .get_index_of(name)
            .map(LayerId)
            .ok_or_else(|| GridError::LayerNotFound {
                name: name.to_string(),
            })
    }

    pub fn add_layer(
        &mut self,
        name: impl Into<String>,
        kind: LayerKind,
    ) -> Result<LayerId, GridError> {
        let name = name.into();
        if self.layers.contains_key(&name) {
            return Err(GridError::DuplicateLayer { name });
        }
        let layer = match kind {
            LayerKind::Tiles => {
                let tiles = Layer::new(self.bounds, TileId::EMPTY).map_err(|e| {
                    GridError::LayerTooLarge {
                        name: name.clone(),
                        size: e.size,
                    }
                })?;
                AnyLayer::Tiles(tiles)
            }
        };
        let (index, _) = self.layers.insert_full(name, layer);
        Ok(LayerId(index))
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.layers.keys().map(|s| s.as_str())
    }

    pub fn tiles(&self, id: LayerId) -> Result<&Layer<TileId>, GridError> {
        let (name, layer) = self
            .layers
            .get_index(id.0)
            .ok_or(GridError::UnknownLayerId { id })?;
        layer.as_tiles().ok_or_else(|| GridError::WrongLayerKind {
            name: name.to_string(),
            expected: LayerKind::Tiles,
            found: layer.kind(),
        })
    }

    pub fn tiles_mut(&mut self, id: LayerId) -> Result<&mut Layer<TileId>, GridError> {
        let (name, layer) = self
            .layers
            .get_index_mut(id.0)
            .ok_or(GridError::UnknownLayerId { id })?;
        let found = layer.kind();
        layer
            .as_tiles_mut()
            .ok_or_else(|| GridError::WrongLayerKind {
                name: name.to_string(),
                expected: LayerKind::Tiles,
                found,
            })
    }
}
#[cfg(test)]
mod tests;
