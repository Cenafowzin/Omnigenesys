//! Output v2 (PLANNING §2.4): the generated map as JSON, for engines and
//! tools.
//!
//! The format is its own, written by hand: never the serde derive of the
//! internal types (that would freeze their field names as the format by
//! accident), never a `Value` (whose formatting can change between
//! serde_json versions). Only integers and strings, a fixed field order,
//! a trailing newline: the same map always gives the same bytes.
//!
//! ```json
//! {
//!   "format": "omnigenesys-map",
//!   "version": 2,
//!   "engine_version": "0.1.0",
//!   "seed": "512",
//!   "salted": false,
//!   "origin": [0, 0, 0],
//!   "size": [4, 2, 1],
//!   "tiles": ["empty", "floor", "wall"],
//!   "layers": [
//!     {"name": "terrain", "kind": "tiles", "encoding": "rle", "data": [[1,6],[2,2]]}
//!   ],
//!   "objects": [],
//!   "markers": []
//! }
//! ```
//!
//! Cells go x fastest, then y, then z. `tiles[0]` is always `"empty"`; the
//! others follow in the order the pipeline declared them.

use std::io::{self, Write};

use thiserror::Error;

use crate::{
    grid::{Grid, GridError},
    tile::{TileId, TileRegistry},
};

/// Version of the output format (not of the pipeline format).
pub const OUTPUT_VERSION: u32 = 2;

/// The crate version, recorded so a map can be traced to what made it.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// How a layer's cells are written.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Encoding {
    /// Per layer, whichever comes out smaller, counted in bytes (a tie
    /// goes to `dense`, the simpler one).
    #[default]
    Auto,
    /// `[[tile, count], ...]`: tiny for layers with long runs (a floor),
    /// up to three times the dense size for noisy ones.
    Rle,
    /// `[tile, tile, ...]`, one per cell, like LDtk's `intGridCsv`.
    Dense,
}

/// How the map was made. The salt itself is never written: it exists so a
/// shared pipeline or map does not reveal it (PLANNING §2.1), and whoever
/// needs to reproduce the map already has it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Provenance {
    pub seed: u64,
    pub salted: bool,
}

#[derive(Debug, Error)]
pub enum ExportError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Grid(#[from] GridError),
    #[error("layer '{layer}' has tile id {id}, but only {count} tiles are registered")]
    UnknownTile {
        layer: String,
        id: u16,
        count: usize,
    },
}

/// Writes the map as output v2. Writes go out in 64 KiB blocks, so a bare
/// `File` is fine.
///
/// Every layer is checked before the first byte is written, so an error
/// never leaves half a file.
pub fn write_json<W: Write>(
    out: W,
    grid: &Grid,
    tiles: &TileRegistry,
    provenance: Provenance,
    encoding: Encoding,
) -> Result<(), ExportError> {
    let layers = plan(grid, tiles, encoding)?;
    let mut w = Writer {
        out,
        buf: Vec::with_capacity(BLOCK + 64),
        scratch: Vec::new(),
        num: itoa::Buffer::new(),
    };
    let b = grid.bounds();
    w.raw("{\n  \"format\": \"omnigenesys-map\",\n  \"version\": ");
    w.int(OUTPUT_VERSION);
    w.raw(",\n  \"engine_version\": ");
    w.string(ENGINE_VERSION);
    // A string: above 2^53 a JSON number would be rounded by JavaScript.
    w.raw(",\n  \"seed\": \"");
    w.int(provenance.seed);
    w.raw("\",\n  \"salted\": ");
    w.raw(if provenance.salted { "true" } else { "false" });
    w.raw(",\n  \"origin\": ");
    w.triple(b.origin.x, b.origin.y, b.origin.z);
    w.raw(",\n  \"size\": ");
    w.triple(b.size.width, b.size.height, b.size.depth);
    w.raw(",\n  \"tiles\": [");
    for i in 0..tiles.len() {
        if i > 0 {
            w.raw(", ");
        }
        // Ids are the positions 0..len, so every one has a name.
        w.string(tiles.name(TileId(i as u16)).unwrap_or_default());
    }
    w.raw("],\n  \"layers\": [");
    for (i, layer) in layers.iter().enumerate() {
        w.raw(if i == 0 { "\n    " } else { ",\n    " });
        w.layer(layer)?;
    }
    w.raw(if layers.is_empty() { "]" } else { "\n  ]" });
    // Not produced by any operator yet; present so readers can rely on
    // the shape now.
    w.raw(",\n  \"objects\": [],\n  \"markers\": []\n}\n");
    w.finish()?;
    Ok(())
}

/// How a layer will be written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Choice {
    Dense,
    Rle,
    /// `rle` if it comes out smaller: decided while encoding it.
    Smaller,
}

/// A layer ready to write: its cells and how to encode them.
struct Planned<'g> {
    name: &'g str,
    cells: &'g [TileId],
    choice: Choice,
}

fn plan<'g>(
    grid: &'g Grid,
    tiles: &TileRegistry,
    encoding: Encoding,
) -> Result<Vec<Planned<'g>>, ExportError> {
    let mut layers = Vec::new();
    for name in grid.names() {
        let cells = grid.tiles(grid.layer_id(name)?)?.as_slice();
        let (runs, max) = scan(cells);
        if let Some(max) = max
            && max.0 as usize >= tiles.len()
        {
            return Err(ExportError::UnknownTile {
                layer: name.to_string(),
                id: max.0,
                count: tiles.len(),
            });
        }
        let choice = match encoding {
            Encoding::Auto if rle_may_win(cells.len(), runs, max) => Choice::Smaller,
            Encoding::Auto | Encoding::Dense => Choice::Dense,
            Encoding::Rle => Choice::Rle,
        };
        layers.push(Planned {
            name,
            cells,
            choice,
        });
    }
    Ok(layers)
}

