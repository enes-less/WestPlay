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
//! central reservation: each node gets how far to move it across, onto the side it drives ([`on_the_road`]),
//! and the ways through junctions end there.

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use crate::files::{FileError, GameFiles};
use crate::geometry::{self, Point, PrefabPlacement};
use crate::lanes::{self, Carriageways};
use crate::prefab::Prefab;
use crate::sector::ItemData;
use crate::surface::{self, roads::{self, Looks, Section}};
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
    /// The way of each node's roads, either way along them: what a route planned here has no word of, and the game's
    /// route has.
    pub forward: Vec<Point>,
}

/// The game's files (with the map mods the player plays), the map they're read for, and the definitions the
/// junctions need, read once.
pub struct Junctions {
    files: GameFiles,
    map: String,
    definitions: Definitions,
    looks: Looks,
}

impl Junctions {
    /// Opens the game installed in `dir` (the folder with `base.scs`), for its base map `map` (`europe`, `usa`).
    pub fn open(dir: &Path, map: &str) -> Result<Self, FileError> {
        Self::new(GameFiles::open(dir)?, map)
    }

    /// The map `map` (`europe`, or a standalone map mod's) as `files` have it: the game with the map mods the
    /// player plays mounted over it, as the map the cab shows was built from, so their junctions are found.
    pub fn new(files: GameFiles, map: &str) -> Result<Self, FileError> {
        let definitions = world::load_definitions(&files)?;
        let looks = surface::road_looks(&files);
        Ok(Self { files, map: map.to_owned(), definitions, looks })
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
        let map = world::load_sectors(&self.files, &self.map, &sectors)?;
        let mut prefabs_at: HashMap<u64, Vec<usize>> = HashMap::new();
        for (index, item) in map.items.iter().enumerate() {
            if let ItemData::Prefab { nodes, .. } = &item.data {
                for &uid in nodes {
                    prefabs_at.entry(uid).or_default().push(index);
                }
            }
        }
        let mut prefabs = Prefabs::new(&self.files, &self.definitions);
        let shifted = shifted_nodes(&map, &self.definitions, &self.looks, &mut prefabs, &prefabs_at, nodes);
        let mut junctions = Vec::new();
        // The junctions where trucks are kept to lanes of their own (a toll plaza's booths): their way is those lanes.
        let mut trucks_apart = BTreeSet::new();
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
                if prefab.trucks_kept_apart(own(a), own(b)) {
                    trucks_apart.insert(span);
                }
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
                // junction's way takes (a lane leaning towards the exit and back). Not where trucks are kept to
                // lanes of their own: through a toll plaza the game's GPS takes the truck's booth, to the side (owner
                // 2026-11-11: the game's route went to a gate on the right, ours straight through the cars').
                (None, Some((from, _, leaving)), Some((to, _, arriving))) if !trucks_apart.contains(&junction.span) => {
                    hermite(from, leaving, to, arriving)
                }
                // Straight through a toll plaza on the truck's own lanes: on them at its booths, its middle.
                (None, _, _) if trucks_apart.contains(&junction.span) => {
                    onto_ends_from_lanes(&junction.path, at(junction.span), at(junction.span + 1))
                }
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
        let forward = nodes.iter().map(|node| map.nodes.get(&node.uid).map_or([1.0, 0.0], geometry::node_forward)).collect();
        Ok(Ways { junctions, shifts, forward })
    }
}

/// Less than this between a road's two surfaces is them meeting, metres: one asphalt (the gaps the game's looks have are none
/// or from 0.3 m).
const ONE_ASPHALT: f64 = 0.05;

/// A road a route is on: its surface, its two directions' lanes ([`lanes::directions_of`]), and whether the route goes along
/// it.
#[derive(Clone, Copy)]
struct OnRoad<'a> {
    section: &'a Section,
    ways: Option<Carriageways>,
    along: bool,
}

