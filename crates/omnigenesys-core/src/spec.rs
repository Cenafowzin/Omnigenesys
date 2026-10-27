//! The pipeline file, format v2 (PLANNING §2.1): parse it, check it, and
//! build it into a grid and a pipeline. Every problem comes back as a
//! [`Diagnostic`] with the line and column it is about, all of them at
//! once rather than one per run.

mod params;
mod schema;
mod source;

use std::collections::HashMap;
use std::fmt;

use serde::Deserialize;
use serde::de::value::MapAccessDeserializer;
use serde::de::{self, DeserializeOwned, Deserializer, IgnoredAny, MapAccess, Visitor};
use serde_json::Value;

use crate::{
    bounds::Bounds,
    condition::{Condition, ConditionError},
    coords::{Coord, GridSize},
    export::Provenance,
    grid::{Grid, GridError},
    layer::LayerKind,
    object::Lattice,
    operator::{BuildError, Registry},
    pipeline::{Pipeline, StepError, StepSpec},
    tile::{TileError, TileRegistry},
};

pub use params::{Param, ParamType, ParamValue};
pub use schema::schema;
pub(crate) use source::pointer;

use params::Site;
use source::{Spans, push_index, push_key};

/// The format version this crate reads and writes.
pub const FORMAT_VERSION: u64 = 2;

/// The largest seed written as a JSON number: above it, JavaScript (the web
/// editor) would already have rounded it. Larger seeds go in a string.
pub const MAX_NUMBER_SEED: u64 = (1 << 53) - 1;

/// A pipeline file, parsed and checked. The fields can be changed before
/// [`build`](Self::build) (a `--seed` from the command line, a size for a
/// test); diagnostics keep pointing at the file as it was read.
#[derive(Debug, Clone)]
pub struct PipelineSpec {
    pub description: Option<String>,
    /// `None` when the file has none; whoever runs it picks one.
    pub seed: Option<u64>,
    pub salt: String,
    pub size: GridSize,
    /// How many times the map may be made before giving up: an attempt
    /// that fails a step (a `Check`, a step with nothing to pick) is thrown
    /// away and the next one runs, each with its own randomness. 1 = no
    /// retry.
    pub attempts: u32,
    pub layers: Vec<LayerSpec>,
    /// Declared tiles, in order; `"empty"` (id 0) is implicit.
    pub tiles: Vec<String>,
    /// As declared. The values used (overrides or defaults) are already in
    /// `steps`, and in [`param_values`](Self::param_values).
    pub params: indexmap::IndexMap<String, Param>,
    pub steps: Vec<StepSpec>,
    values: Vec<(String, ParamValue)>,
    origin: Origin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerSpec {
    pub name: String,
    pub kind: LayerKind,
    /// Only for an object layer: the slots its objects sit in.
    pub lattice: Option<Lattice>,
}

/// A parsed file and what is wrong with it without being an error (a param
/// never used).
#[derive(Debug, Clone)]
pub struct Parsed {
    pub spec: PipelineSpec,
    pub warnings: Vec<Diagnostic>,
}

/// What [`PipelineSpec::build`] makes: an empty grid with the declared
/// layers, the tiles, and the steps ready to run on that grid.
pub struct Built {
    pub grid: Grid,
    pub tiles: TileRegistry,
    pub pipeline: Pipeline,
}

/// One problem in the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub message: String,
    /// The step it is about, when it is about one.
    pub step_id: Option<String>,
    /// JSON Pointer (RFC 6901) of the value it is about; `""` is the root.
    pub pointer: String,
    /// Where `pointer` is in the text (or its closest parent that is).
    pub span: Option<Span>,
    /// A second place that explains the first. Boxed: it is rare, and a
    /// smaller `Diagnostic` is cheaper to return in every `Result`.
    pub note: Option<Box<Note>>,
    /// For a mistake inside an expression (`"expr"`, `{"$expr": ...}`):
    /// where in its text, as byte offsets `start..end`.
    pub expr_span: Option<(usize, usize)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub message: String,
    pub pointer: String,
    pub span: Option<Span>,
}

/// Byte offsets `start..end` in the text, and the 1-based line and column
/// (in characters) of `start`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}

