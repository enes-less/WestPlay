//! The ways a GPS route takes through junctions. The game's route is a chain of map nodes; between two
//! that are ends of the same prefab (a junction, a roundabout, a lane-closing stretch), the way is the
//! prefab's own lanes (its navigation curves, [`Prefab::path`]): round a roundabout the way traffic goes,
//! through a junction along its turning lane. Only the sectors the route crosses are read, so a new route
//! is resolved in a moment.
//!
//! Each junction where the route has a choice also gets its turn ([`Maneuver`]), in the terms the game's
//! own voice navigation uses: turn, keep or exit left or right, U-turn, a roundabout's exit.
//!
//! The game's route runs on its roads' lines, which on a road drawn as two carriageways (a motorway) is its
//! central reservation: each node gets how far to move it across, onto the lanes the route drives (the
//! ones that lead where it goes, from the junctions' own lanes: lanes.rs), and the ways through junctions
//! end there.

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use crate::files::{FileError, GameFiles};
use crate::geometry::{self, Point, PrefabPlacement};
use crate::lanes;
use crate::prefab::Prefab;
use crate::sector::ItemData;
use crate::world::{self, Definitions, MapError, Prefabs};

/// A map node of a route: its uid and where it is.
#[derive(Clone, Copy, Debug)]
pub struct RouteNode {
    pub uid: u64,
    pub position: Point,
}

/// A way the route turns at a junction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Turn {
    SlightLeft,
    Left,
    SharpLeft,
    SlightRight,
    Right,
    SharpRight,
    UTurn,
    /// Where the road forks: the side of it to keep to.
    KeepLeft,
    KeepRight,
    /// Off a bigger road, onto a lesser one leaving it (a motorway's exit).
    ExitLeft,
    ExitRight,
    /// Round a roundabout, out at its `n`th exit (counted from the way in, the way traffic goes).
    Roundabout(u8),
}

impl Turn {
    /// Its name, as the page knows it (`"keep-left"`, `"roundabout"`…).
    pub fn name(self) -> &'static str {
        match self {
            Self::SlightLeft => "slight-left",
            Self::Left => "left",
            Self::SharpLeft => "sharp-left",
            Self::SlightRight => "slight-right",
            Self::Right => "right",
            Self::SharpRight => "sharp-right",
            Self::UTurn => "u-turn",
            Self::KeepLeft => "keep-left",
            Self::KeepRight => "keep-right",
            Self::ExitLeft => "exit-left",
            Self::ExitRight => "exit-right",
            Self::Roundabout(_) => "roundabout",
        }
    }
}

/// The route's turn at a junction: which, and how far it turns (degrees from the way in to the way out,
/// right positive: for drawing a roundabout's exit where it is).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Maneuver {
    pub turn: Turn,
    pub angle: f32,
}

/// The way the route takes through one junction: from the node at index `span` of the route to the next,
/// its points (metres, x z, starting and ending right on the two nodes), and its turn there if it has a
/// choice.
#[derive(Clone, Debug, PartialEq)]
pub struct Junction {
    pub span: usize,
    pub path: Vec<Point>,
    pub maneuver: Option<Maneuver>,
}

/// What the route takes from the game's files: the way through each junction it crosses, and where it runs
/// across its roads.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ways {
    pub junctions: Vec<Junction>,
    /// For each node of the route on a road (its index), how far to the right of the way the route goes the
    /// route runs there, metres (only those off the road's line).
    pub shifts: Vec<(usize, f32)>,
}

/// The game's files and the definitions the junctions need, read once.
pub struct Junctions {
    files: GameFiles,
    definitions: Definitions,
}

impl Junctions {
    /// Opens the game installed in `dir` (the folder with `base.scs`).
    pub fn open(dir: &Path) -> Result<Self, FileError> {
        let files = GameFiles::open(dir)?;
        let definitions = world::load_definitions(&files)?;
        Ok(Self { files, definitions })
    }

