//! Omnigenesys as a native library, for an engine that makes the map while
//! the game runs (Unity by P/Invoke; anything that calls C). The same
//! pipeline file, seed and engine version make the same map as the CLI and
//! the browser (`tests/same_map.rs`).
//!
//! The shape of the ABI (pesquisa 2026-10-31-adaptador-unity, Q4b):
//!
//! - Every function returns a status (`OMNI_OK` or an error code); on an
//!   error, [`omni_last_error`] has the message, **on the same thread**
//!   (it is per thread, so two maps made at once do not mix their errors).
//! - A map is an opaque handle, owned by the caller from
//!   [`omni_generate`] until [`omni_map_free`]. One thread at a time.
//! - The caller allocates every buffer and the library copies into it:
//!   nothing Rust allocated crosses the boundary but the handle. A text
//!   (UTF-8, no terminating zero) is asked for twice: once with `cap` 0
//!   to learn its length, then with a buffer that size.
//! - Text in goes as pointer + byte length, UTF-8.
//! - A panic never crosses into the engine (from Rust 1.81 that would
//!   abort the game): it becomes `OMNI_ERR_PANIC`.
//! - Cells go x fastest, then y, then z; y grows downwards (output v2).

use std::cell::{OnceCell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};

use omnigenesys_core::export::{self, ENGINE_VERSION, Encoding, Provenance};
use omnigenesys_core::grid::Grid;
use omnigenesys_core::layer::LayerKind;
use omnigenesys_core::operator::Registry;
use omnigenesys_core::spec::{Built, Diagnostic, PipelineSpec};
use omnigenesys_core::tile::TileRegistry;

pub const OMNI_OK: i32 = 0;
/// A null pointer, text that is not UTF-8, params that are not a JSON
/// object.
pub const OMNI_ERR_ARGUMENT: i32 = 1;
/// The pipeline does not parse, build or run (the message says where).
pub const OMNI_ERR_PIPELINE: i32 = 2;
/// A bug in the library: please report the message.
pub const OMNI_ERR_PANIC: i32 = 3;
/// No layer or tile with that index, a layer of another kind, or a buffer
/// of the wrong length.
pub const OMNI_ERR_LAYER: i32 = 4;
/// The map cannot be written as output v2 (a NaN in a scalar layer).
pub const OMNI_ERR_EXPORT: i32 = 5;

pub const OMNI_KIND_TILES: i32 = 0;
pub const OMNI_KIND_SCALAR: i32 = 1;
pub const OMNI_KIND_INDEX: i32 = 2;
pub const OMNI_KIND_OBJECTS: i32 = 3;

/// A map made by [`omni_generate`]. Opaque: only its address crosses.
#[repr(C)]
pub struct OmniMap {
    _private: [u8; 0],
}

/// What a map is, in one call.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct OmniMapInfo {
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub origin_x: i32,
    pub origin_y: i32,
    pub origin_z: i32,
    pub seed: u64,
    /// The attempt that made it (a file's `attempts`), from 0.
    pub attempt: u32,
    pub layer_count: u32,
    /// `tiles[0]` is always "empty".
    pub tile_count: u32,
}

/// What is behind an [`OmniMap`] handle.
struct Map {
    grid: Grid,
    tiles: TileRegistry,
    provenance: Provenance,
    layers: Vec<String>,
    /// Made on the first ask: a text is asked for twice (length, then
    /// bytes), and an engine that does not ask never pays for it.
    objects: OnceCell<Result<String, String>>,
    omap: OnceCell<Result<String, String>>,
}

thread_local! {
    static LAST_ERROR: RefCell<String> = const { RefCell::new(String::new()) };
}

/// An error code and its message.
type Fail = (i32, String);

fn set_error(message: String) {
    LAST_ERROR.with(|e| *e.borrow_mut() = message);
}

