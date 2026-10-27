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

    /// True where `min <= value < max`; NaN is never in range.
    pub(crate) fn from_scalars(layer: &Layer<f32>, min: f32, max: f32) -> CellMask {
        let mut w = MaskWriter::new(layer.bounds());
        for &v in layer.cells() {
            w.push(v >= min && v < max);
        }
        w.finish()
    }

    /// True over `region` where the distance to the nearest edge of `map`
    /// is in `min..=max`; z counts only when the map has depth.
    pub(crate) fn from_edge_distance(region: Bounds, map: Bounds, min: u32, max: u32) -> CellMask {
        let lo = |o: i32| i64::from(o);
        let hi = |o: i32, n: u32| i64::from(o) + i64::from(n) - 1;
        let (x0, x1) = (lo(map.origin.x), hi(map.origin.x, map.size.width));
        let (y0, y1) = (lo(map.origin.y), hi(map.origin.y, map.size.height));
        let (z0, z1) = (lo(map.origin.z), hi(map.origin.z, map.size.depth));
        let flat = map.size.depth <= 1;
        let (min, max) = (i64::from(min), i64::from(max));
        let (rx0, width) = (lo(region.origin.x), region.size.width as usize);
        let rx1 = rx0 + width as i64 - 1;
        let mut mask = CellMask::new(region);
        // A row at distance `dr` from the edges in y (and z) holds where
        // `min <= min(dx, dr) <= max`, `dx` being the distance in x: no
        // cell if `dr < min`; else the cells with `dx >= min`, all of them
        // if `dr <= max`, and otherwise only those with `dx <= max` too, at
        // both ends. At most two runs a row, written a word at a time.
        let mut row = 0;
        for k in 0..region.size.depth {
            let z = lo(region.origin.z) + i64::from(k);
            for j in 0..region.size.height {
                let y = lo(region.origin.y) + i64::from(j);
                let mut dr = (y - y0).min(y1 - y);
                if !flat {
                    dr = dr.min(z - z0).min(z1 - z);
                }
                if dr >= min {
                    let runs = if dr <= max {
                        [(x0 + min, x1 - min), (1, 0)]
                    } else {
                        [(x0 + min, x0 + max), (x1 - max, x1 - min)]
                    };
                    for (a, b) in runs {
                        let (a, b) = (a.max(rx0), b.min(rx1));
                        if a <= b {
                            let start = row * width + (a - rx0) as usize;
                            mask.set_run(start, (b - a + 1) as usize);
                        }
                    }
                }
                row += 1;
            }
        }
        mask
    }

    /// Sets the `len` bits from bit `start`.
    fn set_run(&mut self, start: usize, len: usize) {
        let end = start + len;
        let mut i = start;
        while i < end {
            let (word, bit) = (i >> 6, i & 63);
            let n = (64 - bit).min(end - i);
            let ones = if n == 64 {
                u64::MAX
            } else {
                ((1u64 << n) - 1) << bit
            };
            self.words[word] |= ones;
            i += n;
        }
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
    pub fn iter_ones(&self) -> IterOnes<'_> {
        IterOnes {
            words: &self.words,
            next_word: 0,
            word: 0,
            index: 0,
            local: [0; 3],
            bounds: self.bounds,
        }
    }

    fn clear_tail(&mut self) {
        let tail = self.bounds.size.len() % 64;
        if let (Some(last), true) = (self.words.last_mut(), tail != 0) {
            *last &= (1 << tail) - 1;
        }
    }
}

/// Iterator over the true cells of a [`CellMask`], from [`CellMask::iter_ones`].
///
/// Set bits come in increasing index order, so the coordinate is advanced by
/// the distance from the previous one instead of being divided out of the
/// index: three divisions per cell made iteration the bottleneck of a scatter.
pub struct IterOnes<'a> {
    words: &'a [u64],
    next_word: usize,
    /// Bits of the current word not yielded yet.
    word: u64,
    /// Index of the last cell yielded (0 before the first).
    index: usize,
    /// Local (x, y, z) of `index`.
    local: [u32; 3],
    bounds: Bounds,
}

impl Iterator for IterOnes<'_> {
    type Item = Coord;

    #[inline]
    fn next(&mut self) -> Option<Coord> {
        while self.word == 0 {
            self.word = *self.words.get(self.next_word)?;
            self.next_word += 1;
        }
        let i = (self.next_word - 1) * 64 + self.word.trailing_zeros() as usize;
        self.word &= self.word - 1;

        // Walk from the previous cell: whole rows are skipped one at a time,
        // so over the whole iteration this costs at most one step per row.
        let size = self.bounds.size;
        let [x, y, z] = &mut self.local;
        let mut x_far = *x as usize + (i - self.index);
        self.index = i;
        while x_far >= size.width as usize {
            x_far -= size.width as usize;
            *y += 1;
            if *y == size.height {
                *y = 0;
                *z += 1;
            }
        }
        *x = x_far as u32;

        let o = self.bounds.origin;
        Some(Coord::new(
            o.x + *x as i32,
            o.y + *y as i32,
            o.z + *z as i32,
        ))
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
