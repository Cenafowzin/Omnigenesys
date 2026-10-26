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
//!   "params": {"wall_chance": 0.1, "lake_margin": 2},
//!   "origin": [0, 0, 0],
//!   "size": [4, 2, 1],
//!   "tiles": ["empty", "floor", "wall"],
//!   "layers": [
//!     {"name": "terrain", "kind": "tiles", "encoding": "rle", "data": [[1,6],[2,2]]},
//!     {"name": "height", "kind": "scalar", "encoding": "dense", "data": [0.5, 0.25, 1, 0, 0.125, 0.75, 0.3, 0.6]}
//!   ],
//!   "objects": [],
//!   "markers": []
//! }
//! ```
//!
//! A scalar layer is always dense, each value the shortest decimal that
//! reads back as the same f32; NaN and infinity are an error, not a value.
//! Cells go x fastest, then y, then z. `tiles[0]` is always `"empty"`; the
//! others follow in the order the pipeline declared them.

use std::io::{self, Write};

use thiserror::Error;

use crate::{
    grid::{Grid, GridError},
    layer::LayerKind,
    object::{Object, Value},
    spec::ParamValue,
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

/// How the map was made: with the pipeline file (and the salt, when
/// `salted`), enough to make it again. Usually from
/// [`PipelineSpec::provenance`](crate::spec::PipelineSpec::provenance).
///
/// The salt itself is never written: it exists so a shared pipeline or map
/// does not reveal it (PLANNING §2.1), and whoever needs to reproduce the
/// map already has it.
#[derive(Debug, Clone, PartialEq)]
pub struct Provenance {
    pub seed: u64,
    pub salted: bool,
    /// The attempt that made the map (a file's `attempts`), from 0.
    /// Written only when above 0: the map is the same from the seed
    /// alone, this only says how many tries it took.
    pub attempt: u32,
    /// Every declared param with the value the map was made with (an
    /// override or the default), in declaration order.
    pub params: Vec<(String, ParamValue)>,
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
    #[error("param '{name}' is {value}, which JSON cannot hold")]
    NonFiniteParam { name: String, value: f64 },
    #[error("object {object} has a property '{name}' that JSON cannot hold (NaN or infinite)")]
    NonFiniteProp { object: u32, name: String },
    #[error("layer '{layer}' has {value} at cell {index}, which JSON cannot hold")]
    NonFiniteCell {
        layer: String,
        index: usize,
        value: f32,
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
    provenance: &Provenance,
    encoding: Encoding,
) -> Result<(), ExportError> {
    for (name, value) in &provenance.params {
        if let ParamValue::Number(n) = value
            && !n.is_finite()
        {
            return Err(ExportError::NonFiniteParam {
                name: name.clone(),
                value: *n,
            });
        }
    }
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
    if provenance.attempt > 0 {
        w.raw(",\n  \"attempt\": ");
        w.int(provenance.attempt);
    }
    w.raw(",\n  \"params\": {");
    for (i, (name, value)) in provenance.params.iter().enumerate() {
        if i > 0 {
            w.raw(", ");
        }
        w.string(name);
        w.raw(": ");
        w.param(value);
    }
    w.raw("}");
    w.raw(",\n  \"origin\": ");
    w.triple(b.origin.x, b.origin.y, b.origin.z);
    w.raw(",\n  \"size\": ");
    w.triple(b.size.width, b.size.height, b.size.depth);
    w.raw(",\n  \"tiles\": [");
    for (i, name) in tiles.names().enumerate() {
        if i > 0 {
            w.raw(", ");
        }
        w.string(name);
    }
    w.raw("],\n  \"layers\": [");
    for (i, layer) in layers.iter().enumerate() {
        w.raw(if i == 0 { "\n    " } else { ",\n    " });
        w.layer(layer)?;
    }
    w.raw(if layers.is_empty() { "]" } else { "\n  ]" });
    w.raw(",\n  \"objects\": [");
    let mut first = true;
    for name in grid.names() {
        let id = grid.layer_id(name)?;
        if grid.kind(id)? != LayerKind::Objects {
            continue;
        }
        for o in grid.objects(id)?.iter() {
            w.raw(if first { "\n    " } else { ",\n    " });
            first = false;
            w.object(name, o)?;
        }
    }
    w.raw(if first { "]" } else { "\n  ]" });
    // Not produced by any operator yet; present so readers can rely on
    // the shape now.
    w.raw(",\n  \"markers\": []\n}\n");
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
enum Planned<'g> {
    Tiles {
        name: &'g str,
        cells: &'g [TileId],
        choice: Choice,
    },
    /// Always dense: a float rarely repeats exactly, so runs do not pay.
    Scalar { name: &'g str, cells: &'g [f32] },
}

fn plan<'g>(
    grid: &'g Grid,
    tiles: &TileRegistry,
    encoding: Encoding,
) -> Result<Vec<Planned<'g>>, ExportError> {
    let mut layers = Vec::new();
    for name in grid.names() {
        let id = grid.layer_id(name)?;
        if grid.kind(id)? == LayerKind::Objects {
            continue;
        }
        if grid.kind(id)? == LayerKind::Scalar {
            let cells = grid.scalars(id)?.as_slice();
            if let Some(i) = cells.iter().position(|v| !v.is_finite()) {
                return Err(ExportError::NonFiniteCell {
                    layer: name.to_string(),
                    index: i,
                    value: cells[i],
                });
            }
            layers.push(Planned::Scalar { name, cells });
            continue;
        }
        let cells = grid.tiles(id)?.as_slice();
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
        layers.push(Planned::Tiles {
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

    /// One object: `{"id", "type", "layer", "bbox": [x, y, z, w, h, d],
    /// "props": {"name": {"type", "value"}}}`, props in the order set.
    fn object(&mut self, layer: &str, o: &Object) -> Result<(), ExportError> {
        self.raw("{\"id\": ");
        self.int(o.id.get());
        self.raw(", \"type\": ");
        self.string(&o.kind);
        self.raw(", \"layer\": ");
        self.string(layer);
        let b = o.bbox;
        self.raw(", \"bbox\": [");
        self.int(b.origin.x);
        self.raw(", ");
        self.int(b.origin.y);
        self.raw(", ");
        self.int(b.origin.z);
        self.raw(", ");
        self.int(b.size.width);
        self.raw(", ");
        self.int(b.size.height);
        self.raw(", ");
        self.int(b.size.depth);
        self.raw("], \"props\": {");
        for (i, (name, value)) in o.props.iter().enumerate() {
            if i > 0 {
                self.raw(", ");
            }
            self.string(name);
            self.raw(": ");
            self.value(o, name, value)?;
        }
        self.raw("}}");
        Ok(())
    }

    /// `{"type": ..., "value": ...}`: the type spelled out, as Tiled and
    /// LDtk write properties, so 48 stays an integer for the adapter.
    fn value(&mut self, o: &Object, name: &str, v: &Value) -> Result<(), ExportError> {
        let (ty, _) = match v {
            Value::Int(_) => ("int", ()),
            Value::Float(_) => ("float", ()),
            Value::Bool(_) => ("bool", ()),
            Value::Str(_) => ("string", ()),
            Value::Cell(_) => ("cell", ()),
            Value::Object(_) => ("object", ()),
            Value::List(_) => ("list", ()),
        };
        self.raw("{\"type\": \"");
        self.raw(ty);
        self.raw("\", \"value\": ");
        match v {
            Value::Int(n) => self.int(*n),
            Value::Float(f) => {
                if !f.is_finite() {
                    return Err(ExportError::NonFiniteProp {
                        object: o.id.get(),
                        name: name.to_string(),
                    });
                }
                self.param(&ParamValue::Number(*f));
            }
            Value::Bool(b) => self.raw(if *b { "true" } else { "false" }),
            Value::Str(s) => self.string(s),
            Value::Cell(c) => self.triple(c.x, c.y, c.z),
            Value::Object(id) => self.int(id.get()),
            Value::List(items) => {
                self.raw("[");
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        self.raw(", ");
                    }
                    self.value(o, name, item)?;
                }
                self.raw("]");
            }
        }
        self.raw("}");
        Ok(())
    }

    /// A param value. A float is written by Rust's own formatting, not
    /// serde_json's (whose algorithm has changed): the shortest decimal that
    /// reads back as the same number, never in exponent form. A whole
    /// `number` keeps a `.0` so it still reads as one.
    fn param(&mut self, value: &ParamValue) {
        match value {
            ParamValue::Number(n) => {
                let text = n.to_string();
                self.raw(&text);
                if !text.contains('.') {
                    self.raw(".0");
                }
            }
            ParamValue::Integer(n) => self.int(*n),
            ParamValue::Boolean(b) => self.raw(if *b { "true" } else { "false" }),
            ParamValue::String(s) => self.string(s),
        }
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
        let (name, cells, choice) = match *layer {
            Planned::Tiles {
                name,
                cells,
                choice,
            } => (name, cells, choice),
            Planned::Scalar { name, cells } => return self.scalar_layer(name, cells),
        };
        let rle = match choice {
            Choice::Dense => false,
            Choice::Rle => {
                self.encode_rle(cells);
                true
            }
            Choice::Smaller => self.encode_rle(cells) > self.scratch.len(),
        };
        self.raw("{\"name\": ");
        self.string(name);
        self.raw(", \"kind\": \"tiles\", \"encoding\": ");
        self.raw(if rle { "\"rle\"" } else { "\"dense\"" });
        self.raw(", \"data\": [");
        if rle {
            self.emit_scratch()?;
        } else {
            self.dense(cells)?;
        }
        self.raw("]}");
        Ok(())
    }

    /// A scalar layer: each value as the shortest decimal that reads back
    /// as the same f32 (Rust's own formatting; never in exponent form).
    fn scalar_layer(&mut self, name: &str, cells: &[f32]) -> io::Result<()> {
        use std::fmt::Write as _;
        self.raw("{\"name\": ");
        self.string(name);
        self.raw(", \"kind\": \"scalar\", \"encoding\": \"dense\", \"data\": [");
        let mut text = String::new();
        for (i, v) in cells.iter().enumerate() {
            text.clear();
            // -0.0 and 0.0 are the same height.
            let v = if *v == 0.0 { 0.0 } else { *v };
            let _ = write!(text, "{v}");
            if i > 0 {
                self.raw(",");
            }
            self.raw(&text);
            if self.buf.len() >= BLOCK {
                self.spill()?;
            }
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
