//! Noise, Falloff, Classify and the scalar_in_range condition, through
//! pipeline files as a user writes them.

use serde_json::{Value, json};

use crate::{
    bounds::Bounds,
    coords::{Coord, GridSize},
    export::{self, Encoding, ExportError},
    grid::{Grid, GridError},
    operator::{BuildError, Registry},
    spec::{Built, Diagnostic, PipelineSpec},
};

fn pipeline(size: (u32, u32), steps: Value) -> Value {
    json!({
        "version": 2, "seed": 512, "size": {"width": size.0, "height": size.1},
        "layers": ["terreno", {"name": "altura", "kind": "scalar"}],
        "tiles": ["agua", "areia", "grama"],
        "steps": steps,
    })
}

fn build(p: &Value) -> Result<Built, Vec<Diagnostic>> {
    PipelineSpec::parse(&p.to_string())
        .unwrap()
        .spec
        .build(&Registry::with_builtins())
}

fn run(p: &Value) -> Grid {
    let Built {
        mut grid, pipeline, ..
    } = build(p).unwrap_or_else(|e| panic!("{e:#?}"));
    pipeline.run(&mut grid, "", 512).unwrap();
    grid
}

fn heights(g: &Grid) -> Vec<f32> {
    g.scalars(g.layer_id("altura").unwrap())
        .unwrap()
        .as_slice()
        .to_vec()
}

fn noise(wavelength: f64) -> Value {
    json!({"id": "relevo", "operator": "Noise", "config": {"layer": "altura", "wavelength": wavelength}})
}

#[test]
fn noise_fills_zero_to_one_and_depends_on_the_step() {
    let g = run(&pipeline((64, 48), json!([noise(16.0)])));
    let h = heights(&g);
    assert!(h.iter().all(|v| (0.0..=1.0).contains(v)));
    let (lo, hi) = h
        .iter()
        .fold((1.0f32, 0.0f32), |(l, u), &v| (l.min(v), u.max(v)));
    assert!(lo < 0.3 && hi > 0.7, "uses the range: {lo}..{hi}");
    // Same pipeline, same field; another step id, another field.
    assert_eq!(h, heights(&run(&pipeline((64, 48), json!([noise(16.0)])))));
    let mut other = noise(16.0);
    other["id"] = json!("outro");
    assert_ne!(h, heights(&run(&pipeline((64, 48), json!([other])))));
}

#[test]
fn a_longer_wavelength_is_smoother() {
    let roughness = |w: f64| {
        let h = heights(&run(&pipeline((80, 80), json!([noise(w)]))));
        h.windows(2)
            .map(|p| f64::from((p[1] - p[0]).abs()))
            .sum::<f64>()
            / h.len() as f64
    };
    let (fine, broad) = (roughness(4.0), roughness(40.0));
    assert!(broad * 3.0 < fine, "{broad} vs {fine}");
}

#[test]
fn noise_agrees_between_maps_that_overlap() {
    // The chunk promise: a cell's value depends on its world position only.
    let field = |origin: Coord, size: GridSize| {
        let mut grid = Grid::new(Bounds::new(origin, size));
        let spec = PipelineSpec::parse(
            &pipeline((size.width, size.height), json!([noise(12.0)])).to_string(),
        )
        .unwrap()
        .spec;
        let Built {
            pipeline, tiles, ..
        } = spec.build(&Registry::with_builtins()).unwrap();
        let _ = tiles;
        grid.add_layer("terreno", crate::layer::LayerKind::Tiles)
            .unwrap();
        grid.add_layer("altura", crate::layer::LayerKind::Scalar)
            .unwrap();
        pipeline.run(&mut grid, "", 512).unwrap();
        let id = grid.layer_id("altura").unwrap();
        grid.scalars(id).unwrap().clone()
    };
    let a = field(Coord::new(0, 0, 0), GridSize::new(50, 40, 1));
    let b = field(Coord::new(30, -10, 0), GridSize::new(40, 30, 1));
    let mut checked = 0;
    for c in a.bounds().iter() {
        if let Some(v) = b.get(c) {
            assert_eq!(a.get(c), Some(v), "{c:?}");
            checked += 1;
        }
    }
    assert_eq!(checked, 20 * 20);
}