    /// The way through each prefab the route `nodes` crosses (each pair of consecutive nodes that are ends
    /// of one prefab), and its turn there; and where the route runs across its roads.
    pub fn junctions(&self, nodes: &[RouteNode]) -> Result<Ways, MapError> {
        // Every sector a node is in, and those around it (a prefab and its nodes may lie in different ones).
        let sectors: BTreeSet<(i32, i32)> = nodes
            .iter()
            .flat_map(|node| {
                let (x, z) = world::sector_of(node.position);
                (x - 1..=x + 1).flat_map(move |x| (z - 1..=z + 1).map(move |z| (x, z)))
            })
            .collect();
        let map = world::load_sectors(&self.files, "europe", &sectors)?;
        let mut prefabs_at: HashMap<u64, Vec<usize>> = HashMap::new();
        for (index, item) in map.items.iter().enumerate() {
            if let ItemData::Prefab { nodes, .. } = &item.data {
                for &uid in nodes {
                    prefabs_at.entry(uid).or_default().push(index);
                }
            }
        }
        let mut prefabs = Prefabs::new(&self.files, &self.definitions);
        let shifted = shifted_nodes(&map, &mut prefabs, &prefabs_at, nodes);
        let mut junctions = Vec::new();
        for (span, pair) in nodes.windows(2).enumerate() {
            let (from, to) = (pair[0], pair[1]);
            let Some(items) = prefabs_at.get(&from.uid) else { continue };
            for &index in items {
                let ItemData::Prefab { model, nodes: ends, origin, .. } = &map.items[index].data else { continue };
                let (Some(a), Some(b)) =
                    (ends.iter().position(|&uid| uid == from.uid), ends.iter().position(|&uid| uid == to.uid))
                else {
                    continue;
                };
                let Some(prefab) = prefabs.get(*model) else { continue };
                let count = prefab.nodes.len();
                let (Some(first), Some(origin_node)) =
                    (ends.first().and_then(|uid| map.nodes.get(uid)), prefab.nodes.get(*origin as usize))
                else {
                    continue;
                };
                // The item lists its nodes from the prefab's origin node on (geometry::prefab_world_node).
                let own = |world: usize| (world + *origin as usize) % count;
                let Some(path) = prefab.path(own(a), own(b)) else { continue };
                let placement = PrefabPlacement::new(origin_node, first);
                let placed: Vec<Point> = path.iter().map(|&p| placement.place(p)).collect();
                junctions.push(Junction { span, path: placed, maneuver: maneuver(prefab, own(a), own(b)) });
                break;
            }
        }
        // Each way through a junction ends where the route runs on the roads there; between two junctions,
        // on the way in's end (the lanes of the one meet the other's).
        let ends: HashMap<usize, Point> =
            junctions.iter().map(|junction| (junction.span + 1, junction.path[junction.path.len() - 1])).collect();
        let starts: HashMap<usize, Point> =
            junctions.iter().map(|junction| (junction.span, junction.path[0])).collect();
        let at = |node: usize| {
            shifted[node]
                .map(|(point, _, _)| point)
                .or_else(|| ends.get(&node).copied())
                .or_else(|| starts.get(&node).copied())
                .unwrap_or(nodes[node].position)
        };
        for junction in &mut junctions {
            let (start, end) = (shifted[junction.span], shifted[junction.span + 1]);
            junction.path = match (junction.maneuver, start, end) {
                // Straight on (no turn to make: on along a motorway past its exit, through a lane closing),
                // between roads: on along them, as the road would run, not along whichever lane the
                // junction's way takes (a lane leaning towards the exit and back).
                (None, Some((from, _, leaving)), Some((to, _, arriving))) => hermite(from, leaving, to, arriving),
                _ => onto_ends(&junction.path, at(junction.span), at(junction.span + 1)),
            };
        }
        let shifts = shifted
            .iter()
            .enumerate()
            .filter_map(|(node, shifted)| {
                shifted.filter(|&(_, shift, _)| shift != 0.0).map(|(_, shift, _)| (node, shift))
            })
            .collect();
        Ok(Ways { junctions, shifts })
    }
}