/// How far out of a road's line the middles of its two `ways` lie, its right's and its left's; `None` for a road one way. On
/// two surfaces with a gap between (separate asphalt), each one's middle, not the lanes leading on, which lean left past an
/// exit, where trucks never drive (owner 2026-10-31). On one surface, each way's lanes: a truck drives on its side of the
/// road, not down its middle (owner 2026-11-01), and a route down its middle jogs to them at every junction.
fn ways_apart(section: &Section, ways: Option<Carriageways>) -> Option<[f32; 2]> {
    let ways = ways?;
    let gap = section.inner.filter(|inner| inner[0] + inner[1] >= ONE_ASPHALT);
    Some(gap.map_or([ways.right, -ways.left], |inner| {
        [((inner[0] + section.outer[0]) / 2.0) as f32, ((inner[1] + section.outer[1]) / 2.0) as f32]
    }))
}

/// How far right of the way it goes a route on a road runs: on its own way ([`ways_apart`]), and on a road one way, on the
/// middle of its surface as the map draws it. Going `along` the road or against it; `keeps_right`: the side traffic keeps
/// to, which says which of its two ways it drives.
fn on_the_road(section: &Section, ways: Option<Carriageways>, along: bool, keeps_right: bool) -> f32 {
    match (ways_apart(section, ways), keeps_right, along) {
        (Some([right, _]), true, true) => right,
        (Some([_, left]), true, false) => left,
        (Some([_, left]), false, true) => -left,
        (Some([right, _]), false, false) => -right,
        (None, ..) => {
            let middle = ((section.outer[0] - section.outer[1]) / 2.0) as f32;
            if along { middle } else { -middle }
        }
    }
}