#[test]
fn falloff_follows_its_formula() {
    let base = run(&pipeline((8, 6), json!([noise(5.0)])));
    let e = heights(&base);
    for (shape, mix) in [("square", 0.5), ("round", 1.0), ("square", 0.0)] {
        let g = run(&pipeline(
            (8, 6),
            json!([noise(5.0), {"id": "costa", "operator": "Falloff",
                   "config": {"layer": "altura", "shape": shape, "mix": mix}}]),
        ));
        let out = heights(&g);
        for y in 0..6 {
            for x in 0..8 {
                let nx = (2.0 * (x as f64 + 0.5) / 8.0 - 1.0) as f32;
                let ny = (2.0 * (y as f64 + 0.5) / 6.0 - 1.0) as f32;
                let d = if shape == "square" {
                    1.0 - (1.0 - nx * nx) * (1.0 - ny * ny)
                } else {
                    ((nx * nx + ny * ny) / std::f32::consts::SQRT_2).min(1.0)
                };
                let i = y * 8 + x;
                let want = (e[i] + (1.0 - d - e[i]) * mix as f32).clamp(0.0, 1.0);
                assert_eq!(out[i], want, "{shape} {mix} at ({x}, {y})");
            }
        }
    }
}

#[test]
fn classify_paints_bands_in_order() {
    let g = run(&pipeline(
        (40, 30),
        json!([noise(8.0), {"id": "biomas", "operator": "Classify", "config": {
            "from": "altura", "layer": "terreno",
            "bands": [{"max": 0.4, "tile": "agua"}, {"max": 0.5, "tile": "areia"}, {"tile": "grama"}]}}]),
    ));
    let h = heights(&g);
    let t = g
        .tiles(g.layer_id("terreno").unwrap())
        .unwrap()
        .as_slice()
        .to_vec();
    let id = |n: &str| {
        crate::tile::TileRegistry::new(["agua", "areia", "grama"])
            .unwrap()
            .id(n)
            .unwrap()
    };
    for (v, tile) in h.iter().zip(&t) {
        let want = if *v < 0.4 {
            id("agua")
        } else if *v < 0.5 {
            id("areia")
        } else {
            id("grama")
        };
        assert_eq!(*tile, want, "{v}");
    }
    assert!(t.contains(&id("agua")) && t.contains(&id("grama")));
}

#[test]
fn classify_leaves_cells_above_every_band() {
    let g = run(&pipeline(
        (30, 30),
        json!([noise(6.0), {"id": "rasa", "operator": "Classify", "config": {
            "from": "altura", "layer": "terreno", "bands": [{"max": 0.5, "tile": "agua"}]}}]),
    ));
    let h = heights(&g);
    let t = g.tiles(g.layer_id("terreno").unwrap()).unwrap().as_slice();
    for (v, tile) in h.iter().zip(t) {
        assert_eq!(tile.index() != 0, *v < 0.5, "{v}");
    }
}

#[test]
fn scalar_in_range_reads_the_field() {
    let g = run(&pipeline(
        (40, 30),
        json!([noise(8.0), {"id": "praia", "operator": "Fill", "config": {"layer": "terreno", "tile": "areia"},
            "conditions": [{"type": "scalar_in_range", "layer": "altura", "min": 0.45, "max": 0.55}]}]),
    ));
    let h = heights(&g);
    let t = g.tiles(g.layer_id("terreno").unwrap()).unwrap().as_slice();
    for (v, tile) in h.iter().zip(t) {
        assert_eq!(
            tile.index() != 0,
            (0.45..=0.55).contains(&(f64::from(*v))) && *v >= 0.45f32 && *v <= 0.55f32,
            "{v}"
        );
    }
}

#[test]
fn kinds_are_checked_when_the_pipeline_is_built() {
    let message = |steps: Value| {
        let e = build(&pipeline((8, 8), steps))
            .err()
            .expect("the build fails");
        e[0].message.clone()
    };
    // A tile operator on the scalar layer, and noise on a tile layer.
    assert!(
        message(
            json!([{"id": "a", "operator": "Fill", "config": {"layer": "altura", "tile": "agua"}}])
        )
        .contains("layer 'altura' is scalar, expected tiles")
    );
    assert!(message(json!([{"id": "a", "operator": "Noise", "config": {"layer": "terreno", "wavelength": 8}}]))
        .contains("layer 'terreno' is tiles, expected scalar"));
    // A tile condition on the scalar layer, and the other way round.
    assert!(
        message(
            json!([{"id": "a", "operator": "Fill", "config": {"layer": "terreno", "tile": "agua"},
        "conditions": [{"type": "tile_is", "layer": "altura", "tile": "agua"}]}])
        )
        .contains("expected tiles")
    );
    assert!(
        message(
            json!([{"id": "a", "operator": "Fill", "config": {"layer": "terreno", "tile": "agua"},
        "conditions": [{"type": "scalar_in_range", "layer": "terreno", "min": 0.5}]}])
        )
        .contains("expected scalar")
    );
    assert!(
        message(
            json!([{"id": "a", "operator": "Fill", "config": {"layer": "terreno", "tile": "agua"},
        "conditions": [{"type": "scalar_in_range", "layer": "altura"}]}])
        )
        .contains("at least one of `min` or `max`")
    );
}

