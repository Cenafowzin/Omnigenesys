//! Noise, the gradients, Combine, Curve and the scalar_in_range and edge_distance
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

fn radial(config: Value) -> Vec<f32> {
    let mut config = config;
    config["layer"] = json!("distancia");
    let g = run(&pipeline(
        (8, 6),
        json!([{"id": "centro", "operator": "RadialGradient", "config": config}]),
    ));
    scalars(&g, "distancia")
}

#[test]
fn radial_gradient_follows_its_formula() {
    // The Lp norm written plainly, without the scaling the operator uses.
    for (shape, p) in [(0.0, 1.0), (0.25, 4.0 / 3.0), (0.5, 2.0), (0.75, 4.0)] {
        let (cx, cy, rx, ry, turn) = (0.4f64, 0.6, 0.3, 0.45, 30f64);
        let out = radial(json!({"x": cx, "y": cy, "radius_x": rx, "radius_y": ry,
                                "rotation": turn, "shape": shape}));
        let (sin, cos) = turn.to_radians().sin_cos();
        for y in 0..6 {
            for x in 0..8 {
                let px = x as f64 + 0.5 - cx * 8.0;
                let py = y as f64 + 0.5 - cy * 6.0;
                // The radii are of the shorter side, 6.
                let u = (px * cos + py * sin) / (rx * 6.0);
                let v = (py * cos - px * sin) / (ry * 6.0);
                let want = (u.abs().powf(p) + v.abs().powf(p)).powf(1.0 / p).min(1.0);
                let got = f64::from(out[y * 8 + x]);
                assert!(
                    (got - want).abs() < 1e-6,
                    "shape {shape} at ({x}, {y}): {got} vs {want}"
                );
            }
        }
    }
    // Square is the larger of the two distances, and with equal radii a
    // square on the 8 × 6 map: 3 cells out both ways, not 4 by 3, and 1
    // past it.
    let out = radial(json!({"shape": 1}));
    for y in 0..6 {
        for x in 0..8 {
            let u = ((x as f64 + 0.5) - 4.0) / 3.0;
            let v = ((y as f64 + 0.5) - 3.0) / 3.0;
            let want = u.abs().max(v.abs()).min(1.0);
            assert_eq!(out[y * 8 + x], want as f32, "({x}, {y})");
        }
    }
}

#[test]
fn radial_gradient_shapes_and_turns() {
    // The defaults: a circle in the middle reaching the nearest edges.
    assert_eq!(
        radial(json!({})),
        radial(json!({"x": 0.5, "y": 0.5, "radius_x": 0.5, "radius_y": 0.5,
                      "rotation": 0, "shape": 0.5}))
    );
    // From diamond to square no cell gets farther: the shape fills its box.
    let shapes: Vec<Vec<f32>> = [0.0, 0.3, 0.5, 0.7, 0.9, 1.0]
        .iter()
        .map(|s| radial(json!({"shape": s})))
        .collect();
    for w in shapes.windows(2) {
        for (a, b) in w[0].iter().zip(&w[1]) {
            assert!(*b <= a + 1e-6, "{a} then {b}");
        }
    }
    // A huge exponent is nearly the square, not 0 inside (no underflow).
    let near = radial(json!({"shape": 0.999}));
    for (a, b) in near.iter().zip(&shapes[5]) {
        assert!((a - b).abs() < 0.01, "{a} vs {b}");
    }
    // Turned a quarter, the radii swap (on a square map).
    let square_map = |config: Value| {
        let mut config = config;
        config["layer"] = json!("distancia");
        let g = run(&pipeline(
            (9, 9),
            json!([{"id": "centro", "operator": "RadialGradient", "config": config}]),
        ));
        scalars(&g, "distancia")
    };
    let turned =
        square_map(json!({"radius_x": 0.2, "radius_y": 0.45, "rotation": 90, "shape": 0.7}));
    let swapped = square_map(json!({"radius_x": 0.45, "radius_y": 0.2, "shape": 0.7}));
    for (a, b) in turned.iter().zip(&swapped) {
        assert!((a - b).abs() < 1e-6, "{a} vs {b}");
    }
}

fn linear(size: (u32, u32), config: Value) -> Vec<f32> {
    let mut config = config;
    config["layer"] = json!("distancia");
    let g = run(&pipeline(
        size,
        json!([{"id": "rampa", "operator": "LinearGradient", "config": config}]),
    ));
    scalars(&g, "distancia")
}

