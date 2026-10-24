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
        let too_large = |e: crate::layer::TooLarge| GridError::LayerTooLarge {
            name: name.clone(),
            size: e.size,
        };
        let layer = match kind {
            LayerKind::Tiles => {
                AnyLayer::Tiles(Layer::new(self.bounds, TileId::EMPTY).map_err(too_large)?)
            }
            LayerKind::Scalar => AnyLayer::Scalar(Layer::new(self.bounds, 0.0).map_err(too_large)?),
        };
        let (index, _) = self.layers.insert_full(name, layer);
        Ok(LayerId(index))
    }

    pub fn layer_name(&self, id: LayerId) -> Option<&str> {
        self.layers.get_index(id.0).map(|(name, _)| name.as_str())
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.layers.keys().map(|s| s.as_str())
    }

    /// The kind of layer `id`.
    pub fn kind(&self, id: LayerId) -> Result<LayerKind, GridError> {
        self.layers
            .get_index(id.0)
            .map(|(_, l)| l.kind())
            .ok_or(GridError::UnknownLayerId { id })
    }

    pub fn tiles(&self, id: LayerId) -> Result<&Layer<TileId>, GridError> {
        let (name, layer) = self.entry(id)?;
        layer
            .as_tiles()
            .ok_or_else(|| wrong(name, LayerKind::Tiles, layer.kind()))
    }

    pub fn tiles_mut(&mut self, id: LayerId) -> Result<&mut Layer<TileId>, GridError> {
        let (name, layer) = self.entry_mut(id)?;
        let found = layer.kind();
        layer
            .as_tiles_mut()
            .ok_or_else(|| wrong(name, LayerKind::Tiles, found))
    }

    pub fn scalars(&self, id: LayerId) -> Result<&Layer<f32>, GridError> {
        let (name, layer) = self.entry(id)?;
        layer
            .as_scalar()
            .ok_or_else(|| wrong(name, LayerKind::Scalar, layer.kind()))
    }

    pub fn scalars_mut(&mut self, id: LayerId) -> Result<&mut Layer<f32>, GridError> {
        let (name, layer) = self.entry_mut(id)?;
        let found = layer.kind();
        layer
            .as_scalar_mut()
            .ok_or_else(|| wrong(name, LayerKind::Scalar, found))
    }

    /// A scalar layer to read and a tile layer to write, borrowed together.
    pub fn scalars_and_tiles_mut(
        &mut self,
        read: LayerId,
        write: LayerId,
    ) -> Result<(&Layer<f32>, &mut Layer<TileId>), GridError> {
        let [(rn, r), (wn, w)] = self
            .layers
            .get_disjoint_indices_mut([read.0, write.0])
            .map_err(|_| GridError::UnknownLayerId { id: write })?;
        let (rk, wk) = (r.kind(), w.kind());
        let r = r
            .as_scalar()
            .ok_or_else(|| wrong(rn, LayerKind::Scalar, rk))?;
        let w = w
            .as_tiles_mut()
            .ok_or_else(|| wrong(wn, LayerKind::Tiles, wk))?;
        Ok((r, w))
    }

    fn entry(&self, id: LayerId) -> Result<(&String, &AnyLayer), GridError> {
        self.layers
            .get_index(id.0)
            .ok_or(GridError::UnknownLayerId { id })
    }

    fn entry_mut(&mut self, id: LayerId) -> Result<(&String, &mut AnyLayer), GridError> {
        self.layers
            .get_index_mut(id.0)
            .ok_or(GridError::UnknownLayerId { id })
    }
}

fn wrong(name: &str, expected: LayerKind, found: LayerKind) -> GridError {
    GridError::WrongLayerKind {
        name: name.to_string(),
        expected,
        found,
    }
}

#[cfg(test)]
mod tests;
