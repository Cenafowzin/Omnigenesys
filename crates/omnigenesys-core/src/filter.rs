//! Object filters: which objects a step picks, or which empty slots of a
//! lattice (desenho-pipelines §3.2; pesquisa 2026-10-25-operadores-de-
//! objetos). The object side of a condition: a list is an implicit `all`,
//! and nothing here compares with a fixed name — kinds, props and roles
//! are the file's own words.
//!
//! Measures that do not change while a step runs (hops from the start,
//! connections per room) are not filters: `MeasureObjects` writes them as
//! props, and filters read props. What does change while a step places
//! objects — how many objects are around a slot — is a filter
//! (`neighbours`), so each pick sees the picks before it.
//!
//! As conditions, filters run in two phases: compiled once per step
//! ([`CompiledFilter`]), then prepared when the step starts
//! ([`PreparedFilter`]), which works out the cells a `cells` filter looks
//! at. Only a prepared filter tests, so no step can forget the cells.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    bounds::Bounds,
    condition::{CompiledCondition, Condition, ConditionError},
    grid::{Grid, GridError, LayerId},
    layer::LayerKind,
    mask::CellMask,
    object::{Lattice, Object, ObjectLayer, Value},
    tile::TileRegistry,
};

/// One name or several.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged, expecting = "a name or a list of names")]
pub enum Names {
    One(String),
    Many(Vec<String>),
}

impl Names {
    pub fn names(&self) -> &[String] {
        match self {
            Names::One(n) => std::slice::from_ref(n),
            Names::Many(ns) => ns,
        }
    }
}