impl Diagnostic {
    fn new(pointer: impl Into<String>, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            message: message.into(),
            step_id: None,
            pointer: pointer.into(),
            span: None,
            note: None,
            expr_span: None,
        }
    }

    fn set_note(&mut self, message: impl Into<String>, pointer: impl Into<String>) {
        self.note = Some(Box::new(Note {
            message: message.into(),
            pointer: pointer.into(),
            span: None,
        }));
    }

    fn locate(&mut self, spans: &Spans) {
        if self.span.is_none() {
            self.span = spans.locate(&self.pointer);
        }
        if let Some(note) = &mut self.note
            && note.span.is_none()
        {
            note.span = spans.locate(&note.pointer);
        }
    }
}

/// `12:9: step 'walls': message (note at 4:5)`. Callers add the file name.
impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(s) = self.span {
            write!(f, "{}:{}: ", s.line, s.column)?;
        }
        if let Some(id) = &self.step_id {
            write!(f, "step '{id}': ")?;
        }
        f.write_str(&self.message)?;
        if let Some((start, _)) = self.expr_span {
            write!(f, " (character {} of the expression)", start + 1)?;
        }
        if let Some(note) = &self.note {
            write!(f, "; {}", note.message)?;
            if let Some(s) = note.span {
                write!(f, " at {}:{}", s.line, s.column)?;
            }
        }
        Ok(())
    }
}

impl std::error::Error for Diagnostic {}

/// What the file was, kept to place errors found after parsing.
#[derive(Debug, Clone, Default)]
struct Origin {
    spans: Spans,
    sites: Vec<Site>,
    /// Step id → its index in the file.
    steps: HashMap<String, usize>,
}

impl Origin {
    /// Adds the param note to errors about a substituted value, then
    /// places everything in the text.
    fn finish(&self, mut diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
        for d in &mut diagnostics {
            if d.note.is_none()
                && let Some(site) = self.sites.iter().find(|s| within(&d.pointer, &s.pointer))
            {
                match (&site.expr, site.params.first()) {
                    (Some(text), _) => d.set_note(
                        format!(
                            "the value {} comes from the expression `{text}`",
                            site.value
                        ),
                        site.pointer.clone(),
                    ),
                    (None, Some(param)) => d.set_note(
                        format!("the value comes from param '{param}'"),
                        params::param_pointer(param),
                    ),
                    (None, None) => {}
                }
            }
            d.locate(&self.spans);
        }
        diagnostics
    }
}