/// Runs `f`, turning an error or a panic into a status and the last error.
fn guard(f: impl FnOnce() -> Result<(), Fail>) -> i32 {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(())) => OMNI_OK,
        Ok(Err((code, message))) => {
            set_error(message);
            code
        }
        Err(panic) => {
            let what = panic
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown".to_string());
            set_error(format!(
                "omnigenesys panicked (a bug, please report it): {what}"
            ));
            OMNI_ERR_PANIC
        }
    }
}

fn argument(message: impl Into<String>) -> Fail {
    (OMNI_ERR_ARGUMENT, message.into())
}

/// `len` bytes at `ptr` as text. A null pointer is only allowed with 0.
///
/// # Safety
/// `ptr` must point at `len` readable bytes that outlive the call.
unsafe fn text<'a>(ptr: *const u8, len: usize, what: &str) -> Result<&'a str, Fail> {
    if len == 0 {
        return Ok("");
    }
    if ptr.is_null() {
        return Err(argument(format!("{what} is null with a length of {len}")));
    }
    // SAFETY: the caller's promise, above.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    std::str::from_utf8(bytes).map_err(|e| argument(format!("{what} is not UTF-8: {e}")))
}

/// Copies `s` into `buf` when it fits, and always says its length.
///
/// # Safety
/// `buf` must point at `cap` writable bytes (or `cap` be 0), and `len` at a
/// writable `usize` or be null.
unsafe fn give_text(s: &str, buf: *mut u8, cap: usize, len: *mut usize) {
    if !len.is_null() {
        // SAFETY: the caller's promise.
        unsafe { *len = s.len() };
    }
    if !buf.is_null() && cap >= s.len() {
        // SAFETY: `cap` writable bytes, and `s.len() <= cap`.
        unsafe { std::ptr::copy_nonoverlapping(s.as_ptr(), buf, s.len()) };
    }
}

/// # Safety
/// `map` must be null or a live handle from [`omni_generate`].
unsafe fn map_ref<'a>(map: *const OmniMap) -> Result<&'a Map, Fail> {
    if map.is_null() {
        return Err(argument("the map is null"));
    }
    // SAFETY: a live handle is a `Box<Map>` leaked by `omni_generate`.
    Ok(unsafe { &*(map as *const Map) })
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
fn params(json: &str) -> Result<Vec<(String, String)>, Fail> {
    if json.trim().is_empty() {
        return Ok(Vec::new());
    }
    let map: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(json).map_err(|e| argument(format!("params: {e}")))?;
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

/// The engine version ("0.1.0"), as text. Returns its length; copies it
/// when `cap` is enough.
///
/// # Safety
/// `buf` must point at `cap` writable bytes, or `cap` be 0.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn omni_version(buf: *mut u8, cap: usize) -> usize {
    // SAFETY: the caller's promise; `len` null is allowed.
    unsafe { give_text(ENGINE_VERSION, buf, cap, std::ptr::null_mut()) };
    ENGINE_VERSION.len()
}

/// The message of the last error on this thread. Returns its length;
/// copies it when `cap` is enough.
///
/// # Safety
/// `buf` must point at `cap` writable bytes, or `cap` be 0.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn omni_last_error(buf: *mut u8, cap: usize) -> usize {
    LAST_ERROR.with(|e| {
        let e = e.borrow();
        // SAFETY: the caller's promise.
        unsafe { give_text(&e, buf, cap, std::ptr::null_mut()) };
        e.len()
    })
}