/// A prop value as a file writes it: text, a whole number, a number, or
/// true/false.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged, expecting = "text, a number or true/false")]
pub enum Scalar {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

impl Scalar {
    pub fn to_value(&self) -> Value {
        match self {
            Scalar::Bool(b) => Value::Bool(*b),
            Scalar::Int(n) => Value::Int(*n),
            Scalar::Float(x) => Value::Float(*x),
            Scalar::Str(s) => Value::Str(s.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged, expecting = "a value or a list of values")]
pub enum Scalars {
    One(Scalar),
    Many(Vec<Scalar>),
}

impl Scalars {
    fn values(&self) -> Vec<Value> {
        match self {
            Scalars::One(v) => vec![v.to_value()],
            Scalars::Many(vs) => vs.iter().map(Scalar::to_value).collect(),
        }
    }
}

/// A whole number, or every whole number from `min` to `max` (both
/// counted; either may be left out).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    untagged,
    expecting = "a whole number, or {\"min\": ..., \"max\": ...}"
)]
pub enum IntRange {
    Exact(i64),
    Range(Between),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Between {
    #[serde(default)]
    pub min: Option<i64>,
    #[serde(default)]
    pub max: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ObjectFilter {
    /// Its kind is one of these.
    KindIs {
        kind: Names,
    },
    /// The prop is set to one of these values (3 and 3.0 are the same).
    PropIs {
        name: String,
        value: Scalars,
    },
    /// The prop is set, to anything.
    HasProp {
        name: String,
    },
    /// The prop is a number from `min` to `max` (both counted).
    PropInRange {
        name: String,
        #[serde(default)]
        min: Option<f64>,
        #[serde(default)]
        max: Option<f64>,
    },
    /// How many objects of the same layer are side by side with it
    /// (sharing part of an edge; a corner is not enough) and pass `where`:
    /// from `min` to `max`, both counted. For an empty slot, the objects
    /// around the slot. A dead end is `max: 1`.
    Neighbours {
        #[serde(default)]
        min: Option<u32>,
        #[serde(default)]
        max: Option<u32>,
        #[serde(rename = "where", default)]
        filter: Vec<ObjectFilter>,
    },
    /// How many objects of the same layer its box overlaps once grown by
    /// `margin` cells on every side, and pass `where`: from `min` to `max`,
    /// both counted. Rooms with at least one cell of rock between their
    /// walls: `margin: 1, max: 0` (Brogue's `roomFitsAt`).
    Overlaps {
        #[serde(default)]
        margin: u32,
        #[serde(default)]
        min: Option<u32>,
        #[serde(default)]
        max: Option<u32>,
        #[serde(rename = "where", default)]
        filter: Vec<ObjectFilter>,
    },
    /// Its place on the layer's lattice: column `x` and row `y`, from 0
    /// (for an object that covers several slots, its first, row by row).
    Slot {
        #[serde(default)]
        x: Option<IntRange>,
        #[serde(default)]
        y: Option<IntRange>,
    },
    /// One of its props `props` names an object of `layer` that passes
    /// `where`: a connection into a secret room, a key and its lock.
    RefersTo {
        props: Names,
        layer: String,
        #[serde(rename = "where", default)]
        filter: Vec<ObjectFilter>,
    },
    /// How much of its box lies on cells where `condition` holds, as a
    /// share of the box's cells from `min` to `max` (both counted, 0 to 1):
    /// a house all on grass is `min: 1`, a tree off the water `max: 0`, a
    /// camp mostly in the forest `min: 0.6`. For an empty slot or a place
    /// drawn for a new object, the cells it would cover. The cells as they
    /// were when the step started; past the map's edge a cell does not
    /// hold. Not inside a condition's `in_object` (there a step decides
    /// a chunk at a time, and a box can reach past it): tag the objects
    /// with `TagObjects` first, and pick them by the tag.
    Cells {
        condition: Box<Condition>,
        #[serde(default)]
        min: Option<f64>,
        #[serde(default)]
        max: Option<f64>,
    },
    Any {
        filters: Vec<ObjectFilter>,
    },
    Not {
        filter: Box<ObjectFilter>,
    },
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum FilterError {
    #[error(transparent)]
    Grid(#[from] GridError),
    #[error("`slot` needs a layer with a lattice; '{layer}' has none")]
    NoLattice { layer: String },
    #[error("{what} needs at least one of `min` or `max`")]
    NoBounds { what: &'static str },
    #[error("{what} range is empty: min {min} is above max {max}")]
    EmptyRange {
        what: &'static str,
        min: String,
        max: String,
    },
    #[error("prop_is needs at least one value")]
    NoValues,
    #[error("`any` needs at least one filter")]
    EmptyAny,
    #[error("cells: {0}")]
    Condition(Box<ConditionError>),
    #[error("cells: `{bound}` is {value}, not between 0 and 1 (a share of the box)")]
    ShareOutOfRange { bound: &'static str, value: f64 },
    #[error(
        "`cells` cannot be in a condition's `in_object`: tag the objects with TagObjects, \
         and pick them by the tag"
    )]
    CellsInCondition,
}

/// Filters resolved against a grid: layer names became ids, ranges were
/// checked. Built once per step.
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledFilter {
    all: Vec<F>,
    /// The conditions of its `cells` filters; `F::Cells` holds a place here.
    cells: Vec<CompiledCondition>,
}

/// A [`CompiledFilter`] with the cells its `cells` filters look at worked
/// out, on the grid as it was then. What tests.
#[derive(Debug, Clone)]
pub struct PreparedFilter<'f> {
    all: &'f [F],
    cells: Vec<CellMask>,
}

#[derive(Debug, Clone, PartialEq)]
enum F {
    Kind(Vec<String>),
    PropIs(String, Vec<Value>),
    HasProp(String),
    PropRange(String, f64, f64),
    Neighbours {
        min: u32,
        max: u32,
        filter: Vec<F>,
    },
    Overlaps {
        margin: u32,
        min: u32,
        max: u32,
        filter: Vec<F>,
    },
    Slot {
        x: (i64, i64),
        y: (i64, i64),
    },
    RefersTo {
        props: Vec<String>,
        layer: LayerId,
        filter: Vec<F>,
    },
    Cells {
        condition: usize,
        min: f64,
        max: f64,
    },
    Any(Vec<F>),
    Not(Box<F>),
}

impl CompiledFilter {
    /// `filters` (an implicit `all`) for objects of `layer`.
    pub fn compile(
        filters: &[ObjectFilter],
        grid: &Grid,
        tiles: &TileRegistry,
        layer: LayerId,
    ) -> Result<CompiledFilter, FilterError> {
        let mut c = Compiler {
            grid,
            tiles,
            cells: Vec::new(),
        };
        let all = c.all(filters, layer)?;
        Ok(CompiledFilter {
            all,
            cells: c.cells,
        })
    }

    /// The conditions its `cells` filters test, for the step to declare
    /// what they read.
    pub fn conditions(&self) -> &[CompiledCondition] {
        &self.cells
    }

    /// Works out the cells of its `cells` filters on `grid` as it is now.
    pub fn prepare(&self, grid: &Grid) -> Result<PreparedFilter<'_>, ConditionError> {
        let cells = self
            .cells
            .iter()
            .map(|c| Ok(c.prepare(grid)?.eval(grid.bounds())))
            .collect::<Result<_, ConditionError>>()?;
        Ok(PreparedFilter {
            all: &self.all,
            cells,
        })
    }

    /// No filter: everything passes.
    pub fn is_empty(&self) -> bool {
        self.all.is_empty()
    }

    /// The other object layers it looks into (`refers_to`).
    pub fn layers(&self) -> Vec<LayerId> {
        let mut out = Vec::new();
        layers(&self.all, &mut out);
        out
    }
}

impl PreparedFilter<'_> {
    /// Whether `subject`, of `layer`, passes.
    pub fn test(&self, scene: &Scene, layer: LayerId, subject: Subject) -> bool {
        self.all
            .iter()
            .all(|f| test(f, &self.cells, scene, layer, subject))
    }
}

fn layers(fs: &[F], out: &mut Vec<LayerId>) {
    for f in fs {
        match f {
            F::Neighbours { filter, .. } | F::Overlaps { filter, .. } => layers(filter, out),
            F::RefersTo { layer, filter, .. } => {
                if !out.contains(layer) {
                    out.push(*layer);
                }
                layers(filter, out);
            }
            F::Any(fs) => layers(fs, out),
            F::Not(f) => layers(std::slice::from_ref(f), out),
            _ => {}
        }
    }
}

struct Compiler<'a> {
    grid: &'a Grid,
    tiles: &'a TileRegistry,
    cells: Vec<CompiledCondition>,
}

impl Compiler<'_> {
    fn all(&mut self, fs: &[ObjectFilter], layer: LayerId) -> Result<Vec<F>, FilterError> {
        fs.iter().map(|f| self.one(f, layer)).collect()
    }

