//! The binary as a user runs it: arguments in, bytes and exit code out.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use omnigenesys_core::export::{self, Encoding};
use omnigenesys_core::operator::Registry;
use omnigenesys_core::spec::{Built, PipelineSpec};

const ISLAND: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../crates/omnigenesys-core/tests/pipelines/island.json"
);

fn mapgen(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mapgen"))
        .args(args)
        .env_remove("OMNIGENESYS_SALT")
        .output()
        .unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8(o.stdout.clone()).unwrap()
}

fn stderr(o: &Output) -> String {
    String::from_utf8(o.stderr.clone()).unwrap()
}

/// A fresh directory under target/ for one test's files.
fn scratch(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(test);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// What the library writes for the island, to compare the binary against.
fn library_output(seed: u64, salt: &str) -> String {
    let text = std::fs::read_to_string(ISLAND).unwrap();
    let spec = PipelineSpec::parse(&text).unwrap().spec;
    let Built {
        mut grid,
        tiles,
        pipeline,
    } = spec.build(&Registry::with_builtins()).unwrap();
    pipeline.run(&mut grid, salt, seed).unwrap();
    let mut out = Vec::new();
    let p = spec.provenance(seed, salt);
    export::write_json(&mut out, &grid, &tiles, &p, Encoding::Auto).unwrap();
    String::from_utf8(out).unwrap()
}

#[test]
fn generate_writes_what_the_library_writes() {
    let o = mapgen(&["generate", ISLAND]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(stdout(&o), library_output(512, ""));
    assert_eq!(stderr(&o), "", "nothing on stderr when all is well");
}

#[test]
fn the_seed_flag_wins_over_the_file() {
    let o = mapgen(&["generate", ISLAND, "--seed", "7"]);
    assert_eq!(stdout(&o), library_output(7, ""));
}

#[test]
fn without_any_seed_one_is_drawn_printed_and_recorded() {
    let dir = scratch("drawn_seed");
    let text = std::fs::read_to_string(ISLAND)
        .unwrap()
        .replace("\"seed\": 512,\n", "");
    let file = dir.join("no_seed.json");
    std::fs::write(&file, text).unwrap();
    let o = mapgen(&["generate", file.to_str().unwrap()]);
    assert!(o.status.success(), "{}", stderr(&o));
    let err = stderr(&o);
    let seed: u64 = err
        .strip_prefix("seed: ")
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or_else(|| panic!("no seed in {err:?}"));
    assert!(seed <= omnigenesys_core::spec::MAX_NUMBER_SEED);
    assert!(stdout(&o).contains(&format!("\"seed\": \"{seed}\"")));
}

#[test]
fn a_salt_changes_the_map_and_is_never_written() {
    let o = mapgen(&["generate", ISLAND, "--salt", "my-game"]);
    let out = stdout(&o);
    assert_eq!(out, library_output(512, "my-game"));
    assert_ne!(out, library_output(512, ""));
    assert!(out.contains("\"salted\": true"));
    assert!(!out.contains("my-game"));

    let from_env = Command::new(env!("CARGO_BIN_EXE_mapgen"))
        .args(["generate", ISLAND])
        .env("OMNIGENESYS_SALT", "my-game")
        .output()
        .unwrap();
    assert_eq!(
        stdout(&from_env),
        out,
        "the environment variable does the same"
    );
}

#[test]
fn output_file_and_encoding() {
    let dir = scratch("output_file");
    let file = dir.join("map.json");
    let o = mapgen(&[
        "generate",
        ISLAND,
        "-o",
        file.to_str().unwrap(),
        "--encoding",
        "dense",
    ]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(stdout(&o), "", "the map went to the file");
    let written = std::fs::read_to_string(&file).unwrap();
    assert!(written.contains("\"encoding\": \"dense\""));
    assert!(!written.contains("\"encoding\": \"rle\""));
}

#[test]
fn the_png_has_one_square_per_cell() {
    let dir = scratch("png");
    let file = dir.join("island.png");
    let o = mapgen(&[
        "generate",
        ISLAND,
        "--png",
        file.to_str().unwrap(),
        "--scale",
        "3",
    ]);
    assert!(o.status.success(), "{}", stderr(&o));
    let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&file).unwrap()));
    let reader = decoder.read_info().unwrap();
    let info = reader.info();
    assert_eq!((info.width, info.height), (48 * 3, 16 * 3));
    assert_eq!(info.color_type, png::ColorType::Indexed);
    // "empty" plus the four declared tiles.
    assert_eq!(info.palette.as_ref().unwrap().len(), 5 * 3);
}

