//! Operators: what a step does (PLANNING §2.2 and §2.7).
//!
//! An operator is built once per pipeline from its JSON config. Building is
//! where every name is resolved and every layer access is declared, through
//! the [`BuildContext`]. Running only gets what was declared, through the
//! [`Context`].

use std::collections::BTreeMap;

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::de::DeserializeOwned;
use thiserror::Error;

use crate::{
    access::{Access, Reach, ReadHandle, WriteHandle},
    bounds::Bounds,
    condition::{CompiledCondition, Condition, ConditionError, PreparedCondition},
    grid::{Grid, GridError, LayerId},
    layer::{Layer, OutOfBounds},
    rng::StepRng,
    tile::{TileError, TileId, TileRegistry},
};

/// Map dimensionality an operator supports, checked when the pipeline is built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dimensions {
    TwoD,
    ThreeD,
    Any,
}

impl Dimensions {
    pub fn accepts(self, is_2d: bool) -> bool {
        match self {
            Dimensions::TwoD => is_2d,
            Dimensions::ThreeD => !is_2d,
            Dimensions::Any => true,
        }
    }
}

/// Whether an operator decides cell by cell (`Local`: works on any chunk,
/// draws with `rng.at`) or from the whole map (`Global`: uses the stream).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Local,
    Global,
}

/// What runs. Built by an [`OperatorType`] (or a raw [`Factory`]).
///
/// `&self`: an operator holds no state between runs, so the same built step
/// can run on many chunks, in any order, on any thread.
pub trait Operator: Send + Sync {
    fn execute(&self, ctx: &mut Context<'_>) -> Result<(), OpError>;
}

/// How an operator is registered: its name, what it supports, its config
/// type and how to build it. [`Registry::register`] turns this into a
/// [`Factory`] and a JSON Schema, so authors never touch raw JSON.
pub trait OperatorType: Operator + Sized + 'static {
    const NAME: &'static str;
    const DIMENSIONS: Dimensions;
    const SCOPE: Scope;
    /// Use `#[serde(deny_unknown_fields)]`: a typo must be an error.
    type Config: DeserializeOwned + JsonSchema;