#[test]
fn linear_gradient_runs_any_way_across() {
    let row = [0.125, 0.375, 0.625, 0.875];
    // 0 left to right, the default; 180 back; 90 top to bottom.
    assert_eq!(linear((4, 2), json!({})), [row, row].concat());
    let back = linear((4, 2), json!({"angle": 180}));
    for (i, v) in back.iter().enumerate() {
        assert!((v - (1.0 - row[i % 4])).abs() < 1e-6, "{i}: {v}");
    }
    let down = linear((2, 4), json!({"angle": 90}));
    for (i, v) in down.iter().enumerate() {
        assert!((v - row[i / 2]).abs() < 1e-6, "{i}: {v}");
    }
    // At 45° on a square map: the same along each anti-diagonal, rising
    // from the top-left corner to the bottom-right one.
    let diag = linear((6, 6), json!({"angle": 45}));
    for y in 1..6 {
        for x in 0..5 {
            assert!((diag[y * 6 + x] - diag[(y - 1) * 6 + x + 1]).abs() < 1e-6);
        }
    }
    assert!(diag[0] < 0.1 && diag[35] > 0.9, "{} {}", diag[0], diag[35]);
    // `start` and `end`: 0 before, 1 after, a line between.
    let part = linear((8, 1), json!({"start": 0.25, "end": 0.75}));
    let want = [0.0, 0.0, 0.125, 0.375, 0.625, 0.875, 1.0, 1.0];
    for (v, w) in part.iter().zip(want) {
        assert!((v - w).abs() < 1e-6, "{part:?}");
    }
}

/// `curve` over a ramp from 0 to 1 across `width` cells.
fn curved(width: u32, curve: Value, invert: bool) -> (Vec<f32>, Vec<f32>) {
    let g = run(&pipeline(
        (width, 1),
        json!([{"id": "rampa", "operator": "LinearGradient", "config": {"layer": "distancia"}},
               {"id": "curva", "operator": "Curve", "config": {
                   "from": "distancia", "layer": "altura", "curve": curve, "invert": invert}}]),
    ));
    (scalars(&g, "distancia"), heights(&g))
}

#[test]
fn curve_kinds_follow_their_formulas() {
    type F = fn(f64) -> f64;
    let cases: [(Value, F); 7] = [
        (json!({"kind": "linear"}), |x| x),
        (json!({"kind": "smooth"}), |x| x * x * (3.0 - 2.0 * x)),
        (json!({"kind": "smooth", "order": "quintic"}), |x| {
            6.0 * x.powi(5) - 15.0 * x.powi(4) + 10.0 * x.powi(3)
        }),
        (json!({"kind": "power", "exponent": 2.5}), |x| x.powf(2.5)),
        // Lague's falloff, a = 3 and b = 2.2: the midpoint is b / (1 + b).
        (
            json!({"kind": "sigmoid", "exponent": 3, "midpoint": 0.6875}),
            |x| x.powi(3) / (x.powi(3) + (2.2 - 2.2 * x).powi(3)),
        ),
        (json!({"kind": "terrace", "steps": 4}), |x| {
            (x * 4.0).round() / 4.0
        }),
        (
            json!({"kind": "points", "points": [[0, 0], [0.5, 0.8], [1, 1]],
                   "interpolation": "linear"}),
            |x| {
                if x < 0.5 {
                    x * 1.6
                } else {
                    0.8 + (x - 0.5) * 0.4
                }
            },
        ),
    ];
    for (curve, f) in cases {
        for invert in [false, true] {
            let (input, out) = curved(11, curve.clone(), invert);
            for (x, y) in input.iter().zip(&out) {
                let v = f(f64::from(*x));
                let want = if invert { 1.0 - v } else { v };
                assert!(
                    (f64::from(*y) - want).abs() < 1e-6,
                    "{curve} {invert} at {x}: {y} vs {want}"
                );
            }
        }
    }
}

