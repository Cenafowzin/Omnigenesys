//! PNG preview of a map, for looking at it, not for engines.
//!
//! Each cell is a square of `scale` pixels. Layers are drawn in the order
//! they were declared, so a later layer's non-empty cell covers an earlier
//! one (objects over terrain). The z slices of a 3D map go one under
//! another, a transparent row apart. Indexed colour: palette entry = tile
//! id, entry 0 (`empty`) transparent unless the palette gives it a colour.
//!
//! A tile's colour comes from the user's palette file, else from a table
//! of common names, else from a hash of the name: always the same colour
//! for the same name, across maps and runs.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::Path;

use omnigenesys_core::grid::Grid;
use omnigenesys_core::layer::LayerKind;
use omnigenesys_core::tile::{TileId, TileRegistry};

/// Largest side of the image when the scale is not given.
const TARGET: u32 = 1024;

/// Colours chosen by the user: a JSON object of tile name to `"#rrggbb"`.
///
/// A file of its own, not part of the pipeline: a colour changes nothing in
/// the map, and engines draw tiles their own way. The web app can read and
/// write the same file.
#[derive(Debug, Default)]
pub struct Palette(HashMap<String, [u8; 3]>);

impl Palette {
    pub fn load(path: &Path) -> Result<Palette, String> {
        let at = path.display();
        let text = fs::read_to_string(path).map_err(|e| format!("cannot read {at}: {e}"))?;
        let entries: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&text).map_err(|e| format!("{at}: {e}"))?;
        let mut colors = HashMap::new();
        for (name, value) in entries {
            let rgb = value
                .as_str()
                .and_then(hex)
                .ok_or_else(|| format!("{at}: '{name}': {value} is not a \"#rrggbb\" colour"))?;
            colors.insert(name, rgb);
        }
        Ok(Palette(colors))
    }

    /// Names the map has no tile for, sorted: most likely a tile renamed
    /// in the pipeline and not in the palette.
    pub fn unknown(&self, tiles: &TileRegistry) -> Vec<&str> {
        let mut names: Vec<&str> = self
            .0
            .keys()
            .map(String::as_str)
            .filter(|name| tiles.names().all(|t| t != *name))
            .collect();
        names.sort_unstable();
        names
    }
}

/// `"#c4aa78"` to its three bytes.
fn hex(s: &str) -> Option<[u8; 3]> {
    let digits = s
        .strip_prefix('#')
        .filter(|d| d.len() == 6 && d.is_ascii())?;
    let byte = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

pub fn write(
    path: &Path,
    grid: &Grid,
    tiles: &TileRegistry,
    palette: &Palette,
    scale: Option<u32>,
) -> Result<(), String> {
    if tiles.len() > 256 {
        return Err(format!(
            "the preview draws up to 256 tiles (an indexed PNG); this map has {}",
            tiles.len()
        ));
    }
    let size = grid.bounds().size;
    let (w, h, d) = (size.width, size.height, size.depth);
    // Slices plus one empty row between each two.
    let rows = h * d + (d - 1);
    let scale = scale.unwrap_or_else(|| (TARGET / w.max(rows)).max(1));
    let (width, height) = (w * scale, rows * scale);

    let cells = compose(grid)?;
    let mut data = Vec::with_capacity(width as usize * height as usize);
    for row in 0..rows {
        let (z, y) = (row / (h + 1), row % (h + 1));
        let mut line = Vec::with_capacity(width as usize);
        if y == h {
            line.resize(width as usize, 0);
        } else {
            let start = ((z * h + y) * w) as usize;
            for t in &cells[start..start + w as usize] {
                line.extend(std::iter::repeat_n(t.index() as u8, scale as usize));
            }
        }
        for _ in 0..scale {
            data.extend_from_slice(&line);
        }
    }

    let file = File::create(path).map_err(|e| format!("cannot create {}: {e}", path.display()))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Indexed);
    encoder.set_depth(png::BitDepth::Eight);
    // A preview: speed over size. 1024² island, 30 interleaved runs
    // (2026-10-22): the default (Balanced) took ~110 ms, more than making
    // the map; Fast ~8 ms for a file 13% larger. Pixels are the same.
    encoder.set_compression(png::Compression::Fast);
    encoder.set_palette(colors(tiles, palette));
    // Alpha of entry 0 only; the rest default to opaque. An `empty` with a
    // colour in the palette is a background, drawn opaque.
    if !palette.0.contains_key("empty") {
        encoder.set_trns(vec![0u8]);
    }
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&data).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())
}