/// Where the route runs at each of its nodes, how far that is to the right of the way it goes, and that
/// way: on the lanes it drives, taken from the junctions' own lanes. A road's lanes run along it side by
/// side, so the lanes it drives on a road (or at a node between two junctions) are those both leading on
/// where it goes, into the junction ahead (going on past an exit: not the lane leaving), and having come
/// from where it came, out of the junction behind (past a slip road joining: not its lane, closing
/// further on); or those of either, if they have none in common; or else the road's look's
/// ([`lanes::LookOffset::route_shift`]: on its carriageway). On one road both ways (no central
/// reservation), its middle, as the map draws it. `None` where nothing says.
fn shifted_nodes(
    map: &world::Map,
    prefabs: &mut Prefabs,
    prefabs_at: &HashMap<u64, Vec<usize>>,
    nodes: &[RouteNode],
) -> Vec<Option<(Point, f32, Point)>> {
    let measured = lanes::road_offsets(map, prefabs);
    // The lanes a way through a junction drives at its end `at` (a node of the route), to or from the
    // route's node `other`, in the world; and the road's lanes there if it's one road both ways (traffic
    // each way, no central reservation between: lanes::carriageways_at).
    let mut driven = |at: usize, other: usize, entering: bool| -> (Vec<Point>, Vec<Point>) {
        let (Some(here), Some(there)) = (nodes.get(at).map(|n| n.uid), nodes.get(other).map(|n| n.uid)) else {
            return Default::default();
        };
        let Some(items) = prefabs_at.get(&here) else { return Default::default() };
        items
            .iter()
            .find_map(|&index| {
                let ItemData::Prefab { model, nodes: ends, origin, .. } = &map.items[index].data else { return None };
                let (a, b) = (ends.iter().position(|&uid| uid == here)?, ends.iter().position(|&uid| uid == there)?);
                let prefab = prefabs.get(*model)?;
                let first = ends.first().and_then(|uid| map.nodes.get(uid))?;
                let placement = PrefabPlacement::new(prefab.nodes.get(*origin as usize)?, first);
                let own = |world: usize| geometry::prefab_own_node(world, *origin as usize, prefab.nodes.len());
                let lanes = lanes::lanes_driven(prefab, own(a), own(b), entering);
                let end = &prefab.nodes[own(a)];
                let both_ways = !end.curves_in.is_empty()
                    && !end.curves_out.is_empty()
                    && lanes::carriageways_at(prefab, own(a)).is_none();
                let road = if both_ways { lanes::lanes_at(prefab, own(a)) } else { Vec::new() };
                let place = |lanes: Vec<Point>| lanes.into_iter().map(|lane| placement.place(lane)).collect();
                Some((place(lanes), place(road)))
            })
            .unwrap_or_default()
    };
    let roads: HashMap<(u64, u64), u64> = map
        .items
        .iter()
        .filter_map(|item| match &item.data {
            ItemData::Road { look, start_node, end_node, .. } => Some(((*start_node, *end_node), *look)),
            _ => None,
        })
        .collect();
    // The road between two consecutive nodes: its look, and whether the route goes along it.
    let road = |from: usize, to: usize| -> Option<(u64, bool)> {
        let (a, b) = (nodes.get(from)?.uid, nodes.get(to)?.uid);
        roads.get(&(a, b)).map(|&look| (look, true)).or_else(|| roads.get(&(b, a)).map(|&look| (look, false)))
    };
    // The way the route goes at a node: the node's (its roads') way, or against it.
    let way_at = |index: usize, along: Option<bool>| -> Option<Point> {
        let forward = geometry::node_forward(map.nodes.get(&nodes[index].uid)?);
        let along = along.unwrap_or_else(|| {
            let before = nodes[index.saturating_sub(1)].position;
            let after = nodes[(index + 1).min(nodes.len() - 1)].position;
            forward[0] * (after[0] - before[0]) + forward[1] * (after[1] - before[1]) >= 0.0
        });
        Some(if along { forward } else { [-forward[0], -forward[1]] })
    };
    // How far right of a node's line, facing the way the route goes there, lanes lie.
    let across = |index: usize, way: Point, lanes: &[Point]| -> Vec<f32> {
        let line = nodes[index].position;
        lanes.iter().map(|lane| lanes::right_of(way, [lane[0] - line[0], lane[1] - line[1]])).collect()
    };
    (0..nodes.len())
        .map(|index| {
            let on_road = road(index, index + 1).map(|road| (road, index, index + 1)).or_else(|| {
                index.checked_sub(1).and_then(|before| road(before, index).map(|road| (road, before, index)))
            });
            // The stretch the node is on: a road from `first` to `last`, or the node alone.
            let (look, along, first, last) = match on_road {
                Some(((look, along), first, last)) => (Some(look), Some(along), first, last),
                None => (None, None, index, index),
            };
            let way = way_at(index, along)?;
            let (ahead_way, behind_way) = (way_at(last, along)?, way_at(first, along)?);
            let (ahead, ahead_road) = driven(last, last + 1, true);
            let (behind, behind_road) = match first.checked_sub(1) {
                Some(before) => driven(first, before, false),
                None => Default::default(),
            };
            let (ahead, behind) = (across(last, ahead_way, &ahead), across(first, behind_way, &behind));
            let road_both_ways = [across(last, ahead_way, &ahead_road), across(first, behind_way, &behind_road)];
            let both: Vec<f32> =
                ahead.iter().copied().filter(|a| behind.iter().any(|b| (a - b).abs() < SAME_LANE)).collect();
            let middle = |lanes: &[f32]| (!lanes.is_empty()).then(|| lanes.iter().sum::<f32>() / lanes.len() as f32);
            // One road both ways: on its middle, as the map draws it (the truck shown in the middle of the
            // road); a carriageway or a one-way road: on the lanes driven.
            let undivided = road_both_ways.iter().find(|lanes| !lanes.is_empty()).map(|lanes| {
                look.and_then(|look| measured.get(&look))
                    .map_or_else(|| middle(lanes).unwrap_or(0.0), |offset| offset.route_shift(along == Some(true)))
            });
            let shift = undivided
                .or_else(|| middle(&both))
                .or_else(|| middle(&ahead))
                .or_else(|| middle(&behind))
                .or_else(|| {
                    look.map(|look| measured.get(&look).map_or(0.0, |offset| offset.route_shift(along == Some(true))))
                })?;
            let at = nodes[index].position;
            Some(([at[0] - way[1] * shift, at[1] + way[0] * shift], shift, way))
        })
        .collect()
}