#[test]
fn curve_shapes_behave() {
    // Sigmoid with exponent 1 and midpoint ½ is the line; any exponent
    // crosses ½ at the midpoint.
    let (input, out) = curved(11, json!({"kind": "sigmoid", "exponent": 1}), false);
    for (x, y) in input.iter().zip(&out) {
        assert!((x - y).abs() < 1e-6);
    }
    let (_, out) = curved(1, json!({"kind": "sigmoid", "exponent": 4}), false);
    assert!((out[0] - 0.5).abs() < 1e-6);
    // Smooth terraces go back to the line as the smoothness grows.
    let (input, out) = curved(
        11,
        json!({"kind": "terrace", "steps": 3, "smoothness": 1e9}),
        false,
    );
    for (x, y) in input.iter().zip(&out) {
        assert!((x - y).abs() < 1e-6, "{x} {y}");
    }
    // A monotone curve goes through its points and never overshoots them,
    // where a plain cubic would after the steep rise.
    let (_, out) = curved(
        200,
        json!({"kind": "points", "points": [[0, 0], [0.3, 0.9], [0.6, 1], [1, 1]]}),
        false,
    );
    for w in out.windows(2) {
        assert!(w[1] >= w[0] - 1e-6, "rises only: {} then {}", w[0], w[1]);
    }
    assert!(out.iter().all(|v| (0.0..=1.0).contains(v)));
    let (_, out) = curved(
        5,
        json!({"kind": "points", "points": [[0, 0.2], [0.5, 0.7], [1, 0.2]]}),
        false,
    );
    assert!((out[2] - 0.7).abs() < 1e-6, "through the point: {out:?}");
    let (input, out) = curved(
        11,
        json!({"kind": "points", "points": [[0.2, 0.3], [0.7, 0.6]]}),
        false,
    );
    assert!(input[0] < 0.2 && input[10] > 0.7);
    assert_eq!((out[0], out[10]), (0.3, 0.6), "flat outside the points");
    // The value is read between 0 and 1: three times the ramp is read as 1
    // at the right end.
    let g = run(&pipeline(
        (3, 1),
        json!([{"id": "rampa", "operator": "LinearGradient", "config": {"layer": "distancia"}},
               {"id": "triplo", "operator": "Combine", "config": {
                   "inputs": [{"layer": "distancia", "weight": 3}, {"layer": "altura"}],
                   "op": "sum", "layer": "distancia"}},
               {"id": "curva", "operator": "Curve", "config": {"from": "distancia", "layer": "altura",
                "curve": {"kind": "power", "exponent": 2}}}]),
    ));
    assert_eq!(heights(&g)[2], 1.0);
}

/// `pipeline` with a third scalar layer, `lado`.
fn three_scalars(size: (u32, u32), steps: Value) -> Value {
    let mut p = pipeline(size, steps);
    p["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name": "lado", "kind": "scalar"}));
    p
}

#[test]
fn combine_does_each_operation_cell_by_cell() {
    let sources = json!([noise(5.0),
        {"id": "centro", "operator": "RadialGradient", "config": {"layer": "distancia"}},
        {"id": "faixa", "operator": "LinearGradient", "config": {"layer": "lado"}}]);
    let steps = |op: &str, weights: [Option<f64>; 3], clamp: bool| {
        let inputs: Vec<Value> = ["altura", "distancia", "lado"]
            .iter()
            .zip(weights)
            .map(|(l, w)| match w {
                Some(w) => json!({"layer": l, "weight": w}),
                None => json!({"layer": l}),
            })
            .collect();
        let mut s = sources.as_array().unwrap().clone();
        s.push(json!({"id": "junta", "operator": "Combine", "config": {
            "inputs": inputs, "op": op, "layer": "altura", "clamp": clamp}}));
        Value::Array(s)
    };
    let base = run(&three_scalars((8, 6), sources.clone()));
    let (a, b, c) = (
        heights(&base),
        scalars(&base, "distancia"),
        scalars(&base, "lado"),
    );
    type F = fn(f32, f32, f32) -> f32;
    let cases: [(&str, [Option<f64>; 3], F); 6] = [
        ("sum", [None; 3], |x, y, z| x + y + z),
        ("sum", [Some(0.5), Some(0.3), Some(0.2)], |x, y, z| {
            0.5 * x + 0.3 * y + 0.2 * z
        }),
        ("sum", [None, Some(-1.0), None], |x, y, z| x - y + z),
        ("multiply", [None; 3], |x, y, z| x * y * z),
        ("min", [None; 3], |x, y, z| x.min(y).min(z)),
        ("max", [None; 3], |x, y, z| x.max(y).max(z)),
    ];
    for (op, weights, f) in cases {
        for clamp in [false, true] {
            let out = heights(&run(&three_scalars((8, 6), steps(op, weights, clamp))));
            for i in 0..a.len() {
                let v = f(a[i], b[i], c[i]);
                let want = if clamp { v.clamp(0.0, 1.0) } else { v };
                assert!(
                    (out[i] - want).abs() < 1e-6,
                    "{op} {weights:?} clamp {clamp} at {i}: {} vs {want}",
                    out[i]
                );
            }
        }
    }
}

