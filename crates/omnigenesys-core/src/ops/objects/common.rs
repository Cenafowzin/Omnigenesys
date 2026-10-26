//! What the object operators share: how many to take, in what order, and
//! the copy of the layers they look at.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    access::{ReadHandle, WriteHandle},
    expr::{self, Expr, Type},
    filter::{CompiledFilter, ObjectFilter, Scalar, Scene, Subject},
    grid::{LayerId, Split},
    object::Value,
    operator::{BuildContext, BuildError, OpError},
    rng::{Chance, StepRng, pick},
};

/// How many: a whole number, a range drawn once by the step (both ends
/// counted), or `"all"`. Fewer than the least asked for fails the step,
/// so a pipeline with `attempts` makes the map again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(
    untagged,
    expecting = "a whole number, {\"min\": ..., \"max\": ...} or \"all\""
)]
pub enum Count {
    Exact(u32),
    Range(CountRange),
    Word(CountWord),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CountRange {
    pub min: u32,
    pub max: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CountWord {
    /// Every one there is: never fails for having few.
    All,
}

pub(super) fn one() -> Count {
    Count::Exact(1)
}

/// A [`Count`] checked: the least to take and the most.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Amount {
    min: u32,
    max: u32,
}

impl Amount {
    pub(super) fn new(count: Count) -> Result<Amount, BuildError> {
        match count {
            Count::Exact(n) => Ok(Amount { min: n, max: n }),
            Count::Range(CountRange { min, max }) if min > max => Err(BuildError::Invalid {
                field: "count".into(),
                message: format!("min {min} is above max {max}"),
            }),
            Count::Range(CountRange { min, max }) => Ok(Amount { min, max }),
            Count::Word(CountWord::All) => Ok(Amount {
                min: 0,
                max: u32::MAX,
            }),
        }
    }

    /// The least that must be taken.
    pub(super) fn min(self) -> u32 {
        self.min
    }

    /// How many to take this time: drawn from the stream when it is a range.
    pub(super) fn draw(self, rng: &mut StepRng) -> u32 {
        if self.min == self.max || self.max == u32::MAX && self.min == 0 {
            return self.max;
        }
        match (self.max - self.min).checked_add(1) {
            Some(span) => self.min + pick(rng.next_u32(), span),
            None => rng.next_u32(),
        }
    }
}

/// The chance a step does anything: a failed roll does nothing, and is
/// not a failure (Isaac's sacrifice room, 1 in 7).
pub(super) fn chance(p: f64, field: &str) -> Result<Chance, BuildError> {
    Chance::new(p).ok_or_else(|| BuildError::Invalid {
        field: field.into(),
        message: format!("{p} is not between 0 and 1"),
    })
}

pub(super) fn always() -> f64 {
    1.0
}

/// Prop values from the file, as objects hold them.
pub(super) fn props(map: &BTreeMap<String, Scalar>) -> Vec<(String, Value)> {
    map.iter().map(|(k, v)| (k.clone(), v.to_value())).collect()
}

/// One key of an order: candidates are sorted by the first key, ties by
/// the next, and the last ties keep the candidates' own order (objects as
/// made, slots row by row).
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(tag = "by", rename_all = "snake_case", deny_unknown_fields)]
pub enum OrderKey {
    /// A fresh random number for each candidate.
    Random,
    /// A numeric prop; candidates without it go last either way.
    Prop {
        name: String,
        #[serde(default)]
        desc: bool,
    },
    /// Objects side by side with the candidate that pass `where`.
    Neighbours {
        #[serde(rename = "where", default)]
        filter: Vec<ObjectFilter>,
        #[serde(default)]
        desc: bool,
    },
    /// An expression over `neighbours` (objects side by side, a whole
    /// number) and `random` (fresh for each candidate, from 0 to below 1):
    /// Isaac Rebirth's secret room weighs `10 + floor(random * 5)`, minus
    /// 3 with two neighbours and 6 with one.
    Score {
        expr: String,
        #[serde(default)]
        desc: bool,
    },
}

#[derive(Debug, Clone)]
enum Key {
    Random,
    Prop(String),
    Neighbours(CompiledFilter),
    Score { expr: Expr, random: bool },
}

/// [`OrderKey`]s, compiled.
#[derive(Debug, Clone)]
pub(super) struct Order {
    keys: Vec<(Key, bool)>,
}

const SCORE_NAMES: [(&str, Type); 2] = [("neighbours", Type::Integer), ("random", Type::Number)];

impl Order {
    pub(super) fn new(
        keys: &[OrderKey],
        bcx: &mut BuildContext<'_>,
        layer: LayerId,
    ) -> Result<Order, BuildError> {
        let invalid = |message: String| BuildError::Invalid {
            field: "order_by".into(),
            message,
        };
        let mut out = Vec::new();
        for key in keys {
            out.push(match key {
                OrderKey::Random => (Key::Random, false),
                OrderKey::Prop { name, desc } => (Key::Prop(name.clone()), *desc),
                OrderKey::Neighbours { filter, desc } => (
                    Key::Neighbours(bcx.filter("order_by", filter, layer)?),
                    *desc,
                ),
                OrderKey::Score { expr: text, desc } => {
                    let e = Expr::parse(text)
                        .map_err(|e| invalid(format!("score `{text}`: {}", e.message)))?;
                    match e.check(&SCORE_NAMES) {
                        Ok(Type::Integer | Type::Number) => {}
                        Ok(t) => {
                            return Err(invalid(format!("score `{text}` is {t}, not a number")));
                        }
                        Err(errors) => {
                            return Err(invalid(format!("score `{text}`: {}", errors[0].message)));
                        }
                    }
                    let random = e.names().iter().any(|(n, _)| *n == "random");
                    (Key::Score { expr: e, random }, *desc)
                }
            });
        }
        Ok(Order { keys: out })
    }