/// Whether `pointer` is `parent` or inside it.
fn within(pointer: &str, parent: &str) -> bool {
    pointer
        .strip_prefix(parent)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// The root as serde reads it. `params` and `steps` come as raw JSON: each
/// is checked on its own, so one bad step does not hide the others.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRoot {
    /// For editors; ignored.
    #[serde(rename = "$schema", default)]
    _schema: Option<String>,
    /// Already checked by `check_version`.
    #[serde(rename = "version")]
    _version: u64,
    description: Option<String>,
    seed: Option<Seed>,
    #[serde(default)]
    salt: String,
    size: GridSize,
    #[serde(default = "one")]
    attempts: u32,
    layers: Vec<LayerSpec>,
    #[serde(default)]
    tiles: Vec<String>,
    /// Already read by `params::declarations`.
    #[serde(rename = "params", default)]
    _params: IgnoredAny,
    steps: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStep {
    id: String,
    operator: String,
    #[serde(default = "enabled")]
    enabled: bool,
    #[serde(rename = "description", default)]
    _description: Option<String>,
    #[serde(default = "empty_config")]
    config: Value,
    #[serde(default)]
    conditions: Vec<Condition>,
}

fn enabled() -> bool {
    true
}

fn one() -> u32 {
    1
}

/// The most attempts a file may ask for: each runs every step again.
pub const MAX_ATTEMPTS: u32 = 1000;

fn empty_config() -> Value {
    Value::Object(serde_json::Map::new())
}

impl PipelineSpec {
    /// Reads a pipeline file. On success, `warnings` may still say
    /// something; on failure, every error found is returned.
    pub fn parse(text: &str) -> Result<Parsed, Vec<Diagnostic>> {
        PipelineSpec::parse_with(text, &[])
    }

    /// As [`parse`](Self::parse), with param values given from outside
    /// the file (`--param name=value`), as text read by each param's
    /// declared type. `params` keeps the declarations; the steps get the
    /// overridden values.
    pub fn parse_with(
        text: &str,
        overrides: &[(String, String)],
    ) -> Result<Parsed, Vec<Diagnostic>> {
        let (mut root, spans) = source::parse(text)?;
        let mut origin = Origin {
            spans,
            ..Origin::default()
        };
        if let Some(d) = check_version(&root) {
            return Err(origin.finish(vec![d]));
        }

        let mut errors = Vec::new();
        let mut declared = params::declarations(root.get("params"), &mut errors);
        params::apply_overrides(&mut declared, overrides, &mut errors);
        params::compute(&mut declared, &mut errors);
        origin.sites = params::substitute(&mut root, &declared, &mut errors);
        let values = declared
            .params
            .keys()
            .filter_map(|name| Some((name.clone(), declared.value(name)?.clone())))
            .collect();
        let params = declared.params;
        let raw: RawRoot = match deserialize(root, "") {
            Ok(raw) => raw,
            Err(d) => {
                errors.push(*d);
                return Err(origin.finish(errors));
            }
        };
        let layers_and_tiles = check_grid(raw.size, &raw.layers, &raw.tiles);
        errors.extend(layers_and_tiles);
        if !(1..=MAX_ATTEMPTS).contains(&raw.attempts) {
            errors.push(Diagnostic::new(
                "/attempts",
                format!("{} attempts; from 1 to {MAX_ATTEMPTS}", raw.attempts),
            ));
        }

        let mut steps = Vec::with_capacity(raw.steps.len());
        for (i, value) in raw.steps.into_iter().enumerate() {
            let at = format!("/steps/{i}");
            // Read before deserializing, so even a broken step names itself.
            let id = value.get("id").and_then(Value::as_str).map(str::to_string);
            let step = deserialize::<RawStep>(value, &at).and_then(|s| {
                match check_step_id(&s.id, &at, &origin.steps) {
                    Some(d) => Err(Box::new(d)),
                    None => Ok(s),
                }
            });
            match step {
                Ok(s) => {
                    origin.steps.insert(s.id.clone(), i);
                    steps.push(StepSpec {
                        id: s.id,
                        operator: s.operator,
                        enabled: s.enabled,
                        config: s.config,
                        conditions: s.conditions,
                    });
                }
                Err(mut d) => {
                    d.step_id = id;
                    errors.push(*d);
                }
            }
        }
        if !errors.is_empty() {
            return Err(origin.finish(errors));
        }

        let used = params::used(&params, &origin.sites);
        let warnings = params
            .keys()
            .filter(|name| !used.contains(*name))
            .map(|name| {
                Diagnostic::new(
                    params::param_pointer(name),
                    format!("param '{name}' is never used"),
                )
            })
            .collect();
        let warnings = origin.finish(warnings);
        let spec = PipelineSpec {
            description: raw.description,
            seed: raw.seed.map(|s| s.0),
            salt: raw.salt,
            size: raw.size,
            attempts: raw.attempts,
            layers: raw.layers,
            tiles: raw.tiles,
            params,
            steps,
            values,
            origin,
        };
        Ok(Parsed { spec, warnings })
    }

    /// Every `{"$expr": ...}` in the steps, by the pointer it stood at,
    /// with its text and the value it gave: for an editor to show next to it.
    pub fn expressions(&self) -> impl Iterator<Item = (&str, &str, &ParamValue)> {
        self.origin
            .sites
            .iter()
            .filter_map(|s| Some((s.pointer.as_str(), s.expr.as_deref()?, &s.value)))
    }

    /// Every declared param with the value the steps got: the override if
    /// there was one, else the default. In declaration order.
    pub fn param_values(&self) -> &[(String, ParamValue)] {
        &self.values
    }

    /// What the output records about a map made from this spec with `seed`
    /// and `salt`.
    pub fn provenance(&self, seed: u64, salt: &str) -> Provenance {
        Provenance {
            seed,
            salted: !salt.is_empty(),
            attempt: 0,
            params: self.values.clone(),
        }
    }

    /// Makes the grid, the tiles and the pipeline. Every step is built and
    /// every error returned.
    pub fn build(&self, registry: &Registry) -> Result<Built, Vec<Diagnostic>> {
        let errors = check_grid(self.size, &self.layers, &self.tiles);
        if !errors.is_empty() {
            return Err(self.origin.finish(errors));
        }
        // After check_grid these cannot fail; mapped rather than unwrapped
        // so a gap between the two is an error, not a panic.
        let fail = |pointer: &str, e: &dyn fmt::Display| {
            self.origin
                .finish(vec![Diagnostic::new(pointer, e.to_string())])
        };
        let mut grid = Grid::new(Bounds::new(Coord::ZERO, self.size));
        for layer in &self.layers {
            match layer.lattice {
                Some(lattice) => grid.add_object_layer(layer.name.clone(), lattice),
                None => grid.add_layer(layer.name.clone(), layer.kind),
            }
            .map_err(|e| fail("/layers", &e))?;
        }
        let tiles =
            TileRegistry::new(self.tiles.iter().cloned()).map_err(|e| fail("/tiles", &e))?;
        let pipeline = Pipeline::build(registry, &grid, &tiles, &self.steps).map_err(|errors| {
            let errors = errors
                .into_iter()
                .map(|e| self.step_diagnostic(e))
                .collect();
            self.origin.finish(errors)
        })?;
        Ok(Built {
            grid,
            tiles,
            pipeline,
        })
    }

    /// Steps that read a layer no earlier step writes: they see it empty,
    /// which is rarely what was meant (a step moved above the one it
    /// depends on). Warnings, not errors: reading `empty` on purpose is
    /// allowed. `built` is this spec, built.
    pub fn unwritten_reads(&self, built: &Built) -> Vec<Diagnostic> {
        let mut written = std::collections::HashSet::new();
        let mut out = Vec::new();
        for (i, s) in self.steps.iter().enumerate().filter(|(_, s)| s.enabled) {
            let Some(access) = built.pipeline.access(&s.id) else {
                continue;
            };
            for l in access.layers() {
                if access.read_reach(l.layer).is_some() && !written.contains(&l.layer) {
                    let name = built.grid.layer_name(l.layer).unwrap_or("?");
                    let mut d = Diagnostic::new(
                        format!("/steps/{i}"),
                        format!(
                            "reads layer '{name}', but no step before it writes there: \
                             it is still empty"
                        ),
                    );
                    d.step_id = Some(s.id.clone());
                    out.push(d);
                }
            }
            written.extend(
                access
                    .layers()
                    .iter()
                    .filter(|l| l.write.is_some())
                    .map(|l| l.layer),
            );
        }
        self.origin.finish(out)
    }

    /// Places a build error inside its step, as precisely as the error
    /// allows.
    fn step_diagnostic(&self, e: StepError<BuildError>) -> Diagnostic {
        let index = self
            .origin
            .steps
            .get(&e.step_id)
            .copied()
            .or_else(|| self.steps.iter().position(|s| s.id == e.step_id));
        let mut at = String::from("/steps");
        if let Some(i) = index {
            push_index(&mut at, i);
        }
        let step = self.steps.iter().find(|s| s.id == e.step_id);
        let pointer = match &e.error {
            BuildError::UnknownOperator { .. } | BuildError::WrongDimensions { .. } => {
                format!("{at}/operator")
            }
            BuildError::Config { path, .. } => format!("{at}/config{path}"),
            BuildError::Invalid { field, .. } | BuildError::Filter { field, .. } => {
                let mut p = format!("{at}/config");
                push_key(&mut p, field);
                p
            }
            BuildError::ConditionsNotSupported { .. } => format!("{at}/conditions"),
            BuildError::ReadWriteConflict { .. } => format!("{at}/config"),
            BuildError::DuplicateStepId { .. } => format!("{at}/id"),
            other => named(other)
                .zip(step)
                .and_then(|(name, step)| find_name(step, name))
                .map_or(at.clone(), |p| format!("{at}{p}")),
        };
        let mut d = Diagnostic::new(pointer, e.error.to_string());
        d.step_id = Some(e.step_id);
        d
    }
}

/// The layer or tile name a build error is about.
fn named(e: &BuildError) -> Option<&str> {
    match e {
        BuildError::Grid(g) | BuildError::Condition(ConditionError::Grid(g)) => match g {
            GridError::LayerNotFound { name } | GridError::WrongLayerKind { name, .. } => {
                Some(name)
            }
            _ => None,
        },
        BuildError::Tile(TileError::UnknownTile { name })
        | BuildError::Condition(ConditionError::Tile(TileError::UnknownTile { name })) => {
            Some(name)
        }
        _ => None,
    }
}

/// Where in the step a name is written: its config first, then its
/// conditions. Pointer relative to the step.
fn find_name(step: &StepSpec, name: &str) -> Option<String> {
    let mut p = String::from("/config");
    if find_string(&step.config, name, &mut p) {
        return Some(p);
    }
    let conditions = serde_json::to_value(&step.conditions).ok()?;
    let mut p = String::from("/conditions");
    find_string(&conditions, name, &mut p).then_some(p)
}

/// Depth-first search for a string value; on success `pointer` ends at it.
fn find_string(value: &Value, needle: &str, pointer: &mut String) -> bool {
    let len = pointer.len();
    match value {
        Value::String(s) => s == needle,
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                push_index(pointer, i);
                if find_string(item, needle, pointer) {
                    return true;
                }
                pointer.truncate(len);
            }
            false
        }
        Value::Object(map) => {
            for (key, item) in map {
                push_key(pointer, key);
                if find_string(item, needle, pointer) {
                    return true;
                }
                pointer.truncate(len);
            }
            false
        }
        _ => false,
    }
}

