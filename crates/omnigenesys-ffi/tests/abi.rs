//! The C ABI as an engine uses it, through the same functions the C#
//! bindings call: the map it makes is the map the core makes.

use std::ptr;

use omnigenesys::*;
use omnigenesys_core::export::{self, Encoding};
use omnigenesys_core::operator::Registry;
use omnigenesys_core::spec::{Built, PipelineSpec};

const LOST_FIELDS: &str = include_str!("../../omnigenesys-core/tests/pipelines/lost_fields.json");
const ILHA: &str = include_str!("../../omnigenesys-core/tests/pipelines/ilha.json");

fn generate(
    file: &str,
    params: &str,
    seed: Option<u64>,
    salt: Option<&str>,
) -> Result<*mut OmniMap, (i32, String)> {
    let mut map = ptr::null_mut();
    // SAFETY: every pointer with its length; `map` writable.
    let code = unsafe {
        omni_generate(
            file.as_ptr(),
            file.len(),
            params.as_ptr(),
            params.len(),
            seed.is_some(),
            seed.unwrap_or(0),
            salt.map_or(ptr::null(), str::as_ptr),
            salt.map_or(0, str::len),
            &mut map,
        )
    };
    if code == OMNI_OK {
        Ok(map)
    } else {
        Err((code, last_error()))
    }
}

fn last_error() -> String {
    // SAFETY: the length first, then a buffer that size.
    unsafe {
        let n = omni_last_error(ptr::null_mut(), 0);
        let mut buf = vec![0u8; n];
        omni_last_error(buf.as_mut_ptr(), n);
        String::from_utf8(buf).unwrap()
    }
}

/// A text the way an engine asks for it: length, then bytes.
fn ask(mut f: impl FnMut(*mut u8, usize, *mut usize) -> i32) -> String {
    let mut n = 0;
    assert_eq!(f(ptr::null_mut(), 0, &mut n), OMNI_OK);
    let mut buf = vec![0u8; n];
    assert_eq!(f(buf.as_mut_ptr(), n, &mut n), OMNI_OK);
    String::from_utf8(buf).unwrap()
}

fn info(map: *mut OmniMap) -> OmniMapInfo {
    let mut out = OmniMapInfo::default();
    // SAFETY: a live map.
    assert_eq!(unsafe { omni_map_info(map, &mut out) }, OMNI_OK);
    out
}

/// The core's own output v2 for `file` with `seed`.
fn core_omap(file: &str, seed: u64) -> String {
    let spec = PipelineSpec::parse(file).unwrap().spec;
    let Built {
        grid,
        tiles,
        pipeline,
    } = spec.build(&Registry::with_builtins()).unwrap();
    let made = pipeline
        .run_attempts(&grid, &spec.salt, seed, spec.attempts)
        .unwrap();
    let mut p = spec.provenance(seed, &spec.salt);
    p.attempt = made.attempt;
    let mut out = Vec::new();
    export::write_json(&mut out, &made.grid, &tiles, &p, Encoding::Auto).unwrap();
    String::from_utf8(out).unwrap()
}

/// A layer's cells as numbers, decoded from the file.
fn file_cells(f: &serde_json::Value) -> Vec<f64> {
    if f["encoding"] == "rle" {
        return f["runs"]
            .as_array()
            .unwrap()
            .chunks(2)
            .flat_map(|r| {
                std::iter::repeat_n(r[0].as_f64().unwrap(), r[1].as_u64().unwrap() as usize)
            })
            .collect();
    }
    let key = if f["kind"] == "scalar" {
        "values"
    } else {
        "cells"
    };
    f[key]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap())
        .collect()
}

#[test]
fn the_map_is_the_cores_map() {
    let map = generate(LOST_FIELDS, "", Some(77), None).unwrap();
    // SAFETY: a live map, freed once at the end.
    let omap = ask(|b, c, l| unsafe { omni_map_to_omap(map, b, c, l) });
    assert_eq!(omap, core_omap(LOST_FIELDS, 77));
    let v: serde_json::Value = serde_json::from_str(&omap).unwrap();

    let i = info(map);
    assert_eq!((i.width, i.height, i.depth, i.seed), (80, 80, 1, 77));
    assert_eq!(i.tile_count as usize, v["tiles"].as_array().unwrap().len());
    for t in 0..i.tile_count {
        // SAFETY: a live map.
        let name = ask(|b, c, l| unsafe { omni_map_tile_name(map, t, b, c, l) });
        assert_eq!(name, v["tiles"][t as usize]);
    }

    // Every layer, by kind, against the file's cells.
    let cells = (i.width * i.height * i.depth) as usize;
    let file_layers = v["layers"].as_array().unwrap();
    let mut written = 0;
    let mut kinds = Vec::new();
    for l in 0..i.layer_count {
        let mut kind = -1;
        // SAFETY: a live map; `kind` writable.
        let name = ask(|b, c, n| unsafe { omni_map_layer(map, l, &mut kind, b, c, n) });
        kinds.push(kind);
        if kind == OMNI_KIND_OBJECTS {
            continue;
        }
        let f = &file_layers[written];
        written += 1;
        assert_eq!(f["name"], name);
        // SAFETY: buffers of the map's cell count.
        let got: Vec<f64> = unsafe {
            match kind {
                OMNI_KIND_TILES => {
                    let mut d = vec![0u16; cells];
                    assert_eq!(omni_map_copy_tiles(map, l, d.as_mut_ptr(), cells), OMNI_OK);
                    d.into_iter().map(f64::from).collect()
                }
                OMNI_KIND_INDEX => {
                    let mut d = vec![0u32; cells];
                    assert_eq!(omni_map_copy_index(map, l, d.as_mut_ptr(), cells), OMNI_OK);
                    d.into_iter().map(f64::from).collect()
                }
                _ => {
                    let mut d = vec![0f32; cells];
                    assert_eq!(
                        omni_map_copy_scalars(map, l, d.as_mut_ptr(), cells),
                        OMNI_OK
                    );
                    // The file holds the shortest decimal of each f32.
                    d.into_iter()
                        .map(|x| x.to_string().parse::<f64>().unwrap())
                        .collect()
                }
            }
        };
        assert_eq!(got, file_cells(f), "layer {name}");
    }
    assert_eq!(written, file_layers.len());
    for k in [
        OMNI_KIND_TILES,
        OMNI_KIND_SCALAR,
        OMNI_KIND_INDEX,
        OMNI_KIND_OBJECTS,
    ] {
        assert!(kinds.contains(&k), "Lost Fields has every kind: {kinds:?}");
    }

    // SAFETY: a live map.
    let objects = ask(|b, c, l| unsafe { omni_map_objects_json(map, b, c, l) });
    let objects: serde_json::Value = serde_json::from_str(&objects).unwrap();
    assert_eq!(objects, v["objects"]);
    assert!(objects.as_array().unwrap().len() > 5);
    // SAFETY: freed once.
    unsafe { omni_map_free(map) };
}

