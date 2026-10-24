use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    access::{Reach, WriteHandle},
    condition::CompiledCondition,
    operator::{
        BuildContext, BuildError, Context, Dimensions, OpError, Operator, OperatorType, Scope,
    },
    tile::TileId,
};

use super::bits::pack;

/// Runs a birth/survival cellular automaton over two tiles of a layer: the
/// cave generator (PLANNING §2.8).
///
/// A cell counts its 8 neighbours that are `alive` (itself not counted). A
/// dead cell becomes alive when the count is in `birth`; a live cell stays
/// alive when it is in `survive`, and dies otherwise. Every cell is updated
/// from the map as the previous pass left it, never from cells already
/// updated in the same pass. Cells of other tiles count as dead and never
/// change; with step conditions, only the cells where they hold change.
///
/// No randomness: the starting noise is a `Scatter` before it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CellularAutomatonConfig {
    pub layer: String,
    /// The tile of live cells (the walls of a cave).
    pub alive: String,
    /// The tile a live cell becomes when it dies (the floor of a cave).
    pub dead: String,
    /// Neighbour counts, 0 to 8, at which a dead cell becomes alive.
    #[schemars(inner(range(max = 8)))]
    pub birth: Vec<u8>,
    /// Neighbour counts, 0 to 8, at which a live cell stays alive.
    #[schemars(inner(range(max = 8)))]
    pub survive: Vec<u8>,
    /// Passes over the whole layer. Stops early once a pass changes nothing.
    #[schemars(range(min = 1))]
    pub iterations: u32,
    /// What the cells past the edge of the map count as.
    pub outside: Outside,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Outside {
    Alive,
    Dead,
}

pub struct CellularAutomaton {
    layer: WriteHandle,
    alive: TileId,
    dead: TileId,
    rule: Rule,
    iterations: u32,
    outside: Outside,
    when: Option<CompiledCondition>,
}

impl OperatorType for CellularAutomaton {
    const NAME: &'static str = "CellularAutomaton";
    const DIMENSIONS: Dimensions = Dimensions::TwoD;
    const SCOPE: Scope = Scope::Local;
    type Config = CellularAutomatonConfig;

    fn build(
        config: CellularAutomatonConfig,
        bcx: &mut BuildContext<'_>,
    ) -> Result<CellularAutomaton, BuildError> {
        let invalid = |field: &str, message: String| BuildError::Invalid {
            field: field.to_string(),
            message,
        };
        if config.iterations == 0 {
            return Err(invalid("iterations", "must be at least 1".to_string()));
        }
        let birth = counts(&config.birth).map_err(|m| invalid("birth", m))?;
        let survive = counts(&config.survive).map_err(|m| invalid("survive", m))?;
        let alive = bcx.tile(&config.alive)?;
        let dead = bcx.tile(&config.dead)?;
        if alive == dead {
            return Err(invalid(
                "dead",
                format!("is the same tile as `alive` ('{}')", config.dead),
            ));
        }
        // Each pass sees one cell further than the last.
        let reach = Reach::symmetric(config.iterations, config.iterations, 0);
        Ok(CellularAutomaton {
            layer: bcx.read_write(&config.layer, reach)?,
            alive,
            dead,
            rule: Rule::new(birth, survive),
            iterations: config.iterations,
            outside: config.outside,
            when: bcx.step_conditions(),
        })
    }
}

/// Bit n set when count n is in the list.
fn counts(list: &[u8]) -> Result<u16, String> {
    let mut set = 0;
    for &n in list {
        if n > 8 {
            return Err(format!("{n} is not a neighbour count (0 to 8)"));
        }
        set |= 1 << n;
    }
    Ok(set)
}