/// Where the route runs at each of its nodes, how far that is to the right of the way it goes, and that
/// way: on a road, as [`on_the_road`] says; at a node with no road (between two junctions), as on a
/// road: on the lanes it drives, taken from the junctions' own lanes: those both leading on where it goes,
/// into the junction ahead, and having come from where it came, out of the junction behind; or those of
/// either, if they have none in common; with none driven, the middle of the road's. `None` where nothing says.
fn shifted_nodes(
    map: &world::Map,
    definitions: &Definitions,
    looks: &Looks,
    prefabs: &mut Prefabs,
    prefabs_at: &HashMap<u64, Vec<usize>>,
    nodes: &[RouteNode],
) -> Vec<Option<(Point, f32, Point)>> {
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
    let sections: HashMap<usize, Section> = roads::road_sections(map, definitions, looks).into_iter().collect();
    let roads: HashMap<(u64, u64), usize> = map
        .items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| match &item.data {
            ItemData::Road { start_node, end_node, .. } => Some(((*start_node, *end_node), index)),
            _ => None,
        })
        .collect();
    // The road between two consecutive nodes.
    let road = |from: usize, to: usize| -> Option<OnRoad> {
        let (a, b) = (nodes.get(from)?.uid, nodes.get(to)?.uid);
        let (index, along) = roads.get(&(a, b)).map(|&road| (road, true)).or_else(|| roads.get(&(b, a)).map(|&road| (road, false)))?;
        let ItemData::Road { look, .. } = &map.items[index].data else { return None };
        Some(OnRoad { section: sections.get(&index)?, ways: looks.get(*look).and_then(|look| look.ways), along })
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
    // Each node's way, its road, the middle of the lanes driven there, and of one road both ways there.
    let found: Vec<_> = (0..nodes.len())
        .map(|index| {
            let on_road = road(index, index + 1).map(|road| (road, index, index + 1)).or_else(|| {
                index.checked_sub(1).and_then(|before| road(before, index).map(|road| (road, before, index)))
            });
            // The stretch the node is on: a road from `first` to `last`, or the node alone.
            let (on, first, last) = match on_road {
                Some((on, first, last)) => (Some(on), first, last),
                None => (None, index, index),
            };
            let along = on.map(|on| on.along);
            let way = way_at(index, along)?;
            let (ahead_way, behind_way) = (way_at(last, along)?, way_at(first, along)?);
            let (ahead, ahead_road) = driven(last, last + 1, true);
            let (behind, behind_road) = first.checked_sub(1).map_or_else(Default::default, |before| driven(first, before, false));
            let (ahead, behind) = (across(last, ahead_way, &ahead), across(first, behind_way, &behind));
            let road_both_ways = [across(last, ahead_way, &ahead_road), across(first, behind_way, &behind_road)];
            let both: Vec<f32> =
                ahead.iter().copied().filter(|a| behind.iter().any(|b| (a - b).abs() < SAME_LANE)).collect();
            let middle = |lanes: &[f32]| (!lanes.is_empty()).then(|| lanes.iter().sum::<f32>() / lanes.len() as f32);
            let driven = middle(&both).or_else(|| middle(&ahead)).or_else(|| middle(&behind));
            Some((way, on, driven, road_both_ways.iter().find_map(|lanes| middle(lanes))))
        })
        .collect();
    // The side traffic keeps to at each node of a road whose ways run apart, by the side the lanes driven lie on (the UK's
    // and Ireland's roads are the same looks); where no junction near says, as at the nearest node that does.
    let says: Vec<Option<bool>> = found
        .iter()
        .map(|node| {
            let (_, on, lanes, _) = node.as_ref()?;
            on.filter(|on| ways_apart(on.section, on.ways).is_some())?;
            lanes.map(|lanes| lanes >= 0.0)
        })
        .collect();
    let keeps_right = |index: usize| {
        let (back, on) = (says[..=index].iter().rev().flatten().next(), says[index..].iter().flatten().next());
        back.or(on).copied().unwrap_or(true)
    };
    let shifts: Vec<Option<f32>> = found
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let (_, on, lanes, both_ways) = node.as_ref()?;
            match on {
                Some(on) => Some(on_the_road(on.section, on.ways, on.along, keeps_right(index))),
                None => lanes.or(*both_ways),
            }
        })
        .collect();
    // How far off its lanes a route runs on a road, where a junction's lanes meet them.
    let off_lanes: Vec<Option<f32>> = found
        .iter()
        .zip(&shifts)
        .map(|(node, shift)| {
            let (_, on, lanes, _) = node.as_ref()?;
            on.as_ref()?;
            Some((*shift)? - (*lanes)?)
        })
        .collect();
    // A node between two junctions, on a carriageway: as far off the lanes as the roads either side of the junctions run,
    // going over from one to the other along the way, so the line doesn't step across in a short piece (ProMods joins its
    // roads by 6.6 m pieces).
    let between_junctions = |node: usize| matches!(found[node], Some((_, None, _, None)));
    let off_between = |index: usize| -> f32 {
        let before = (0..index).rev().find(|&node| !between_junctions(node));
        let after = (index + 1..nodes.len()).find(|&node| !between_junctions(node));
        let off = |node: Option<usize>| node.and_then(|node| off_lanes[node]).unwrap_or(0.0);
        let along = |from: usize, to: usize| (from..to).map(|node| distance(nodes[node].position, nodes[node + 1].position)).sum::<f32>();
        let (first, last) = (before.unwrap_or(0), after.unwrap_or(nodes.len() - 1));
        off(before) + (off(after) - off(before)) * along(first, index) / along(first, last).max(1e-3)
    };
    found
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let (way, ..) = node.as_ref()?;
            let shift = shifts[index]? + if between_junctions(index) { off_between(index) } else { 0.0 };
            let at = nodes[index].position;
            Some(([at[0] - way[1] * shift, at[1] + way[0] * shift], shift, *way))
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