/// Deserializes `value`, placing an error at `at` + where serde stopped.
/// Boxed: the error is the rare path, and `Diagnostic` is large.
fn deserialize<T: DeserializeOwned>(value: Value, at: &str) -> Result<T, Box<Diagnostic>> {
    serde_path_to_error::deserialize(value).map_err(|e| {
        let pointer = format!("{at}{}", pointer(e.path()));
        Box::new(Diagnostic::new(pointer, e.into_inner().to_string()))
    })
}

/// Checked before anything else: a newer file must say "update", not list
/// the fields this version does not know. The problem, if there is one.
fn check_version(root: &Value) -> Option<Diagnostic> {
    let Some(root) = root.as_object() else {
        return Some(Diagnostic::new("", "a pipeline is a JSON object"));
    };
    let Some(version) = root.get("version") else {
        return Some(Diagnostic::new(
            "",
            format!("missing `version` (this is format version {FORMAT_VERSION})"),
        ));
    };
    match version.as_u64() {
        Some(FORMAT_VERSION) => None,
        Some(v) if v > FORMAT_VERSION => Some(Diagnostic::new(
            "/version",
            format!(
                "format version {v} is newer than this omnigenesys reads \
                 ({FORMAT_VERSION}); update omnigenesys"
            ),
        )),
        Some(v) => Some(Diagnostic::new(
            "/version",
            format!("format version {v} is not supported; this omnigenesys reads {FORMAT_VERSION}"),
        )),
        None => Some(Diagnostic::new("/version", "`version` must be an integer")),
    }
}