/// Makes a map from a pipeline file (its text, UTF-8).
///
/// - `params`: a JSON object of param overrides, `{"name": value}`, or
///   empty (length 0) for the file's.
/// - `seed`: used when `use_seed`; otherwise the file's, which it must
///   then have.
/// - `salt`: null for the file's (length 0 with a pointer is the empty
///   salt).
///
/// On `OMNI_OK`, `*out` is the map, to free with [`omni_map_free`].
///
/// # Safety
/// Each pointer must point at its length of readable bytes (or be null as
/// allowed above), and `out` at a writable pointer.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn omni_generate(
    pipeline: *const u8,
    pipeline_len: usize,
    params_json: *const u8,
    params_len: usize,
    use_seed: bool,
    seed: u64,
    salt: *const u8,
    salt_len: usize,
    out: *mut *mut OmniMap,
) -> i32 {
    guard(|| {
        if out.is_null() {
            return Err(argument("out is null"));
        }
        // SAFETY: the caller's promises.
        let (file, overrides) = unsafe {
            (
                text(pipeline, pipeline_len, "the pipeline")?,
                params(text(params_json, params_len, "params")?)?,
            )
        };
        let salt = if salt.is_null() {
            None
        } else {
            // SAFETY: the caller's promise.
            Some(unsafe { text(salt, salt_len, "the salt")? })
        };
        let map = make(file, &overrides, use_seed.then_some(seed), salt)?;
        let handle = Box::into_raw(Box::new(map)) as *mut OmniMap;
        // SAFETY: `out` is writable (checked not null above).
        unsafe { *out = handle };
        Ok(())
    })
}

fn make(
    file: &str,
    overrides: &[(String, String)],
    seed: Option<u64>,
    salt: Option<&str>,
) -> Result<Map, Fail> {
    let pipeline_error = |m: String| (OMNI_ERR_PIPELINE, m);
    let spec = PipelineSpec::parse_with(file, overrides)
        .map_err(|e| pipeline_error(diagnostics(&e)))?
        .spec;
    let Built {
        grid,
        tiles,
        pipeline,
    } = spec
        .build(&Registry::with_builtins())
        .map_err(|e| pipeline_error(diagnostics(&e)))?;
    let seed = seed
        .or(spec.seed)
        .ok_or_else(|| pipeline_error("/seed: the pipeline has no seed: give one".into()))?;
    let salt = salt.unwrap_or(&spec.salt);
    let made = pipeline
        .run_attempts(&grid, salt, seed, spec.attempts)
        .map_err(|e| pipeline_error(e.to_string()))?;
    let mut provenance = spec.provenance(seed, salt);
    provenance.attempt = made.attempt;
    Ok(Map {
        layers: made.grid.names().map(str::to_string).collect(),
        grid: made.grid,
        tiles,
        provenance,
        objects: OnceCell::new(),
        omap: OnceCell::new(),
    })
}

/// Frees a map. Null is allowed and does nothing.
///
/// # Safety
/// `map` must be null or a live handle, not used again after this.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn omni_map_free(map: *mut OmniMap) {
    if !map.is_null() {
        // SAFETY: a live handle is a leaked `Box<Map>`, freed once.
        drop(unsafe { Box::from_raw(map as *mut Map) });
    }
}

/// The map's size, origin, seed and counts.
///
/// # Safety
/// `map` a live handle; `out` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn omni_map_info(map: *const OmniMap, out: *mut OmniMapInfo) -> i32 {
    guard(|| {
        // SAFETY: the caller's promise.
        let m = unsafe { map_ref(map)? };
        if out.is_null() {
            return Err(argument("out is null"));
        }
        let b = m.grid.bounds();
        let info = OmniMapInfo {
            width: b.size.width,
            height: b.size.height,
            depth: b.size.depth,
            origin_x: b.origin.x,
            origin_y: b.origin.y,
            origin_z: b.origin.z,
            seed: m.provenance.seed,
            attempt: m.provenance.attempt,
            layer_count: m.layers.len() as u32,
            tile_count: m.tiles.len() as u32,
        };
        // SAFETY: checked not null.
        unsafe { *out = info };
        Ok(())
    })
}