    fn build(config: Self::Config, bcx: &mut BuildContext<'_>) -> Result<Self, BuildError>;
}

/// Builds an operator from raw JSON. [`Registry::register`] makes one from an
/// [`OperatorType`]; operators that are not Rust types (WASM, FFI) provide
/// their own through [`Registry::register_entry`].
pub type Factory =
    fn(serde_json::Value, &mut BuildContext<'_>) -> Result<Box<dyn Operator>, BuildError>;

#[derive(Debug, Clone, Copy)]
pub struct Entry {
    pub factory: Factory,
    /// The config's schema, its definitions added to the generator: the
    /// whole-file schema shares one table across operators and conditions.
    pub schema: fn(&mut SchemaGenerator) -> Schema,
    pub dimensions: Dimensions,
    pub scope: Scope,
}

/// Operators by name.
#[derive(Debug, Default)]
pub struct Registry {
    entries: BTreeMap<String, Entry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("operator '{name}' is already registered")]
pub struct DuplicateOperator {
    pub name: String,
}

impl Registry {
    pub fn new() -> Registry {
        Registry::default()
    }

    /// Every operator that ships with the core.
    pub fn with_builtins() -> Registry {
        let mut r = Registry::new();
        crate::ops::register_builtins(&mut r);
        r
    }

    pub fn register<O: OperatorType>(&mut self) -> Result<(), DuplicateOperator> {
        self.register_entry(
            O::NAME,
            Entry {
                factory: build_boxed::<O>,
                schema: schema_of::<O::Config>,
                dimensions: O::DIMENSIONS,
                scope: O::SCOPE,
            },
        )
    }

    pub fn register_entry(
        &mut self,
        name: impl Into<String>,
        entry: Entry,
    ) -> Result<(), DuplicateOperator> {
        let name = name.into();
        if self.entries.contains_key(&name) {
            return Err(DuplicateOperator { name });
        }
        self.entries.insert(name, entry);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.entries.get(name)
    }

    /// Registered names, sorted.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }
}

fn build_boxed<O: OperatorType>(
    config: serde_json::Value,
    bcx: &mut BuildContext<'_>,
) -> Result<Box<dyn Operator>, BuildError> {
    // serde_path_to_error keeps where in the config it failed ("/radius",
    // "/rooms/2/size"), and not only "invalid type".
    let config: O::Config =
        serde_path_to_error::deserialize(config).map_err(|e| BuildError::Config {
            path: crate::spec::pointer(e.path()),
            message: e.inner().to_string(),
        })?;
    Ok(Box::new(O::build(config, bcx)?))
}

fn schema_of<T: JsonSchema>(generator: &mut SchemaGenerator) -> Schema {
    generator.subschema_for::<T>()
}

/// Errors while building a step. The pipeline wraps them with the step id.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum BuildError {
    #[error("unknown operator '{name}'")]
    UnknownOperator { name: String },
    /// `path` is a JSON Pointer into the config; `""` is the config itself.
    #[error("invalid config{}: {message}", at(.path))]
    Config { path: String, message: String },
    #[error("invalid '{field}': {message}")]
    Invalid { field: String, message: String },
    #[error("{operator} does not support {found} maps")]
    WrongDimensions {
        operator: String,
        found: &'static str,
    },
    #[error("{operator} does not take conditions")]
    ConditionsNotSupported { operator: String },
    #[error(
        "layer '{layer}' is read around each cell and written in the same step, \
         so the result would depend on the order cells are visited"
    )]
    ReadWriteConflict { layer: String },
    #[error("step id '{id}' is already used by another step")]
    DuplicateStepId { id: String },
    #[error(transparent)]
    Grid(#[from] GridError),
    #[error(transparent)]
    Tile(#[from] TileError),
    #[error(transparent)]
    Condition(#[from] ConditionError),
}

/// " at '/radius'", or nothing for the config itself.
fn at(path: &str) -> String {
    if path.is_empty() {
        String::new()
    } else {
        format!(" at '{path}'")
    }
}

/// What an operator gets while it is built: name resolution, and the
/// declarations that become its [`Access`].
pub struct BuildContext<'a> {
    grid: &'a Grid,
    tiles: &'a TileRegistry,
    access: Access,
    conditions: Option<CompiledCondition>,
}

impl<'a> BuildContext<'a> {
    pub(crate) fn new(
        grid: &'a Grid,
        tiles: &'a TileRegistry,
        conditions: Option<CompiledCondition>,
    ) -> BuildContext<'a> {
        let mut bcx = BuildContext {
            grid,
            tiles,
            access: Access::default(),
            conditions: None,
        };
        if let Some(c) = conditions {
            bcx.record_condition(&c);
            bcx.conditions = Some(c);
        }
        bcx
    }

    /// Whether the map is 2D (depth 1).
    pub fn is_2d(&self) -> bool {
        self.grid.bounds().size.is_2d()
    }

    /// Declares a live read of `layer`, `reach` cells around each cell.
    pub fn read(&mut self, layer: &str, reach: Reach) -> Result<ReadHandle, BuildError> {
        let id = self.grid.layer_id(layer)?;
        let reach = self.normalize(reach);
        self.access.add_read(id, reach);
        Ok(ReadHandle { layer: id })
    }

    /// Declares a write to `layer`, at the cell only.
    pub fn write(&mut self, layer: &str) -> Result<WriteHandle, BuildError> {
        self.write_reaching(layer, Reach::ZERO)
    }

    /// Declares a write to `layer` that can land up to `reach` cells away
    /// from the cell being decided (a stamp that crosses a chunk border).
    pub fn write_reaching(&mut self, layer: &str, reach: Reach) -> Result<WriteHandle, BuildError> {
        let id = self.grid.layer_id(layer)?;
        let reach = self.normalize(reach);
        self.access.add_write(id, reach);
        Ok(WriteHandle { layer: id })
    }

    /// Declares a synchronous update of `layer`: each cell's new value comes
    /// from the layer as the step found it, up to `reach` cells around (a
    /// cellular automaton). Not a conflict, unlike [`read`](Self::read) and
    /// [`write`](Self::write) of the same layer: the operator reads the whole
    /// layer through [`GridView::tiles_mut`] before writing any of it, and
    /// keeps its own double buffer (PLANNING §2.7 item 15, §2.8).
    pub fn read_write(&mut self, layer: &str, reach: Reach) -> Result<WriteHandle, BuildError> {
        let id = self.grid.layer_id(layer)?;
        let reach = self.normalize(reach);
        self.access.add_snapshot_read(id, reach);
        self.access.add_write(id, Reach::ZERO);
        Ok(WriteHandle { layer: id })
    }

    pub fn tile(&self, name: &str) -> Result<TileId, BuildError> {
        Ok(self.tiles.id(name)?)
    }

    /// Compiles a condition from the operator's own config; its reads join
    /// the step's access.
    pub fn condition(&mut self, c: &Condition) -> Result<CompiledCondition, BuildError> {
        let compiled = c.compile(self.grid, self.tiles)?;
        self.record_condition(&compiled);
        Ok(compiled)
    }

    /// The step's `conditions` (an implicit `all`), for an operator that
    /// honours them. `None` when the step has none. If the step has some and
    /// the operator never takes them, building the step fails: ignoring them
    /// silently would be the Go bug of unknown conditions all over again.
    pub fn step_conditions(&mut self) -> Option<CompiledCondition> {
        self.conditions.take()
    }

    fn record_condition(&mut self, c: &CompiledCondition) {
        for (layer, reach) in c.reads() {
            self.access.add_snapshot_read(layer, reach);
        }
    }

    fn normalize(&self, reach: Reach) -> Reach {
        if self.is_2d() {
            reach.flatten_z()
        } else {
            reach
        }
    }

    /// The access, checked; `operator` names the operator in errors.
    pub(crate) fn finish(self, operator: &str) -> Result<Access, BuildError> {
        if self.conditions.is_some() {
            return Err(BuildError::ConditionsNotSupported {
                operator: operator.to_string(),
            });
        }
        if let Some(layer) = self.access.read_write_conflict() {
            return Err(BuildError::ReadWriteConflict {
                layer: self.grid.layer_name(layer).unwrap_or("?").to_string(),
            });
        }
        Ok(self.access)
    }
}