/// Size, layer names and tile names, without allocating the grid.
fn check_grid(size: GridSize, layers: &[LayerSpec], tiles: &[String]) -> Vec<Diagnostic> {
    let mut errors = Vec::new();
    for (axis, n) in [
        ("width", size.width),
        ("height", size.height),
        ("depth", size.depth),
    ] {
        if n == 0 {
            errors.push(Diagnostic::new(
                format!("/size/{axis}"),
                format!("{axis} must be at least 1"),
            ));
        }
    }
    if size.checked_len().is_none() {
        errors.push(Diagnostic::new("/size", "the map has too many cells"));
    }
    let names = layers.iter().map(|l| l.name.as_str());
    errors.extend(check_names("/layers", "layer", names, None));
    for (i, layer) in layers.iter().enumerate() {
        let Some(lattice) = &layer.lattice else {
            continue;
        };
        let at = format!("/layers/{i}/lattice");
        if layer.kind != LayerKind::Objects {
            errors.push(Diagnostic::new(
                at,
                format!(
                    "layer '{}' is {}; only an object layer has a lattice",
                    layer.name, layer.kind
                ),
            ));
        } else if let Some(problem) = lattice.problem() {
            errors.push(Diagnostic::new(at, problem));
        }
    }
    let names = tiles.iter().map(String::as_str);
    errors.extend(check_names("/tiles", "tile", names, Some("empty")));
    if tiles.len() > u16::MAX as usize {
        errors.push(Diagnostic::new(
            "/tiles",
            format!("{} tiles; at most {}", tiles.len(), u16::MAX),
        ));
    }
    errors
}