/// Lanes this close across their road (metres) at its two ends are the same lane (a lane is 4.5 m).
const SAME_LANE: f32 = 1.0;

/// The game's road curve from `from` (leaving along `leaving`) to `to` (arriving along `arriving`): a cubic
/// Hermite curve with tangents as long as the distance between them (geometry::road_curve), a point about
/// every 8 m.
fn hermite(from: Point, leaving: Point, to: Point, arriving: Point) -> Vec<Point> {
    let length = distance(from, to);
    let segments = (length / 8.0).ceil().clamp(1.0, 64.0) as usize;
    (0..=segments)
        .map(|i| {
            let s = i as f32 / segments as f32;
            let (s2, s3) = (s * s, s * s * s);
            let (h00, h10, h01, h11) = (2.0 * s3 - 3.0 * s2 + 1.0, s3 - 2.0 * s2 + s, -2.0 * s3 + 3.0 * s2, s3 - s2);
            let along = |axis: usize| {
                h00 * from[axis] + h10 * leaving[axis] * length + h01 * to[axis] + h11 * arriving[axis] * length
            };
            [along(0), along(1)]
        })
        .collect()
}

/// Below this (degrees, either way), a way out of a junction goes on straight.
const STRAIGHT: f32 = 20.0;
/// A fork: the way the route doesn't take goes on within this of straight, and the route's within
/// `FORK_TAKEN` (a slip road leaves its motorway at an angle).
const FORK_OTHER: f32 = 25.0;
const FORK_TAKEN: f32 = 50.0;
const SLIGHT: f32 = 45.0;
const TURN: f32 = 135.0;
const SHARP: f32 = 165.0;