/// Errors while a step runs. The pipeline wraps them with the step id.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum OpError {
    #[error("layer {layer:?} was not declared for this access at build time")]
    Undeclared { layer: LayerId },
    #[error("{message}")]
    Failed { message: String },
    #[error(transparent)]
    Grid(#[from] GridError),
    #[error(transparent)]
    Condition(#[from] ConditionError),
    #[error(transparent)]
    OutOfBounds(#[from] OutOfBounds),
}

/// What an operator gets while it runs. Split it with [`parts`](Self::parts).
pub struct Context<'a> {
    grid: &'a mut Grid,
    access: &'a Access,
    rng: StepRng,
}

/// The pieces of a [`Context`], borrowed separately so an operator can read
/// the grid and draw from the rng in the same loop. `#[non_exhaustive]`:
/// new pieces (objects, markers) can be added without breaking operators.
#[non_exhaustive]
pub struct Parts<'c> {
    pub grid: GridView<'c>,
    pub rng: &'c mut StepRng,
}

impl<'a> Context<'a> {
    pub(crate) fn new(grid: &'a mut Grid, access: &'a Access, rng: StepRng) -> Context<'a> {
        Context { grid, access, rng }
    }

    pub fn parts(&mut self) -> Parts<'_> {
        Parts {
            grid: GridView {
                grid: self.grid,
                access: self.access,
            },
            rng: &mut self.rng,
        }
    }
}

/// The grid as one step may see it: only the layers it declared.
pub struct GridView<'c> {
    grid: &'c mut Grid,
    access: &'c Access,
}

impl GridView<'_> {
    /// The cells this run decides.
    pub fn region(&self) -> Bounds {
        self.grid.bounds()
    }

    pub fn tiles(&self, h: ReadHandle) -> Result<&Layer<TileId>, OpError> {
        if !self.access.reads_live(h.layer) {
            return Err(OpError::Undeclared { layer: h.layer });
        }
        Ok(self.grid.tiles(h.layer)?)
    }

    pub fn tiles_mut(&mut self, h: WriteHandle) -> Result<&mut Layer<TileId>, OpError> {
        if !self.access.writes(h.layer) {
            return Err(OpError::Undeclared { layer: h.layer });
        }
        Ok(self.grid.tiles_mut(h.layer)?)
    }

    /// A snapshot of what the condition reads, taken now.
    pub fn prepare(&self, c: &CompiledCondition) -> Result<PreparedCondition, OpError> {
        Ok(c.prepare(self.grid)?)
    }
}

#[cfg(test)]
mod tests;
