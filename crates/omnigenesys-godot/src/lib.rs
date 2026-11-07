//! Omnigenesys inside Godot: makes a map while the game runs, for the addon
//! in `adapters/godot` (pesquisa 2026-10-31-adaptador-godot). One class,
//! [`OmniGeneration`]; the node people use, `OmniGenerator`, is GDScript, so
//! the addon still loads where this library does not exist.
//!
//! The map is made on a thread of its own, in plain Rust: Godot's objects
//! are only touched on the thread that calls the class, so this needs no
//! `experimental-threads`. GDScript starts it and asks if it is done.

use std::thread::{self, JoinHandle};

use godot::prelude::*;
use omnigenesys_core::export::{self, ENGINE_VERSION, Encoding, Provenance};
use omnigenesys_core::grid::Grid;
use omnigenesys_core::layer::LayerKind;
use omnigenesys_core::operator::Registry;
use omnigenesys_core::spec::{Built, Diagnostic, PipelineSpec};
use omnigenesys_core::tile::TileRegistry;

struct OmnigenesysExtension;

#[gdextension]
unsafe impl ExtensionLibrary for OmnigenesysExtension {}

/// A finished map.
struct Made {
    grid: Grid,
    tiles: TileRegistry,
    provenance: Provenance,
}

/// One line per diagnostic: `line:column pointer: message`.
fn diagnostics(errors: &[Diagnostic]) -> String {
    errors
        .iter()
        .map(|d| {
            let at = d
                .span
                .map(|s| format!("{}:{} ", s.line, s.column))
                .unwrap_or_default();
            let step = d
                .step_id
                .as_deref()
                .map(|s| format!("step '{s}': "))
                .unwrap_or_default();
            format!("{at}{}: {step}{}", d.pointer, d.message)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `{"name": value}` to the `NAME=VALUE` overrides the parser reads by each
/// param's declared type.
fn overrides(json: &str) -> Result<Vec<(String, String)>, String> {
    if json.trim().is_empty() {
        return Ok(Vec::new());
    }
    let map: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(json).map_err(|e| format!("params: {e}"))?;
    Ok(map
        .into_iter()
        .map(|(name, value)| {
            let text = match value {
                serde_json::Value::String(s) => s,
                other => other.to_string(),
            };
            (name, text)
        })
        .collect())
}

fn make(file: &str, params: &str, seed: Option<u64>, salt: Option<&str>) -> Result<Made, String> {
    let params = overrides(params)?;
    let spec = PipelineSpec::parse_with(file, &params)
        .map_err(|e| diagnostics(&e))?
        .spec;
    let Built {
        grid,
        tiles,
        pipeline,
    } = spec
        .build(&Registry::with_builtins())
        .map_err(|e| diagnostics(&e))?;
    let seed = seed
        .or(spec.seed)
        .ok_or_else(|| "/seed: the pipeline has no seed: give one".to_string())?;
    let salt = salt.unwrap_or(&spec.salt);
    let made = pipeline
        .run_attempts(&grid, salt, seed, spec.attempts)
        .map_err(|e| e.to_string())?;
    let mut provenance = spec.provenance(seed, salt);
    provenance.attempt = made.attempt;
    Ok(Made {
        grid: made.grid,
        tiles,
        provenance,
    })
}

/// What a panic carried, as text.
fn panic_text(panic: Box<dyn std::any::Any + Send>) -> String {
    let what = panic
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown".to_string());
    format!("omnigenesys panicked (a bug, please report it): {what}")
}

/// One map being made: [`start`](Self::start), wait for
/// [`is_done`](Self::is_done), then [`result`](Self::result).
#[derive(GodotClass)]
#[class(init, base=RefCounted)]
pub struct OmniGeneration {
    job: Option<JoinHandle<Result<Made, String>>>,
    finished: Option<Result<Made, String>>,
}

#[godot_api]
impl OmniGeneration {
    /// The engine's version.
    #[func]
    fn version() -> GString {
        ENGINE_VERSION.into()
    }

    /// Starts making a map from a pipeline file's text. `params_json` is a
    /// JSON object of param overrides, or empty. `seed` is a whole number as
    /// text (a seed goes up to 2^64 - 1, beyond Godot's int), or empty for
    /// the file's. `salt` is used when `use_salt`, otherwise the file's.
    #[func]
    fn start(
        &mut self,
        pipeline: GString,
        params_json: GString,
        seed: GString,
        use_salt: bool,
        salt: GString,
    ) {
        let (file, params, seed_text) = (
            pipeline.to_string(),
            params_json.to_string(),
            seed.to_string(),
        );
        let salt = use_salt.then(|| salt.to_string());
        self.finished = None;
        self.job = Some(thread::spawn(move || {
            let seed = if seed_text.is_empty() {
                None
            } else {
                Some(
                    seed_text
                        .parse::<u64>()
                        .map_err(|_| format!("seed '{seed_text}' is not a whole number"))?,
                )
            };
            make(&file, &params, seed, salt.as_deref())
        }));
    }

    /// True once the map is made or has failed (also before any start).
    #[func]
    fn is_done(&self) -> bool {
        self.job.as_ref().is_none_or(JoinHandle::is_finished)
    }

    /// The map, for `OmniMap.from_native`, or `{"error": message}`. Waits
    /// if it is not done.
    #[func]
    fn result(&mut self) -> VarDictionary {
        self.finish();
        match &self.finished {
            None => error("nothing was started"),
            Some(Err(e)) => error(e),
            Some(Ok(made)) => decoded(made).unwrap_or_else(|e| error(&e)),
        }
    }

    /// The map as a .omap file's text (output v2), empty if there is none
    /// (`result` says why). To save a seed worth keeping.
    #[func]
    fn omap_text(&mut self) -> GString {
        self.finish();
        let Some(Ok(made)) = &self.finished else {
            return GString::new();
        };
        let mut out = Vec::new();
        let written = export::write_json(
            &mut out,
            &made.grid,
            &made.tiles,
            &made.provenance,
            Encoding::Auto,
        );
        match written.map(|_| String::from_utf8(out)) {
            Ok(Ok(text)) => text.as_str().into(),
            _ => GString::new(),
        }
    }
}

impl OmniGeneration {
    fn finish(&mut self) {
        if let Some(job) = self.job.take() {
            self.finished = Some(job.join().unwrap_or_else(|p| Err(panic_text(p))));
        }
    }
}

fn error(message: &str) -> VarDictionary {
    let mut d = VarDictionary::new();
    d.set("error", message);
    d
}

/// The map in Godot's types: packed arrays, copied once, so no number goes
/// through JSON (pesquisa §2: Godot's parser makes a Variant of each).
fn decoded(made: &Made) -> Result<VarDictionary, String> {
    let g = &made.grid;
    let b = g.bounds();
    let mut d = VarDictionary::new();
    d.set("width", b.size.width as i64);
    d.set("height", b.size.height as i64);
    d.set("depth", b.size.depth as i64);
    d.set("origin", Vector3i::new(b.origin.x, b.origin.y, b.origin.z));
    d.set("seed", made.provenance.seed.to_string());
    d.set("attempt", made.provenance.attempt as i64);

    let mut tiles = PackedStringArray::new();
    for name in made.tiles.names() {
        tiles.push(name);
    }
    d.set("tiles", &tiles);

    let mut layers = VarArray::new();
    for name in g.names() {
        let id = g.layer_id(name).map_err(|e| e.to_string())?;
        let mut l = VarDictionary::new();
        l.set("name", name);
        match g.kind(id).map_err(|e| e.to_string())? {
            LayerKind::Tiles => {
                let ids: Vec<i64> = g
                    .tiles(id)
                    .map_err(|e| e.to_string())?
                    .as_slice()
                    .iter()
                    .map(|t| i64::from(t.index()))
                    .collect();
                l.set("kind", "tiles");
                l.set("ids", &PackedInt64Array::from(ids.as_slice()));
            }
            LayerKind::Index => {
                let ids: Vec<i64> = g
                    .indices(id)
                    .map_err(|e| e.to_string())?
                    .as_slice()
                    .iter()
                    .map(|&i| i64::from(i))
                    .collect();
                l.set("kind", "index");
                l.set("ids", &PackedInt64Array::from(ids.as_slice()));
            }
            LayerKind::Scalar => {
                let values = g.scalars(id).map_err(|e| e.to_string())?.as_slice();
                l.set("kind", "scalar");
                l.set("values", &PackedFloat32Array::from(values));
            }
            // In `objects`, as in a .omap file.
            LayerKind::Objects => continue,
        }
        layers.push(&l.to_variant());
    }
    d.set("layers", &layers);

    let mut objects = Vec::new();
    export::write_objects(&mut objects, g).map_err(|e| e.to_string())?;
    d.set(
        "objects_json",
        String::from_utf8(objects).map_err(|e| e.to_string())?,
    );
    Ok(d)
}
