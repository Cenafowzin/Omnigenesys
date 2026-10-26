//! Objects: things with a place and a kind but no value per cell — rooms,
//! the connections between them, later props and spawn points (PLANNING
//! §2.12). They live in object layers, next to tile and scalar layers, so
//! one step can make them and later steps read them; the map export carries
//! them to the engines.

use schemars::JsonSchema;
use serde::Deserialize;

use crate::bounds::Bounds;
use crate::coords::{Coord, GridSize};

/// An object's identity: from a counter per map, never its position in a
/// list (which changes when objects are removed). Steps make objects in a
/// fixed order, so ids are the same on every run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectId(pub(crate) u32);

impl ObjectId {
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// A property's value: a closed set of types, as Tiled and LDtk have, so
/// an adapter can fill an engine field with the right type (48 stays an
/// integer, a reference stays a reference).
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Cell(Coord),
    /// Another object, by id.
    Object(ObjectId),
    List(Vec<Value>),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<ObjectId> {
        match self {
            Value::Object(id) => Some(*id),
            _ => None,
        }
    }

    pub fn as_cell(&self) -> Option<Coord> {
        match self {
            Value::Cell(c) => Some(*c),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(n) => Some(*n),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Object {
    pub id: ObjectId,
    /// What it is ("room", "connection"): what an adapter dispatches on.
    pub kind: String,
    /// Where it is, in cells.
    pub bbox: Bounds,
    /// In the order they were set: the export writes them in that order.
    pub props: Vec<(String, Value)>,
}

impl Object {
    pub fn prop(&self, name: &str) -> Option<&Value> {
        self.props.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }

    /// Sets `name`, replacing it in place if it was set.
    pub fn set(&mut self, name: &str, value: Value) {
        match self.props.iter_mut().find(|(n, _)| n == name) {
            Some((_, v)) => *v = value,
            None => self.props.push((name.to_string(), value)),
        }
    }

    /// Unsets `name`, if it was set.
    pub fn remove(&mut self, name: &str) {
        self.props.retain(|(n, _)| n != name);
    }
}

/// A coarse grid of slots an object layer declares once (PLANNING §2.12;
/// pesquisa 2026-10-25-operadores-de-objetos, item 1): Isaac's 9 × 8 rooms,
/// Spelunky's 4 × 4, Unexplored's 5 × 5. Steps that place objects on the
/// layer pick among its slots; every step agrees on where they are, because
/// none of them repeats the numbers. 2D: a slot spans the map's depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Lattice {
    pub columns: u32,
    pub rows: u32,
    /// The size of one slot, in cells.
    pub cell: CellSize,
    /// Where slot (0, 0) starts, in cells from the map's origin.
    #[serde(default)]
    pub origin: Offset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CellSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Offset {
    pub x: i32,
    pub y: i32,
}

/// The most slots a lattice may have: candidates are listed one by one.
pub const MAX_SLOTS: u32 = 4096;

impl Lattice {
    /// Why it cannot be used, if it cannot.
    pub fn problem(&self) -> Option<String> {
        if self.columns == 0 || self.rows == 0 {
            return Some("a lattice needs at least 1 column and 1 row".into());
        }
        if self.cell.width == 0 || self.cell.height == 0 {
            return Some("a lattice cell is at least 1 × 1".into());
        }
        if u64::from(self.columns) * u64::from(self.rows) > u64::from(MAX_SLOTS) {
            return Some(format!(
                "{} × {} slots; at most {MAX_SLOTS}",
                self.columns, self.rows
            ));
        }
        let fits = |n: u32, size: u32, at: i32| {
            i64::from(at) + i64::from(n) * i64::from(size) <= i64::from(i32::MAX)
        };
        if !fits(self.columns, self.cell.width, self.origin.x)
            || !fits(self.rows, self.cell.height, self.origin.y)
        {
            return Some("the lattice reaches past the largest coordinate".into());
        }
        None
    }

    pub fn slots(&self) -> usize {
        (self.columns * self.rows) as usize
    }

    /// Slot `(column, row)`'s index, row by row.
    pub fn index(&self, column: u32, row: u32) -> usize {
        (row * self.columns + column) as usize
    }

    /// Slot `i`'s column and row.
    pub fn slot(&self, i: usize) -> (u32, u32) {
        let i = i as u32;
        (i % self.columns, i / self.columns)
    }

    /// The cells slot `(column, row)` covers, on a map starting at `map`.
    pub fn bounds(&self, map: Bounds, column: u32, row: u32) -> Bounds {
        Bounds::new(
            Coord::new(
                map.origin.x + self.origin.x + (column * self.cell.width) as i32,
                map.origin.y + self.origin.y + (row * self.cell.height) as i32,
                map.origin.z,
            ),
            GridSize::new(self.cell.width, self.cell.height, map.size.depth),
        )
    }

    /// The slots `bbox` overlaps, row by row (an object's footprint).
    pub fn footprint(&self, map: Bounds, bbox: Bounds) -> Vec<usize> {
        let (w, h) = (i64::from(self.cell.width), i64::from(self.cell.height));
        let x0 = i64::from(map.origin.x) + i64::from(self.origin.x);
        let y0 = i64::from(map.origin.y) + i64::from(self.origin.y);
        let lo = |a: i32, o: i64, s: i64| (i64::from(a) - o).div_euclid(s);
        let hi =
            |a: i32, n: u32, o: i64, s: i64| (i64::from(a) + i64::from(n) - 1 - o).div_euclid(s);
        if bbox.size.width == 0 || bbox.size.height == 0 {
            return Vec::new();
        }
        let (c0, c1) = (
            lo(bbox.origin.x, x0, w).max(0),
            hi(bbox.origin.x, bbox.size.width, x0, w).min(i64::from(self.columns) - 1),
        );
        let (r0, r1) = (
            lo(bbox.origin.y, y0, h).max(0),
            hi(bbox.origin.y, bbox.size.height, y0, h).min(i64::from(self.rows) - 1),
        );
        let mut out = Vec::new();
        for r in r0..=r1 {
            for c in c0..=c1 {
                out.push(self.index(c as u32, r as u32));
            }
        }
        out
    }
}

/// The objects of one layer, in the order they were made.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectLayer {
    /// The map it belongs to; objects may reach past it (a room cut by the
    /// edge), so it does not bound them.
    bounds: Bounds,
    lattice: Option<Lattice>,
    items: Vec<Object>,
}

impl ObjectLayer {
    pub fn new(bounds: Bounds) -> ObjectLayer {
        ObjectLayer {
            bounds,
            lattice: None,
            items: Vec::new(),
        }
    }

    pub fn with_lattice(bounds: Bounds, lattice: Lattice) -> ObjectLayer {
        ObjectLayer {
            lattice: Some(lattice),
            ..ObjectLayer::new(bounds)
        }
    }

    pub fn bounds(&self) -> Bounds {
        self.bounds
    }

    pub fn lattice(&self) -> Option<&Lattice> {
        self.lattice.as_ref()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Object> {
        self.items.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Object> {
        self.items.iter_mut()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn get(&self, id: ObjectId) -> Option<&Object> {
        self.items.iter().find(|o| o.id == id)
    }

    pub fn get_mut(&mut self, id: ObjectId) -> Option<&mut Object> {
        self.items.iter_mut().find(|o| o.id == id)
    }

    /// Adds an object with the next id of the map.
    pub fn push(
        &mut self,
        ids: &mut ObjectIds,
        kind: impl Into<String>,
        bbox: Bounds,
        props: Vec<(String, Value)>,
    ) -> ObjectId {
        let id = ids.fresh();
        self.items.push(Object {
            id,
            kind: kind.into(),
            bbox,
            props,
        });
        id
    }

    /// Adds an object made elsewhere with its id from [`ObjectIds`].
    pub(crate) fn insert(&mut self, object: Object) {
        self.items.push(object);
    }
}

/// The map's id counter, shared by all its object layers: an id names one
/// object in the whole map.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObjectIds {
    next: u32,
}

impl ObjectIds {
    /// # Panics
    ///
    /// After `u32::MAX` objects in one map.
    pub fn fresh(&mut self) -> ObjectId {
        self.next = self
            .next
            .checked_add(1)
            .expect("fewer than u32::MAX objects");
        ObjectId(self.next)
    }
}