/// The route's turn through `prefab`, in at its node `from` and out at `to`; `None` where there's nothing
/// to do: no choice there (a bend, a lane closing, a one-way piece: only the one way on from where it
/// comes in), or the road simply goes on (straight through a crossing, on along a motorway past an exit).
///
/// - A roundabout (lanes running in a loop): out at the exit it reaches in turn, the exits nearer along
///   the ring (shorter ways from the way in) counted first.
/// - Otherwise by how far the way out turns from the way in. Where another way goes on straight too, it's
///   a fork: an exit where the way taken has fewer lanes than the one going on (a motorway's slip road),
///   nothing where it has more (the motorway, past its exit), else kept to on its side (the side of its
///   end across the way in). Otherwise a slight, plain or sharp turn, or back the way it came; straight on
///   is nothing to do.
pub fn maneuver(prefab: &Prefab, from: usize, to: usize) -> Option<Maneuver> {
    let (_, taken) = prefab.lanes(from, to)?;
    let ways: Vec<(usize, f32)> = (0..prefab.nodes.len())
        .filter(|&end| end != from && end != to)
        .filter_map(|end| prefab.lanes(from, end).map(|(_, length)| (end, length)))
        .collect();
    if ways.is_empty() {
        return None;
    }
    // A prefab node's direction points into the prefab: the way in at the node the route comes in at, and
    // back against the way out at the others.
    let way_in = prefab.nodes[from].direction;
    let turn_to = |end: usize| signed_angle(way_in, prefab.nodes[end].direction.map(|d| -d));
    let angle = turn_to(to);
    if prefab.has_ring() {
        let nearer = ways.iter().filter(|&&(_, length)| length < taken).count();
        return Some(Maneuver { turn: Turn::Roundabout((nearer + 1).min(u8::MAX as usize) as u8), angle });
    }
    let other = ways
        .iter()
        .map(|&(end, _)| (end, turn_to(end)))
        .filter(|&(_, other)| other.abs() < FORK_OTHER)
        .min_by(|a, b| a.1.abs().total_cmp(&b.1.abs()));
    let turn = match (angle.abs(), other) {
        (taken_angle, Some((other, _))) if taken_angle < FORK_TAKEN => {
            // Which side of the other way the route's end lies, across the way in.
            let ends = (prefab.nodes[to].position, prefab.nodes[other].position);
            let across = (ends.0[0] - ends.1[0]) * -way_in[1] + (ends.0[1] - ends.1[1]) * way_in[0];
            let right = if across.abs() > 0.5 { across > 0.0 } else { angle > turn_to(other) };
            let lanes = |end: usize| prefab.nodes[end].curves_out.len();
            match (lanes(to).cmp(&lanes(other)), right) {
                (Ordering::Less, true) => Turn::ExitRight,
                (Ordering::Less, false) => Turn::ExitLeft,
                (Ordering::Greater, _) => return None,
                (Ordering::Equal, true) => Turn::KeepRight,
                (Ordering::Equal, false) => Turn::KeepLeft,
            }
        }
        (a, _) if a < STRAIGHT => return None,
        (a, _) if a < SLIGHT => {
            if angle > 0.0 {
                Turn::SlightRight
            } else {
                Turn::SlightLeft
            }
        }
        (a, _) if a < TURN => {
            if angle > 0.0 {
                Turn::Right
            } else {
                Turn::Left
            }
        }
        (a, _) if a < SHARP => {
            if angle > 0.0 {
                Turn::SharpRight
            } else {
                Turn::SharpLeft
            }
        }
        _ => Turn::UTurn,
    };
    Some(Maneuver { turn, angle })
}

