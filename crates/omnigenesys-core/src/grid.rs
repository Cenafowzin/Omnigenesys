use crate::bounds::Bounds;
use crate::coords::GridSize;
use crate::layer::{AnyLayer, Layer, LayerKind};
use crate::object::{ObjectIds, ObjectLayer};
use crate::tile::TileId;
use indexmap::IndexMap;
use thiserror::Error;

#[derive(Debug)]
pub struct Grid {
    /// The cells this grid holds: the whole map, or one chunk of it with
    /// its padding.
    bounds: Bounds,
    /// The whole map. The same as `bounds` until maps are made in chunks;
    /// operators that depend on the map's edges (a border, an island's
    /// falloff) use this, never `bounds`, so every chunk agrees.
    map: Bounds,
    layers: IndexMap<String, AnyLayer>,
    /// One counter for every object layer: an id names one object in the map.
    ids: ObjectIds,
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
    #[error("layer '{name}' asked for twice in one step")]
    SameLayerTwice { name: String },
}

impl Grid {
    /// A grid holding a whole map.
    pub fn new(bounds: Bounds) -> Grid {
        Grid::chunk(bounds, bounds)
    }

    /// A grid holding `bounds`, one part of the map `map`.
    pub fn chunk(bounds: Bounds, map: Bounds) -> Grid {
        Self {
            bounds,
            map,
            layers: IndexMap::new(),
            ids: ObjectIds::default(),
        }
    }

    /// The whole map this grid belongs to.
    pub fn map(&self) -> Bounds {
        self.map
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
            LayerKind::Objects => AnyLayer::Objects(ObjectLayer::new(self.bounds)),
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

    pub fn objects(&self, id: LayerId) -> Result<&ObjectLayer, GridError> {
        let (name, layer) = self.entry(id)?;
        layer
            .as_objects()
            .ok_or_else(|| wrong(name, LayerKind::Objects, layer.kind()))
    }

    /// Several layers at once, each by id: `reads` shared, `writes`
    /// exclusive, plus the map's object id counter. For a step that reads
    /// one layer and writes two (rooms in, corridors and doors out). The
    /// same layer twice is an error.
    pub fn split(&mut self, reads: &[LayerId], writes: &[LayerId]) -> Result<Split<'_>, GridError> {
        let wanted: Vec<LayerId> = reads.iter().chain(writes).copied().collect();
        for (i, id) in wanted.iter().enumerate() {
            if id.0 >= self.layers.len() {
                return Err(GridError::UnknownLayerId { id: *id });
            }
            if wanted[..i].contains(id) {
                return Err(GridError::SameLayerTwice {
                    name: self.layer_name(*id).unwrap_or("?").to_string(),
                });
            }
        }
        // Walking every layer once hands out disjoint borrows by
        // construction; then put them in the order asked.
        let mut got: Vec<Option<&mut AnyLayer>> = (0..wanted.len()).map(|_| None).collect();
        for (i, layer) in self.layers.values_mut().enumerate() {
            if let Some(k) = wanted.iter().position(|w| w.0 == i) {
                got[k] = Some(layer);
            }
        }
        let mut got = got.into_iter().map(|l| l.expect("every id was found"));
        let reads: Vec<&AnyLayer> = got.by_ref().take(reads.len()).map(|l| &*l).collect();
        let writes: Vec<&mut AnyLayer> = got.collect();
        Ok(Split {
            reads,
            writes,
            ids: &mut self.ids,
        })
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

/// Layers lent together by [`Grid::split`], in the order asked.
pub struct Split<'g> {
    pub reads: Vec<&'g AnyLayer>,
    pub writes: Vec<&'g mut AnyLayer>,
    pub ids: &'g mut ObjectIds,
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