/// Empty, reserved and repeated names in a list.
fn check_names<'a>(
    list: &str,
    what: &str,
    names: impl Iterator<Item = &'a str>,
    reserved: Option<&str>,
) -> Vec<Diagnostic> {
    let mut errors = Vec::new();
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for (i, name) in names.enumerate() {
        let at = format!("{list}/{i}");
        if name.is_empty() {
            errors.push(Diagnostic::new(at, format!("{what} name is empty")));
        } else if Some(name) == reserved {
            errors.push(Diagnostic::new(
                at,
                format!("'{name}' is reserved and cannot be declared"),
            ));
        } else if let Some(first) = seen.get(name) {
            let mut d = Diagnostic::new(at, format!("{what} '{name}' is declared twice"));
            d.set_note("first declared here", format!("{list}/{first}"));
            errors.push(d);
        } else {
            seen.insert(name, i);
        }
    }
    errors
}

/// `^[a-z][a-z0-9_-]{0,63}$`, unique. The id seeds the step's randomness
/// (PLANNING §2.1, item 7): renaming a step changes its map.
fn check_step_id(id: &str, at: &str, seen: &HashMap<String, usize>) -> Option<Diagnostic> {
    let b = id.as_bytes();
    let valid = (1..=64).contains(&b.len())
        && b[0].is_ascii_lowercase()
        && b.iter()
            .all(|&c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-');
    if !valid {
        return Some(Diagnostic::new(
            format!("{at}/id"),
            format!(
                "step id '{id}' must start with a-z and use only a-z, 0-9, '_' and '-', \
                 up to 64 characters"
            ),
        ));
    }
    if let Some(first) = seen.get(id) {
        let mut d = Diagnostic::new(
            format!("{at}/id"),
            format!("step id '{id}' is already used by another step"),
        );
        d.set_note("first used here", format!("/steps/{first}/id"));
        return Some(d);
    }
    None
}

/// A number up to [`MAX_NUMBER_SEED`], or a decimal string with any u64.
struct Seed(u64);

impl<'de> Deserialize<'de> for Seed {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Seed, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = Seed;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an integer from 0 to 2^53-1, or a decimal string")
            }
            fn visit_u64<E: de::Error>(self, n: u64) -> Result<Seed, E> {
                if n <= MAX_NUMBER_SEED {
                    Ok(Seed(n))
                } else {
                    Err(E::custom(format!(
                        "seed {n} is above 2^53-1, where JavaScript rounds numbers; \
                         write it as a string: \"{n}\""
                    )))
                }
            }
            fn visit_i64<E: de::Error>(self, n: i64) -> Result<Seed, E> {
                Err(E::invalid_value(de::Unexpected::Signed(n), &self))
            }
            fn visit_f64<E: de::Error>(self, n: f64) -> Result<Seed, E> {
                if n.fract() == 0.0 && n > MAX_NUMBER_SEED as f64 {
                    Err(E::custom(
                        "seed is too large for a number; write it as a decimal string",
                    ))
                } else {
                    Err(E::invalid_type(de::Unexpected::Float(n), &self))
                }
            }
            fn visit_str<E: de::Error>(self, s: &str) -> Result<Seed, E> {
                if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(E::invalid_value(de::Unexpected::Str(s), &self));
                }
                s.parse()
                    .map(Seed)
                    .map_err(|_| E::custom(format!("seed \"{s}\" does not fit in 64 bits")))
            }
        }
        d.deserialize_any(V)
    }
}

// A layer is a name (a tile layer) or `{"name", "kind"}`. By hand for the
// same reason as params: precise errors inside the object form.
impl<'de> Deserialize<'de> for LayerSpec {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<LayerSpec, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Full {
            name: String,
            #[serde(default = "tiles")]
            kind: LayerKind,
            #[serde(default)]
            lattice: Option<Lattice>,
        }
        fn tiles() -> LayerKind {
            LayerKind::Tiles
        }

        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = LayerSpec;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a layer name, or {\"name\": ..., \"kind\": ...}")
            }
            fn visit_str<E: de::Error>(self, name: &str) -> Result<LayerSpec, E> {
                Ok(LayerSpec {
                    name: name.to_string(),
                    kind: LayerKind::Tiles,
                    lattice: None,
                })
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<LayerSpec, A::Error> {
                let full = Full::deserialize(MapAccessDeserializer::new(map))?;
                Ok(LayerSpec {
                    name: full.name,
                    kind: full.kind,
                    lattice: full.lattice,
                })
            }
        }
        d.deserialize_any(V)
    }
}

#[cfg(test)]
mod tests;