/// The turn from the way `from` to the way `to` (unit directions on the ground, x east and z south),
/// degrees, right positive.
fn signed_angle(from: Point, to: Point) -> f32 {
    let cross = from[0] * to[1] - from[1] * to[0];
    let dot = from[0] * to[0] + from[1] * to[1];
    cross.atan2(dot).to_degrees()
}

/// `path` moved to start at `start` and end at `end`: shifted by the gap at each end, easing from the
/// one to the other along it. A prefab's lanes start beside its nodes (in their lane), and the route
/// runs on along the roads' middles.
fn onto_ends(path: &[Point], start: Point, end: Point) -> Vec<Point> {
    let (Some(&first), Some(&last)) = (path.first(), path.last()) else { return Vec::new() };
    let (from, to) = ([start[0] - first[0], start[1] - first[1]], [end[0] - last[0], end[1] - last[1]]);
    let mut along = vec![0.0];
    for pair in path.windows(2) {
        along.push(along.last().unwrap() + distance(pair[0], pair[1]));
    }
    let total = along.last().copied().unwrap_or(0.0).max(f32::EPSILON);
    path.iter()
        .zip(&along)
        .map(|(p, &at)| {
            let t = at / total;
            [p[0] + from[0] + (to[0] - from[0]) * t, p[1] + from[1] + (to[1] - from[1]) * t]
        })
        .collect()
}