#[test]
fn bad_field_configs_fail_the_build() {
    let field = |op: &str, config: Value| {
        build(&pipeline(
            (8, 8),
            json!([{"id": "a", "operator": op, "config": config}]),
        ))
        .err()
        .expect("the build fails")[0]
            .message
            .clone()
    };
    for (op, config, needle) in [
        (
            "Noise",
            json!({"layer": "altura", "wavelength": 0.5}),
            "wavelength",
        ),
        (
            "Noise",
            json!({"layer": "altura", "wavelength": 8, "octaves": 0}),
            "octaves",
        ),
        (
            "Noise",
            json!({"layer": "altura", "wavelength": 8, "persistence": 1.5}),
            "persistence",
        ),
        (
            "Noise",
            json!({"layer": "altura", "wavelength": 8, "kind": "billow"}),
            "kind",
        ),
        ("Falloff", json!({"layer": "altura", "mix": 2}), "mix"),
        (
            "Classify",
            json!({"from": "altura", "layer": "terreno", "bands": []}),
            "at least one band",
        ),
        (
            "Classify",
            json!({"from": "altura", "layer": "terreno",
            "bands": [{"max": 0.5, "tile": "agua"}, {"max": 0.4, "tile": "areia"}]}),
            "not above",
        ),
        (
            "Classify",
            json!({"from": "altura", "layer": "terreno",
            "bands": [{"tile": "agua"}, {"max": 0.4, "tile": "areia"}]}),
            "only the last",
        ),
    ] {
        let m = field(op, config.clone());
        assert!(m.contains(needle), "{op} {config}: {m}");
    }
}

#[test]
fn a_scalar_layer_is_exported_and_reads_back() {
    let g = run(&pipeline((7, 5), json!([noise(3.0)])));
    let built = build(&pipeline((7, 5), json!([]))).unwrap();
    let mut out = Vec::new();
    let p = crate::export::Provenance {
        seed: 512,
        salted: false,
        params: Vec::new(),
    };
    export::write_json(&mut out, &g, &built.tiles, &p, Encoding::Auto).unwrap();
    let v: Value = serde_json::from_slice(&out).unwrap();
    let layer = &v["layers"][1];
    assert_eq!(layer["name"], "altura");
    assert_eq!(layer["kind"], "scalar");
    assert_eq!(layer["encoding"], "dense");
    let back: Vec<f32> = layer["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap() as f32)
        .collect();
    assert_eq!(
        back,
        heights(&g),
        "the shortest decimal reads back as the same f32"
    );

    // NaN is an error, not a value.
    let mut g = g;
    let id = g.layer_id("altura").unwrap();
    g.scalars_mut(id).unwrap().as_mut_slice()[3] = f32::NAN;
    let e = export::write_json(&mut Vec::new(), &g, &built.tiles, &p, Encoding::Auto).unwrap_err();
    assert!(
        matches!(e, ExportError::NonFiniteCell { index: 3, .. }),
        "{e}"
    );
}

#[test]
fn the_grid_knows_the_kinds() {
    let mut g = Grid::new(Bounds::new(Coord::ZERO, GridSize::new(2, 2, 1)));
    let t = g.add_layer("t", crate::layer::LayerKind::Tiles).unwrap();
    let s = g.add_layer("s", crate::layer::LayerKind::Scalar).unwrap();
    assert!(g.scalars(s).unwrap().as_slice().iter().all(|v| *v == 0.0));
    assert!(matches!(g.tiles(s), Err(GridError::WrongLayerKind { .. })));
    assert!(matches!(
        g.scalars(t),
        Err(GridError::WrongLayerKind { .. })
    ));
    let _ = BuildError::Grid(GridError::UnknownLayerId { id: t });
}
