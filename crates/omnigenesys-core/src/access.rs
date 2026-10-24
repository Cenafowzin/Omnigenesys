//! What a step reads and writes, declared at build time (PLANNING §2.7).
//!
//! The declaration is the single source of chunk padding (Part 2): a step
//! that reads `terrain` 5 cells around each cell makes the step that writes
//! `terrain` generate 5 more cells.

use crate::grid::LayerId;
use crate::operator::Scope;

/// How far an access reaches around each cell, per axis and per side.
/// `below` points toward negative coordinates, `above` toward positive ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Reach {
    pub below: [u32; 3],
    pub above: [u32; 3],
}

impl Reach {
    /// The cell itself and nothing around it.
    pub const ZERO: Reach = Reach {
        below: [0; 3],
        above: [0; 3],
    };

    /// `r` cells on every side of every axis.
    pub const fn uniform(r: u32) -> Reach {
        Reach::symmetric(r, r, r)
    }

    /// `x`, `y` and `z` cells on both sides of each axis.
    pub const fn symmetric(x: u32, y: u32, z: u32) -> Reach {
        Reach {
            below: [x, y, z],
            above: [x, y, z],
        }
    }

    pub fn is_zero(&self) -> bool {
        *self == Reach::ZERO
    }

    /// The smallest reach covering both.
    #[must_use]
    pub fn union(self, other: Reach) -> Reach {
        let max = |a: [u32; 3], b: [u32; 3]| [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])];
        Reach {
            below: max(self.below, other.below),
            above: max(self.above, other.above),
        }
    }

    /// A 2D map has no z neighbourhood, whatever the operator asked for.
    /// Same rule as condition distances (PLANNING §2.6): dimensionality is a
    /// property of the map.
    pub(crate) fn flatten_z(self) -> Reach {
        Reach {
            below: [self.below[0], self.below[1], 0],
            above: [self.above[0], self.above[1], 0],
        }
    }
}

/// How one step uses one layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayerAccess {
    pub layer: LayerId,
    /// Read live while the step runs, through a [`ReadHandle`].
    pub read: Option<Reach>,
    /// Read as the step found it, so it never sees the step's own writes:
    /// through conditions (a snapshot taken when the step starts), or by an
    /// operator that updates the layer synchronously
    /// ([`BuildContext::read_write`](crate::operator::BuildContext::read_write)).
    pub snapshot_read: Option<Reach>,
    /// Written, and how far past the cell (a stamp that crosses a border).
    pub write: Option<Reach>,
}

/// Everything one step reads and writes. Built by the
/// [`BuildContext`](crate::operator::BuildContext) as the operator asks for
/// layers, so it cannot disagree with what the operator does.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Access {
    layers: Vec<LayerAccess>,
    scope: Scope,
}

impl Access {
    pub(crate) fn with_scope(scope: Scope) -> Access {
        Access {
            layers: Vec::new(),
            scope,
        }
    }

    /// Whether the step can run on a chunk (`Local`) or needs the whole
    /// map (`Global`).
    pub fn scope(&self) -> Scope {
        self.scope
    }

    pub(crate) fn set_scope(&mut self, scope: Scope) {
        self.scope = scope;
    }

    pub fn layers(&self) -> &[LayerAccess] {
        &self.layers
    }

    /// Reach of every read of `layer`, live or through a condition: what
    /// chunk padding has to cover.
    pub fn read_reach(&self, layer: LayerId) -> Option<Reach> {
        let a = self.get(layer)?;
        match (a.read, a.snapshot_read) {
            (Some(r), Some(c)) => Some(r.union(c)),
            (r, c) => r.or(c),
        }
    }

    pub fn writes(&self, layer: LayerId) -> bool {
        self.get(layer).is_some_and(|a| a.write.is_some())
    }

    pub(crate) fn reads_live(&self, layer: LayerId) -> bool {
        self.get(layer).is_some_and(|a| a.read.is_some())
    }

    fn get(&self, layer: LayerId) -> Option<&LayerAccess> {
        self.layers.iter().find(|a| a.layer == layer)
    }

    fn entry(&mut self, layer: LayerId) -> &mut LayerAccess {
        let i = match self.layers.iter().position(|a| a.layer == layer) {
            Some(i) => i,
            None => {
                self.layers.push(LayerAccess {
                    layer,
                    read: None,
                    snapshot_read: None,
                    write: None,
                });
                self.layers.len() - 1
            }
        };
        &mut self.layers[i]
    }

    pub(crate) fn add_read(&mut self, layer: LayerId, reach: Reach) {
        let e = self.entry(layer);
        e.read = Some(e.read.map_or(reach, |r| r.union(reach)));
    }

    pub(crate) fn add_snapshot_read(&mut self, layer: LayerId, reach: Reach) {
        let e = self.entry(layer);
        e.snapshot_read = Some(e.snapshot_read.map_or(reach, |r| r.union(reach)));
    }

    pub(crate) fn add_write(&mut self, layer: LayerId, reach: Reach) {
        let e = self.entry(layer);
        e.write = Some(e.write.map_or(reach, |r| r.union(reach)));
    }

    /// A layer read live with a neighbourhood and written in the same step:
    /// the result would depend on iteration order (PLANNING §2.7, item 15).
    pub(crate) fn read_write_conflict(&self) -> Option<LayerId> {
        self.layers
            .iter()
            .find(|a| a.write.is_some() && a.read.is_some_and(|r| !r.is_zero()))
            .map(|a| a.layer)
    }
}

/// Permission to read a layer, obtained from
/// [`BuildContext::read`](crate::operator::BuildContext::read).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadHandle {
    pub(crate) layer: LayerId,
}

/// Permission to write a layer, obtained from
/// [`BuildContext::write`](crate::operator::BuildContext::write).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteHandle {
    pub(crate) layer: LayerId,
}