    /// Sorts `candidates` of `layer`. Random numbers are drawn candidate
    /// by candidate, key by key, in the candidates' order.
    pub(super) fn sort(
        &self,
        scene: &Scene,
        layer: LayerId,
        candidates: &mut Vec<Subject>,
        rng: &mut StepRng,
    ) -> Result<(), OpError> {
        if self.keys.is_empty() {
            return Ok(());
        }
        let mut keyed = Vec::with_capacity(candidates.len());
        for &c in candidates.iter() {
            let mut values = Vec::with_capacity(self.keys.len());
            for (key, _) in &self.keys {
                values.push(self.value(key, scene, layer, c, rng)?);
            }
            keyed.push((values, c));
        }
        // Stable: the last ties keep the candidates' order.
        keyed.sort_by(|(a, _), (b, _)| {
            for ((x, y), (_, desc)) in a.iter().zip(b).zip(&self.keys) {
                let o = match (x, y) {
                    (Some(x), Some(y)) if *desc => y.total_cmp(x),
                    (Some(x), Some(y)) => x.total_cmp(y),
                    (Some(_), None) => Ordering::Less,
                    (None, Some(_)) => Ordering::Greater,
                    (None, None) => Ordering::Equal,
                };
                if o != Ordering::Equal {
                    return o;
                }
            }
            Ordering::Equal
        });
        *candidates = keyed.into_iter().map(|(_, c)| c).collect();
        Ok(())
    }

    fn value(
        &self,
        key: &Key,
        scene: &Scene,
        layer: LayerId,
        c: Subject,
        rng: &mut StepRng,
    ) -> Result<Option<f64>, OpError> {
        let neighbours = |filter: Option<&CompiledFilter>| {
            scene
                .neighbours(layer, c)
                .filter(|&j| filter.is_none_or(|f| f.test(scene, layer, Subject::Object(j))))
                .count()
        };
        Ok(match key {
            Key::Random => Some(unit(rng.next_u32())),
            Key::Prop(name) => match c {
                Subject::Object(i) => scene.objects(layer)[i]
                    .prop(name)
                    .and_then(crate::filter::number),
                Subject::Slot(_) | Subject::Box(_) => None,
            },
            Key::Neighbours(f) => Some(neighbours(Some(f)) as f64),
            Key::Score { expr, random } => {
                let n = neighbours(None) as i64;
                let r = if *random { unit(rng.next_u32()) } else { 0.0 };
                let v = expr
                    .eval(&|name| match name {
                        "neighbours" => Some(expr::Value::Integer(n)),
                        "random" => Some(expr::Value::Number(r)),
                        _ => None,
                    })
                    .map_err(|e| OpError::Failed {
                        message: format!("order_by score: {}", e.message),
                    })?;
                match v {
                    expr::Value::Integer(n) => Some(n as f64),
                    expr::Value::Number(x) => Some(x),
                    _ => None,
                }
            }
        })
    }
}

/// `r / 2^32`: exact in f64, from 0 to below 1.
fn unit(r: u32) -> f64 {
    f64::from(r) / 4_294_967_296.0
}

/// Copies the object layers of a split (reads, then writes) into a scene.
pub(super) fn scene(
    map: crate::bounds::Bounds,
    split: &Split<'_>,
    reads: &[ReadHandle],
    writes: &[WriteHandle],
) -> Scene {
    let r = reads
        .iter()
        .zip(&split.reads)
        .filter_map(|(h, l)| Some((h.layer(), l.as_objects()?)));
    let w = writes
        .iter()
        .zip(&split.writes)
        .filter_map(|(h, l)| Some((h.layer(), l.as_objects()?)));
    Scene::new(map, r.chain(w))
}

/// The layers to read besides the ones a step writes: those its filters
/// look into.
pub(super) fn reads_besides(layers: &[LayerId], writes: &[WriteHandle]) -> Vec<ReadHandle> {
    let mut out: Vec<ReadHandle> = Vec::new();
    for &l in layers {
        if writes.iter().all(|w| w.layer() != l) && out.iter().all(|r| r.layer() != l) {
            out.push(ReadHandle { layer: l });
        }
    }
    out
}

/// The first of `filters` that some candidate passes, and those that pass
/// it: `where`, then each `fallback` in order (pesquisa 2026-10-25, item
/// 8: Gungeon's secret room goes in a dead end, or anywhere).
pub(super) fn passing(
    filters: &[CompiledFilter],
    scene: &Scene,
    layer: LayerId,
    all: &[Subject],
) -> Vec<Subject> {
    for f in filters {
        let found: Vec<Subject> = all
            .iter()
            .copied()
            .filter(|&s| f.test(scene, layer, s))
            .collect();
        if !found.is_empty() {
            return found;
        }
    }
    Vec::new()
}

/// `where` and the fallbacks after it.
pub(super) fn filters(
    bcx: &mut BuildContext<'_>,
    filter: &[ObjectFilter],
    fallback: &[Vec<ObjectFilter>],
    layer: LayerId,
) -> Result<Vec<CompiledFilter>, BuildError> {
    let mut out = vec![bcx.filter("where", filter, layer)?];
    for f in fallback {
        out.push(bcx.filter("fallback", f, layer)?);
    }
    Ok(out)
}

/// The step fails: fewer than the least asked for, and why.
pub(super) fn too_few(did: &str, got: u32, min: u32, why: &str) -> OpError {
    OpError::Failed {
        message: format!("{did} {got}, fewer than the {min} asked for: {why}"),
    }
}