impl Operator for CellularAutomaton {
    fn execute(&self, ctx: &mut Context<'_>) -> Result<(), OpError> {
        let parts = ctx.parts();
        let mut grid = parts.grid;
        let region = grid.region();
        let mask = match &self.when {
            Some(c) => Some(grid.prepare(c)?.eval(region)),
            None => None,
        };
        let layer = grid.tiles_mut(self.layer)?;
        let (width, height) = (region.size.width as usize, region.size.height as usize);
        if width == 0 || height == 0 {
            return Ok(());
        }
        // `outside` frames the region. For a whole map that is the map's
        // edge. For a chunk, the region's edge lies in its padding, which
        // reaches `iterations` cells: no cell inside the chunk can see that
        // far in `iterations` passes, so the frame never reaches it.
        let fill = match self.outside {
            Outside::Alive => !0,
            Outside::Dead => 0,
        };

        // Everything is read before anything is written (the contract of
        // `read_write`).
        let (start, mut free) =
            Board::from_cells(width, height, fill, layer.as_slice(), self.alive, self.dead);
        if let Some(mask) = mask {
            let mut allowed = Board::new(width, height, 0);
            for c in mask.iter_ones() {
                let (x, y) = (c.x - region.origin.x, c.y - region.origin.y);
                allowed.set(x as usize, y as usize);
            }
            free.and(&allowed);
        }

        let mut current = start.clone();
        // Same frame of `fill` as `current`; a pass writes only the inside.
        let mut next = start.clone();
        let mut sums = RowSums::new(current.inner);
        for _ in 0..self.iterations {
            let changed = current.step(&mut next, &free, &self.rule, &mut sums);
            std::mem::swap(&mut current, &mut next);
            if !changed {
                break;
            }
        }
        current.write_changes(&start, layer.as_mut_slice(), self.alive, self.dead);
        Ok(())
    }
}

/// The rule as two count sets, in the form one pass needs: a cell flips
/// exactly where `(alive & keep) ^ born` is set, with `born` the birth set
/// and `keep` the counts where birth and survival agree (an alive cell
/// flips where it would not survive; a dead one where it would be born).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rule {
    born: Membership,
    keep: Membership,
}

impl Rule {
    fn new(birth: u16, survive: u16) -> Rule {
        Rule {
            born: Membership::new(birth),
            keep: Membership::new(!(birth ^ survive)),
        }
    }
}

/// Whether each of 64 counts 0..=8 is in a set, as a tree of selects on
/// the count's bits with the set's own bits as constants: 20 operations
/// for 64 counts, whatever the set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Membership {
    /// For each pair of counts (0,1) (2,3) (4,5) (6,7): the word of the
    /// even one, and that xor the word of the odd one.
    pairs: [(u64, u64); 4],
    /// The word of 8.
    eight: u64,
}

impl Membership {
    fn new(set: u16) -> Membership {
        let word = |n: usize| if set >> n & 1 == 1 { !0u64 } else { 0 };
        Membership {
            pairs: [0, 2, 4, 6].map(|n| (word(n), word(n) ^ word(n + 1))),
            eight: word(8),
        }
    }

    /// `sel ? b : a`, bitwise.
    #[inline]
    fn pick(sel: u64, a: u64, b: u64) -> u64 {
        a ^ (sel & (a ^ b))
    }

    #[inline]
    fn test(&self, n: Count) -> u64 {
        let [p0, p1, p2, p3] = self.pairs;
        let low = Self::pick(n.n1, p0.0 ^ (n.n0 & p0.1), p1.0 ^ (n.n0 & p1.1));
        let high = Self::pick(n.n1, p2.0 ^ (n.n0 & p2.1), p3.0 ^ (n.n0 & p3.1));
        // n3 means exactly 8 (no count passes it), and then n0..n2 are clear.
        Self::pick(n.n3, Self::pick(n.n2, low, high), self.eight)
    }
}

/// One bit per cell of a 2D layer inside a frame of `outside`: bit `x % 64`
/// of word `x / 64 + 1` of row `y + 1`, with a word of `outside` before and
/// after every row, a row of it above and below, and the bits past the
/// width set to it too. A word's neighbours, frame included, are then
/// always the words around it, with no edge case in the loops.
///
/// Built and read with shifts and masks only, never by reinterpreting the
/// words as bytes: the same result on every platform, big-endian included.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Board {
    width: usize,
    height: usize,
    /// Words of the inside of a row.
    inner: usize,
    /// Words of a row with its frame: `inner + 2`.
    stride: usize,
    words: Vec<u64>,
}

impl Board {
    fn new(width: usize, height: usize, fill: u64) -> Board {
        let inner = width.div_ceil(64);
        let stride = inner + 2;
        let mut words = vec![fill; stride * (height + 2)];
        for y in 1..=height {
            words[y * stride + 1..y * stride + 1 + inner].fill(0);
        }
        let mut b = Board {
            width,
            height,
            inner,
            stride,
            words,
        };
        b.pad(fill);
        b
    }

