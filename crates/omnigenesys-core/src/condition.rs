use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    access::Reach,
    bounds::Bounds,
    coords::Coord,
    filter::{CompiledFilter, FilterError, ObjectFilter, Scene, Subject},
    grid::{Grid, GridError, LayerId},
    layer::{Layer, LayerKind},
    mask::{CellMask, MaskWriter},
    tile::{TileError, TileId, TileRegistry, TileSet},
};

#[derive(Debug, Clone, PartialEq)]
pub struct CompiledCondition(Compiled);

#[derive(Debug, Clone)]
pub struct PreparedCondition(Prepared);

#[derive(Debug, Clone)]
enum Prepared {
    TileIs(CellMask),
    Any(Vec<Prepared>),
    All(Vec<Prepared>),
    Not(Box<Prepared>),
    /// The four distance conditions: true when the number of matching cells
    /// in the box around the cell is in `min..=max`.
    Window {
        sums: PrefixSums,
        radius: Radius,
        min: u64,
        max: u64,
    },
}

#[derive(Debug, Clone, PartialEq)]
enum Compiled {
    TileIs {
        layer: LayerId,
        tiles: Vec<TileId>,
    },
    NearType {
        layer: LayerId,
        tiles: Vec<TileId>,
        radius: Radius,
    },
    NotNearType {
        layer: LayerId,
        tiles: Vec<TileId>,
        radius: Radius,
    },
    AreaIs {
        layer: LayerId,
        tiles: Vec<TileId>,
        radius: Radius,
    },
    Count {
        layer: LayerId,
        tiles: Vec<TileId>,
        radius: Radius,
        min: u32,
        max: u32,
    },
    /// `min <= value < max` in a scalar layer; NaN never holds.
    ScalarInRange {
        layer: LayerId,
        min: f32,
        max: f32,
    },
    /// Cells to the nearest edge of the map in `min..=max`.
    EdgeDistance {
        min: u32,
        max: u32,
    },
    InObject {
        layer: LayerId,
        part: Part,
        filter: CompiledFilter,
    },
    Any(Vec<Compiled>),
    All(Vec<Compiled>),
    Not(Box<Compiled>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Radius {
    x: u32,
    y: u32,
    z: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged, expecting = "a tile name or a list of tile names")]
pub enum TileSpec {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Condition {
    TileIs {
        layer: String,
        tile: TileSpec,
    },
    NearType {
        layer: String,
        tile: TileSpec,
        distance: u32,
    },
    NotNearType {
        layer: String,
        tile: TileSpec,
        distance: u32,
    },
    AreaIs {
        layer: String,
        tile: TileSpec,
        distance: u32,
    },
    Count {
        layer: String,
        tile: TileSpec,
        distance: u32,
        min: Option<u32>,
        max: Option<u32>,
    },
    /// The value of a scalar layer at the cell is at least `min` and below
    /// `max` (either may be left out): height, moisture, a noise threshold.
    /// Half-open, so bands written one after another (`max` of one is the
    /// `min` of the next) neither overlap nor leave a gap.
    ScalarInRange {
        layer: String,
        min: Option<f64>,
        max: Option<f64>,
    },
    /// Cells from the cell to the nearest edge of the **map** (not of a
    /// chunk), from `min` to `max` (both counted, as all whole-number
    /// ranges are): `max: 0` is the outermost ring, a border of 3 is
    /// `max: 2`. In 3D the top and bottom count as edges too.
    EdgeDistance {
        #[serde(default)]
        min: Option<u32>,
        #[serde(default)]
        max: Option<u32>,
    },
    /// The cell is inside an object of the object layer `layer` that
    /// passes `where`: anywhere in its box, only on its `wall` (the box's
    /// outer ring, one cell thick, in x and y) or only `inside` it. A
    /// room's floor, its walls, the door cells of a connection.
    InObject {
        layer: String,
        #[serde(default)]
        part: Part,
        #[serde(rename = "where", default)]
        filter: Vec<ObjectFilter>,
    },
    Any {
        conditions: Vec<Condition>,
    },
    All {
        conditions: Vec<Condition>,
    },
    Not {
        condition: Box<Condition>,
    },
}

/// Which cells of an object's box: all, the outer ring, or the rest.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Part {
    #[default]
    Any,
    Wall,
    Inside,
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum ConditionError {
    #[error(transparent)]
    Grid(#[from] GridError),
    #[error(transparent)]
    Tile(#[from] TileError),
    #[error("count needs at least one of `min` or `max`")]
    CountWithNoBounds,
    #[error("count range is empty: min {min} > max {max}")]
    EmptyCountRange { min: u32, max: u32 },
    #[error("scalar_in_range needs at least one of `min` or `max`")]
    RangeWithNoBounds,
    #[error("scalar range is empty: min {min} is not below max {max}")]
    EmptyScalarRange { min: f64, max: f64 },
    #[error("edge_distance needs at least one of `min` or `max`")]
    EdgeWithNoBounds,
    #[error("edge_distance range is empty: min {min} > max {max}")]
    EmptyEdgeRange { min: u32, max: u32 },
    #[error("layer has {cells} cells, more than a distance condition can count")]
    TooManyCellsToCount { cells: usize },
    #[error("in_object: {0}")]
    Filter(#[from] FilterError),
}

/// Entry `(x, y, z)` counts the matches in the local box `[0, x) × [0, y) × [0, z]`.
/// x and y have a zero border (one entry larger), so a query never branches
/// on them. z has none: a 2D map would pay a whole zero plane for it, which
/// doubled the build time (measured 2026-10-21); queries test `z0 == 0`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PrefixSums {
    bounds: Bounds,
    sw: usize,
    sh: usize,
    sums: Box<[u32]>,
}

impl PrefixSums {
    fn from_layer(layer: &Layer<TileId>, tiles: &TileSet) -> Result<PrefixSums, ConditionError> {
        let bounds = layer.bounds();
        let cells = bounds.size.len();
        let too_many = ConditionError::TooManyCellsToCount { cells };
        if cells > u32::MAX as usize {
            return Err(too_many);
        }
        // Queries clip to nothing before reading. Without this, a width of
        // u32::MAX and height 0 would allocate billions of entries.
        if cells == 0 {
            return Ok(PrefixSums {
                bounds,
                sw: 0,
                sh: 0,
                sums: Box::new([]),
            });
        }
        let sw = bounds.size.width as usize + 1;
        let sh = bounds.size.height as usize + 1;
        let plane = sw * sh;
        // Can overflow usize on 32-bit targets (WASM).
        let len = plane
            .checked_mul(bounds.size.depth as usize)
            .ok_or(too_many)?;
        let mut sums: Box<[u32]> = vec![0u32; len].into_boxed_slice();

        // Passes x and y in one sweep: an entry is the entry above it plus
        // the running count of its own row.
        let mut cells = layer.cells();
        for p in sums.chunks_exact_mut(plane) {
            for y in 1..sh {
                let (above, row) = p[(y - 1) * sw..][..2 * sw].split_at_mut(sw);
                let mut run = 0;
                // The cells go last in the zip: when the row ends, the next
                // row's first cell is not consumed.
                for ((entry, above), cell) in
                    row[1..].iter_mut().zip(&above[1..]).zip(cells.by_ref())
                {
                    run += u32::from(tiles.contains(*cell));
                    *entry = above + run;
                }
            }
        }
        accumulate_blocks(&mut sums, plane);

        Ok(PrefixSums {
            bounds,
            sw,
            sh,
            sums,
        })
    }

    #[inline]
    fn count(&self, c: Coord, r: Radius) -> u32 {
        let (o, s) = (self.bounds.origin, self.bounds.size);
        let (Some((x0, x1)), Some((y0, y1)), Some((z0, z1))) = (
            span(local(c.x, o.x, 0), r.x, s.width),
            span(local(c.y, o.y, 0), r.y, s.height),
            span(local(c.z, o.z, 0), r.z, s.depth),
        ) else {
            return 0;
        };
        let p = |x: usize, y: usize, z: usize| self.sums[x + self.sw * (y + self.sh * z)];
        // Matches in [x0, x1) × [y0, y1) × [0, z]. Partial results can go
        // below zero; wrapping keeps the final count exact, since it fits u32.
        let rect = |z: usize| {
            p(x1, y1, z)
                .wrapping_sub(p(x0, y1, z))
                .wrapping_sub(p(x1, y0, z))
                .wrapping_add(p(x0, y0, z))
        };
        let below = if z0 == 0 { 0 } else { rect(z0 - 1) };
        rect(z1 - 1).wrapping_sub(below)
    }

    /// Calls `f` with the box count of every cell of `region`, in z → y → x
    /// order. Per row, the (y, z) corner rows of the table (two in 2D, four
    /// in 3D) combine into one row `e`, and a cell costs two reads:
    /// `e[x1] - e[x0]`.
    fn for_each_count(&self, region: Bounds, r: Radius, mut f: impl FnMut(u32)) {
        let (o, s) = (self.bounds.origin, self.bounds.size);
        let ro = region.origin;
        let row = |y: usize, z: usize| &self.sums[self.sw * (y + self.sh * z)..][..self.sw];
        let mut e = vec![0u32; self.sw];
        for k in 0..region.size.depth {
            let zs = span(local(ro.z, o.z, k), r.z, s.depth);
            for j in 0..region.size.height {
                let ys = span(local(ro.y, o.y, j), r.y, s.height);
                let (Some((y0, y1)), Some((z0, z1))) = (ys, zs) else {
                    (0..region.size.width).for_each(|_| f(0));
                    continue;
                };
                for (e, (a, b)) in e
                    .iter_mut()
                    .zip(row(y1, z1 - 1).iter().zip(row(y0, z1 - 1)))
                {
                    *e = a.wrapping_sub(*b);
                }
                if z0 > 0 {
                    let below = row(y1, z0 - 1).iter().zip(row(y0, z0 - 1));
                    for (e, (c, d)) in e.iter_mut().zip(below) {
                        *e = e.wrapping_sub(*c).wrapping_add(*d);
                    }
                }
                for i in 0..region.size.width {
                    f(match span(local(ro.x, o.x, i), r.x, s.width) {
                        Some((x0, x1)) => e[x1].wrapping_sub(e[x0]),
                        None => 0,
                    });
                }
            }
        }
    }
}

/// Position `start + offset` relative to `origin`, in i64 so that extreme
/// coordinates cannot overflow.
#[inline]
fn local(start: i32, origin: i32, offset: u32) -> i64 {
    i64::from(start) - i64::from(origin) + i64::from(offset)
}

/// Block k adds block k - 1, for consecutive blocks of `block` entries.
fn accumulate_blocks(sums: &mut [u32], block: usize) {
    for k in 1..sums.len() / block {
        let (done, rest) = sums.split_at_mut(k * block);
        let prev = &done[(k - 1) * block..];
        for (entry, below) in rest[..block].iter_mut().zip(prev) {
            *entry += *below;
        }
    }
}

/// `[center - r, center + r]` (center local to the layer) clipped to the
/// layer, as half-open table indices; `None` when it misses the layer.
#[inline]
fn span(center: i64, r: u32, size: u32) -> Option<(usize, usize)> {
    let lo = (center - i64::from(r)).max(0);
    let hi = (center + i64::from(r) + 1).min(i64::from(size));
    (lo < hi).then_some((lo as usize, hi as usize))
}

impl Radius {
    /// Saturates: no count reaches a volume that large anyway.
    fn volume(self) -> u64 {
        let side = |r: u32| 2 * u64::from(r) + 1;
        side(self.x)
            .saturating_mul(side(self.y))
            .saturating_mul(side(self.z))
    }
}

impl TileSpec {
    pub fn names(&self) -> &[String] {
        match self {
            TileSpec::One(name) => std::slice::from_ref(name),
            TileSpec::Many(names) => names,
        }
    }
}

impl CompiledCondition {
    pub fn prepare(&self, grid: &Grid) -> Result<PreparedCondition, ConditionError> {
        prepare(&self.0, grid).map(PreparedCondition)
    }

    /// Every layer the condition reads, and how far around each cell.
    pub fn reads(&self) -> Vec<(LayerId, Reach)> {
        let mut out = Vec::new();
        reads(&self.0, &mut out);
        out
    }
}

fn reads(c: &Compiled, out: &mut Vec<(LayerId, Reach)>) {
    match c {
        Compiled::TileIs { layer, .. } | Compiled::ScalarInRange { layer, .. } => {
            out.push((*layer, Reach::ZERO));
        }
        // Reads no layer: only where the cell is in the map.
        Compiled::EdgeDistance { .. } => {}
        Compiled::InObject { layer, filter, .. } => {
            out.push((*layer, Reach::ZERO));
            out.extend(filter.layers().into_iter().map(|l| (l, Reach::ZERO)));
        }
        Compiled::NearType { layer, radius, .. }
        | Compiled::NotNearType { layer, radius, .. }
        | Compiled::AreaIs { layer, radius, .. }
        | Compiled::Count { layer, radius, .. } => {
            out.push((*layer, Reach::symmetric(radius.x, radius.y, radius.z)));
        }
        Compiled::Any(cs) | Compiled::All(cs) => cs.iter().for_each(|c| reads(c, out)),
        Compiled::Not(c) => reads(c, out),
    }
}

impl Condition {
    pub fn compile(
        &self,
        grid: &Grid,
        tiles: &TileRegistry,
    ) -> Result<CompiledCondition, ConditionError> {
        compile(self, grid, tiles).map(CompiledCondition)
    }
}

impl PreparedCondition {
    /// The answer for one cell. For sparse queries; to answer a whole
    /// region, [`eval`](Self::eval) is several times faster.
    #[inline]
    pub fn test(&self, c: Coord) -> bool {
        test(&self.0, c)
    }

    /// The answer for every cell of `region`, which may differ from the
    /// grid's bounds. Same answers as [`test`](Self::test), cell by cell.
    ///
    /// # Panics
    ///
    /// If `region` has more cells than `usize` can index.
    pub fn eval(&self, region: Bounds) -> CellMask {
        eval(&self.0, region)
    }
}

fn eval(p: &Prepared, region: Bounds) -> CellMask {
    match p {
        Prepared::TileIs(mask) if mask.bounds() == region => mask.clone(),
        Prepared::TileIs(mask) => {
            let mut w = MaskWriter::new(region);
            for c in region.iter() {
                w.push(mask.get(c));
            }
            w.finish()
        }
        Prepared::Any(ps) => ps.iter().fold(CellMask::new(region), |mut m, p| {
            m |= &eval(p, region);
            m
        }),
        Prepared::All(ps) => ps.iter().fold(CellMask::full(region), |mut m, p| {
            m &= &eval(p, region);
            m
        }),
        Prepared::Not(p) => !eval(p, region),
        Prepared::Window {
            sums,
            radius,
            min,
            max,
        } => {
            let mut w = MaskWriter::new(region);
            sums.for_each_count(region, *radius, |n| {
                w.push((*min..=*max).contains(&u64::from(n)));
            });
            w.finish()
        }
    }
}

#[inline]
fn test(p: &Prepared, c: Coord) -> bool {
    match p {
        Prepared::TileIs(mask) => mask.get(c),
        Prepared::Any(ps) => ps.iter().any(|p| test(p, c)),
        Prepared::All(ps) => ps.iter().all(|p| test(p, c)),
        Prepared::Not(p) => !test(p, c),
        Prepared::Window {
            sums,
            radius,
            min,
            max,
        } => (*min..=*max).contains(&u64::from(sums.count(c, *radius))),
    }
}

fn prepare(c: &Compiled, grid: &Grid) -> Result<Prepared, ConditionError> {
    Ok(match c {
        Compiled::TileIs { layer, tiles } => Prepared::TileIs(CellMask::from_layer(
            grid.tiles(*layer)?,
            &TileSet::new(tiles),
        )),
        Compiled::ScalarInRange { layer, min, max } => {
            Prepared::TileIs(CellMask::from_scalars(grid.scalars(*layer)?, *min, *max))
        }
        Compiled::EdgeDistance { min, max } => Prepared::TileIs(CellMask::from_edge_distance(
            grid.bounds(),
            grid.map(),
            *min,
            *max,
        )),
        Compiled::InObject {
            layer,
            part,
            filter,
        } => Prepared::TileIs(in_object(grid, *layer, *part, filter)?),
        Compiled::Any(cs) => Prepared::Any(prepare_all(cs, grid)?),
        Compiled::All(cs) => Prepared::All(prepare_all(cs, grid)?),
        Compiled::Not(c) => Prepared::Not(Box::new(prepare(c, grid)?)),
        Compiled::NearType {
            layer,
            tiles,
            radius,
        } => window(grid, *layer, tiles, *radius, 1, u64::MAX)?,
        Compiled::NotNearType {
            layer,
            tiles,
            radius,
        } => window(grid, *layer, tiles, *radius, 0, 0)?,
        // The full box, not the clipped one: near the edge part of the box
        // is outside the layer, never matches, so the area is not clear.
        Compiled::AreaIs {
            layer,
            tiles,
            radius,
        } => window(
            grid,
            *layer,
            tiles,
            *radius,
            radius.volume(),
            radius.volume(),
        )?,
        Compiled::Count {
            layer,
            tiles,
            radius,
            min,
            max,
        } => window(grid, *layer, tiles, *radius, (*min).into(), (*max).into())?,
    })
}

/// The cells of `grid` in the `part` of an object of `layer` that passes
/// `filter`.
fn in_object(
    grid: &Grid,
    layer: LayerId,
    part: Part,
    filter: &CompiledFilter,
) -> Result<CellMask, ConditionError> {
    let mut ids = vec![layer];
    ids.extend(filter.layers().into_iter().filter(|l| *l != layer));
    let layers = ids
        .iter()
        .map(|&id| Ok((id, grid.objects(id)?)))
        .collect::<Result<Vec<_>, GridError>>()?;
    let scene = Scene::new(grid.map(), layers);
    let region = grid.bounds();
    let mut hit = vec![false; region.size.len()];
    for (i, o) in scene.objects(layer).iter().enumerate() {
        if !filter.test(&scene, layer, Subject::Object(i)) {
            continue;
        }
        let b = o.bbox;
        let (x1, y1) = (
            i64::from(b.origin.x) + i64::from(b.size.width) - 1,
            i64::from(b.origin.y) + i64::from(b.size.height) - 1,
        );
        for c in b.iter() {
            let Some(k) = region.index_of(c) else {
                continue;
            };
            let (x, y) = (i64::from(c.x), i64::from(c.y));
            let ring =
                x == i64::from(b.origin.x) || x == x1 || y == i64::from(b.origin.y) || y == y1;
            hit[k] |= match part {
                Part::Any => true,
                Part::Wall => ring,
                Part::Inside => !ring,
            };
        }
    }
    let mut w = MaskWriter::new(region);
    for h in hit {
        w.push(h);
    }
    Ok(w.finish())
}

fn window(
    grid: &Grid,
    layer: LayerId,
    tiles: &[TileId],
    radius: Radius,
    min: u64,
    max: u64,
) -> Result<Prepared, ConditionError> {
    Ok(Prepared::Window {
        sums: PrefixSums::from_layer(grid.tiles(layer)?, &TileSet::new(tiles))?,
        radius,
        min,
        max,
    })
}

fn prepare_all(cs: &[Compiled], grid: &Grid) -> Result<Vec<Prepared>, ConditionError> {
    cs.iter().map(|c| prepare(c, grid)).collect()
}

fn compile(c: &Condition, grid: &Grid, tiles: &TileRegistry) -> Result<Compiled, ConditionError> {
    Ok(match c {
        Condition::TileIs { layer, tile } => Compiled::TileIs {
            layer: tile_layer(grid, layer)?,
            tiles: resolve_tiles(tile, tiles)?,
        },
        Condition::NearType {
            layer,
            tile,
            distance,
        } => Compiled::NearType {
            layer: tile_layer(grid, layer)?,
            tiles: resolve_tiles(tile, tiles)?,
            radius: radius(*distance, grid),
        },
        Condition::NotNearType {
            layer,
            tile,
            distance,
        } => Compiled::NotNearType {
            layer: tile_layer(grid, layer)?,
            tiles: resolve_tiles(tile, tiles)?,
            radius: radius(*distance, grid),
        },
        Condition::AreaIs {
            layer,
            tile,
            distance,
        } => Compiled::AreaIs {
            layer: tile_layer(grid, layer)?,
            tiles: resolve_tiles(tile, tiles)?,
            radius: radius(*distance, grid),
        },
        Condition::Count {
            layer,
            tile,
            distance,
            min,
            max,
        } => {
            let (min, max) = match (*min, *max) {
                (None, None) => return Err(ConditionError::CountWithNoBounds),
                (Some(min), Some(max)) if min > max => {
                    return Err(ConditionError::EmptyCountRange { min, max });
                }
                (min, max) => (min.unwrap_or(0), max.unwrap_or(u32::MAX)),
            };
            Compiled::Count {
                layer: tile_layer(grid, layer)?,
                tiles: resolve_tiles(tile, tiles)?,
                radius: radius(*distance, grid),
                min,
                max,
            }
        }
        Condition::ScalarInRange { layer, min, max } => {
            let (min, max) = match (*min, *max) {
                (None, None) => return Err(ConditionError::RangeWithNoBounds),
                (Some(min), Some(max)) if min >= max => {
                    return Err(ConditionError::EmptyScalarRange { min, max });
                }
                (min, max) => (
                    min.map_or(f32::NEG_INFINITY, |v| v as f32),
                    max.map_or(f32::INFINITY, |v| v as f32),
                ),
            };
            Compiled::ScalarInRange {
                layer: kind_layer(grid, layer, LayerKind::Scalar)?,
                min,
                max,
            }
        }
        Condition::EdgeDistance { min, max } => match (*min, *max) {
            (None, None) => return Err(ConditionError::EdgeWithNoBounds),
            (Some(min), Some(max)) if min > max => {
                return Err(ConditionError::EmptyEdgeRange { min, max });
            }
            (min, max) => Compiled::EdgeDistance {
                min: min.unwrap_or(0),
                max: max.unwrap_or(u32::MAX),
            },
        },
        Condition::InObject {
            layer,
            part,
            filter,
        } => {
            let id = kind_layer(grid, layer, LayerKind::Objects)?;
            Compiled::InObject {
                layer: id,
                part: *part,
                filter: CompiledFilter::compile(filter, grid, id).map_err(|e| match e {
                    FilterError::Grid(e) => ConditionError::Grid(e),
                    e => ConditionError::Filter(e),
                })?,
            }
        }
        Condition::Any { conditions } => Compiled::Any(compile_all(conditions, grid, tiles)?),
        Condition::All { conditions } => Compiled::All(compile_all(conditions, grid, tiles)?),
        Condition::Not { condition } => Compiled::Not(Box::new(compile(condition, grid, tiles)?)),
    })
}

/// A layer a tile condition reads: one of tiles, or the build fails naming it.
fn tile_layer(grid: &Grid, name: &str) -> Result<LayerId, ConditionError> {
    kind_layer(grid, name, LayerKind::Tiles)
}

fn kind_layer(grid: &Grid, name: &str, kind: LayerKind) -> Result<LayerId, ConditionError> {
    let id = grid.layer_id(name)?;
    let found = grid.kind(id)?;
    if found != kind {
        return Err(GridError::WrongLayerKind {
            name: name.to_string(),
            expected: kind,
            found,
        }
        .into());
    }
    Ok(id)
}

fn compile_all(
    conditions: &[Condition],
    grid: &Grid,
    tiles: &TileRegistry,
) -> Result<Vec<Compiled>, ConditionError> {
    conditions.iter().map(|c| compile(c, grid, tiles)).collect()
}

fn resolve_tiles(spec: &TileSpec, tiles: &TileRegistry) -> Result<Vec<TileId>, ConditionError> {
    let mut ids = spec
        .names()
        .iter()
        .map(|name| tiles.id(name))
        .collect::<Result<Vec<_>, _>>()?;
    ids.sort_unstable();
    ids.dedup();
    Ok(ids)
}

fn radius(distance: u32, grid: &Grid) -> Radius {
    let z = if grid.bounds().size.is_2d() {
        0
    } else {
        distance
    };
    Radius {
        x: distance,
        y: distance,
        z,
    }
}

#[cfg(test)]
mod tests;