/// For each cell, the tile of the last layer that is not empty there.
fn compose(grid: &Grid) -> Result<Vec<TileId>, String> {
    let mut out: Vec<TileId> = Vec::new();
    for name in grid.names() {
        let id = grid.layer_id(name).map_err(|e| e.to_string())?;
        // Scalar layers are fields, not tiles: the preview draws tiles.
        if grid.kind(id).map_err(|e| e.to_string())? != LayerKind::Tiles {
            continue;
        }
        let layer = grid.tiles(id).map_err(|e| e.to_string())?.as_slice();
        if out.is_empty() {
            out = layer.to_vec();
            continue;
        }
        for (o, &t) in out.iter_mut().zip(layer) {
            if t != TileId::EMPTY {
                *o = t;
            }
        }
    }
    if out.is_empty() {
        let cells = grid.bounds().size.checked_len().unwrap_or(0);
        out = vec![TileId::EMPTY; cells];
    }
    Ok(out)
}

/// RGB triples, entry i for tile id i.
fn colors(tiles: &TileRegistry, palette: &Palette) -> Vec<u8> {
    tiles
        .names()
        .enumerate()
        .flat_map(|(i, name)| match palette.0.get(name) {
            Some(rgb) => *rgb,
            None if i == 0 => [0, 0, 0],
            None => color_of(name),
        })
        .collect()
}

/// Colours for common tile names, so the usual maps read at a glance.
const KNOWN: &[(&str, [u8; 3])] = &[
    ("floor", [196, 170, 120]),
    ("ground", [150, 115, 75]),
    ("dirt", [130, 95, 60]),
    ("grass", [95, 165, 70]),
    ("sand", [225, 205, 140]),
    ("wall", [85, 85, 95]),
    ("stone", [125, 125, 130]),
    ("rock", [105, 100, 100]),
    ("water", [55, 115, 200]),
    ("lava", [225, 85, 30]),
    ("tree", [30, 105, 45]),
    ("foliage", [60, 135, 60]),
    ("path", [210, 190, 150]),
    ("snow", [240, 245, 250]),
];

/// A colour from a name: the table above, else FNV-1a hash to hue, with
/// saturation and lightness kept in a band where colours are vivid and
/// tell apart.
fn color_of(name: &str) -> [u8; 3] {
    if let Some((_, rgb)) = KNOWN.iter().find(|(known, _)| *known == name) {
        return *rgb;
    }
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in name.bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    let hue = (h % 360) as f64;
    let saturation = 0.45 + ((h >> 16) % 30) as f64 / 100.0;
    let lightness = 0.40 + ((h >> 32) % 25) as f64 / 100.0;
    hsl(hue, saturation, lightness)
}

fn hsl(h: f64, s: f64, l: f64) -> [u8; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match h as u32 / 60 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let byte = |v: f64| ((v + m) * 255.0).round() as u8;
    [byte(r), byte(g), byte(b)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_reads_six_digits_after_a_hash() {
        assert_eq!(hex("#c4aa78"), Some([0xc4, 0xaa, 0x78]));
        assert_eq!(hex("#C4AA78"), Some([0xc4, 0xaa, 0x78]));
        for bad in ["c4aa78", "#c4aa7", "#c4aa780", "#gggggg", "#c4aaé", ""] {
            assert_eq!(hex(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_palette_wins_over_the_table_and_the_hash() {
        let tiles = TileRegistry::new(["floor", "cave_wall"]).unwrap();
        let mut p = Palette::default();
        assert_eq!(
            &colors(&tiles, &p)[3..6],
            &KNOWN[0].1,
            "floor from the table"
        );
        assert_eq!(
            &colors(&tiles, &p)[6..9],
            &color_of("cave_wall"),
            "the hash"
        );
        p.0.insert("floor".to_string(), [1, 2, 3]);
        p.0.insert("cave_wall".to_string(), [4, 5, 6]);
        assert_eq!(colors(&tiles, &p), [0, 0, 0, 1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn unknown_names_are_listed_sorted() {
        let tiles = TileRegistry::new(["floor"]).unwrap();
        let mut p = Palette::default();
        for name in ["wal", "floor", "empty", "cave"] {
            p.0.insert(name.to_string(), [0, 0, 0]);
        }
        assert_eq!(p.unknown(&tiles), ["cave", "wal"]);
    }
}