    fn one(&mut self, f: &ObjectFilter, layer: LayerId) -> Result<F, FilterError> {
        let grid = self.grid;
        Ok(match f {
            ObjectFilter::KindIs { kind } => F::Kind(kind.names().to_vec()),
            ObjectFilter::PropIs { name, value } => {
                let values = value.values();
                if values.is_empty() {
                    return Err(FilterError::NoValues);
                }
                F::PropIs(name.clone(), values)
            }
            ObjectFilter::HasProp { name } => F::HasProp(name.clone()),
            ObjectFilter::PropInRange { name, min, max } => {
                let (min, max) = match (*min, *max) {
                    (None, None) => {
                        return Err(FilterError::NoBounds {
                            what: "prop_in_range",
                        });
                    }
                    (Some(a), Some(b)) if a > b => {
                        return Err(FilterError::EmptyRange {
                            what: "prop_in_range",
                            min: a.to_string(),
                            max: b.to_string(),
                        });
                    }
                    (min, max) => (
                        min.unwrap_or(f64::NEG_INFINITY),
                        max.unwrap_or(f64::INFINITY),
                    ),
                };
                F::PropRange(name.clone(), min, max)
            }
            ObjectFilter::Neighbours { min, max, filter } => {
                let (min, max) = count_range("neighbours", *min, *max)?;
                F::Neighbours {
                    min,
                    max,
                    filter: self.all(filter, layer)?,
                }
            }
            ObjectFilter::Overlaps {
                margin,
                min,
                max,
                filter,
            } => {
                let (min, max) = count_range("overlaps", *min, *max)?;
                F::Overlaps {
                    margin: *margin,
                    min,
                    max,
                    filter: self.all(filter, layer)?,
                }
            }
            ObjectFilter::Slot { x, y } => {
                let objects = grid.objects(layer)?;
                if objects.lattice().is_none() {
                    return Err(FilterError::NoLattice {
                        layer: grid.layer_name(layer).unwrap_or("?").to_string(),
                    });
                }
                let range = |r: &Option<IntRange>| -> Result<(i64, i64), FilterError> {
                    match r {
                        None => Ok((i64::MIN, i64::MAX)),
                        Some(IntRange::Exact(n)) => Ok((*n, *n)),
                        Some(IntRange::Range(Between { min, max })) => match (*min, *max) {
                            (None, None) => Err(FilterError::NoBounds { what: "slot" }),
                            (Some(a), Some(b)) if a > b => Err(FilterError::EmptyRange {
                                what: "slot",
                                min: a.to_string(),
                                max: b.to_string(),
                            }),
                            (a, b) => Ok((a.unwrap_or(i64::MIN), b.unwrap_or(i64::MAX))),
                        },
                    }
                };
                F::Slot {
                    x: range(x)?,
                    y: range(y)?,
                }
            }
            ObjectFilter::RefersTo {
                props,
                layer: target,
                filter,
            } => {
                let id = grid.layer_id(target)?;
                let found = grid.kind(id)?;
                if found != LayerKind::Objects {
                    return Err(GridError::WrongLayerKind {
                        name: target.clone(),
                        expected: LayerKind::Objects,
                        found,
                    }
                    .into());
                }
                F::RefersTo {
                    props: props.names().to_vec(),
                    layer: id,
                    filter: self.all(filter, id)?,
                }
            }
            ObjectFilter::Any { filters } => {
                if filters.is_empty() {
                    return Err(FilterError::EmptyAny);
                }
                F::Any(self.all(filters, layer)?)
            }
            ObjectFilter::Not { filter } => F::Not(Box::new(self.one(filter, layer)?)),
            ObjectFilter::Cells {
                condition,
                min,
                max,
            } => {
                for (bound, v) in [("min", min), ("max", max)] {
                    if let Some(v) = *v
                        && !(0.0..=1.0).contains(&v)
                    {
                        return Err(FilterError::ShareOutOfRange { bound, value: v });
                    }
                }
                let (min, max) = match (*min, *max) {
                    (None, None) => return Err(FilterError::NoBounds { what: "cells" }),
                    (Some(a), Some(b)) if a > b => {
                        return Err(FilterError::EmptyRange {
                            what: "cells",
                            min: a.to_string(),
                            max: b.to_string(),
                        });
                    }
                    (min, max) => (min.unwrap_or(0.0), max.unwrap_or(1.0)),
                };
                let c = condition
                    .compile(grid, self.tiles)
                    .map_err(|e| FilterError::Condition(Box::new(e)))?;
                self.cells.push(c);
                F::Cells {
                    condition: self.cells.len() - 1,
                    min,
                    max,
                }
            }
        })
    }
}