fn distance(a: Point, b: Point) -> f32 {
    (b[0] - a[0]).hypot(b[1] - a[1])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prefab::{NavCurve, PrefabNode};

    #[test]
    fn straight_on_runs_along_the_road_between_the_carriageway_ends() {
        // Heading east on the carriageway 6.5 m right of a motorway's line: on it the whole way.
        let curve = hermite([0.0, 6.5], [1.0, 0.0], [100.0, 6.5], [1.0, 0.0]);
        assert_eq!((curve[0], curve[curve.len() - 1]), ([0.0, 6.5], [100.0, 6.5]));
        assert!(curve.iter().all(|p| (p[1] - 6.5).abs() < 1e-4));
        assert_eq!(curve.len(), 14);
    }

    /// A prefab from its ends (where, and the way into the prefab there) and its lanes (from, to, and the
    /// lanes they lead on to), each lane straight.
    fn prefab(ends: &[(Point, Point)], lanes: &[(Point, Point, Vec<usize>)]) -> Prefab {
        let near = |a: Point, b: Point| (a[0] - b[0]).hypot(a[1] - b[1]) < 0.1;
        let nodes = ends
            .iter()
            .map(|&(position, direction)| PrefabNode {
                height: 0.0,
                position,
                direction,
                lanes: 0,
                curves_in: (0..lanes.len()).filter(|&i| near(lanes[i].0, position)).collect(),
                curves_out: (0..lanes.len()).filter(|&i| near(lanes[i].1, position)).collect(),
            })
            .collect();
        let curves = lanes
            .iter()
            .map(|(start, end, next)| {
                let length = (end[0] - start[0]).hypot(end[1] - start[1]);
                let way = [(end[0] - start[0]) / length, (end[1] - start[1]) / length];
                NavCurve {
                    heights: [0.0, 0.0],
                    start: *start,
                    end: *end,
                    start_direction: way,
                    end_direction: way,
                    length,
                    next: next.clone(),
                }
            })
            .collect();
        Prefab { nodes, map_points: Vec::new(), spawn_points: Vec::new(), triggers: Vec::new(), curves }
    }

    fn turn(prefab: &Prefab, from: usize, to: usize) -> Option<Turn> {
        maneuver(prefab, from, to).map(|maneuver| maneuver.turn)
    }

    #[test]
    fn turns_at_a_crossing_by_how_far_the_way_out_turns() {
        // In from the south (heading north, -z); out east, north or west. x east, z south.
        let (south, east, north, west) = ([0.0, 20.0], [20.0, 0.0], [0.0, -20.0], [-20.0, 0.0]);
        let crossing = prefab(
            &[(south, [0.0, -1.0]), (east, [-1.0, 0.0]), (north, [0.0, 1.0]), (west, [1.0, 0.0])],
            &[(south, east, vec![]), (south, north, vec![]), (south, west, vec![])],
        );
        assert_eq!(turn(&crossing, 0, 1), Some(Turn::Right));
        assert_eq!(turn(&crossing, 0, 3), Some(Turn::Left));
        // Straight on is nothing to do.
        assert_eq!(turn(&crossing, 0, 2), None);
        // Where there's no other way, there's no turn to make.
        let bend = prefab(&[(south, [0.0, -1.0]), (east, [-1.0, 0.0])], &[(south, east, vec![])]);
        assert_eq!(turn(&bend, 0, 1), None);
    }

    #[test]
    fn a_slip_road_is_an_exit_and_the_motorway_past_it_nothing() {
        // A three-lane motorway heading north: two lanes go on, one leaves to the right.
        let (start, on, off) = ([0.0, 0.0], [-2.0, -40.0], [15.0, -38.0]);
        let exit = prefab(
            &[(start, [0.0, -1.0]), (on, [0.0, 1.0]), (off, [-0.3, 0.95])],
            &[(start, on, vec![]), ([1.0, 0.0], on, vec![]), (start, off, vec![])],
        );
        assert_eq!(turn(&exit, 0, 2), Some(Turn::ExitRight));
        assert_eq!(turn(&exit, 0, 1), None);
        // As many lanes each way: a fork, kept to on its side.
        let fork = prefab(
            &[(start, [0.0, -1.0]), (on, [0.0, 1.0]), (off, [-0.3, 0.95])],
            &[(start, on, vec![]), (start, off, vec![])],
        );
        assert_eq!(turn(&fork, 0, 2), Some(Turn::KeepRight));
        assert_eq!(turn(&fork, 0, 1), Some(Turn::KeepLeft));
    }

    #[test]
    fn a_roundabouts_exits_count_the_way_traffic_goes() {
        // Driving on the right: round anticlockwise as seen from above (x east, z south: south, east, north,
        // west). In from the south; its exits east (1st), north (2nd), west (3rd).
        let ring = [[0.0, 10.0], [10.0, 0.0], [0.0, -10.0], [-10.0, 0.0]];
        let (south, east, north, west) = ([0.0, 30.0], [30.0, 0.0], [0.0, -30.0], [-30.0, 0.0]);
        let roundabout = prefab(
            &[(south, [0.0, -1.0]), (east, [-1.0, 0.0]), (north, [0.0, 1.0]), (west, [1.0, 0.0])],
            &[
                (south, ring[0], vec![1]),
                (ring[0], ring[1], vec![2, 5]),
                (ring[1], ring[2], vec![3, 6]),
                (ring[2], ring[3], vec![4, 7]),
                (ring[3], ring[0], vec![1]),
                (ring[1], east, vec![]),
                (ring[2], north, vec![]),
                (ring[3], west, vec![]),
            ],
        );
        assert!(roundabout.has_ring());
        assert_eq!(turn(&roundabout, 0, 1), Some(Turn::Roundabout(1)));
        assert_eq!(turn(&roundabout, 0, 2), Some(Turn::Roundabout(2)));
        assert_eq!(turn(&roundabout, 0, 3), Some(Turn::Roundabout(3)));
    }

    #[test]
    fn a_path_through_a_prefab_meets_the_route_on_its_nodes() {
        // A lane 2 m to the side of the road's middle, from one node to the next.
        let lane = [[0.0, 2.0], [10.0, 2.0], [20.0, 2.0]];
        let path = onto_ends(&lane, [0.0, 0.0], [20.0, 0.0]);
        assert_eq!(path.first(), Some(&[0.0, 0.0]));
        assert_eq!(path.last(), Some(&[20.0, 0.0]));
        assert!((path[1][1]).abs() < 1e-6);
        // Each end its own gap: halfway, half of each.
        let path = onto_ends(&lane, [0.0, 0.0], [20.0, 4.0]);
        assert!((path[1][1] - 2.0).abs() < 1e-6 && (path[1][0] - 10.0).abs() < 1e-6);
    }
}