#[test]
fn errors_point_into_the_file_and_exit_1() {
    let dir = scratch("errors");
    let text = std::fs::read_to_string(ISLAND)
        .unwrap()
        .replace("\"operator\": \"Scatter\"", "\"operator\": \"Scater\"");
    let file = dir.join("bad.json");
    std::fs::write(&file, &text).unwrap();
    let path = file.to_str().unwrap();
    for command in ["validate", "generate"] {
        let o = mapgen(&[command, path]);
        assert_eq!(o.status.code(), Some(1), "{command}");
        assert_eq!(stdout(&o), "", "{command}: no map on failure");
        let err = stderr(&o);
        assert!(
            err.contains("error: step 'walls': unknown operator 'Scater'"),
            "{err}"
        );
        assert!(err.contains(&format!("{path}:26:7")), "{err}");
        assert!(err.contains("^^^^^^^^^^^^^^^^^^^^"), "{err}");
        assert!(err.ends_with(&format!("{path}: 2 errors\n")), "{err}");
    }
}

#[test]
fn validate_reports_ok_and_warnings() {
    let o = mapgen(&["validate", ISLAND]);
    assert!(o.status.success());
    assert_eq!(
        stderr(&o).lines().last(),
        Some(&*format!("ok: {ISLAND}: 4 steps (4 enabled)"))
    );

    let dir = scratch("warnings");
    let text = std::fs::read_to_string(ISLAND)
        .unwrap()
        .replace("\"lake_margin\": 2", "\"lake_margin\": 2, \"unused\": 1");
    let file = dir.join("unused.json");
    std::fs::write(&file, text).unwrap();
    let o = mapgen(&["validate", file.to_str().unwrap()]);
    assert!(o.status.success(), "a warning is not a failure");
    assert!(stderr(&o).starts_with("warning: param 'unused' is never used"));
}

#[test]
fn a_missing_file_is_an_error_not_a_panic() {
    let o = mapgen(&["validate", "does-not-exist.json"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).starts_with("error: cannot read does-not-exist.json"));
}

#[test]
fn bad_arguments_exit_2() {
    assert_eq!(mapgen(&["generate"]).status.code(), Some(2));
    assert_eq!(
        mapgen(&["generate", ISLAND, "--scale", "2"]).status.code(),
        Some(2)
    );
    assert_eq!(
        mapgen(&["generate", ISLAND, "--seed", "-1"]).status.code(),
        Some(2)
    );
}

#[test]
fn schema_and_operators_go_to_stdout() {
    let o = mapgen(&["schema"]);
    let schema: serde_json::Value = serde_json::from_str(&stdout(&o)).unwrap();
    assert_eq!(schema["$schema"], "http://json-schema.org/draft-07/schema#");
    assert!(stdout(&o).ends_with("}\n"));

    let o = mapgen(&["operators"]);
    assert_eq!(
        stdout(&o),
        "AssignRooms        2d/3d  global\n\
         CellularAutomaton  2d     local\n\
         Classify           2d/3d  local\n\
         ConnectRooms       2d     global\n\
         Falloff            2d/3d  local\n\
         Fill               2d/3d  local\n\
         FillBorder         2d     local\n\
         FillSmallRegions   2d/3d  local\n\
         KeepLargestRegion  2d/3d  global\n\
         Noise              2d     local\n\
         PlaceRooms         2d     global\n\
         RoomGrid           2d     global\n\
         Scatter            2d/3d  local\n"
    );
}

