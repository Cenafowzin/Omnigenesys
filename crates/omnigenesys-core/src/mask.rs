use std::ops::{BitAndAssign, BitOrAssign, Not};

use crate::{
    bounds::Bounds,
    coords::Coord,
    layer::Layer,
    tile::{TileId, TileSet},
};

/// One bit per cell of a region, in the same z → y → x order as a [`Layer`].
///
/// Bits past the last cell are always zero, so masks over the same bounds
/// compare and count correctly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellMask {
    bounds: Bounds,
    words: Box<[u64]>,
}

impl CellMask {
    /// A mask with every cell false.
    ///
    /// # Panics
    ///
    /// If `bounds` has more cells than `usize` can index.
    pub fn new(bounds: Bounds) -> CellMask {
        let words = vec![0; bounds.size.len().div_ceil(64)].into_boxed_slice();
        CellMask { bounds, words }
    }

    /// A mask with every cell true.
    ///
    /// # Panics
    ///
    /// If `bounds` has more cells than `usize` can index.
    pub fn full(bounds: Bounds) -> CellMask {
        !CellMask::new(bounds)
    }

    pub(crate) fn from_layer(layer: &Layer<TileId>, tiles: &TileSet) -> CellMask {
        let mut w = MaskWriter::new(layer.bounds());
        for cell in layer.cells() {
            w.push(tiles.contains(*cell));
        }
        w.finish()
    }

    pub fn bounds(&self) -> Bounds {
        self.bounds
    }

    /// False outside the bounds.
    #[inline]
    pub fn get(&self, c: Coord) -> bool {
        match self.bounds.index_of(c) {
            Some(i) => (self.words[i >> 6] >> (i & 63)) & 1 == 1,
            None => false,
        }
    }

    /// Number of true cells.
    pub fn count_ones(&self) -> usize {
        self.words.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// The true cells, in z → y → x order.
    pub fn iter_ones(&self) -> impl Iterator<Item = Coord> + '_ {
        let Bounds { origin, size } = self.bounds;
        let (w, h) = (size.width as usize, size.height as usize);
        self.words
            .iter()
            .enumerate()
            .flat_map(|(k, &word)| {
                let mut word = word;
                std::iter::from_fn(move || {
                    if word == 0 {
                        return None;
                    }
                    let i = k * 64 + word.trailing_zeros() as usize;
                    word &= word - 1;
                    Some(i)
                })
            })
            .map(move |i| {
                Coord::new(
                    origin.x + (i % w) as i32,
                    origin.y + (i / w % h) as i32,
                    origin.z + (i / (w * h)) as i32,
                )
            })
    }

    fn clear_tail(&mut self) {
        let tail = self.bounds.size.len() % 64;
        if let (Some(last), true) = (self.words.last_mut(), tail != 0) {
            *last &= (1 << tail) - 1;
        }
    }
}

/// Cell-wise AND. Panics if the bounds differ.
impl BitAndAssign<&CellMask> for CellMask {
    fn bitand_assign(&mut self, rhs: &CellMask) {
        assert_eq!(self.bounds, rhs.bounds, "masks over different bounds");
        for (a, b) in self.words.iter_mut().zip(&rhs.words) {
            *a &= b;
        }
    }
}

/// Cell-wise OR. Panics if the bounds differ.
impl BitOrAssign<&CellMask> for CellMask {
    fn bitor_assign(&mut self, rhs: &CellMask) {
        assert_eq!(self.bounds, rhs.bounds, "masks over different bounds");
        for (a, b) in self.words.iter_mut().zip(&rhs.words) {
            *a |= b;
        }
    }
}

impl Not for CellMask {
    type Output = CellMask;

    fn not(mut self) -> CellMask {
        for w in self.words.iter_mut() {
            *w = !*w;
        }
        self.clear_tail();
        self
    }
}

/// Builds a mask one cell at a time, in z → y → x order. Bits gather in a
/// word that is stored every 64 cells, so a push is a shift and an OR.
pub(crate) struct MaskWriter {
    bounds: Bounds,
    words: Vec<u64>,
    word: u64,
    filled: u32,
}

impl MaskWriter {
    pub(crate) fn new(bounds: Bounds) -> MaskWriter {
        MaskWriter {
            bounds,
            words: Vec::with_capacity(bounds.size.len().div_ceil(64)),
            word: 0,
            filled: 0,
        }
    }

    #[inline]
    pub(crate) fn push(&mut self, bit: bool) {
        self.word |= u64::from(bit) << self.filled;
        self.filled += 1;
        if self.filled == 64 {
            self.words.push(self.word);
            self.word = 0;
            self.filled = 0;
        }
    }

    pub(crate) fn finish(mut self) -> CellMask {
        assert_eq!(
            self.words.len() * 64 + self.filled as usize,
            self.bounds.size.len(),
            "pushed a different number of cells than the bounds hold"
        );
        if self.filled > 0 {
            self.words.push(self.word);
        }
        CellMask {
            bounds: self.bounds,
            words: self.words.into_boxed_slice(),
        }
    }
}

#[cfg(test)]
mod tests;
