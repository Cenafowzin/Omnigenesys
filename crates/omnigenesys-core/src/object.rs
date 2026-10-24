//! Objects: things with a place and a kind but no value per cell — rooms,
//! the connections between them, later props and spawn points (PLANNING
//! §2.12). They live in object layers, next to tile and scalar layers, so
//! one step can make them and later steps read them; the map export carries
//! them to the engines.

use crate::bounds::Bounds;
use crate::coords::Coord;

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
}

/// The objects of one layer, in the order they were made.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectLayer {
    /// The map it belongs to; objects may reach past it (a room cut by the
    /// edge), so it does not bound them.
    bounds: Bounds,
    items: Vec<Object>,
}

impl ObjectLayer {
    pub fn new(bounds: Bounds) -> ObjectLayer {
        ObjectLayer {
            bounds,
            items: Vec::new(),
        }
    }

    pub fn bounds(&self) -> Bounds {
        self.bounds
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
