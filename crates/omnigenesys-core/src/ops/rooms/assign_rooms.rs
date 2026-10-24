use std::collections::VecDeque;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    access::{ReadHandle, WriteHandle},
    object::{Object, ObjectId, Value},
    operator::{
        BuildContext, BuildError, Context, Dimensions, OpError, Operator, OperatorType, Scope,
    },
    rng::pick,
    tile::TileId,
};

use super::common::interior;

/// Gives rooms their roles — start, boss, treasure, shop — and paints each
/// role's floor, as Isaac does after laying out a floor: the start is the
/// first room, the boss the dead end farthest from it **through the
/// connections** (graph distance, not a straight line), the other special
/// rooms in dead ends.
///
/// Every reachable room gets `depth` (doors from the start); each rule, in
/// order, gives `role` to one room that has none yet. A rule for a role some
/// rooms already have (`secret`, set by `RoomGrid`) only paints them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AssignRoomsConfig {
    pub rooms: String,
    pub connections: String,
    /// Tile layer where role floors are painted.
    pub layer: String,
    /// The floor tile that a role's tile replaces inside its room.
    pub floor: String,
    pub roles: Vec<RoleRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RoleRule {
    pub role: String,
    #[serde(default)]
    pub pick: Pick,
    /// Only rooms with a single connection.
    #[serde(default)]
    pub dead_end: bool,
    /// Floor of rooms with this role; unchanged if left out.
    #[serde(default)]
    pub tile: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Pick {
    /// The first room made (the start, for RoomGrid).
    #[default]
    First,
    /// The most doors away from the start.
    Farthest,
    /// The fewest doors away from the start.
    Nearest,
    /// Any, from the step's random stream.
    Random,
}

struct Rule {
    role: String,
    pick: Pick,
    dead_end: bool,
    tile: Option<TileId>,
}

pub struct AssignRooms {
    rooms: WriteHandle,
    connections: ReadHandle,
    layer: WriteHandle,
    floor: TileId,
    rules: Vec<Rule>,
}

impl OperatorType for AssignRooms {
    const NAME: &'static str = "AssignRooms";
    const DIMENSIONS: Dimensions = Dimensions::Any;
    const SCOPE: Scope = Scope::Global;
    type Config = AssignRoomsConfig;

    fn build(c: AssignRoomsConfig, bcx: &mut BuildContext<'_>) -> Result<AssignRooms, BuildError> {
        let rules = c
            .roles
            .iter()
            .map(|r| {
                Ok(Rule {
                    role: r.role.clone(),
                    pick: r.pick,
                    dead_end: r.dead_end,
                    tile: r.tile.as_deref().map(|t| bcx.tile(t)).transpose()?,
                })
            })
            .collect::<Result<Vec<_>, BuildError>>()?;
        Ok(AssignRooms {
            rooms: bcx.write_objects(&c.rooms)?,
            connections: bcx.read_objects(&c.connections)?,
            layer: bcx.write(&c.layer)?,
            floor: bcx.tile(&c.floor)?,
            rules,
        })
    }
}

/// Doors from the first room to each room, through the connections;
/// `None` for rooms it cannot reach.
fn depths(rooms: &[Object], links: &[(ObjectId, ObjectId)]) -> Vec<Option<u32>> {
    let index = |id: ObjectId| rooms.iter().position(|r| r.id == id);
    let mut depth = vec![None; rooms.len()];
    if rooms.is_empty() {
        return depth;
    }
    depth[0] = Some(0);
    let mut queue = VecDeque::from([0usize]);
    while let Some(i) = queue.pop_front() {
        let d = depth[i].expect("queued rooms have a depth");
        for &(a, b) in links {
            let other = if a == rooms[i].id {
                index(b)
            } else if b == rooms[i].id {
                index(a)
            } else {
                None
            };
            if let Some(j) = other
                && depth[j].is_none()
            {
                depth[j] = Some(d + 1);
                queue.push_back(j);
            }
        }
    }
    depth
}

impl Operator for AssignRooms {
    fn execute(&self, ctx: &mut Context<'_>) -> Result<(), OpError> {
        let parts = ctx.parts();
        let mut grid = parts.grid;
        let rng = parts.rng;
        let mut split = grid.split(&[self.connections], &[self.rooms, self.layer])?;
        let links: Vec<(ObjectId, ObjectId)> = split.reads[0]
            .as_objects()
            .expect("checked when built")
            .iter()
            .filter_map(|c| Some((c.prop("from")?.as_object()?, c.prop("to")?.as_object()?)))
            .collect();
        let [rooms, tiles] = split.writes.as_mut_slice() else {
            unreachable!("two layers asked for");
        };
        let rooms = rooms.as_objects_mut().expect("checked when built");
        let tiles = tiles.as_tiles_mut().expect("checked when built");
        let snapshot: Vec<Object> = rooms.iter().cloned().collect();
        let depth = depths(&snapshot, &links);
        let degree = |id: ObjectId| links.iter().filter(|(a, b)| *a == id || *b == id).count();
        let mut role: Vec<Option<String>> = snapshot
            .iter()
            .map(|r| r.prop("role").and_then(Value::as_str).map(str::to_string))
            .collect();

        for rule in &self.rules {
            let mut given: Vec<usize> = (0..snapshot.len())
                .filter(|&i| role[i].as_deref() == Some(rule.role.as_str()))
                .collect();
            if given.is_empty() {
                let candidates: Vec<usize> = (0..snapshot.len())
                    .filter(|&i| role[i].is_none() && depth[i].is_some())
                    .filter(|&i| !rule.dead_end || degree(snapshot[i].id) == 1)
                    .collect();
                let chosen = match rule.pick {
                    Pick::First => candidates.first().copied(),
                    // Ties go to the room made first.
                    Pick::Farthest => candidates
                        .iter()
                        .copied()
                        .max_by_key(|&i| (depth[i], std::cmp::Reverse(i))),
                    Pick::Nearest => candidates.iter().copied().min_by_key(|&i| (depth[i], i)),
                    Pick::Random => (!candidates.is_empty()).then(|| {
                        candidates[pick(rng.next_u32(), candidates.len() as u32) as usize]
                    }),
                };
                let Some(i) = chosen else {
                    return Err(OpError::Failed {
                        message: format!(
                            "no room left for the role '{}'{}",
                            rule.role,
                            if rule.dead_end {
                                " among the dead ends"
                            } else {
                                ""
                            }
                        ),
                    });
                };
                role[i] = Some(rule.role.clone());
                given.push(i);
            }
            if let Some(tile) = rule.tile {
                for &i in &given {
                    for c in interior(snapshot[i].bbox).iter() {
                        if tiles.get(c) == Some(&self.floor) {
                            let _ = tiles.set(c, tile);
                        }
                    }
                }
            }
        }

        for (o, (r, d)) in rooms.iter_mut().zip(role.iter().zip(&depth)) {
            if let Some(d) = d {
                o.set("depth", Value::Int(i64::from(*d)));
            }
            if let Some(r) = r {
                o.set("role", Value::Str(r.clone()));
            }
        }
        Ok(())
    }
}