/// A count's range, both ends counted; at least one must be given.
fn count_range(
    what: &'static str,
    min: Option<u32>,
    max: Option<u32>,
) -> Result<(u32, u32), FilterError> {
    match (min, max) {
        (None, None) => Err(FilterError::NoBounds { what }),
        (Some(a), Some(b)) if a > b => Err(FilterError::EmptyRange {
            what,
            min: a.to_string(),
            max: b.to_string(),
        }),
        (min, max) => Ok((min.unwrap_or(0), max.unwrap_or(u32::MAX))),
    }
}

/// What a filter is asked about: an object of the layer (by its place in
/// the layer's list), an empty slot of its lattice, or a box no object
/// fills yet (a place drawn for a new object, on a layer without one).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subject {
    Object(usize),
    Slot(usize),
    Box(Bounds),
}

fn test(f: &F, cells: &[CellMask], scene: &Scene, layer: LayerId, subject: Subject) -> bool {
    let object = || match subject {
        Subject::Object(i) => scene.objects(layer).get(i),
        Subject::Slot(_) | Subject::Box(_) => None,
    };
    let within =
        |n: usize, min: u32, max: u32| u32::try_from(n).is_ok_and(|n| (min..=max).contains(&n));
    let passes = |filter: &[F], j: usize| {
        filter
            .iter()
            .all(|f| test(f, cells, scene, layer, Subject::Object(j)))
    };
    match f {
        F::Kind(kinds) => object().is_some_and(|o| kinds.contains(&o.kind)),
        F::PropIs(name, values) => object()
            .and_then(|o| o.prop(name))
            .is_some_and(|v| values.iter().any(|w| same(v, w))),
        F::HasProp(name) => object().is_some_and(|o| o.prop(name).is_some()),
        F::PropRange(name, min, max) => object()
            .and_then(|o| number(o.prop(name)?))
            .is_some_and(|x| *min <= x && x <= *max),
        F::Neighbours { min, max, filter } => {
            let n = scene
                .neighbours(layer, subject)
                .filter(|&j| passes(filter, j))
                .count();
            within(n, *min, *max)
        }
        F::Overlaps {
            margin,
            min,
            max,
            filter,
        } => {
            let n = scene
                .overlapping(layer, subject, *margin)
                .filter(|&j| passes(filter, j))
                .count();
            within(n, *min, *max)
        }
        F::Slot { x, y } => scene.slot_of(layer, subject).is_some_and(|(c, r)| {
            (x.0..=x.1).contains(&i64::from(c)) && (y.0..=y.1).contains(&i64::from(r))
        }),
        F::RefersTo {
            props,
            layer: target,
            filter,
        } => object().is_some_and(|o| {
            props
                .iter()
                .filter_map(|p| o.prop(p)?.as_object())
                .any(|id| {
                    scene
                        .objects(*target)
                        .iter()
                        .position(|t| t.id == id)
                        .is_some_and(|j| {
                            filter
                                .iter()
                                .all(|f| test(f, cells, scene, *target, Subject::Object(j)))
                        })
                })
        }),
        F::Cells {
            condition,
            min,
            max,
        } => scene.bounds(layer, subject).is_some_and(|b| {
            let holds = &cells[*condition];
            let n = b.iter().filter(|&c| holds.get(c)).count();
            let share = n as f64 / b.size.len() as f64;
            *min <= share && share <= *max
        }),
        F::Any(fs) => fs.iter().any(|f| test(f, cells, scene, layer, subject)),
        F::Not(f) => !test(f, cells, scene, layer, subject),
    }
}