/// Layer `i` (in draw order, the first at the bottom): its kind
/// (`OMNI_KIND_*`) and its name, as text (see the module for the two asks).
///
/// # Safety
/// `map` a live handle; `kind` and `name_len` writable or null; `name`
/// `cap` writable bytes or `cap` 0.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn omni_map_layer(
    map: *const OmniMap,
    i: u32,
    kind: *mut i32,
    name: *mut u8,
    cap: usize,
    name_len: *mut usize,
) -> i32 {
    guard(|| {
        // SAFETY: the caller's promise.
        let m = unsafe { map_ref(map)? };
        let layer = m.layers.get(i as usize).ok_or_else(|| {
            (
                OMNI_ERR_LAYER,
                format!("no layer {i}: the map has {}", m.layers.len()),
            )
        })?;
        let id = m
            .grid
            .layer_id(layer)
            .map_err(|e| (OMNI_ERR_LAYER, e.to_string()))?;
        let k = match m
            .grid
            .kind(id)
            .map_err(|e| (OMNI_ERR_LAYER, e.to_string()))?
        {
            LayerKind::Tiles => OMNI_KIND_TILES,
            LayerKind::Scalar => OMNI_KIND_SCALAR,
            LayerKind::Index => OMNI_KIND_INDEX,
            LayerKind::Objects => OMNI_KIND_OBJECTS,
        };
        if !kind.is_null() {
            // SAFETY: the caller's promise.
            unsafe { *kind = k };
        }
        // SAFETY: the caller's promise.
        unsafe { give_text(layer, name, cap, name_len) };
        Ok(())
    })
}

/// The name of tile id `i` (`0` is "empty"), as text.
///
/// # Safety
/// `map` a live handle; `name` `cap` writable bytes or `cap` 0; `name_len`
/// writable or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn omni_map_tile_name(
    map: *const OmniMap,
    i: u32,
    name: *mut u8,
    cap: usize,
    name_len: *mut usize,
) -> i32 {
    guard(|| {
        // SAFETY: the caller's promise.
        let m = unsafe { map_ref(map)? };
        let tile = m.tiles.names().nth(i as usize).ok_or_else(|| {
            (
                OMNI_ERR_LAYER,
                format!("no tile {i}: the map has {}", m.tiles.len()),
            )
        })?;
        // SAFETY: the caller's promise.
        unsafe { give_text(tile, name, cap, name_len) };
        Ok(())
    })
}

/// Checks `len` against the map's cell count and finds layer `i`.
fn cells_of(m: &Map, i: u32, len: usize, dst_null: bool) -> Result<&str, Fail> {
    let cells = m.grid.bounds().size.len();
    if len != cells {
        return Err((
            OMNI_ERR_LAYER,
            format!("the buffer holds {len} cells; the map has {cells}"),
        ));
    }
    if dst_null && len > 0 {
        return Err(argument("the buffer is null"));
    }
    m.layers.get(i as usize).map(String::as_str).ok_or_else(|| {
        (
            OMNI_ERR_LAYER,
            format!("no layer {i}: the map has {}", m.layers.len()),
        )
    })
}

fn wrong_kind(e: impl std::fmt::Display) -> Fail {
    (OMNI_ERR_LAYER, e.to_string())
}

/// Copies tile layer `i` into `dst`: one tile id per cell, `len` the
/// map's cell count.
///
/// # Safety
/// `map` a live handle; `dst` `len` writable `u16`s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn omni_map_copy_tiles(
    map: *const OmniMap,
    i: u32,
    dst: *mut u16,
    len: usize,
) -> i32 {
    guard(|| {
        // SAFETY: the caller's promise.
        let m = unsafe { map_ref(map)? };
        let name = cells_of(m, i, len, dst.is_null())?;
        let id = m.grid.layer_id(name).map_err(wrong_kind)?;
        let layer = m.grid.tiles(id).map_err(wrong_kind)?.as_slice();
        if len > 0 {
            // SAFETY: `len` writable u16s, and `layer.len() == len`.
            let out = unsafe { std::slice::from_raw_parts_mut(dst, len) };
            for (o, t) in out.iter_mut().zip(layer) {
                *o = t.index();
            }
        }
        Ok(())
    })
}

