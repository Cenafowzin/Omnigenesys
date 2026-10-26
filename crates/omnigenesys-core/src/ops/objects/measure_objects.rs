use std::collections::VecDeque;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    access::{ReadHandle, WriteHandle},
    filter::{CompiledFilter, ObjectFilter, Scene, Subject},
    grid::LayerId,
    object::{ObjectId, Value},
    operator::{
        BuildContext, BuildError, Context, Dimensions, OpError, Operator, OperatorType, Scope,
    },
};

use super::common::{reads_besides, scene};

/// Writes a measure of each object as a prop, for later steps to filter
/// and sort by: hops from the start, connections per room. A measure is
/// data in sight, not a hidden term of some other step (Houdini's
/// "comparison key" attribute, UE PCG's Distance → Filter → Sort;
/// pesquisa 2026-10-25, item 12). One measure per step.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MeasureObjectsConfig {
    /// Object layer whose objects are measured.
    pub layer: String,
    /// The prop that gets the measure.
    pub into: String,
    pub measure: Measure,
    /// Which objects get it (all, if left out); the rest keep their props.
    #[serde(rename = "where", default)]
    pub filter: Vec<ObjectFilter>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Measure {
    /// Hops from the nearest object that passes `from`: from object to
    /// object side by side, or along the connections of `connections`
    /// (objects with `from` and `to`) when given. An object it cannot
    /// reach gets no prop. With `fraction`, hops over the most hops
    /// measured: from 0 to 1, the same scale however big the map
    /// (Brogue's "farthest quarter", pesquisa item 6).
    Distance {
        from: Vec<ObjectFilter>,
        #[serde(default)]
        connections: Option<String>,
        #[serde(default)]
        fraction: bool,
    },
    /// Connections of `connections` that start or end at it.
    Degree { connections: String },
    /// Objects side by side with it that pass `where`.
    Neighbours {
        #[serde(rename = "where", default)]
        filter: Vec<ObjectFilter>,
    },
}

enum Compiled {
    Distance {
        from: CompiledFilter,
        connections: Option<ReadHandle>,
        fraction: bool,
    },
    Degree {
        connections: ReadHandle,
    },
    Neighbours {
        filter: CompiledFilter,
    },
}

pub struct MeasureObjects {
    layer: WriteHandle,
    into: String,
    measure: Compiled,
    filter: CompiledFilter,
}

impl OperatorType for MeasureObjects {
    const NAME: &'static str = "MeasureObjects";
    const DIMENSIONS: Dimensions = Dimensions::Any;
    const SCOPE: Scope = Scope::Global;
    type Config = MeasureObjectsConfig;

    fn build(
        c: MeasureObjectsConfig,
        bcx: &mut BuildContext<'_>,
    ) -> Result<MeasureObjects, BuildError> {
        let layer = bcx.write_objects(&c.layer)?;
        let id = layer.layer();
        let connections = |bcx: &mut BuildContext<'_>, name: &str| {
            if name == c.layer {
                return Err(BuildError::Invalid {
                    field: "measure".into(),
                    message: format!(
                        "'{name}' is the layer being measured; connections are in a layer of their own"
                    ),
                });
            }
            bcx.read_objects(name)
        };
        let measure = match &c.measure {
            Measure::Distance {
                from,
                connections: via,
                fraction,
            } => Compiled::Distance {
                from: bcx.filter("measure", from, id)?,
                connections: via.as_deref().map(|n| connections(bcx, n)).transpose()?,
                fraction: *fraction,
            },
            Measure::Degree { connections: via } => Compiled::Degree {
                connections: connections(bcx, via)?,
            },
            Measure::Neighbours { filter } => Compiled::Neighbours {
                filter: bcx.filter("measure", filter, id)?,
            },
        };
        Ok(MeasureObjects {
            layer,
            into: c.into,
            measure,
            filter: bcx.filter("where", &c.filter, id)?,
        })
    }
}

/// The links between objects of a connection layer: (from, to) by id.
fn links(scene: &Scene, connections: LayerId) -> Vec<(ObjectId, ObjectId)> {
    scene
        .objects(connections)
        .iter()
        .filter_map(|c| Some((c.prop("from")?.as_object()?, c.prop("to")?.as_object()?)))
        .collect()
}