/// Equal as props: numbers by value, whatever their type.
fn same(a: &Value, b: &Value) -> bool {
    match (number(a), number(b)) {
        (Some(x), Some(y)) => x == y,
        _ => a == b,
    }
}

pub(crate) fn number(v: &Value) -> Option<f64> {
    match v {
        Value::Int(n) => Some(*n as f64),
        Value::Float(x) => Some(*x),
        _ => None,
    }
}

/// Whether two boxes are side by side: they do not overlap, and they
/// share part of an edge (not only a corner). In x and y; z must overlap.
pub fn touching(a: Bounds, b: Bounds) -> bool {
    let span = |o: i32, n: u32| (i64::from(o), i64::from(o) + i64::from(n));
    let (ax, bx) = (
        span(a.origin.x, a.size.width),
        span(b.origin.x, b.size.width),
    );
    let (ay, by) = (
        span(a.origin.y, a.size.height),
        span(b.origin.y, b.size.height),
    );
    let (az, bz) = (
        span(a.origin.z, a.size.depth),
        span(b.origin.z, b.size.depth),
    );
    let overlap = |p: (i64, i64), q: (i64, i64)| p.0 < q.1 && q.0 < p.1;
    let meet = |p: (i64, i64), q: (i64, i64)| p.1 == q.0 || q.1 == p.0;
    overlap(az, bz) && ((meet(ax, bx) && overlap(ay, by)) || (meet(ay, by) && overlap(ax, bx)))
}

/// Whether `a`, grown by `margin` cells on every side in x and y, overlaps
/// `b`. With `margin` 0, whether they share a cell. z must overlap.
pub fn overlapping(a: Bounds, b: Bounds, margin: u32) -> bool {
    let span = |o: i32, n: u32| (i64::from(o), i64::from(o) + i64::from(n));
    let grow = |(lo, hi): (i64, i64)| (lo - i64::from(margin), hi + i64::from(margin));
    let overlap = |p: (i64, i64), q: (i64, i64)| p.0 < q.1 && q.0 < p.1;
    overlap(
        grow(span(a.origin.x, a.size.width)),
        span(b.origin.x, b.size.width),
    ) && overlap(
        grow(span(a.origin.y, a.size.height)),
        span(b.origin.y, b.size.height),
    ) && overlap(
        span(a.origin.z, a.size.depth),
        span(b.origin.z, b.size.depth),
    )
}

/// The object layers one step looks at, copied when it starts. A step
/// that adds objects adds them here too, so its later picks see them (a
/// slot taken a moment ago is not empty any more).
#[derive(Debug, Clone)]
pub struct Scene {
    map: Bounds,
    layers: Vec<SceneLayer>,
}

#[derive(Debug, Clone)]
struct SceneLayer {
    id: LayerId,
    lattice: Option<Lattice>,
    objects: Vec<Object>,
    /// With a lattice: the object in each slot, by its place in `objects`.
    slots: Vec<Option<usize>>,
}

impl Scene {
    pub fn new<'a>(
        map: Bounds,
        layers: impl IntoIterator<Item = (LayerId, &'a ObjectLayer)>,
    ) -> Scene {
        let mut scene = Scene {
            map,
            layers: Vec::new(),
        };
        for (id, layer) in layers {
            if scene.layers.iter().any(|l| l.id == id) {
                continue;
            }
            let lattice = layer.lattice().copied();
            let mut entry = SceneLayer {
                id,
                lattice,
                objects: Vec::new(),
                slots: vec![None; lattice.map_or(0, |l| l.slots())],
            };
            for o in layer.iter() {
                entry.push(map, o.clone());
            }
            scene.layers.push(entry);
        }
        scene
    }

