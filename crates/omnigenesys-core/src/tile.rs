use indexmap::IndexSet;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileId(pub(crate) u16);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileRegistry {
    names: IndexSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TileError {
    #[error("'{name}' is reserved and cannot be declared")]
    ReservedName { name: String },
    #[error("tile '{name}' is a duplicate")]
    DuplicateTile { name: String },
    #[error("tile '{name}' is unknown")]
    UnknownTile { name: String },
    #[error("too many tiles registered: {count}")]
    TooManyTiles { count: usize },
}

impl TileId {
    pub const EMPTY: TileId = TileId(0);
}

/// Set of tile ids with constant-time lookup: one bit per id, up to the
/// largest one in the set (at most 8 KiB).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TileSet {
    words: Box<[u64]>,
}

impl TileSet {
    pub(crate) fn new(ids: &[TileId]) -> TileSet {
        let bits = ids.iter().max().map_or(0, |t| t.0 as usize + 1);
        let mut words = vec![0u64; bits.div_ceil(64)].into_boxed_slice();
        for t in ids {
            words[t.0 as usize >> 6] |= 1 << (t.0 & 63);
        }
        TileSet { words }
    }

    #[inline]
    pub(crate) fn contains(&self, t: TileId) -> bool {
        let i = t.0 as usize;
        self.words
            .get(i >> 6)
            .is_some_and(|w| (w >> (i & 63)) & 1 == 1)
    }
}

impl TileRegistry {
    pub fn new<I, S>(declared: I) -> Result<TileRegistry, TileError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut names = IndexSet::new();
        names.insert("empty".to_string());

        for name in declared {
            let name = name.into();
            if name == "empty" {
                return Err(TileError::ReservedName { name });
            }
            if !names.insert(name.clone()) {
                return Err(TileError::DuplicateTile { name });
            }
        }
        // Positions 0..=u16::MAX fit in a TileId.
        if names.len() > u16::MAX as usize + 1 {
            return Err(TileError::TooManyTiles {
                count: names.len() - 1,
            });
        }
        Ok(TileRegistry { names })
    }

    pub fn id(&self, name: &str) -> Result<TileId, TileError> {
        self.names
            .get_index_of(name)
            .map(|index| TileId(index as u16))
            .ok_or_else(|| TileError::UnknownTile {
                name: name.to_string(),
            })
    }

    pub fn name(&self, id: TileId) -> Option<&str> {
        self.names.get_index(id.0 as usize).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Always false: "empty" is always registered.
    pub fn is_empty(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests;