/// The island's map with `wall_chance` changed in the file itself.
fn island_with_wall_chance(chance: &str) -> String {
    let dir = scratch(&format!("wall_chance_{chance}"));
    let text = std::fs::read_to_string(ISLAND)
        .unwrap()
        .replace("\"default\": 0.1", &format!("\"default\": {chance}"));
    let file = dir.join("island.json");
    std::fs::write(&file, text).unwrap();
    stdout(&mapgen(&["generate", file.to_str().unwrap()]))
}

#[test]
fn a_param_from_the_command_line_is_the_same_as_editing_the_file() {
    let o = mapgen(&["generate", ISLAND, "--param", "wall_chance=0.3"]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(stdout(&o), island_with_wall_chance("0.3"));
    assert_ne!(stdout(&o), library_output(512, ""));
}

#[test]
fn a_bad_param_fails_with_what_it_accepts() {
    let o = mapgen(&["validate", ISLAND, "--param", "wall_chance=2"]);
    assert_eq!(o.status.code(), Some(1));
    let err = stderr(&o);
    assert!(
        err.starts_with("error: override wall_chance=2: 2 is outside min..=max"),
        "{err}"
    );
    // Shown at the declaration, which says what the param accepts.
    assert!(err.contains("\"wall_chance\": {"), "{err}");

    let o = mapgen(&["generate", ISLAND, "--param", "wal_chance=0.2"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("no param 'wal_chance' (declared: wall_chance, lake_margin)"));

    for malformed in ["wall_chance", "=0.2"] {
        let o = mapgen(&["generate", ISLAND, "--param", malformed]);
        assert_eq!(o.status.code(), Some(2), "{malformed}");
    }
}

fn png_palette(file: &Path) -> (Vec<u8>, Option<Vec<u8>>) {
    let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(file).unwrap()));
    let reader = decoder.read_info().unwrap();
    let info = reader.info();
    (
        info.palette.as_ref().unwrap().to_vec(),
        info.trns.as_ref().map(|t| t.to_vec()),
    )
}

#[test]
fn a_palette_file_sets_the_colours() {
    let dir = scratch("palette");
    let palette = dir.join("colors.json");
    std::fs::write(
        &palette,
        r##"{"wall": "#102030", "empty": "#000000", "cave": "#ffffff"}"##,
    )
    .unwrap();
    let png = dir.join("island.png");
    let o = mapgen(&[
        "generate",
        ISLAND,
        "-o",
        dir.join("map.json").to_str().unwrap(),
        "--png",
        png.to_str().unwrap(),
        "--palette",
        palette.to_str().unwrap(),
    ]);
    assert!(o.status.success(), "{}", stderr(&o));
    let (colors, trns) = png_palette(&png);
    // Tiles: empty, floor, wall, water, tree.
    assert_eq!(&colors[6..9], &[0x10, 0x20, 0x30]);
    assert_eq!(trns, None, "an empty with a colour is an opaque background");
    assert_eq!(
        stderr(&o),
        "warning: the palette colours 'cave', which is not a tile of this map\n"
    );
}

#[test]
fn a_bad_palette_fails_before_the_map_is_made() {
    let dir = scratch("bad_palette");
    let palette = dir.join("colors.json");
    std::fs::write(&palette, r#"{"wall": "grey"}"#).unwrap();
    let png = dir.join("island.png");
    let o = mapgen(&[
        "generate",
        ISLAND,
        "--png",
        png.to_str().unwrap(),
        "--palette",
        palette.to_str().unwrap(),
    ]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        stderr(&o).contains("'wall': \"grey\" is not a \"#rrggbb\" colour"),
        "{}",
        stderr(&o)
    );
    assert_eq!(stdout(&o), "", "no map");

    let o = mapgen(&["generate", ISLAND, "--palette", palette.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(2), "--palette needs --png");
}