    fn layer(&self, id: LayerId) -> &SceneLayer {
        self.layers
            .iter()
            .find(|l| l.id == id)
            .expect("every layer a step filters is in its scene")
    }

    fn layer_mut(&mut self, id: LayerId) -> &mut SceneLayer {
        self.layers
            .iter_mut()
            .find(|l| l.id == id)
            .expect("every layer a step filters is in its scene")
    }

    pub fn map(&self) -> Bounds {
        self.map
    }

    pub fn objects(&self, layer: LayerId) -> &[Object] {
        &self.layer(layer).objects
    }

    pub fn lattice(&self, layer: LayerId) -> Option<&Lattice> {
        self.layer(layer).lattice.as_ref()
    }

    /// Adds an object; its index in [`objects`](Self::objects).
    pub fn push(&mut self, layer: LayerId, object: Object) -> usize {
        let map = self.map;
        self.layer_mut(layer).push(map, object)
    }

    pub fn set_prop(&mut self, layer: LayerId, i: usize, name: &str, value: Value) {
        self.layer_mut(layer).objects[i].set(name, value);
    }

    pub fn remove_prop(&mut self, layer: LayerId, i: usize, name: &str) {
        self.layer_mut(layer).objects[i].remove(name);
    }

    /// The object in slot `i`, if any.
    pub fn in_slot(&self, layer: LayerId, i: usize) -> Option<usize> {
        self.layer(layer).slots.get(i).copied().flatten()
    }

    /// The empty slots, row by row; none without a lattice.
    pub fn empty_slots(&self, layer: LayerId) -> Vec<usize> {
        let l = self.layer(layer);
        (0..l.slots.len())
            .filter(|&i| l.slots[i].is_none())
            .collect()
    }

    /// The cells `subject` covers.
    pub fn bounds(&self, layer: LayerId, subject: Subject) -> Option<Bounds> {
        let l = self.layer(layer);
        match subject {
            Subject::Object(i) => l.objects.get(i).map(|o| o.bbox),
            Subject::Slot(i) => l.lattice.and_then(|lat| {
                (i < lat.slots()).then(|| {
                    let (c, r) = lat.slot(i);
                    lat.bounds(self.map, c, r)
                })
            }),
            Subject::Box(b) => Some(b),
        }
    }

    /// Column and row of `subject` on the lattice.
    pub fn slot_of(&self, layer: LayerId, subject: Subject) -> Option<(u32, u32)> {
        let l = self.layer(layer);
        let lat = l.lattice?;
        let i = match subject {
            Subject::Slot(i) => i,
            Subject::Object(i) => *lat.footprint(self.map, l.objects.get(i)?.bbox).first()?,
            Subject::Box(b) => *lat.footprint(self.map, b).first()?,
        };
        Some(lat.slot(i))
    }

    /// The objects side by side with `subject`, in the layer's order.
    pub fn neighbours(&self, layer: LayerId, subject: Subject) -> impl Iterator<Item = usize> + '_ {
        let b = self.bounds(layer, subject);
        let objects = &self.layer(layer).objects;
        (0..objects.len()).filter(move |&j| {
            subject != Subject::Object(j) && b.is_some_and(|b| touching(b, objects[j].bbox))
        })
    }

    /// The objects `subject`'s box overlaps once grown by `margin`, in the
    /// layer's order; not `subject` itself.
    pub fn overlapping(
        &self,
        layer: LayerId,
        subject: Subject,
        margin: u32,
    ) -> impl Iterator<Item = usize> + '_ {
        let b = self.bounds(layer, subject);
        let objects = &self.layer(layer).objects;
        (0..objects.len()).filter(move |&j| {
            subject != Subject::Object(j)
                && b.is_some_and(|b| overlapping(b, objects[j].bbox, margin))
        })
    }
}

impl SceneLayer {
    fn push(&mut self, map: Bounds, object: Object) -> usize {
        let i = self.objects.len();
        if let Some(lat) = self.lattice {
            for s in lat.footprint(map, object.bbox) {
                self.slots[s].get_or_insert(i);
            }
        }
        self.objects.push(object);
        i
    }
}

#[cfg(test)]
mod tests;
