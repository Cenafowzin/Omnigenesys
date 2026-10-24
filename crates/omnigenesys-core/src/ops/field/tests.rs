//! Noise, Gradient, Combine and the scalar_in_range and edge_distance
//! conditions, through pipeline files as a user writes them.

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
        "layers": ["terreno", {"name": "altura", "kind": "scalar"},
                   {"name": "distancia", "kind": "scalar"}],
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
    scalars(g, "altura")
}

fn scalars(g: &Grid, layer: &str) -> Vec<f32> {
    g.scalars(g.layer_id(layer).unwrap())
        .unwrap()
        .as_slice()
        .to_vec()
}

fn terrain(g: &Grid) -> Vec<crate::tile::TileId> {
    g.tiles(g.layer_id("terreno").unwrap())
        .unwrap()
        .as_slice()
        .to_vec()
}

fn tile(name: &str) -> crate::tile::TileId {
    crate::tile::TileRegistry::new(["agua", "areia", "grama"])
        .unwrap()
        .id(name)
        .unwrap()
}

/// A `Fill` of `tile` where `altura` is in `min..max`.
fn band(id: &str, tile: &str, min: Option<f64>, max: Option<f64>) -> Value {
    json!({"id": id, "operator": "Fill", "config": {"layer": "terreno", "tile": tile},
           "conditions": [{"type": "scalar_in_range", "layer": "altura", "min": min, "max": max}]})
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
fn gradient_follows_its_formula() {
    for (shape, invert) in [("square", false), ("round", false), ("round", true)] {
        let g = run(&pipeline(
            (8, 6),
            json!([{"id": "centro", "operator": "Gradient",
                    "config": {"layer": "distancia", "shape": shape, "invert": invert}}]),
        ));
        let out = scalars(&g, "distancia");
        for y in 0..6 {
            for x in 0..8 {
                let nx = (2.0 * (x as f64 + 0.5) / 8.0 - 1.0) as f32;
                let ny = (2.0 * (y as f64 + 0.5) / 6.0 - 1.0) as f32;
                let d = if shape == "square" {
                    1.0 - (1.0 - nx * nx) * (1.0 - ny * ny)
                } else {
                    ((nx * nx + ny * ny) / std::f32::consts::SQRT_2).min(1.0)
                };
                let want = if invert { 1.0 - d } else { d };
                assert_eq!(out[y * 8 + x], want, "{shape} at ({x}, {y})");
            }
        }
    }
    let g = run(&pipeline(
        (4, 2),
        json!([{"id": "lado", "operator": "Gradient",
                "config": {"layer": "distancia", "shape": "horizontal"}}]),
    ));
    assert_eq!(
        scalars(&g, "distancia"),
        [0.125, 0.375, 0.625, 0.875, 0.125, 0.375, 0.625, 0.875]
    );
}

#[test]
fn combine_does_each_operation_cell_by_cell() {
    let steps = |op: &str, mix: Option<f64>, clamp: bool| {
        json!([noise(5.0),
            {"id": "centro", "operator": "Gradient", "config": {"layer": "distancia", "shape": "round"}},
            {"id": "junta", "operator": "Combine", "config": {
                "a": "altura", "b": "distancia", "op": op, "mix": mix, "layer": "altura", "clamp": clamp}}])
    };
    let base = run(&pipeline((8, 6), json!([noise(5.0)])));
    let base_g = run(&pipeline(
        (8, 6),
        json!([{"id": "centro", "operator": "Gradient", "config": {"layer": "distancia", "shape": "round"}}]),
    ));
    let (a, b) = (heights(&base), scalars(&base_g, "distancia"));
    type F = fn(f32, f32) -> f32;
    let cases: [(&str, Option<f64>, F); 6] = [
        ("lerp", Some(0.25), |x, y| x + (y - x) * 0.25),
        ("add", None, |x, y| x + y),
        ("subtract", None, |x, y| x - y),
        ("multiply", None, |x, y| x * y),
        ("min", None, f32::min),
        ("max", None, f32::max),
    ];
    for (op, mix, f) in cases {
        for clamp in [false, true] {
            let out = heights(&run(&pipeline((8, 6), steps(op, mix, clamp))));
            for i in 0..a.len() {
                let v = f(a[i], b[i]);
                let want = if clamp { v.clamp(0.0, 1.0) } else { v };
                assert_eq!(out[i], want, "{op} clamp {clamp} at {i}");
            }
        }
    }
}

#[test]
fn bands_are_fills_with_half_open_ranges() {
    // Classify's bands, as steps: each `max` is the next band's `min`, and
    // a value right at a limit belongs to the band above it.
    let g = run(&pipeline(
        (40, 30),
        json!([
            noise(8.0),
            band("mar", "agua", None, Some(0.4)),
            band("praia", "areia", Some(0.4), Some(0.5)),
            band("campo", "grama", Some(0.5), None)
        ]),
    ));
    let h = heights(&g);
    let t = terrain(&g);
    for (v, got) in h.iter().zip(&t) {
        let want = if *v < 0.4 {
            tile("agua")
        } else if *v < 0.5 {
            tile("areia")
        } else {
            tile("grama")
        };
        assert_eq!(*got, want, "{v}");
    }
    assert!(t.contains(&tile("agua")) && t.contains(&tile("grama")));
}

#[test]
fn scalar_in_range_is_half_open() {
    let g = run(&pipeline(
        (40, 30),
        json!([noise(8.0), band("praia", "areia", Some(0.45), Some(0.55))]),
    ));
    let h = heights(&g);
    for (v, t) in h.iter().zip(terrain(&g)) {
        assert_eq!(t.index() != 0, *v >= 0.45f32 && *v < 0.55f32, "{v}");
    }
    // A value exactly at the limit: a gradient of width 4 gives 0.375.
    let at_limit = |min: Option<f64>, max: Option<f64>| {
        let g = run(&pipeline(
            (4, 1),
            json!([{"id": "lado", "operator": "Gradient",
                    "config": {"layer": "altura", "shape": "horizontal"}},
                   band("b", "areia", min, max)]),
        ));
        terrain(&g)[1].index() != 0
    };
    assert!(at_limit(Some(0.375), None), "min counts");
    assert!(!at_limit(None, Some(0.375)), "max does not");
}

#[test]
fn edge_distance_counts_from_the_maps_edge() {
    let g = run(&pipeline(
        (7, 5),
        json!([{"id": "borda", "operator": "Fill", "config": {"layer": "terreno", "tile": "areia"},
                "conditions": [{"type": "edge_distance", "max": 0}]},
               {"id": "anel", "operator": "Fill", "config": {"layer": "terreno", "tile": "grama"},
                "conditions": [{"type": "edge_distance", "min": 2}]}]),
    ));
    let pic: Vec<String> = terrain(&g)
        .chunks(7)
        .map(|r| {
            r.iter()
                .map(|t| match t.index() {
                    0 => '.',
                    i if tile("areia").index() == i => 's',
                    _ => 'g',
                })
                .collect()
        })
        .collect();
    assert_eq!(pic, ["sssssss", "s.....s", "s.ggg.s", "s.....s", "sssssss"]);
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
    let condition = |c: Value| {
        message(json!([{"id": "a", "operator": "Fill",
            "config": {"layer": "terreno", "tile": "agua"}, "conditions": [c]}]))
    };
    assert!(
        condition(json!({"type": "scalar_in_range", "layer": "altura", "min": 0.5, "max": 0.5}))
            .contains("not below max")
    );
    assert!(condition(json!({"type": "edge_distance"})).contains("at least one of"));
    assert!(condition(json!({"type": "edge_distance", "min": 3, "max": 2})).contains("empty"));
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
        (
            "Combine",
            json!({"a": "altura", "b": "distancia", "op": "lerp", "mix": 2, "layer": "altura"}),
            "not between 0 and 1",
        ),
        (
            "Combine",
            json!({"a": "altura", "b": "distancia", "op": "lerp", "layer": "altura"}),
            "lerp needs mix",
        ),
        (
            "Combine",
            json!({"a": "altura", "b": "distancia", "op": "add", "mix": 0.5, "layer": "altura"}),
            "only lerp takes mix",
        ),
        (
            "Combine",
            json!({"a": "altura", "b": "terreno", "op": "add", "layer": "altura"}),
            "expected scalar",
        ),
        (
            "Gradient",
            json!({"layer": "altura", "shape": "diamond"}),
            "shape",
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
