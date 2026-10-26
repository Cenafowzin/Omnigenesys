use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    access::{ReadHandle, WriteHandle},
    bounds::Bounds,
    condition::CompiledCondition,
    coords::Coord,
    filter::{CompiledFilter, ObjectFilter, Scene, Subject},
    object::{Object, Value},
    operator::{
        BuildContext, BuildError, Context, Dimensions, OpError, Operator, OperatorType, Scope,
    },
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
        })
    }
}

/// A box's middle cell.
fn centre(b: Bounds) -> Coord {
    Coord::new(
        b.origin.x + (b.size.width / 2) as i32,
        b.origin.y + (b.size.height / 2) as i32,
        b.origin.z,
    )
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
    /// The cells of a path from `a` to `b`.
    fn path(&self, a: Coord, b: Coord, rng: &mut StepRng) -> Vec<Coord> {
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
        }
        cells
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
        let mask = match &self.when {
            Some(c) => Some(grid.prepare(c)?.eval(grid.region())),
            None => None,
        };
        let mut reads = vec![self.connections, self.objects];
        for l in self.filter.layers() {
            if reads.iter().all(|r| r.layer() != l) {
                reads.push(ReadHandle { layer: l });
            }
        }
        let mut split = grid.split(&reads, &[self.layer])?;
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
            Ok(centre(o.bbox))
        };
        let mut paths = Vec::new();
        for (i, c) in scene.objects(id).iter().enumerate() {
            if !self.filter.test(&scene, id, Subject::Object(i)) {
                continue;
            }
            paths.push(self.path(end(c, "from")?, end(c, "to")?, rng));
        }

        let tiles = split.writes[0].as_tiles_mut().expect("checked when built");
        let w = self.width as i32;
        let lo = -(w - 1) / 2;
        for cells in paths {
            for c in cells {
                for dy in lo..lo + w {
                    for dx in lo..lo + w {
                        let p = Coord::new(c.x + dx, c.y + dy, c.z);
                        if mask.as_ref().is_none_or(|m| m.get(p)) {
                            // Outside the layer: the brush hangs off the
                            // edge, nothing to write.
                            let _ = tiles.set(p, self.tile);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
