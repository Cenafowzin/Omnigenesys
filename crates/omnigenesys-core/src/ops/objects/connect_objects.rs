use std::collections::{BTreeMap, BTreeSet, VecDeque};

use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    access::{ReadHandle, WriteHandle},
    bounds::Bounds,
    coords::{Coord, GridSize},
    filter::{CompiledFilter, ObjectFilter, Subject},
    object::{Object, ObjectId, Value},
    operator::{
        BuildContext, BuildError, Context, Dimensions, OpError, Operator, OperatorType, Scope,
    },
    rng::Chance,
};

use super::common::{chance, reads_besides, scene};
use super::grow_objects::parent;

/// A graph over objects: one connection object per pair it joins, with
/// `from` and `to` naming the two ends. Only the graph — no tiles. A
/// connection's box spans the centres of its two ends, so `in_object`
/// finds the cells between them (a door through two walls side by side)
/// and `CarvePaths` digs along it.
///
/// Which pairs, by `strategy`:
/// - `neighbours`: every two objects side by side (Isaac's doors).
/// - `parent`: each object to the one it grew from (`GrowObjects`): only
///   the tree the growth made, not every adjacency it left behind.
/// - `mst`: the shortest tree over the objects' centres (Prim; TinyKeep,
///   Adonaac): everything connected, with no loop.
/// - `nearest`: each object to its `k` nearest.
/// - `chain`: each object to the one made before it (the tutorial's).
///
/// Then each pair, in the strategy's order (`nearest` shortest first),
/// is joined only if it is not joined in `layer` already, its ends are at
/// least `min_hops` connections apart there (no path counts as far), and
/// it wins the `keep` roll. A second step over the same layer adds loops
/// to a tree: Brogue's `addLoops` opens a wall only where it saves a long
/// walk (pesquisa 2026-10-26-masmorra-em-metodos, item 2).
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConnectObjectsConfig {
    /// Object layer with the objects to connect.
    pub objects: String,
    /// Object layer that gets the connections.
    pub layer: String,
    pub strategy: Strategy,
    /// With `nearest`: how many of the nearest (1 to 16; 3 if left out).
    #[serde(default)]
    #[schemars(range(min = 1, max = 16))]
    pub k: Option<u32>,
    /// Join a pair only if its ends are at least this many connections
    /// apart in `layer` (counting the ones this step made before it).
    #[serde(default)]
    pub min_hops: Option<u32>,
    /// Chance each pair that is left is joined (0 to 1).
    #[serde(default = "always")]
    #[schemars(range(min = 0, max = 1))]
    pub keep: f64,
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

fn always() -> f64 {
    1.0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Strategy {
    Neighbours,
    Parent,
    Mst,
    Nearest,
    Chain,
}

const MAX_K: u32 = 16;
const DEFAULT_K: u32 = 3;

pub struct ConnectObjects {
    objects: ReadHandle,
    layer: WriteHandle,
    strategy: Strategy,
    k: usize,
    min_hops: Option<u32>,
    keep: Option<Chance>,
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
        let invalid = |field: &str, message: String| BuildError::Invalid {
            field: field.into(),
            message,
        };
        if c.objects == c.layer {
            return Err(invalid(
                "layer",
                "connections go in a layer of their own, not the objects' layer".into(),
            ));
        }
        let k = match (c.strategy, c.k) {
            (Strategy::Nearest, k) => {
                let k = k.unwrap_or(DEFAULT_K);
                if !(1..=MAX_K).contains(&k) {
                    return Err(invalid("k", format!("{k} is not between 1 and {MAX_K}")));
                }
                k
            }
            (_, Some(_)) => {
                return Err(invalid("k", "only `nearest` takes a `k`".into()));
            }
            (_, None) => 0,
        };
        if c.min_hops == Some(0) {
            return Err(invalid(
                "min_hops",
                "0 lets every pair through: leave it out, or give 2 or more".into(),
            ));
        }
        let keep = chance(c.keep, "keep")?;
        let objects = bcx.read_objects(&c.objects)?;
        Ok(ConnectObjects {
            objects,
            layer: bcx.write_objects(&c.layer)?,
            strategy: c.strategy,
            k: k as usize,
            min_hops: c.min_hops,
            keep: (c.keep < 1.0).then_some(keep),
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

/// The squared distance between two boxes' middles, in half cells (exact,
/// so ties are ties).
fn distance(a: Bounds, b: Bounds) -> i64 {
    let mid = |o: i32, n: u32| 2 * i64::from(o) + i64::from(n);
    let d = |p: i64, q: i64| (p - q) * (p - q);
    d(mid(a.origin.x, a.size.width), mid(b.origin.x, b.size.width))
        + d(
            mid(a.origin.y, a.size.height),
            mid(b.origin.y, b.size.height),
        )
        + d(mid(a.origin.z, a.size.depth), mid(b.origin.z, b.size.depth))
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

impl ConnectObjects {
    /// The pairs the strategy proposes, by place in `objects`, in the order
    /// they are considered.
    fn pairs(
        &self,
        objects: &[Object],
        part: &[usize],
        scene_pairs: Vec<(usize, usize)>,
    ) -> Vec<(usize, usize)> {
        let d = |a: usize, b: usize| distance(objects[a].bbox, objects[b].bbox);
        match self.strategy {
            Strategy::Neighbours | Strategy::Parent => scene_pairs,
            Strategy::Chain => part.windows(2).map(|w| (w[0], w[1])).collect(),
            Strategy::Mst => {
                // Prim from the first; ties go to the lower places.
                let mut tree = Vec::new();
                let mut inside = vec![false; objects.len()];
                let Some(&first) = part.first() else {
                    return tree;
                };
                inside[first] = true;
                for _ in 1..part.len() {
                    let mut best: Option<(i64, usize, usize)> = None;
                    for &a in part.iter().filter(|&&a| inside[a]) {
                        for &b in part.iter().filter(|&&b| !inside[b]) {
                            let e = (d(a, b), a.min(b), a.max(b));
                            if best.is_none_or(|x| e < x) {
                                best = Some(e);
                            }
                        }
                    }
                    let (_, a, b) = best.expect("an object is left outside");
                    let newcomer = if inside[a] { b } else { a };
                    inside[newcomer] = true;
                    tree.push((a, b));
                }
                tree
            }
            Strategy::Nearest => {
                let mut found: BTreeSet<(i64, usize, usize)> = BTreeSet::new();
                for &a in part {
                    let mut near: Vec<(i64, usize)> = part
                        .iter()
                        .filter(|&&b| b != a)
                        .map(|&b| (d(a, b), b))
                        .collect();
                    near.sort_unstable();
                    for &(dist, b) in near.iter().take(self.k) {
                        found.insert((dist, a.min(b), a.max(b)));
                    }
                }
                found.into_iter().map(|(_, a, b)| (a, b)).collect()
            }
        }
    }
}

/// Connections between objects, by id, both ways.
#[derive(Default)]
struct Graph {
    edges: BTreeMap<ObjectId, Vec<ObjectId>>,
}

impl Graph {
    fn add(&mut self, a: ObjectId, b: ObjectId) {
        self.edges.entry(a).or_default().push(b);
        self.edges.entry(b).or_default().push(a);
    }

    fn joined(&self, a: ObjectId, b: ObjectId) -> bool {
        self.edges.get(&a).is_some_and(|n| n.contains(&b))
    }

    /// Connections on the shortest path from `a` to `b`; `None` if there
    /// is none.
    fn hops(&self, a: ObjectId, b: ObjectId) -> Option<u32> {
        let mut seen = BTreeSet::from([a]);
        let mut queue = VecDeque::from([(a, 0)]);
        while let Some((x, h)) = queue.pop_front() {
            if x == b {
                return Some(h);
            }
            for &y in self.edges.get(&x).into_iter().flatten() {
                if seen.insert(y) {
                    queue.push_back((y, h + 1));
                }
            }
        }
        None
    }
}

impl Operator for ConnectObjects {
    fn execute(&self, ctx: &mut Context<'_>) -> Result<(), OpError> {
        let parts = ctx.parts();
        let mut grid = parts.grid;
        let rng = parts.rng;
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
        let part: Vec<usize> = (0..objects.len())
            .filter(|&i| self.filter.test(&scene, id, Subject::Object(i)))
            .collect();
        let takes_part = |i: usize| part.binary_search(&i).is_ok();

        let mut scene_pairs: Vec<(usize, usize)> = Vec::new();
        match self.strategy {
            Strategy::Neighbours => {
                for &i in &part {
                    for j in scene.neighbours(id, Subject::Object(i)) {
                        if i < j && takes_part(j) {
                            scene_pairs.push((i, j));
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
                        && takes_part(i)
                        && takes_part(j)
                    {
                        scene_pairs.push((i, j));
                    }
                }
            }
            _ => {}
        }

        // The connections the layer has: a pair joined there is not joined
        // again, and `min_hops` walks them.
        let mut graph = Graph::default();
        for c in scene.objects(self.layer.layer()) {
            let end = |p: &str| c.prop(p).and_then(Value::as_object);
            if let (Some(a), Some(b)) = (end("from"), end("to")) {
                graph.add(a, b);
            }
        }

        let mut made: Vec<Object> = Vec::new();
        for (a, b) in self.pairs(objects, &part, scene_pairs) {
            let (oa, ob) = (&objects[a], &objects[b]);
            if graph.joined(oa.id, ob.id) {
                continue;
            }
            if let Some(min) = self.min_hops
                && graph.hops(oa.id, ob.id).is_some_and(|h| h < min)
            {
                continue;
            }
            if let Some(keep) = self.keep
                && !keep.hit(rng.next_u32())
            {
                continue;
            }
            graph.add(oa.id, ob.id);
            made.push(Object {
                id: split.ids.fresh(),
                kind: self.kind.clone(),
                bbox: span(centre(oa.bbox), centre(ob.bbox)),
                props: vec![
                    ("from".to_string(), Value::Object(oa.id)),
                    ("to".to_string(), Value::Object(ob.id)),
                ],
            });
        }
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