impl MeasureObjects {
    /// Hops from the sources to every object (by its place in the layer).
    fn hops(
        &self,
        scene: &Scene,
        id: LayerId,
        from: &CompiledFilter,
        via: Option<LayerId>,
    ) -> Vec<Option<u32>> {
        let objects = scene.objects(id);
        let n = objects.len();
        // The graph: each object's neighbours, by place in the layer.
        let mut next: Vec<Vec<usize>> = vec![Vec::new(); n];
        match via {
            None => {
                for (i, list) in next.iter_mut().enumerate() {
                    list.extend(scene.neighbours(id, Subject::Object(i)));
                }
            }
            Some(c) => {
                let place = |oid: ObjectId| objects.iter().position(|o| o.id == oid);
                for (a, b) in links(scene, c) {
                    if let (Some(a), Some(b)) = (place(a), place(b)) {
                        next[a].push(b);
                        next[b].push(a);
                    }
                }
            }
        }
        let mut hops = vec![None; n];
        let mut queue = VecDeque::new();
        for (i, h) in hops.iter_mut().enumerate() {
            if from.test(scene, id, Subject::Object(i)) {
                *h = Some(0);
                queue.push_back(i);
            }
        }
        while let Some(i) = queue.pop_front() {
            let d = hops[i].expect("queued objects have hops") + 1;
            for &j in &next[i] {
                if hops[j].is_none() {
                    hops[j] = Some(d);
                    queue.push_back(j);
                }
            }
        }
        hops
    }
}

impl Operator for MeasureObjects {
    fn execute(&self, ctx: &mut Context<'_>) -> Result<(), OpError> {
        let parts = ctx.parts();
        let mut grid = parts.grid;
        let map = grid.map();
        let id = self.layer.layer();

        let mut layers = self.filter.layers();
        match &self.measure {
            Compiled::Distance {
                from, connections, ..
            } => {
                layers.extend(from.layers());
                layers.extend(connections.map(|h| h.layer()));
            }
            Compiled::Degree { connections } => layers.push(connections.layer()),
            Compiled::Neighbours { filter } => layers.extend(filter.layers()),
        }
        let reads = reads_besides(&layers, &[self.layer]);
        let mut split = grid.split(&reads, &[self.layer])?;
        let scene = scene(map, &split, &reads, &[self.layer]);
        let n = scene.objects(id).len();

        let values: Vec<Option<Value>> = match &self.measure {
            Compiled::Distance {
                from,
                connections,
                fraction,
            } => {
                let hops = self.hops(&scene, id, from, connections.map(|h| h.layer()));
                let measured = |i: usize| self.filter.test(&scene, id, Subject::Object(i));
                if *fraction {
                    let most = (0..n)
                        .filter(|&i| measured(i))
                        .filter_map(|i| hops[i])
                        .max()
                        .unwrap_or(0);
                    hops.iter()
                        .map(|h| {
                            h.map(|h| {
                                Value::Float(if most == 0 {
                                    0.0
                                } else {
                                    f64::from(h) / f64::from(most)
                                })
                            })
                        })
                        .collect()
                } else {
                    hops.iter()
                        .map(|h| h.map(|h| Value::Int(i64::from(h))))
                        .collect()
                }
            }
            Compiled::Degree { connections } => {
                let links = links(&scene, connections.layer());
                scene
                    .objects(id)
                    .iter()
                    .map(|o| {
                        let d = links
                            .iter()
                            .filter(|(a, b)| *a == o.id || *b == o.id)
                            .count();
                        Some(Value::Int(d as i64))
                    })
                    .collect()
            }
            Compiled::Neighbours { filter } => (0..n)
                .map(|i| {
                    let k = scene
                        .neighbours(id, Subject::Object(i))
                        .filter(|&j| filter.test(&scene, id, Subject::Object(j)))
                        .count();
                    Some(Value::Int(k as i64))
                })
                .collect(),
        };
        let measured: Vec<bool> = (0..n)
            .map(|i| self.filter.test(&scene, id, Subject::Object(i)))
            .collect();
        let layer = split.writes[0]
            .as_objects_mut()
            .expect("checked when built");
        for ((o, v), m) in layer.iter_mut().zip(values).zip(measured) {
            if !m {
                continue;
            }
            match v {
                Some(v) => o.set(&self.into, v),
                None => o.remove(&self.into),
            }
        }
        Ok(())
    }
}