#[test]
fn weights_summing_to_one_keep_the_mix_between_the_inputs() {
    // What `lerp` was: two weights summing to 1, no clamp needed.
    let g = run(&pipeline(
        (16, 12),
        json!([noise(5.0),
            {"id": "centro", "operator": "RadialGradient", "config": {"layer": "distancia"}},
            {"id": "costa", "operator": "Combine", "config": {
                "inputs": [{"layer": "altura", "weight": 0.45}, {"layer": "distancia", "weight": 0.55}],
                "op": "sum", "layer": "distancia"}}]),
    ));
    let base = run(&pipeline((16, 12), json!([noise(5.0)])));
    let (mixed, a) = (scalars(&g, "distancia"), heights(&base));
    let gradient = scalars(
        &run(&pipeline(
            (16, 12),
            json!([{"id": "centro", "operator": "RadialGradient", "config": {"layer": "distancia"}}]),
        )),
        "distancia",
    );
    for i in 0..mixed.len() {
        let (lo, hi) = (a[i].min(gradient[i]), a[i].max(gradient[i]));
        assert!(mixed[i] >= lo - 1e-6 && mixed[i] <= hi + 1e-6, "at {i}");
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
            json!([{"id": "lado", "operator": "LinearGradient", "config": {"layer": "altura"}},
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
            json!({"inputs": [{"layer": "altura"}], "op": "sum", "layer": "altura"}),
            "at least two layers, got 1",
        ),
        (
            "Combine",
            json!({"inputs": [{"layer": "altura"}, {"layer": "altura"}], "op": "sum", "layer": "altura"}),
            "`altura` is listed twice",
        ),
        (
            "Combine",
            json!({"inputs": [{"layer": "altura"}, {"layer": "distancia", "weight": 2}],
                   "op": "max", "layer": "altura"}),
            "only sum takes weights",
        ),
        (
            "Combine",
            json!({"inputs": [{"layer": "altura"}, {"layer": "distancia"}], "op": "lerp", "layer": "altura"}),
            "op",
        ),
        (
            "Combine",
            json!({"inputs": [{"layer": "altura"}, {"layer": "terreno"}], "op": "sum", "layer": "altura"}),
            "expected scalar",
        ),
        (
            "RadialGradient",
            json!({"layer": "altura", "shape": 1.5}),
            "not between 0 and 1",
        ),
        (
            "RadialGradient",
            json!({"layer": "altura", "radius_y": 0}),
            "radius_y",
        ),
        (
            "RadialGradient",
            json!({"layer": "altura", "shape": "round"}),
            "shape",
        ),
        (
            "LinearGradient",
            json!({"layer": "altura", "start": 0.6, "end": 0.6}),
            "not above start",
        ),
        (
            "LinearGradient",
            json!({"layer": "altura", "end": 2}),
            "not between 0 and 1",
        ),
        (
            "Curve",
            json!({"from": "altura", "layer": "altura", "curve": {"kind": "power", "exponent": 0}}),
            "not above 0",
        ),
        (
            "Curve",
            json!({"from": "altura", "layer": "altura",
                   "curve": {"kind": "sigmoid", "exponent": 2, "midpoint": 1}}),
            "midpoint",
        ),
        (
            "Curve",
            json!({"from": "altura", "layer": "altura", "curve": {"kind": "terrace", "steps": 0}}),
            "at least 1",
        ),
        (
            "Curve",
            json!({"from": "altura", "layer": "altura", "curve": {"kind": "points", "points": [[0, 0]]}}),
            "at least two points",
        ),
        (
            "Curve",
            json!({"from": "altura", "layer": "altura",
                   "curve": {"kind": "points", "points": [[0.5, 0], [0.5, 1]]}}),
            "does not come after",
        ),
        (
            "Curve",
            json!({"from": "altura", "layer": "altura",
                   "curve": {"kind": "power", "exponent": 2, "steps": 3}}),
            "steps",
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
        attempt: 0,
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