#[test]
fn seed_salt_and_params_override_the_file() {
    let a = generate(ILHA, "", Some(5), None).unwrap();
    let b = generate(ILHA, "", Some(5), Some("outro")).unwrap();
    let c = generate(ILHA, r#"{"tamanho": 0.5}"#, Some(5), None).unwrap();
    let file = generate(ILHA, "", None, None).unwrap();
    let text = |m: *mut OmniMap| {
        // SAFETY: a live map.
        ask(|b, c, l| unsafe { omni_map_to_omap(m, b, c, l) })
    };
    assert_ne!(text(a), text(b), "the salt changes the map");
    assert_ne!(text(a), text(c), "a param changes the map");
    let spec_seed = PipelineSpec::parse(ILHA).unwrap().spec.seed.unwrap();
    assert_eq!(info(file).seed, spec_seed);
    for m in [a, b, c, file] {
        // SAFETY: each freed once.
        unsafe { omni_map_free(m) };
    }
}

#[test]
fn errors_come_with_a_code_and_where() {
    let (code, message) = generate("{ not json", "", Some(1), None).unwrap_err();
    assert_eq!(code, OMNI_ERR_PIPELINE);
    assert!(message.starts_with("1:"), "{message}");

    let typo = LOST_FIELDS.replacen("\"tries\"", "\"triez\"", 1);
    let (code, message) = generate(&typo, "", Some(1), None).unwrap_err();
    assert_eq!(code, OMNI_ERR_PIPELINE);
    assert!(
        message.contains("triez") && message.contains("/steps/"),
        "{message}"
    );

    let (code, message) = generate(ILHA, "[1]", Some(1), None).unwrap_err();
    assert_eq!(code, OMNI_ERR_ARGUMENT, "{message}");

    let bad = [0xffu8, 0xfe];
    let mut map = ptr::null_mut();
    // SAFETY: two readable bytes; `map` writable.
    let code = unsafe {
        omni_generate(
            bad.as_ptr(),
            2,
            ptr::null(),
            0,
            true,
            1,
            ptr::null(),
            0,
            &mut map,
        )
    };
    assert_eq!(code, OMNI_ERR_ARGUMENT);
    assert!(last_error().contains("UTF-8"));
    assert!(map.is_null(), "no map on an error");
}

#[test]
fn wrong_buffers_and_layers_are_refused() {
    let map = generate(LOST_FIELDS, "", Some(1), None).unwrap();
    let cells = 80 * 80;
    let mut d = vec![0u16; cells];
    let mut f = vec![0f32; cells];
    // SAFETY: a live map; buffers of the lengths given.
    unsafe {
        assert_eq!(
            omni_map_copy_tiles(map, 0, d.as_mut_ptr(), cells - 1),
            OMNI_ERR_LAYER
        );
        assert!(last_error().contains("6399"), "{}", last_error());
        assert_eq!(
            omni_map_copy_tiles(map, 999, d.as_mut_ptr(), cells),
            OMNI_ERR_LAYER
        );
        assert_eq!(
            omni_map_copy_scalars(map, 0, f.as_mut_ptr(), cells),
            OMNI_ERR_LAYER
        );
        assert_eq!(
            omni_map_copy_tiles(map, 0, ptr::null_mut(), cells),
            OMNI_ERR_ARGUMENT
        );
        assert_eq!(
            omni_map_info(ptr::null(), &mut OmniMapInfo::default()),
            OMNI_ERR_ARGUMENT
        );
        // A buffer too small for a text: nothing written, the length given.
        let mut n = 0;
        let mut small = [0u8; 2];
        assert_eq!(
            omni_map_to_omap(map, small.as_mut_ptr(), 2, &mut n),
            OMNI_OK
        );
        assert_eq!(small, [0, 0]);
        assert!(n > 1000);
        omni_map_free(map);
        omni_map_free(ptr::null_mut());
    }
}

#[test]
fn the_version_is_the_engines() {
    let mut buf = [0u8; 32];
    // SAFETY: 32 writable bytes.
    let n = unsafe { omni_version(buf.as_mut_ptr(), buf.len()) };
    assert_eq!(&buf[..n], export::ENGINE_VERSION.as_bytes());
}
