use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    access::{ReadHandle, WriteHandle},
    bounds::Bounds,
    condition::{CompiledCondition, Condition},
    coords::Coord,
    filter::{CompiledFilter, ObjectFilter, Scene, Subject},
    mask::CellMask,
    object::{Object, Value},
    operator::{
        BuildContext, BuildError, Context, Dimensions, GridView, OpError, Operator, OperatorType,
        Scope,
    },
    pathfinding::{BLOCKED, cheapest_path},
    rng::{StepRng, pick},
    tile::TileId,
};

/// Digs a path for each connection of an object layer: from the middle of
/// the object its `from` names to the middle of the one its `to` names
/// (the props `ConnectObjects` writes), and writes `tile` along it, `width`
/// cells wide. The step's conditions say where it may write, as in `Fill`.
///
/// - `l`: one straight leg and then the other; which goes first is a coin
///   flip (Wolverson's tutorial).
/// - `z`: along the longer axis, turning once at a random spot across
///   (Rogue's `conn`).
/// - `cheapest`: the walk that costs least (A*), side to side. A cell
///   costs what the first rule of `costs` whose conditions all hold there
///   says, a number or `"blocked"`, and 1 where none does; `turn` is added
///   at each bend. The costs are worked out again after each path, so the
///   next one sees it: a cheap rule on the path's tile makes paths share
///   corridors (vazgriz), an expensive one on walls makes a path cross a
///   wall once instead of running along it — pesquisa
///   2026-10-26-portas-e-astar. A path wider than 1 is planned for its
///   middle line, and a cell costs the most of any under the brush there:
///   the path fits through no gap narrower than itself, and never paints
///   a blocked cell (the obstacles grown by the walker's size, as in
///   configuration space).
///
/// A path crosses whatever is in the way, rooms too: a wall it goes
/// through is where a door goes (`Fill` with `in_object {wall}` and the
/// path's tile) — pesquisa 2026-10-26-masmorra-em-metodos, item 3.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CarvePathsConfig {
    /// Object layer with the connections.
    pub connections: String,
    /// Object layer with the objects the connections join.
    pub objects: String,
    /// Tile layer to write.
    pub layer: String,
    pub tile: String,
    #[serde(default)]
    pub shape: Shape,
    /// Cells across (1 to 5).
    #[serde(default = "one")]
    #[schemars(range(min = 1, max = 5))]
    pub width: u32,
    /// Which connections get a path.
    #[serde(rename = "where", default)]
    pub filter: Vec<ObjectFilter>,
    /// `cheapest` only: what walking into a cell costs, by rules in order.
    #[serde(default)]
    pub costs: Vec<CostRule>,
    /// `cheapest` only: added at each bend (0 to 1000, default 0).
    #[serde(default)]
    #[schemars(range(max = 1000))]
    pub turn: Option<u32>,
}

/// Cells where all of `where` holds cost `cost`.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CostRule {
    #[serde(rename = "where")]
    pub conditions: Vec<Condition>,
    pub cost: Cost,
}