/// `path` moved to start at `start` and end at `end` as [`onto_ends`], each end's gap easing out by the middle of
/// it: on its own lanes there (a toll plaza's truck booths, which the game keeps trucks to).
fn onto_ends_from_lanes(path: &[Point], start: Point, end: Point) -> Vec<Point> {
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
            let (a, b) = ((1.0 - 2.0 * t).max(0.0), (2.0 * t - 1.0).max(0.0));
            [p[0] + from[0] * a + to[0] * b, p[1] + from[1] * a + to[1] * b]
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

    fn section(outer: [f64; 2], inner: Option<[f64; 2]>) -> Section {
        Section { outer, inner, strips: Vec::new(), ground_from: outer }
    }

    /// Lanes lying apart, each way's middle this far out.
    fn apart(out: f32) -> Option<Carriageways> {
        Some(Carriageways { left: -out, left_lanes: 2, right: out, right_lanes: 2 })
    }

    #[test]
    fn a_route_runs_on_the_middle_of_the_surface_of_its_own_carriageway() {
        // Two lanes and a shoulder each way, 4.25 m out from the road's line to 17.25 m.
        let motorway = section([17.25, 17.25], Some([4.25, 4.25]));
        assert_eq!(on_the_road(&motorway, apart(6.5), true, true), 10.75);
        assert_eq!(on_the_road(&motorway, apart(6.5), false, true), 10.75);
        // Keeping left, as in the UK: left of the way it goes.
        assert_eq!(on_the_road(&motorway, apart(6.5), true, false), -10.75);
        assert_eq!(on_the_road(&motorway, apart(6.5), false, false), -10.75);
        // Wider on one side: each way on its own.
        let uneven = section([14.0, 18.0], Some([4.0, 4.0]));
        assert_eq!((on_the_road(&uneven, apart(6.5), true, true), on_the_road(&uneven, apart(6.5), false, true)), (9.0, 11.0));
        // Lanes 4 m apart, surfaces 2 m (ATS's `us_tmpl14`, 3+3: read as one road the route ran down the gap).
        assert_eq!(on_the_road(&section([19.0, 19.0], Some([1.0, 1.0])), apart(8.75), true, true), 10.0);
    }

    #[test]
    fn on_one_surface_a_route_runs_on_its_lanes_if_they_lie_apart() {
        // A 2-1-2 road: 6.75 m out each way, the turning lane between them paved.
        let road = section([12.45, 12.45], None);
        assert_eq!((on_the_road(&road, apart(6.75), true, true), on_the_road(&road, apart(6.75), false, true)), (6.75, 6.75));
        assert_eq!((on_the_road(&road, apart(6.75), true, false), on_the_road(&road, apart(6.75), false, false)), (-6.75, -6.75));
    }

    #[test]
    fn on_a_road_both_ways_a_route_runs_on_its_own_side() {
        // One lane each way, nothing between them.
        let road = section([3.75, 3.75], None);
        assert_eq!((on_the_road(&road, apart(2.25), true, true), on_the_road(&road, apart(2.25), false, true)), (2.25, 2.25));
        assert_eq!(on_the_road(&road, apart(2.25), true, false), -2.25);
        // Two surfaces meeting, a painted middle.
        assert_eq!(on_the_road(&section([9.5, 9.5], Some([0.0, 0.0])), apart(4.75), true, true), 4.75);
        // Finland's `balt23`, two lanes each way, a 2 m reservation: on the surface of its own side, not on the reservation.
        let split = section([11.0, 11.0], Some([1.0, 1.0]));
        assert_eq!((on_the_road(&split, apart(5.5), true, true), on_the_road(&split, apart(5.5), false, true)), (6.0, 6.0));
    }

    #[test]
    fn on_a_road_one_way_a_route_runs_on_its_middle() {
        assert_eq!(on_the_road(&section([9.0, 9.0], None), None, true, true), 0.0);
        // A road whose surface lies off its line, seen from either way along it.
        let aside = section([0.0, 4.5], None);
        assert_eq!((on_the_road(&aside, None, true, true), on_the_road(&aside, None, false, true)), (-2.25, 2.25));
    }

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
                rise: 0.0,
                terrain_points: Vec::new(),
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
                    rotations: [[1.0, 0.0, 0.0, 0.0]; 2],
                    length,
                    next: next.clone(),
                    trucks: true,
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

    #[test]
    fn through_a_toll_plaza_the_route_runs_on_the_trucks_booth() {
        // The truck's booth 12 m right of the road's middle, its lanes there from 10 m to 30 m; the road's middle at
        // either end, the lanes in and out 2 m beside it.
        let lanes = [[0.0, 2.0], [10.0, 12.0], [20.0, 12.0], [30.0, 12.0], [40.0, 2.0]];
        let path = onto_ends_from_lanes(&lanes, [0.0, 0.0], [40.0, 0.0]);
        assert_eq!((path[0], path[4]), ([0.0, 0.0], [40.0, 0.0]));
        // At the booth (the middle), on the truck's lane; on the way to it, the gap easing out.
        assert_eq!(path[2], [20.0, 12.0]);
        assert!(path[1][1] > 10.0 && path[1][1] < 12.0 && path[3][1] > 10.0 && path[3][1] < 12.0);
    }
}
