use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    access::{ReadHandle, WriteHandle},
    bounds::Bounds,
    coords::{Coord, GridSize},
    filter::{CompiledFilter, ObjectFilter, Subject},
    object::{Object, Value},
    operator::{
        BuildContext, BuildError, Context, Dimensions, OpError, Operator, OperatorType, Scope,
    },
};

use super::common::{reads_besides, scene};
use super::grow_objects::parent;

/// A graph over objects: one connection object per pair it joins, with
/// `from` and `to` naming the two ends. Only the graph — no tiles. A
/// connection's box spans the centres of its two ends, so `in_object`
/// finds the cells between them (a door through two walls side by side)
/// and a path step can follow it.
///
/// - `neighbours`: every two objects side by side (Isaac's doors).
/// - `parent`: each object to the one it grew from (`GrowObjects`): only
///   the tree the growth made, not every adjacency it left behind.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConnectObjectsConfig {
    /// Object layer with the objects to connect.
    pub objects: String,
    /// Object layer that gets the connections.
    pub layer: String,
    pub strategy: Strategy,
    /// Which objects take part: both ends of a connection pass it.
    #[serde(rename = "where", default)]
    pub filter: Vec<ObjectFilter>,
    /// What the connections are.
    #[serde(default = "connection")]
    pub kind: String,
    /// With `parent`: the prop that names the object each one grew from.
    #[serde(default = "parent")]
    pub parent: String,
}

fn connection() -> String {
    "connection".to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Strategy {
    Neighbours,
    Parent,
}

pub struct ConnectObjects {
    objects: ReadHandle,
    layer: WriteHandle,
    strategy: Strategy,
    filter: CompiledFilter,
    kind: String,
    parent: String,
}

impl OperatorType for ConnectObjects {
    const NAME: &'static str = "ConnectObjects";
    const DIMENSIONS: Dimensions = Dimensions::Any;
    const SCOPE: Scope = Scope::Global;
    type Config = ConnectObjectsConfig;

    fn build(
        c: ConnectObjectsConfig,
        bcx: &mut BuildContext<'_>,
    ) -> Result<ConnectObjects, BuildError> {
        if c.objects == c.layer {
            return Err(BuildError::Invalid {
                field: "layer".into(),
                message: "connections go in a layer of their own, not the objects' layer".into(),
            });
        }
        let objects = bcx.read_objects(&c.objects)?;
        Ok(ConnectObjects {
            objects,
            layer: bcx.write_objects(&c.layer)?,
            strategy: c.strategy,
            filter: bcx.filter("where", &c.filter, objects.layer())?,
            kind: c.kind,
            parent: c.parent,
        })
    }
}

/// A box's middle cell.
fn centre(b: Bounds) -> Coord {
    Coord::new(
        b.origin.x + (b.size.width / 2) as i32,
        b.origin.y + (b.size.height / 2) as i32,
        b.origin.z + (b.size.depth / 2) as i32,
    )
}

/// The box spanning two cells.
fn span(a: Coord, b: Coord) -> Bounds {
    let side = |p: i32, q: i32| {
        (
            p.min(q),
            (i64::from(p) - i64::from(q)).unsigned_abs() as u32 + 1,
        )
    };
    let (x, w) = side(a.x, b.x);
    let (y, h) = side(a.y, b.y);
    let (z, d) = side(a.z, b.z);
    Bounds::new(Coord::new(x, y, z), GridSize::new(w, h, d))
}

impl Operator for ConnectObjects {
    fn execute(&self, ctx: &mut Context<'_>) -> Result<(), OpError> {
        let parts = ctx.parts();
        let mut grid = parts.grid;
        let map = grid.map();
        let mut reads = vec![self.objects];
        reads.extend(
            reads_besides(&self.filter.layers(), &[self.layer])
                .into_iter()
                .filter(|r| r.layer() != self.objects.layer()),
        );
        let mut split = grid.split(&reads, &[self.layer])?;
        let scene = scene(map, &split, &reads, &[self.layer]);
        let id = self.objects.layer();
        let objects = scene.objects(id);
        let takes_part: Vec<bool> = (0..objects.len())
            .map(|i| self.filter.test(&scene, id, Subject::Object(i)))
            .collect();

        let mut pairs: Vec<(usize, usize)> = Vec::new();
        match self.strategy {
            Strategy::Neighbours => {
                for i in 0..objects.len() {
                    for j in scene.neighbours(id, Subject::Object(i)) {
                        if i < j && takes_part[i] && takes_part[j] {
                            pairs.push((i, j));
                        }
                    }
                }
            }
            Strategy::Parent => {
                for (j, o) in objects.iter().enumerate() {
                    let Some(p) = o.prop(&self.parent).and_then(Value::as_object) else {
                        continue;
                    };
                    if let Some(i) = objects.iter().position(|x| x.id == p)
                        && takes_part[i]
                        && takes_part[j]
                    {
                        pairs.push((i, j));
                    }
                }
            }
        }

        let made: Vec<Object> = pairs
            .into_iter()
            .map(|(a, b)| Object {
                id: split.ids.fresh(),
                kind: self.kind.clone(),
                bbox: span(centre(objects[a].bbox), centre(objects[b].bbox)),
                props: vec![
                    ("from".to_string(), Value::Object(objects[a].id)),
                    ("to".to_string(), Value::Object(objects[b].id)),
                ],
            })
            .collect();
        let layer = split
            .writes
            .last_mut()
            .and_then(|l| l.as_objects_mut())
            .expect("checked when built");
        for o in made {
            layer.insert(o);
        }
        Ok(())
    }
}