/// A whole number from 1 to 10000, or `"blocked"`: never walked into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(untagged, expecting = "a number from 1 to 10000, or \"blocked\"")]
pub enum Cost {
    Cells(u32),
    Word(Blocked),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Blocked {
    Blocked,
}

fn one() -> u32 {
    1
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Shape {
    #[default]
    L,
    Z,
    Cheapest,
}

pub struct CarvePaths {
    connections: ReadHandle,
    objects: ReadHandle,
    layer: WriteHandle,
    tile: TileId,
    shape: Shape,
    width: u32,
    filter: CompiledFilter,
    when: Option<CompiledCondition>,
    costs: Vec<(CompiledCondition, u32)>,
    turn: u32,
}

impl OperatorType for CarvePaths {
    const NAME: &'static str = "CarvePaths";
    const DIMENSIONS: Dimensions = Dimensions::TwoD;
    const SCOPE: Scope = Scope::Global;
    type Config = CarvePathsConfig;

    fn build(c: CarvePathsConfig, bcx: &mut BuildContext<'_>) -> Result<CarvePaths, BuildError> {
        if !(1..=5).contains(&c.width) {
            return Err(BuildError::Invalid {
                field: "width".into(),
                message: format!("{} is not between 1 and 5", c.width),
            });
        }
        if c.objects == c.connections {
            return Err(BuildError::Invalid {
                field: "objects".into(),
                message: "the objects the connections join are in a layer of their own".into(),
            });
        }
        if c.shape != Shape::Cheapest && (c.turn.is_some() || !c.costs.is_empty()) {
            let field = if c.turn.is_some() { "turn" } else { "costs" };
            return Err(BuildError::Invalid {
                field: field.into(),
                message: "only a `cheapest` path has costs and turns".into(),
            });
        }
        if let Some(turn) = c.turn.filter(|t| *t > 1000) {
            return Err(BuildError::Invalid {
                field: "turn".into(),
                message: format!("{turn} is above 1000"),
            });
        }
        let mut costs = Vec::new();
        for (i, rule) in c.costs.iter().enumerate() {
            let cost = match rule.cost {
                Cost::Cells(n) if (1..=10_000).contains(&n) => n,
                Cost::Cells(n) => {
                    return Err(BuildError::Invalid {
                        field: "costs".into(),
                        message: format!("rule {i}: a cost of {n} is not between 1 and 10000"),
                    });
                }
                Cost::Word(Blocked::Blocked) => BLOCKED,
            };
            let all = Condition::All {
                conditions: rule.conditions.clone(),
            };
            costs.push((bcx.condition(&all)?, cost));
        }
        let connections = bcx.read_objects(&c.connections)?;
        // Declares the layers it looks into, too.
        let filter = bcx.filter("where", &c.filter, connections.layer())?;
        Ok(CarvePaths {
            connections,
            objects: bcx.read_objects(&c.objects)?,
            layer: bcx.write(&c.layer)?,
            tile: bcx.tile(&c.tile)?,
            shape: c.shape,
            width: c.width,
            filter,
            when: bcx.step_conditions(),
            costs,
            turn: c.turn.unwrap_or(0),
        })
    }
}

/// The cells from `from` to `to` along one axis, both ends included.
fn line(cells: &mut Vec<Coord>, from: Coord, to: Coord) {
    let (dx, dy) = ((to.x - from.x).signum(), (to.y - from.y).signum());
    let mut c = from;
    cells.push(c);
    while c != to {
        c = Coord::new(c.x + dx, c.y + dy, c.z);
        cells.push(c);
    }
}

impl CarvePaths {
    /// The cells of a path from `a` to `b`, on the grid as it is now.
    fn path(
        &self,
        grid: &GridView<'_>,
        a: Coord,
        b: Coord,
        rng: &mut StepRng,
    ) -> Result<Option<Vec<Coord>>, OpError> {
        let mut cells = Vec::new();
        match self.shape {
            Shape::L => {
                let corner = if rng.next_u32() & 1 == 0 {
                    Coord::new(b.x, a.y, a.z)
                } else {
                    Coord::new(a.x, b.y, a.z)
                };
                line(&mut cells, a, corner);
                line(&mut cells, corner, b);
            }
            Shape::Z => {
                let turn_at = |from: i32, to: i32, rng: &mut StepRng| {
                    let span = (to - from).unsigned_abs();
                    from + (to - from).signum() * pick(rng.next_u32(), span.max(1)) as i32
                };
                if (b.x - a.x).abs() >= (b.y - a.y).abs() {
                    let t = turn_at(a.x, b.x, rng);
                    line(&mut cells, a, Coord::new(t, a.y, a.z));
                    line(&mut cells, Coord::new(t, a.y, a.z), Coord::new(t, b.y, a.z));
                    line(&mut cells, Coord::new(t, b.y, a.z), b);
                } else {
                    let t = turn_at(a.y, b.y, rng);
                    line(&mut cells, a, Coord::new(a.x, t, a.z));
                    line(&mut cells, Coord::new(a.x, t, a.z), Coord::new(b.x, t, a.z));
                    line(&mut cells, Coord::new(b.x, t, a.z), b);
                }
            }
            Shape::Cheapest => {
                let cost = self.costs(grid)?;
                return Ok(cheapest_path(grid.region(), &cost, self.turn, a, b));
            }
        }
        Ok(Some(cells))
    }

    /// What walking into each cell of the region costs, from the rules, on
    /// the grid as it is now.
    fn costs(&self, grid: &GridView<'_>) -> Result<Vec<u32>, OpError> {
        let region = grid.region();
        // 0: no rule has said yet.
        let mut cost = vec![0; region.size.len()];
        for (rule, c) in &self.costs {
            let holds = grid.prepare(rule)?.eval(region);
            for (k, p) in region.iter().enumerate() {
                if cost[k] == 0 && holds.get(p) {
                    cost[k] = *c;
                }
            }
        }
        cost.iter_mut().filter(|c| **c == 0).for_each(|c| *c = 1);
        Ok(self.widen(cost, region))
    }

    /// Each cell's cost as the dearest under the brush centred there (the
    /// cells of the region it covers): rows, then columns, as the brush is
    /// a square.
    fn widen(&self, cost: Vec<u32>, region: Bounds) -> Vec<u32> {
        if self.width == 1 {
            return cost;
        }
        let (w, h) = (region.size.width as usize, region.size.height as usize);
        let lo = -(self.width as i32 - 1) / 2;
        let hi = lo + self.width as i32 - 1;
        let dearest = |src: &[u32], along_x: bool| -> Vec<u32> {
            let mut out = vec![0; src.len()];
            for y in 0..h {
                for x in 0..w {
                    let (at, len) = if along_x { (x, w) } else { (y, h) };
                    let from = (at as i32 + lo).max(0) as usize;
                    let to = (at as i32 + hi).min(len as i32 - 1) as usize;
                    out[y * w + x] = (from..=to)
                        .map(|j| {
                            if along_x {
                                src[y * w + j]
                            } else {
                                src[j * w + x]
                            }
                        })
                        .max()
                        .unwrap_or(0);
                }
            }
            out
        };
        dearest(&dearest(&cost, true), false)
    }

    /// Writes the tile over `cells`, `width` across, where `mask` lets it.
    fn brush(
        &self,
        grid: &mut GridView<'_>,
        cells: &[Coord],
        mask: Option<&CellMask>,
    ) -> Result<(), OpError> {
        let tiles = grid.tiles_mut(self.layer)?;
        let w = self.width as i32;
        let lo = -(w - 1) / 2;
        for c in cells {
            for dy in lo..lo + w {
                for dx in lo..lo + w {
                    let p = Coord::new(c.x + dx, c.y + dy, c.z);
                    if mask.is_none_or(|m| m.get(p)) {
                        // Outside the layer: the brush hangs off the
                        // edge, nothing to write.
                        let _ = tiles.set(p, self.tile);
                    }
                }
            }
        }
        Ok(())
    }
}

impl Operator for CarvePaths {
    fn execute(&self, ctx: &mut Context<'_>) -> Result<(), OpError> {
        let parts = ctx.parts();
        let mut grid = parts.grid;
        let rng = parts.rng;
        let map = grid.map();
        // Taken before writing, as in `Fill`: conditions on the same layer
        // see it as it was when the step started.
        let mask = grid.mask(self.when.as_ref())?;
        let filter = grid.prepare_filter(&self.filter)?;
        let mut reads = vec![self.connections, self.objects];
        for l in self.filter.layers() {
            if reads.iter().all(|r| r.layer() != l) {
                reads.push(ReadHandle { layer: l });
            }
        }
        let split = grid.split(&reads, &[])?;
        let scene = Scene::new(
            map,
            reads
                .iter()
                .zip(&split.reads)
                .filter_map(|(h, l)| Some((h.layer(), l.as_objects()?))),
        );
        let id = self.connections.layer();
        let ends: &[Object] = scene.objects(self.objects.layer());
        let end = |c: &Object, prop: &str| -> Result<Coord, OpError> {
            let target =
                c.prop(prop)
                    .and_then(Value::as_object)
                    .ok_or_else(|| OpError::Failed {
                        message: format!("connection {:?} has no `{prop}` naming an object", c.id),
                    })?;
            let o = ends
                .iter()
                .find(|o| o.id == target)
                .ok_or_else(|| OpError::Failed {
                    message: format!(
                        "connection {:?} joins object {target:?}, which is not in `objects`",
                        c.id
                    ),
                })?;
            Ok(o.bbox.centre())
        };
        let mut ends = Vec::new();
        for (i, c) in scene.objects(id).iter().enumerate() {
            if !filter.test(&scene, id, Subject::Object(i)) {
                continue;
            }
            ends.push((c.id, end(c, "from")?, end(c, "to")?));
        }
        drop(split);

        for (id, a, b) in ends {
            let cells = self
                .path(&grid, a, b, rng)?
                .ok_or_else(|| OpError::Failed {
                    message: format!(
                        "connection {id:?}: blocked cells cut ({}, {}) off from ({}, {})",
                        a.x, a.y, b.x, b.y
                    ),
                })?;
            self.brush(&mut grid, &cells, mask.as_ref())?;
        }
        Ok(())
    }
}