/// Copies index layer `i` (variants, regions) into `dst`.
///
/// # Safety
/// `map` a live handle; `dst` `len` writable `u32`s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn omni_map_copy_index(
    map: *const OmniMap,
    i: u32,
    dst: *mut u32,
    len: usize,
) -> i32 {
    guard(|| {
        // SAFETY: the caller's promise.
        let m = unsafe { map_ref(map)? };
        let name = cells_of(m, i, len, dst.is_null())?;
        let id = m.grid.layer_id(name).map_err(wrong_kind)?;
        let layer = m.grid.indices(id).map_err(wrong_kind)?.as_slice();
        if len > 0 {
            // SAFETY: `len` writable u32s; the same length.
            unsafe { std::ptr::copy_nonoverlapping(layer.as_ptr(), dst, len) };
        }
        Ok(())
    })
}

/// Copies scalar layer `i` (heights, densities) into `dst`.
///
/// # Safety
/// `map` a live handle; `dst` `len` writable `f32`s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn omni_map_copy_scalars(
    map: *const OmniMap,
    i: u32,
    dst: *mut f32,
    len: usize,
) -> i32 {
    guard(|| {
        // SAFETY: the caller's promise.
        let m = unsafe { map_ref(map)? };
        let name = cells_of(m, i, len, dst.is_null())?;
        let id = m.grid.layer_id(name).map_err(wrong_kind)?;
        let layer = m.grid.scalars(id).map_err(wrong_kind)?.as_slice();
        if len > 0 {
            // SAFETY: `len` writable f32s; the same length.
            unsafe { std::ptr::copy_nonoverlapping(layer.as_ptr(), dst, len) };
        }
        Ok(())
    })
}

/// Gives a cached text, making it on the first ask.
///
/// # Safety
/// As [`give_text`].
unsafe fn give_cached(
    cell: &OnceCell<Result<String, String>>,
    make: impl FnOnce() -> Result<String, String>,
    buf: *mut u8,
    cap: usize,
    len: *mut usize,
) -> Result<(), Fail> {
    let text = cell
        .get_or_init(make)
        .as_ref()
        .map_err(|e| (OMNI_ERR_EXPORT, e.clone()))?;
    // SAFETY: the caller's promise.
    unsafe { give_text(text, buf, cap, len) };
    Ok(())
}

fn utf8(bytes: Vec<u8>) -> Result<String, String> {
    String::from_utf8(bytes).map_err(|e| e.to_string())
}

/// The map's objects as JSON: the `objects` array of output v2, the same
/// shape a `.omap` file has, so one parser reads both.
///
/// # Safety
/// `map` a live handle; `buf` `cap` writable bytes or `cap` 0; `len`
/// writable or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn omni_map_objects_json(
    map: *const OmniMap,
    buf: *mut u8,
    cap: usize,
    len: *mut usize,
) -> i32 {
    guard(|| {
        // SAFETY: the caller's promise.
        let m = unsafe { map_ref(map)? };
        let make = || {
            let mut out = Vec::new();
            export::write_objects(&mut out, &m.grid).map_err(|e| e.to_string())?;
            utf8(out)
        };
        // SAFETY: the caller's promise.
        unsafe { give_cached(&m.objects, make, buf, cap, len) }
    })
}

/// The whole map as output v2 (a `.omap` file's text): to save a seed
/// worth keeping.
///
/// # Safety
/// As [`omni_map_objects_json`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn omni_map_to_omap(
    map: *const OmniMap,
    buf: *mut u8,
    cap: usize,
    len: *mut usize,
) -> i32 {
    guard(|| {
        // SAFETY: the caller's promise.
        let m = unsafe { map_ref(map)? };
        let make = || {
            let mut out = Vec::new();
            export::write_json(&mut out, &m.grid, &m.tiles, &m.provenance, Encoding::Auto)
                .map_err(|e| e.to_string())?;
            utf8(out)
        };
        // SAFETY: the caller's promise.
        unsafe { give_cached(&m.omap, make, buf, cap, len) }
    })
}

#[cfg(test)]
mod tests;