    /// The live cells (in a frame of `fill`) and the cells that may change
    /// (`alive` or `dead`, in a frame of nothing), in one read of the layer.
    fn from_cells(
        width: usize,
        height: usize,
        fill: u64,
        cells: &[TileId],
        alive: TileId,
        dead: TileId,
    ) -> (Board, Board) {
        let mut live = Board::new(width, height, fill);
        let mut free = Board::new(width, height, 0);
        // A byte per cell first (compares that vectorize), then 8 bytes to
        // 8 bits at a time.
        let bytes = |chunk: &[TileId; 64]| {
            let (mut a, mut f) = ([0u8; 64], [0u8; 64]);
            for i in 0..64 {
                a[i] = u8::from(chunk[i] == alive);
                f[i] = u8::from(chunk[i] == alive || chunk[i] == dead);
            }
            (pack(&a), pack(&f))
        };
        let mut last = [TileId::EMPTY; 64];
        for (y, row) in cells.chunks_exact(width).enumerate() {
            let at = (y + 1) * live.stride + 1;
            let mut chunks = row.chunks_exact(64);
            for (k, chunk) in chunks.by_ref().enumerate() {
                let (a, f) = bytes(chunk.try_into().expect("64 cells"));
                live.words[at + k] = a;
                free.words[at + k] = f;
            }
            let rest = chunks.remainder();
            if !rest.is_empty() {
                last[..rest.len()].copy_from_slice(rest);
                let (a, f) = bytes(&last);
                // The bits past the width keep what `new` set them to.
                let used = !0u64 >> (64 - rest.len());
                live.words[at + live.inner - 1] |= a & used;
                free.words[at + live.inner - 1] |= f & used;
            }
        }
        (live, free)
    }

    /// Sets the bits past the width of every row to `fill`.
    fn pad(&mut self, fill: u64) {
        let used = self.width % 64;
        if used == 0 {
            return;
        }
        let past = !0u64 << used;
        for y in 1..=self.height {
            let last = y * self.stride + self.inner;
            self.words[last] = (self.words[last] & !past) | (fill & past);
        }
    }

    fn set(&mut self, x: usize, y: usize) {
        self.words[(y + 1) * self.stride + 1 + x / 64] |= 1 << (x % 64);
    }

    fn and(&mut self, other: &Board) {
        for (a, b) in self.words.iter_mut().zip(&other.words) {
            *a &= b;
        }
    }

    /// One pass into `next` (whose frame is already `fill`); whether any
    /// cell changed.
    fn step(&self, next: &mut Board, free: &Board, rule: &Rule, sums: &mut RowSums) -> bool {
        let (s, inner) = (self.stride, self.inner);
        let row = |y: usize| &self.words[y * s..(y + 1) * s];
        let mut changed = 0;
        // Rolling sums of the rows above, at and below the one being made;
        // the frame rows are rows like any other.
        sums.row(1, row(0));
        sums.row(2, row(1));
        for y in 1..=self.height {
            sums.rotate();
            sums.row(2, row(y + 1));
            let at = y * s + 1;
            let cur = &self.words[at..at + inner];
            let free = &free.words[at..at + inner];
            let out = &mut next.words[at..at + inner];
            let [up, mid, down] = &sums.rows;
            let (u0, u1) = (&up.three[0][..inner], &up.three[1][..inner]);
            let (m0, m1) = (&mid.two[0][..inner], &mid.two[1][..inner]);
            let (d0, d1) = (&down.three[0][..inner], &down.three[1][..inner]);
            for k in 0..inner {
                let n = count((u0[k], u1[k]), (m0[k], m1[k]), (d0[k], d1[k]));
                let flip = free[k] & ((cur[k] & rule.keep.test(n)) ^ rule.born.test(n));
                changed |= flip;
                out[k] = cur[k] ^ flip;
            }
        }
        changed != 0
    }

