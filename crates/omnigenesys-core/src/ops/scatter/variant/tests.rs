use serde_json::{Value as Json, json};

use crate::{
    bounds::Bounds,
    condition::Condition,
    coords::{Coord, GridSize},
    grid::Grid,
    layer::LayerKind,
    operator::{BuildError, Registry},
    pipeline::{Pipeline, StepSpec},
    tile::TileRegistry,
};

const MAP: Bounds = Bounds::new(Coord::new(-10, 4, 0), GridSize::new(80, 60, 1));

fn step(id: &str, op: &str, config: Json, conditions: Json) -> StepSpec {
    StepSpec {
        id: id.to_string(),
        operator: op.to_string(),
        enabled: true,
        config,
        conditions: serde_json::from_value::<Vec<Condition>>(conditions).unwrap(),
    }
}

fn grid(bounds: Bounds) -> Grid {
    let mut g = Grid::chunk(bounds, MAP);
    g.add_layer("vegetacao", LayerKind::Tiles).unwrap();
    g.add_layer("variante", LayerKind::Scalar).unwrap();
    g
}

fn tiles() -> TileRegistry {
    TileRegistry::new(["mato", "flor"]).unwrap()
}

/// Bushes where a scatter puts them, then a variant for each by `weights`.
fn steps(weights: Json, bush_chance: f64) -> Vec<StepSpec> {
    vec![
        step(
            "mato",
            "Scatter",
            json!({"layer": "vegetacao", "tile": "mato", "chance": bush_chance}),
            json!([]),
        ),
        step(
            "variantes",
            "PickVariant",
            json!({"layer": "variante", "weights": weights}),
            json!([{"type": "tile_is", "layer": "vegetacao", "tile": "mato"}]),
        ),
    ]
}

/// The variant layer after `steps` on `bounds`, and the bush cells.
fn run(steps: &[StepSpec], bounds: Bounds) -> (Vec<f32>, Vec<bool>) {
    let mut g = grid(bounds);
    let t = tiles();
    Pipeline::build(&Registry::with_builtins(), &g, &t, steps)
        .unwrap()
        .run(&mut g, "", 7)
        .unwrap();
    let v = g
        .scalars(g.layer_id("variante").unwrap())
        .unwrap()
        .as_slice()
        .to_vec();
    let mato = t.id("mato").unwrap();
    let bush = g
        .tiles(g.layer_id("vegetacao").unwrap())
        .unwrap()
        .as_slice()
        .iter()
        .map(|&id| id == mato)
        .collect();
    (v, bush)
}

#[test]
fn variants_come_by_their_weights_and_only_where_the_conditions_hold() {
    let (v, bush) = run(&steps(json!([6, 0, 3, 1]), 0.5), MAP);
    let mut count = [0usize; 4];
    for (x, b) in v.iter().zip(&bush) {
        if *b {
            count[*x as usize] += 1;
        } else {
            // Not a bush: left as the layer was.
            assert_eq!(*x, 0.0);
        }
    }
    let n: usize = count.iter().sum();
    assert!(n > 2000, "{n}");
    assert_eq!(count[1], 0, "a weight of 0 never comes");
    let share = |i: usize| count[i] as f64 / n as f64;
    assert!((share(0) - 0.6).abs() < 0.03, "{count:?}");
    assert!((share(2) - 0.3).abs() < 0.03, "{count:?}");
    assert!((share(3) - 0.1).abs() < 0.02, "{count:?}");
}

#[test]
fn a_chunk_gets_the_same_variants_and_other_steps_do_not_shuffle_them() {
    let (whole, _) = run(&steps(json!([1, 1, 1]), 1.0), MAP);
    let chunk = Bounds::new(Coord::new(15, 20, 0), GridSize::new(23, 17, 1));
    let (part, _) = run(&steps(json!([1, 1, 1]), 1.0), chunk);
    for (k, c) in chunk.iter().enumerate() {
        assert_eq!(part[k], whole[MAP.index_of(c).unwrap()], "{c:?}");
    }
    // Fewer bushes (another step changed): the cells still bushes keep
    // their variant.
    let (fewer, bush) = run(&steps(json!([1, 1, 1]), 0.3), MAP);
    for (k, b) in bush.iter().enumerate() {
        if *b {
            assert_eq!(fewer[k], whole[k]);
        }
    }
}

#[test]
fn weights_are_checked_when_built() {
    let bad = |weights: Json| {
        let g = grid(MAP);
        match Pipeline::build(
            &Registry::with_builtins(),
            &g,
            &tiles(),
            &steps(weights, 0.5),
        ) {
            Err(e) => e[0].error.clone(),
            Ok(_) => panic!("built"),
        }
    };
    let message = |e: BuildError| e.to_string();
    assert!(message(bad(json!([]))).contains("0 weights"));
    assert!(message(bad(json!([0, 0]))).contains("every weight is 0"));
    assert!(message(bad(json!([1, 2_000_000]))).contains("weight 1 is 2000000"));
    assert!(message(bad(json!(vec![1; 257]))).contains("257 weights"));
}