/// Number of runs and the largest id, comparing neighbours: a loop the
/// compiler vectorizes.
fn scan(cells: &[TileId]) -> (usize, Option<TileId>) {
    let Some(&first) = cells.first() else {
        return (0, None);
    };
    let (mut runs, mut max) = (1, first);
    for pair in cells.windows(2) {
        runs += usize::from(pair[0] != pair[1]);
        max = max.max(pair[1]);
    }
    (runs, Some(max))
}

/// Whether rle could take fewer bytes than dense. A run costs rle at
/// least 6 bytes (`[1,1],`) and a cell costs dense at most
/// `digits(max) + 1`, so a noisy layer is settled here, from the scan.
/// Counting it run by run would cost 9× the scan (measured 2026-10-22,
/// 1024², 40 interleaved runs).
fn rle_may_win(cells: usize, runs: usize, max: Option<TileId>) -> bool {
    max.is_some_and(|max| 6 * runs < (digits(max.0 as usize) + 1) * cells)
}

/// Decimal digits of `n`.
fn digits(n: usize) -> usize {
    n.checked_ilog10().map_or(1, |l| l as usize + 1)
}

/// Data accumulates in `buf` and goes to `out` a block at a time. Into a
/// `Vec<u8>` this measured the same as writing each piece straight to it
/// (2026-10-22, 40 interleaved runs); it is here so a bare `File` does not
/// make a system call per number.
const BLOCK: usize = 64 * 1024;

struct Writer<W> {
    out: W,
    buf: Vec<u8>,
    /// A layer's rle, encoded before its header is written: for
    /// [`Choice::Smaller`] the header says which encoding won. Reused
    /// across layers.
    scratch: Vec<u8>,
    /// Reused for every number: formatting does not allocate.
    num: itoa::Buffer,
}

impl<W: Write> Writer<W> {
    #[inline]
    fn raw(&mut self, s: &str) {
        self.buf.extend_from_slice(s.as_bytes());
    }

    #[inline]
    fn int(&mut self, n: impl itoa::Integer) {
        self.buf.extend_from_slice(self.num.format(n).as_bytes());
    }

    /// Hands a full block to `out`. Called in the data loops; the header is
    /// small enough to wait for the first block.
    #[inline]
    fn spill(&mut self) -> io::Result<()> {
        if self.buf.len() >= BLOCK {
            self.out.write_all(&self.buf)?;
            self.buf.clear();
        }
        Ok(())
    }

    fn finish(mut self) -> io::Result<()> {
        self.out.write_all(&self.buf)?;
        self.out.flush()
    }

    fn triple(&mut self, a: impl itoa::Integer, b: impl itoa::Integer, c: impl itoa::Integer) {
        self.raw("[");
        self.int(a);
        self.raw(", ");
        self.int(b);
        self.raw(", ");
        self.int(c);
        self.raw("]");
    }

    /// A JSON string: `"` and `\` escaped, control characters as `\u00XX`.
    fn string(&mut self, s: &str) {
        self.raw("\"");
        for c in s.chars() {
            match c {
                '"' => self.raw("\\\""),
                '\\' => self.raw("\\\\"),
                c if (c as u32) < 0x20 => self.raw(&format!("\\u{:04x}", c as u32)),
                c => self.raw(c.encode_utf8(&mut [0; 4])),
            }
        }
        self.raw("\"");
    }

    fn layer(&mut self, layer: &Planned<'_>) -> io::Result<()> {
        let rle = match layer.choice {
            Choice::Dense => false,
            Choice::Rle => {
                self.encode_rle(layer.cells);
                true
            }
            Choice::Smaller => self.encode_rle(layer.cells) > self.scratch.len(),
        };
        self.raw("{\"name\": ");
        self.string(layer.name);
        self.raw(", \"kind\": \"tiles\", \"encoding\": ");
        self.raw(if rle { "\"rle\"" } else { "\"dense\"" });
        self.raw(", \"data\": [");
        if rle {
            self.emit_scratch()?;
        } else {
            self.dense(layer.cells)?;
        }
        self.raw("]}");
        Ok(())
    }

    /// Encodes `cells` as rle into `scratch` and returns the exact size the
    /// same cells take dense: one pass over the runs for both, so when rle
    /// wins (the usual case here) the cells are not walked again.
    fn encode_rle(&mut self, cells: &[TileId]) -> usize {
        self.scratch.clear();
        let mut dense = 0;
        for (i, run) in cells.chunk_by(|a, b| a == b).enumerate() {
            self.scratch
                .extend_from_slice(if i == 0 { b"[" } else { b",[" });
            let id = self.num.format(run[0].0);
            // `id,` per cell.
            dense += run.len() * (id.len() + 1);
            self.scratch.extend_from_slice(id.as_bytes());
            self.scratch.push(b',');
            self.scratch
                .extend_from_slice(self.num.format(run.len()).as_bytes());
            self.scratch.push(b']');
        }
        // No separator after the last cell.
        dense.saturating_sub(1)
    }

    fn emit_scratch(&mut self) -> io::Result<()> {
        if self.scratch.len() < BLOCK {
            self.buf.extend_from_slice(&self.scratch);
            self.spill()
        } else {
            self.out.write_all(&self.buf)?;
            self.buf.clear();
            self.out.write_all(&self.scratch)
        }
    }

    fn dense(&mut self, cells: &[TileId]) -> io::Result<()> {
        for (i, t) in cells.iter().enumerate() {
            if i > 0 {
                self.raw(",");
            }
            self.int(t.0);
            self.spill()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