    /// Writes to `cells` the cells that differ from `start`.
    fn write_changes(&self, start: &Board, cells: &mut [TileId], alive: TileId, dead: TileId) {
        for (y, row) in cells.chunks_exact_mut(self.width).enumerate() {
            let at = (y + 1) * self.stride + 1;
            for (k, chunk) in row.chunks_mut(64).enumerate() {
                let (now, then) = (self.words[at + k], start.words[at + k]);
                let diff = now ^ then;
                if diff == 0 {
                    continue;
                }
                // Selects by mask, not by branch: about half the cells of a
                // noisy map change, and a branch per cell mispredicts all
                // along. Bits to byte masks first, by table.
                let (changed, live) = (unpack(diff), unpack(now));
                for (i, cell) in chunk.iter_mut().enumerate() {
                    let (c, l) = (u16::from(changed[i]) * 0x0101, u16::from(live[i]) * 0x0101);
                    let new = (alive.0 & l) | (dead.0 & !l);
                    cell.0 = (cell.0 & !c) | (new & c);
                }
            }
        }
    }
}

/// The sums of one framed row, per inside word: each cell with its left
/// and right neighbours (0..=3, two bit planes), and the two neighbours
/// alone (0..=2), the one the middle row needs.
struct Sums {
    three: [Vec<u64>; 2],
    two: [Vec<u64>; 2],
}

/// [`Sums`] of the rows above, at and below the row being made.
struct RowSums {
    rows: [Sums; 3],
}

impl RowSums {
    fn new(inner: usize) -> RowSums {
        let planes = || [vec![0; inner], vec![0; inner]];
        let sums = || Sums {
            three: planes(),
            two: planes(),
        };
        RowSums {
            rows: [sums(), sums(), sums()],
        }
    }

    /// The row about to be made becomes the one above, the one below the
    /// one at it.
    fn rotate(&mut self) {
        self.rows.rotate_left(1);
    }

    /// The sums of a framed row of words, into `slot`.
    fn row(&mut self, slot: usize, words: &[u64]) {
        let Sums {
            three: [t0, t1],
            two: [w0, w1],
        } = &mut self.rows[slot];
        let inner = t0.len();
        let (before, at, after) = (&words[..inner], &words[1..=inner], &words[2..]);
        for k in 0..inner {
            let a = at[k];
            // Bit i: the cell left of i, and right of i.
            let l = (a << 1) | (before[k] >> 63);
            let r = (a >> 1) | (after[k] << 63);
            let lr = l ^ r;
            t0[k] = lr ^ a;
            t1[k] = (l & r) | (lr & a);
            w0[k] = lr;
            w1[k] = l & r;
        }
    }
}

/// 64 bits to 64 bytes, bit i to byte i: 0 or 0xff.
#[inline]
fn unpack(word: u64) -> [u8; 64] {
    let mut out = [0u8; 64];
    for (j, b) in out.chunks_exact_mut(8).enumerate() {
        b.copy_from_slice(&SPREAD[(word >> (8 * j)) as usize & 0xff].to_le_bytes());
    }
    out
}

/// Byte `b` of entry `i` is 0xff when bit `b` of `i` is set.
static SPREAD: [u64; 256] = {
    let mut table = [0u64; 256];
    let mut i = 0;
    while i < 256 {
        let mut b = 0;
        while b < 8 {
            if i >> b & 1 == 1 {
                table[i] |= 0xff << (8 * b);
            }
            b += 1;
        }
        i += 1;
    }
    table
};

/// 64 counts 0..=8, bit-sliced: bit i of `n0..n3` are the bits of count i.
#[derive(Clone, Copy)]
struct Count {
    n0: u64,
    n1: u64,
    n2: u64,
    n3: u64,
}

/// The 8-neighbour count: the rows above and below with the cell's column,
/// the middle one without it. Two-bit planes in, four out.
#[inline]
fn count(up: (u64, u64), mid: (u64, u64), down: (u64, u64)) -> Count {
    let ((u0, u1), (m0, m1), (d0, d1)) = (up, mid, down);
    // up + down: 0..=6 in three planes.
    let p0 = u0 ^ d0;
    let c0 = u0 & d0;
    let x = u1 ^ d1;
    let p1 = x ^ c0;
    let p2 = (u1 & d1) | (x & c0);
    // + middle: 0..=8 in four.
    let n0 = p0 ^ m0;
    let k0 = p0 & m0;
    let y = p1 ^ m1;
    let n1 = y ^ k0;
    let k1 = (p1 & m1) | (y & k0);
    Count {
        n0,
        n1,
        n2: p2 ^ k1,
        n3: p2 & k1,
    }
}

#[cfg(test)]
mod tests;
