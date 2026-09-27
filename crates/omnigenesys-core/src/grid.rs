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

    pub fn add_layer(&mut self, name: impl Into<String>, kind: LayerKind) -> Result<(), GridError> {
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
        self.layers.insert(name, layer);
        Ok(())
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.layers.keys().map(|s| s.as_str())
    }

    pub fn tiles(&self, name: &str) -> Result<&Layer<TileId>, GridError> {
        let layer = self
            .layers
            .get(name)
            .ok_or_else(|| GridError::LayerNotFound {
                name: name.to_string(),
            })?;
        layer.as_tiles().ok_or_else(|| GridError::WrongLayerKind {
            name: name.to_string(),
            expected: LayerKind::Tiles,
            found: layer.kind(),
        })
    }

    pub fn tiles_mut(&mut self, name: &str) -> Result<&mut Layer<TileId>, GridError> {
        let layer = self
            .layers
            .get_mut(name)
            .ok_or_else(|| GridError::LayerNotFound {
                name: name.to_string(),
            })?;
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
