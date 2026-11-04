//! The map's surfaces as the game has them: roads, junctions, paved ground and what stands on it, drawn as exact outlines
//! from the game's own geometry, into vector tiles of their own beside the map's others.
//!
//! Everything comes from what the game builds: a road's edges from its look's template and edge models along its curve,
//! a prefab's surfaces from its model, sorted by where its lanes run ([`sort`]), its edges' terrain, corner models and
//! the terrain mods build junctions of ([`ground`]). All of it shows, as the truck sees it, roads the game leaves off its
//! own map too; further out the map's road lines take over ([`lines`]). Secret roads never show: they're for finding.
//!
//! The geometry was proven against the item boxes the game stores (every terrain, curve and model item's box is its
//! generated geometry plus exactly 0.125 m a side): curves are 3D Hermite splines with tangents the stored length along
//! the nodes' forward, a road's cross-lines are its end nodes' right vectors blended by the spline's own weights,
//! prefabs are placed by their origin node's full 3D rotation.

mod faces;
mod ground;
mod levels;
mod lines;
mod prefabs;
pub mod raised;
pub mod roads;
pub mod service;
mod shapes;
mod sort;
pub(crate) mod tile;
mod water;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

pub use water::{find_water, river};

use crate::files::GameFiles;
use crate::geometry;
use crate::prefab::{self, Prefab};
use crate::sector::{Item, ItemData, Node};
use crate::tiles::Feature;
use crate::token::string_to_token;
use crate::world::{Definitions, Map, RoadClass};

use faces::Materials;
use ground::{Paints, Painted, Quad};
use levels::Face;
use lines::{RoadLine, Tubes, road_features, road_zoom};
use prefabs::{CornerModels, Drawing, ModelKey, Sorted};
use roads::{Looks, Network};
use sort::{FaceKind, Rules};
use tile::{Block, Claim, Shape};

/// A ring in a prefab's own space: x, y, z.
pub type Own3 = Vec<[f32; 3]>;
/// A ring on the ground: x, y, z.
type Own64 = Vec<[f64; 3]>;

/// What a surface is on the map.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    /// Paved ground, sidewalks, shoulders: the map's one quiet colour.
    Quiet,
    /// A flat roof or a slab high above the lanes.
    Building,
    /// Water in a model: a pool, a fountain, a harbour's basin.
    Water,
    Rail,
    Road(RoadClass),
    /// A road in a tunnel.
    Tunnel(RoadClass),
}

impl Kind {
    fn of(class: RoadClass) -> Self {
        if class == RoadClass::Rail { Self::Rail } else { Self::Road(class) }
    }
}

/// The action the game offers rest with: its areas are parking, never road.
const PARKING: &str = "hud_parking";
/// The zoom tiles are worked out on, and the furthest out roads show as surfaces: its outlines hold every detail. Further
/// out a road is barely wider than a line, and the map's road lines read better.
pub const SURFACE_ZOOM: u8 = 15;
/// The furthest-out zoom with water in models, made from the zoom before.
pub const SURFACE_FAR_ZOOM: u8 = 13;
/// The zoom items are shared out to threads by: a tile a few kilometres across.
const BLOCK_ZOOM: u8 = 12;
/// A road piece under this long with an end joining nothing is a driveway, from far, m.
const STUB: f32 = 100.0;
/// How many rounds of stubs of stubs are taken off.
const STUB_ROUNDS: usize = 3;
/// From far, a bit of the game's network joined to nothing bigger, under this long in all, isn't part of it, m: it hangs
/// off roads the game leaves off its map.
const FRAGMENT: f32 = 400.0;
/// A road under a tunnel's roof for less than this, in one run, is a road under a deck, m: the map shows the road it is
/// (the A1 into Paris runs under 50 m decks with as much open between them: it would look cut).
const TUNNEL: f32 = 100.0;
/// How many prefabs in a row a road's class is carried across.
const CLASS_ROUNDS: usize = 8;
/// A lane this close above or below painted ground runs on it, m.
const ON_GROUND: f64 = 1.0;
/// How far from a lane with no surface of its own the painted ground counts as its road, m.
pub(crate) const CLAIM_REACH: f64 = 2.75;
/// A lane's half width, m: its band where nothing's drawn under a lane between two shown roads.
pub(crate) const LANE_HALF: f64 = 2.25;
/// The most a place's ways through reach past their lanes, m.
const PLACE_REACH: f64 = 4.0;
/// A rail track's half width, m (the game's rail templates).
pub(crate) const TRACK_HALF: f64 = 1.49;
/// A road's line from far has a point about every this many metres.
const LINE_STEP: f64 = 8.0;
/// Items that may lie over each other are looked for in cells this big, m.
const STACK_CELL: f64 = 128.0;

pub fn rotate(q: [f64; 4], v: [f64; 3]) -> [f64; 3] {
    let (w, x, y, z) = (q[0], q[1], q[2], q[3]);
    let t = [2.0 * (y * v[2] - z * v[1]), 2.0 * (z * v[0] - x * v[2]), 2.0 * (x * v[1] - y * v[0])];
    [v[0] + w * t[0] + (y * t[2] - z * t[1]), v[1] + w * t[1] + (z * t[0] - x * t[2]), v[2] + w * t[2] + (x * t[1] - y * t[0])]
}

fn multiply(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    [
        a[0] * b[0] - a[1] * b[1] - a[2] * b[2] - a[3] * b[3],
        a[0] * b[1] + a[1] * b[0] + a[2] * b[3] - a[3] * b[2],
        a[0] * b[2] - a[1] * b[3] + a[2] * b[0] + a[3] * b[1],
        a[0] * b[3] + a[1] * b[2] - a[2] * b[1] + a[3] * b[0],
    ]
}

/// The rotation taking -z to `d`: a pitch about x, then a turn about y.
fn look_along(d: [f64; 3]) -> [f64; 4] {
    let flat = d[0].hypot(d[2]);
    let yaw = (-d[0]).atan2(-d[2]);
    let pitch = d[1].atan2(flat);
    let (sy, cy) = (yaw / 2.0).sin_cos();
    let (sp, cp) = (pitch / 2.0).sin_cos();
    multiply([cy, 0.0, sy, 0.0], [cp, sp, 0.0, 0.0])
}

/// The game's strip along two nodes at parameter `s`: the 3D curve point and the cross-line (roads and terrain: the
/// ends' cross-directions blended; curve items: the curve's own normal).
pub fn frame(a: &Node, b: &Node, length: f64, s: f64, blended: bool) -> ([f64; 3], [f64; 2]) {
    let (qa, qb) = (a.rotation.map(f64::from), b.rotation.map(f64::from));
    let (p0, p1) = (a.position.map(f64::from), b.position.map(f64::from));
    let (m0, m1) = (rotate(qa, [0.0, 0.0, -length]), rotate(qb, [0.0, 0.0, -length]));
    let (s2, s3) = (s * s, s * s * s);
    let (h00, h10, h01, h11) = (2.0 * s3 - 3.0 * s2 + 1.0, s3 - 2.0 * s2 + s, -2.0 * s3 + 3.0 * s2, s3 - s2);
    let p = [0, 1, 2].map(|k| h00 * p0[k] + h10 * m0[k] + h01 * p1[k] + h11 * m1[k]);
    if blended {
        let (r0, r1) = (rotate(qa, [1.0, 0.0, 0.0]), rotate(qb, [1.0, 0.0, 0.0]));
        let r = [h00 * r0[0] + h01 * r1[0], h00 * r0[2] + h01 * r1[2]];
        let long = r[0].hypot(r[1]).max(1e-12);
        (p, [r[0] / long, r[1] / long])
    } else {
        let (d00, d10, d01, d11) = (6.0 * s2 - 6.0 * s, 3.0 * s2 - 4.0 * s + 1.0, -6.0 * s2 + 6.0 * s, 3.0 * s2 - 2.0 * s);
        let d = [0, 2].map(|k| d00 * p0[k] + d10 * m0[k] + d01 * p1[k] + d11 * m1[k]);
        let long = d[0].hypot(d[1]).max(1e-12);
        (p, [-d[1] / long, d[0] / long])
    }
}

/// Where a prefab is: its own space turned and moved on to the map by its origin node.
#[derive(Clone, Copy, Debug)]
struct Placed {
    turn: [f64; 4],
    position: [f64; 3],
    own_at: [f64; 3],
}

impl Placed {
    fn new(prefab: &Prefab, origin: usize, at: &Node) -> Option<Self> {
        let node = prefab.nodes.get(origin)?;
        let own_at = [f64::from(node.position[0]), f64::from(node.height), f64::from(node.position[1])];
        let way = [f64::from(node.direction[0]), f64::from(node.rise), f64::from(node.direction[1])];
        let own = look_along(way);
        let turn = multiply(at.rotation.map(f64::from), [own[0], -own[1], -own[2], -own[3]]);
        Some(Self { turn, position: at.position.map(f64::from), own_at })
    }

    fn world(&self, v: [f64; 3]) -> [f64; 3] {
        let r = rotate(self.turn, [v[0] - self.own_at[0], v[1] - self.own_at[1], v[2] - self.own_at[2]]);
        [self.position[0] + r[0], self.position[1] + r[1], self.position[2] + r[2]]
    }
}

/// What the tables are that every surface is read with.
struct Tables<'a> {
    files: &'a GameFiles,
    definitions: &'a Definitions,
    materials: Materials,
    rules: Rules,
    paints: Paints,
    looks: Looks,
    corners: CornerModels,
}

/// The class of a road at each node, carried across prefabs to the nodes with no road on them.
struct Classes {
    node: HashMap<u64, RoadClass>,
    beyond: HashMap<(u64, usize), RoadClass>,
    /// What's left seen from far: the roads that aren't stubs, by item; each node's class among them.
    stubs: HashSet<usize>,
    far: HashMap<u64, RoadClass>,
}

fn prefab_nodes(item: &Item) -> Option<(&[u64], usize, u64)> {
    match &item.data {
        ItemData::Prefab { nodes, origin, model, .. } => Some((nodes, *origin as usize, *model)),
        _ => None,
    }
}

/// A service area being worked out: the box its paving can be in, where it offers fuel or rest, what the blocks done so far
/// found of its paving, and how many are still to come.
struct Gathering {
    reach: [f64; 4],
    points: Vec<[f64; 2]>,
    left: usize,
    found: Vec<service::Paving>,
}

/// A block's quiet ground and road surfaces for the service areas round it, each with its box.
struct BlockPaving {
    pieces: Vec<(service::Piece, [f64; 4])>,
    roads: Vec<(u32, Vec<service::Piece>, [f64; 4])>,
    within: [f64; 4],
}

fn box_of<'a>(rings: impl Iterator<Item = &'a Own64>) -> [f64; 4] {
    rings.map(|ring| ring_box(ring)).fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |b, r| {
        [b[0].min(r[0]), b[1].min(r[1]), b[2].max(r[2]), b[3].max(r[3])]
    })
}

impl BlockPaving {
    fn new(pieces: Vec<service::Piece>, roads: Vec<(u32, Vec<service::Piece>)>, within: [f64; 4]) -> Self {
        let pieces = pieces
            .into_iter()
            .map(|piece| {
                let found = box_of(piece.iter());
                (piece, found)
            })
            .collect();
        let roads = roads
            .into_iter()
            .map(|(road, pieces)| {
                let found = box_of(pieces.iter().flatten());
                (road, pieces, found)
            })
            .collect();
        Self { pieces, roads, within }
    }

    /// What's in `reach`.
    fn in_reach(&self, reach: [f64; 4]) -> service::Paving {
        let meets = |b: &[f64; 4]| b[0] <= reach[2] && b[2] >= reach[0] && b[1] <= reach[3] && b[3] >= reach[1];
        service::Paving {
            within: self.within,
            pieces: self.pieces.iter().filter(|(_, found)| meets(found)).map(|(piece, _)| piece.clone()).collect(),
            roads: self.roads.iter().filter(|(.., found)| meets(found)).map(|(road, pieces, _)| (*road, pieces.clone())).collect(),
        }
    }
}

/// Where the map offers fuel or rest: what its prefabs' spawn points and triggers say, and the rest triggers laid by
/// themselves (what the map's icons come from).
fn service_points(map: &Map, near: &[usize], keys: &HashMap<usize, ModelKey>, prefabs: &HashMap<u64, Arc<Prefab>>, parking: u64) -> Vec<[f64; 2]> {
    let mut points = Vec::new();
    for &index in near {
        let Some((nodes, origin, model)) = prefab_nodes(&map.items[index]) else { continue };
        let (true, Some(prefab), Some(at)) = (keys.contains_key(&index), prefabs.get(&model), nodes.first().and_then(|uid| map.nodes.get(uid))) else { continue };
        let Some(placed) = Placed::new(prefab, origin, at) else { continue };
        let mut add = |own: [f32; 2]| {
            let world = placed.world([f64::from(own[0]), placed.own_at[1], f64::from(own[1])]);
            points.push([world[0], world[2]]);
        };
        for spawn in &prefab.spawn_points {
            if matches!(spawn.kind, prefab::SpawnKind::Fuel | prefab::SpawnKind::Parking) {
                add(spawn.position);
            }
        }
        for ring in sort::trigger_rings(prefab, parking) {
            let count = ring.len().max(1) as f32;
            add([ring.iter().map(|p| p[0]).sum::<f32>() / count, ring.iter().map(|p| p[1]).sum::<f32>() / count]);
        }
    }
    for item in &map.items {
        if let ItemData::RestTrigger { nodes } = &item.data {
            let corners: Vec<&Node> = nodes.iter().filter_map(|uid| map.nodes.get(uid)).collect();
            let count = corners.len() as f64;
            if count > 0.0 {
                points.push([
                    corners.iter().map(|n| f64::from(n.position[0])).sum::<f64>() / count,
                    corners.iter().map(|n| f64::from(n.position[2])).sum::<f64>() / count,
                ]);
            }
        }
    }
    points
}

/// What a node is joined to, in a forest of joined nodes.
pub(crate) fn root(parent: &mut HashMap<u64, u64>, mut at: u64) -> u64 {
    loop {
        let above = *parent.entry(at).or_insert(at);
        if above == at {
            return at;
        }
        let further = *parent.entry(above).or_insert(above);
        parent.insert(at, further);
        at = further;
    }
}

/// The roads through tunnels, by item: those with a tunnel's railings, in runs of [`TUNNEL`] or more.
fn tunnel_roads(map: &Map, definitions: &Definitions) -> HashSet<usize> {
    let roofed: Vec<(usize, u64, u64, f32)> = map
        .items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| match &item.data {
            ItemData::Road { railings, start_node, end_node, length, .. }
                if railings.iter().any(|railing| definitions.tunnel_railings.contains(railing)) =>
            {
                Some((index, *start_node, *end_node, *length))
            }
            _ => None,
        })
        .collect();
    let mut parent: HashMap<u64, u64> = HashMap::new();
    for &(_, start, end, _) in &roofed {
        let (a, b) = (root(&mut parent, start), root(&mut parent, end));
        parent.insert(a, b);
    }
    let mut runs: HashMap<u64, f32> = HashMap::new();
    for &(_, start, _, length) in &roofed {
        *runs.entry(root(&mut parent, start)).or_default() += length;
    }
    roofed.iter().filter(|(_, start, ..)| runs[&root(&mut parent, *start)] >= TUNNEL).map(|road| road.0).collect()
}

/// The prefabs in tunnels, by item: those whose lanes run in tubes, more of them in than out, and those with nothing but
/// `roads` joined to them.
fn tunnel_prefabs(
    map: &Map,
    prefabs: &HashMap<u64, Arc<Prefab>>,
    sorted: &HashMap<ModelKey, Arc<Sorted>>,
    keys: &HashMap<usize, ModelKey>,
    tubes: &Tubes,
    roads: &HashSet<usize>,
    road_ends: &HashMap<u64, Vec<(usize, bool)>>,
) -> HashSet<usize> {
    let width = RoadClass::Local.typical_width();
    let in_tubes = |index: usize, key: &ModelKey| -> Option<bool> {
        let (nodes, origin, model) = prefab_nodes(&map.items[index])?;
        let (sorted, prefab) = (sorted.get(key)?, prefabs.get(&model)?);
        let placed = Placed::new(prefab, origin, map.nodes.get(nodes.first()?)?)?;
        let (mut inside, mut steps) = (0, 0);
        for lane in sorted.lanes.iter().filter(|_| !sorted.is_place) {
            let line: Vec<[f64; 3]> = lane.iter().map(|p| placed.world(p.map(f64::from))).collect();
            let along = tubes.steps_inside(&line, width);
            inside += along.iter().filter(|&&step| step).count();
            steps += along.len();
        }
        Some(inside * 2 > steps)
    };
    let between_roads = |index: usize| {
        let Some((nodes, ..)) = prefab_nodes(&map.items[index]) else { return false };
        let joined: Vec<usize> = nodes.iter().filter_map(|node| road_ends.get(node)).flatten().map(|end| end.0).collect();
        joined.len() > 1 && joined.iter().all(|road| roads.contains(road))
    };
    keys.iter().filter(|&(&index, key)| in_tubes(index, key) == Some(true) || between_roads(index)).map(|(&index, _)| index).collect()
}

/// Whether an item is on the game's own map.
fn on_the_map(item: &Item) -> bool {
    match &item.data {
        ItemData::Road { flags, .. } | ItemData::Prefab { flags, .. } => !flags.hidden && !flags.secret,
        _ => true,
    }
}

impl Classes {
    fn new(map: &Map, definitions: &Definitions, prefabs: &HashMap<u64, Arc<Prefab>>, reaches: &HashMap<u64, sort::LaneReach>, near: &[usize]) -> Self {
        let mut node: HashMap<u64, RoadClass> = HashMap::new();
        let mut rail: HashSet<u64> = HashSet::new();
        for item in &map.items {
            if let ItemData::Road { look, start_node, end_node, .. } = &item.data
                && let Some(look) = definitions.road_looks.get(look)
            {
                for n in [start_node, end_node] {
                    if look.class == RoadClass::Rail {
                        rail.insert(*n);
                    } else {
                        let entry = node.entry(*n).or_insert(look.class);
                        *entry = (*entry).max(look.class);
                    }
                }
            }
        }
        for n in rail {
            node.entry(n).or_insert(RoadClass::Rail);
        }
        let mut on_node: HashMap<u64, Vec<usize>> = HashMap::new();
        for &index in near {
            if let Some((nodes, ..)) = prefab_nodes(&map.items[index]) {
                for n in nodes {
                    on_node.entry(*n).or_default().push(index);
                }
            }
        }
        let mut beyond: HashMap<(u64, usize), RoadClass> = HashMap::new();
        for _ in 0..CLASS_ROUNDS {
            for &index in near {
                let Some((nodes, origin, model)) = prefab_nodes(&map.items[index]) else { continue };
                let (Some(prefab), Some(reach)) = (prefabs.get(&model), reaches.get(&model)) else { continue };
                let n = prefab.nodes.len();
                let uid = |k: usize| nodes.get(geometry::prefab_world_node(k, origin, n)).copied();
                for a in 0..n {
                    let Some(at) = uid(a) else { continue };
                    let mut best: Option<RoadClass> = None;
                    for b in 0..n {
                        if a == b || !reach.linked(prefab, a, b) {
                            continue;
                        }
                        let Some(there) = uid(b) else { continue };
                        let seen = node.get(&there).copied().or_else(|| {
                            on_node
                                .get(&there)?
                                .iter()
                                .filter(|&&i| i != index)
                                .filter_map(|&i| beyond.get(&(there, i)).copied())
                                .max()
                        });
                        best = best.max(seen);
                    }
                    if let Some(best) = best {
                        beyond.insert((at, index), best);
                    }
                }
            }
        }
        // From far, a short road that leads nowhere (it ends in a yard, or at nothing) is a driveway, whatever its class. A
        // place a way runs through, from a road to another, leads on: a motorway's service plaza.
        let shown = |look: &u64| definitions.road_looks.get(look).is_some_and(|l| l.class != RoadClass::Rail);
        let mut used: HashMap<u64, usize> = HashMap::new();
        for item in &map.items {
            if let ItemData::Road { look, start_node, end_node, .. } = &item.data
                && shown(look)
                && on_the_map(item)
            {
                *used.entry(*start_node).or_default() += 1;
                *used.entry(*end_node).or_default() += 1;
            }
        }
        for n in near.iter().filter_map(|&index| prefab_nodes(&map.items[index])).flat_map(|(nodes, ..)| nodes) {
            *used.entry(*n).or_default() += 1;
        }
        let mut road_pieces: HashMap<u64, usize> = HashMap::new();
        for &index in near {
            let Some((nodes, origin, model)) = prefab_nodes(&map.items[index]) else { continue };
            let (Some(prefab), Some(reach)) = (prefabs.get(&model), reaches.get(&model)) else { continue };
            let count = prefab.nodes.len();
            let open = |own: usize| nodes.get(geometry::prefab_world_node(own, origin, count)).is_some_and(|uid| used.get(uid).is_some_and(|&on| on > 1));
            let leads_on = prefab.spawn_points.is_empty()
                || (0..count).any(|a| open(a) && (0..count).any(|b| a != b && open(b) && reach.linked(prefab, a, b)));
            if on_the_map(&map.items[index]) && leads_on {
                for n in nodes {
                    *road_pieces.entry(*n).or_default() += 1;
                }
            }
        }
        let mut stubs: HashSet<usize> = HashSet::new();
        for _ in 0..STUB_ROUNDS {
            let mut joins = road_pieces.clone();
            for (index, item) in map.items.iter().enumerate() {
                if let ItemData::Road { look, start_node, end_node, .. } = &item.data
                    && shown(look)
                    && on_the_map(item)
                    && !stubs.contains(&index)
                {
                    *joins.entry(*start_node).or_default() += 1;
                    *joins.entry(*end_node).or_default() += 1;
                }
            }
            let mut found = Vec::new();
            for (index, item) in map.items.iter().enumerate() {
                if let ItemData::Road { look, start_node, end_node, length, .. } = &item.data
                    && shown(look)
                    && on_the_map(item)
                    && !stubs.contains(&index)
                    && *length < STUB
                    && (joins.get(start_node) == Some(&1) || joins.get(end_node) == Some(&1))
                {
                    found.push(index);
                }
            }
            if found.is_empty() {
                break;
            }
            stubs.extend(found);
        }
        // Bits of the network joined to nothing bigger.
        let mut parent: HashMap<u64, u64> = HashMap::new();
        let is_shown = |index: usize, item: &Item| match &item.data {
            ItemData::Road { look, .. } => shown(look) && on_the_map(item) && !stubs.contains(&index),
            _ => false,
        };
        for (index, item) in map.items.iter().enumerate() {
            if let ItemData::Road { start_node, end_node, .. } = &item.data
                && is_shown(index, item)
            {
                let (a, b) = (root(&mut parent, *start_node), root(&mut parent, *end_node));
                parent.insert(a, b);
            }
        }
        for &index in near {
            let Some((nodes, ..)) = prefab_nodes(&map.items[index]) else { continue };
            if on_the_map(&map.items[index])
                && let Some(first) = nodes.first()
            {
                for other in &nodes[1..] {
                    let (a, b) = (root(&mut parent, *first), root(&mut parent, *other));
                    parent.insert(a, b);
                }
            }
        }
        let mut lengths: HashMap<u64, f32> = HashMap::new();
        for (index, item) in map.items.iter().enumerate() {
            if let ItemData::Road { start_node, length, .. } = &item.data
                && is_shown(index, item)
            {
                let group = root(&mut parent, *start_node);
                *lengths.entry(group).or_default() += *length;
            }
        }
        let fragments: Vec<usize> = map
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| match &item.data {
                ItemData::Road { start_node, .. } if is_shown(index, item) => {
                    let group = root(&mut parent, *start_node);
                    (lengths.get(&group).copied().unwrap_or(0.0) < FRAGMENT).then_some(index)
                }
                _ => None,
            })
            .collect();
        stubs.extend(fragments);
        let mut far: HashMap<u64, RoadClass> = HashMap::new();
        for (index, item) in map.items.iter().enumerate() {
            if let ItemData::Road { look, start_node, end_node, .. } = &item.data
                && !stubs.contains(&index)
                && on_the_map(item)
                && let Some(look) = definitions.road_looks.get(look)
                && look.class != RoadClass::Rail
            {
                for n in [start_node, end_node] {
                    let entry = far.entry(*n).or_insert(look.class);
                    *entry = (*entry).max(look.class);
                }
            }
        }
        Self { node, beyond, stubs, far }
    }

    /// The class on a prefab's own nodes, and which have a road shown on them from far.
    fn at_prefab(&self, index: usize, nodes: &[u64], origin: usize, count: usize, on_node: &HashMap<u64, Vec<usize>>) -> (Vec<Option<RoadClass>>, Vec<bool>) {
        let others = |uid: u64| {
            on_node.get(&uid).into_iter().flatten().filter(move |&&i| i != index).filter_map(move |&i| self.beyond.get(&(uid, i)).copied())
        };
        let mut classes = Vec::with_capacity(count);
        let mut far = Vec::with_capacity(count);
        for own in 0..count {
            let Some(&uid) = nodes.get(geometry::prefab_world_node(own, origin, count)) else {
                classes.push(None);
                far.push(false);
                continue;
            };
            classes.push(self.node.get(&uid).copied().or_else(|| others(uid).max()));
            far.push(self.far.contains_key(&uid) || others(uid).next().is_some());
        }
        (classes, far)
    }
}

/// A lane's class: the highest of the roads it leads between (a lane keeps the higher class until the junction piece
/// ends).
fn lane_classes(prefab: &Prefab, reach: &sort::LaneReach, at_nodes: &[Option<RoadClass>]) -> Vec<Option<RoadClass>> {
    let n = prefab.nodes.len();
    (0..prefab.curves.len())
        .map(|lane| {
            let mut best: Option<RoadClass> = None;
            for entry in (0..n).filter(|&entry| reach.from[lane][entry].is_some()) {
                for exit in (0..n).filter(|&exit| reach.to[lane][exit].is_some()) {
                    let (start, end) = (at_nodes[entry].or(at_nodes[exit]), at_nodes[exit].or(at_nodes[entry]));
                    best = best.max(start.max(end));
                }
            }
            best
        })
        .collect()
}

/// What's told of each of a prefab's edges: its terrain, relief, points, and the prefab and where it's placed.
type EachEdge<'a> = dyn FnMut(&crate::sector::TerrainSide, &crate::sector::Relief, &[[f32; 3]], &Prefab, &Placed) + 'a;

/// What sets a prefab's outlines: its sorted faces, its nodes' classes and which show from far, and in steps of
/// [`REACH_STEP`] how far a place's ways through reach past their lanes.
type DrawKey = (ModelKey, Vec<Option<RoadClass>>, Vec<bool>, u16);

/// A place's ways through reach past their lanes by this much at a time, m.
const REACH_STEP: f64 = 0.05;

/// A grid of things by the cells they reach, packed: every cell's things in one list, each cell its part of it (a map of
/// small lists, one a cell, weighed more than the things' numbers themselves). A cell's things are in the order they
/// were added, as a list a cell kept them.
pub(super) struct Cells {
    parts: HashMap<(i32, i32), (u32, u32)>,
    things: Vec<u32>,
}

impl Cells {
    /// From each thing's cells, in the order added.
    pub(super) fn new(mut added: Vec<((i32, i32), u32)>) -> Self {
        // Stable: a cell's things stay in the order they were added.
        added.sort_by_key(|(cell, _)| *cell);
        let mut parts = HashMap::new();
        let mut things = Vec::with_capacity(added.len());
        let mut at = 0;
        while at < added.len() {
            let cell = added[at].0;
            let start = things.len() as u32;
            while at < added.len() && added[at].0 == cell {
                things.push(added[at].1);
                at += 1;
            }
            parts.insert(cell, (start, things.len() as u32 - start));
        }
        parts.shrink_to_fit();
        Self { parts, things }
    }

    /// The things in `cell`, in the order added.
    pub(super) fn at(&self, cell: (i32, i32)) -> &[u32] {
        self.parts.get(&cell).map_or(&[][..], |&(start, count)| &self.things[start as usize..(start + count) as usize])
    }
}

/// How much of the map is worked on at once, by its blocks' items: a block starts only while the blocks being worked on
/// and it hold no more items than the largest block (or none's being worked on), so the dense ones (a city's, half a
/// gigabyte each as they're drawn) never all run at the same time, each still on a thread as soon as there's room.
/// Every block is drawn as it would be alone: only when it's drawn changes (owner 2026-11-05: less memory, the same map).
pub(super) struct Budget {
    limit: usize,
    taken: Mutex<usize>,
    freed: std::sync::Condvar,
}

impl Budget {
    pub(super) fn of(blocks: &[((u32, u32), Vec<u32>)]) -> Self {
        let limit = blocks.iter().map(|(_, items)| items.len()).max().unwrap_or(0).max(1);
        Self { limit, taken: Mutex::new(0), freed: std::sync::Condvar::new() }
    }

    /// Room for a block of `items`, once there is (or `stop` is set): given back as the guard goes.
    pub(super) fn take<'a>(&'a self, items: usize, stop: &AtomicBool) -> Taken<'a> {
        let mut taken = self.taken.lock().unwrap_or_else(|e| e.into_inner());
        while *taken > 0 && *taken + items > self.limit && !stop.load(Ordering::Relaxed) {
            taken = self.freed.wait_timeout(taken, std::time::Duration::from_millis(100)).unwrap_or_else(|e| e.into_inner()).0;
        }
        *taken += items;
        Taken { budget: self, items }
    }
}

/// A block's room in the [`Budget`], given back when it's done.
pub(super) struct Taken<'a> {
    budget: &'a Budget,
    items: usize,
}

impl Drop for Taken<'_> {
    fn drop(&mut self) {
        *self.budget.taken.lock().unwrap_or_else(|e| e.into_inner()) -= self.items;
        self.budget.freed.notify_all();
    }
}

/// Everything the blocks read.
/// An item that can pass over or under something: its index, its box, and how low and high it goes.
type Extent = (usize, [f64; 4], f64, f64);

struct Scene<'a> {
    map: &'a Map,
    tables: &'a Tables<'a>,
    prefabs: &'a HashMap<u64, Arc<Prefab>>,
    sorted: &'a HashMap<ModelKey, Arc<Sorted>>,
    keys: &'a HashMap<usize, ModelKey>,
    classes: &'a Classes,
    network: &'a Network,
    on_node: &'a HashMap<u64, Vec<usize>>,
    tubes: &'a Tubes,
    tunnels: &'a HashSet<usize>,
    /// The roads ending at each node, and whether they arrive at it.
    road_ends: &'a HashMap<u64, Vec<(usize, bool)>>,
    drawings: Mutex<HashMap<DrawKey, Arc<Drawing>>>,
    /// What's kept of models with water: their water seen from above.
    pools: &'a HashMap<u64, Vec<Own3>>,
}

impl Scene<'_> {
    fn drawing(&self, key: DrawKey, sorted: &Sorted, prefab: &Prefab) -> Arc<Drawing> {
        if let Some(known) = self.drawings.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
            return known.clone();
        }
        let classes = lane_classes(prefab, &sorted.reach, &key.1);
        let drawn = Arc::new(sorted.draw(&classes, &key.1, &key.2, f64::from(key.3) * REACH_STEP, prefab));
        self.drawings.lock().unwrap_or_else(|e| e.into_inner()).insert(key, drawn.clone());
        drawn
    }

    /// How far a place's ways through reach past their lanes, in [`REACH_STEP`]s: as far as the roads meeting it reach past
    /// theirs, so that they go on at the same width.
    fn meeting_reach(&self, nodes: &[u64], origin: usize, prefab: &Prefab, sorted: &Sorted) -> u16 {
        let mut reach = LANE_HALF;
        for own in 0..prefab.nodes.len() {
            let Some(uid) = nodes.get(geometry::prefab_world_node(own, origin, prefab.nodes.len())) else { continue };
            for &(road, arrives) in self.road_ends.get(uid).map_or(&[][..], Vec::as_slice) {
                let Some((section, _)) = self.network.roads.get(&road) else { continue };
                let [right, left] = if arrives { section.outer } else { [section.outer[1], section.outer[0]] };
                for (sign, edge) in [(1.0, right), (-1.0, left)] {
                    if let Some(past) = sorted.past_lanes(prefab, own, sign, edge) {
                        reach = reach.max(past);
                    }
                }
            }
        }
        (reach.min(PLACE_REACH) / REACH_STEP).round() as u16
    }

    /// A prefab's or a road's box and how low and how high it goes, for what may pass over what.
    fn extent(&self, index: usize) -> Option<([f64; 4], f64, f64)> {
        let map = self.map;
        let item = &map.items[index];
        let bounds = item.bounds.map(f64::from);
        match &item.data {
            ItemData::Road { start_node, end_node, length, .. } => {
                self.network.roads.get(&index)?;
                let (a, b) = (map.nodes.get(start_node)?, map.nodes.get(end_node)?);
                let heights = (0..=4).map(|i| frame(a, b, f64::from(*length), f64::from(i) / 4.0, true).0[1]);
                let (low, high) = heights.fold((f64::MAX, f64::MIN), |(low, high), y| (low.min(y), high.max(y)));
                Some((bounds, low, high))
            }
            ItemData::Prefab { nodes, origin, .. } => {
                let sorted = self.sorted.get(self.keys.get(&index)?)?;
                let prefab = self.prefabs.get(&self.keys.get(&index)?.model)?;
                let placed = Placed::new(prefab, *origin as usize, map.nodes.get(nodes.first()?)?)?;
                let rise = placed.position[1] - placed.own_at[1];
                // Its tilt moves it by less than this much across a junction.
                const TILT: f64 = 2.0;
                Some((bounds, f64::from(sorted.heights.0) + rise - TILT, f64::from(sorted.heights.1) + rise + TILT))
            }
            _ => None,
        }
    }

    /// Of `items`, those that can pass over or under something, as [`Extent`]s.
    fn extents(&self, items: &[u32]) -> Vec<Extent> {
        // What's in a tunnel is under the map: it passes over nothing, and what passes over it is no bridge.
        let open = items.iter().filter(|&&index| !self.in_tunnel(index as usize));
        open.filter_map(|&index| self.extent(index as usize).map(|(b, low, high)| (index as usize, b, low, high))).collect()
    }

    /// Which of `extents` may lie over or under another.
    fn stacked(extents: &[Extent]) -> Vec<bool> {
        let mut cells: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for (at, (_, b, ..)) in extents.iter().enumerate() {
            for x in (b[0] / STACK_CELL).floor() as i32..=(b[2] / STACK_CELL).floor() as i32 {
                for z in (b[1] / STACK_CELL).floor() as i32..=(b[3] / STACK_CELL).floor() as i32 {
                    cells.entry((x, z)).or_default().push(at as u32);
                }
            }
        }
        let mut involved = vec![false; extents.len()];
        for list in cells.values() {
            for (i, &a) in list.iter().enumerate() {
                let (_, ba, la, ha) = extents[a as usize];
                for &b in &list[i + 1..] {
                    let (_, bb, lb, hb) = extents[b as usize];
                    if ba[0] <= bb[2] && ba[2] >= bb[0] && ba[1] <= bb[3] && ba[3] >= bb[1] && (ha - lb >= levels::CLEAR || hb - la >= levels::CLEAR) {
                        involved[a as usize] = true;
                        involved[b as usize] = true;
                    }
                }
            }
        }
        involved
    }

    /// The faces of what may pass over what among `items`, with each one's level, into `block`. `raised`: the items that
    /// rise.
    fn add_levels(&self, items: &[u32], raised: &HashSet<u32>, block: &mut Block) {
        let extents = self.extents(items);
        let involved = Self::stacked(&extents);
        // What's joined to an item that may be raised takes part too: the surface going on from it isn't an edge.
        let mut joined: HashMap<u64, Vec<u32>> = HashMap::new();
        for (at, (index, ..)) in extents.iter().enumerate() {
            for node in self.ends(*index) {
                joined.entry(node).or_default().push(at as u32);
            }
        }
        let mut with_neighbours = involved.clone();
        for (at, (index, ..)) in extents.iter().enumerate() {
            if involved[at] {
                for node in self.ends(*index) {
                    for &other in joined.get(&node).map_or(&[][..], Vec::as_slice) {
                        with_neighbours[other as usize] = true;
                    }
                }
            }
        }
        let mut faces: Vec<Face> = Vec::new();
        let mut rising = HashSet::new();
        for (at, (index, ..)) in extents.iter().enumerate() {
            if with_neighbours[at] {
                let from = faces.len();
                self.add_faces(*index, roads::FACE_STEP, &mut faces);
                if raised.contains(&(*index as u32)) {
                    rising.extend(from as u32..faces.len() as u32);
                }
            }
        }
        if faces.is_empty() {
            return;
        }
        let levels = levels::levels(&faces);
        if levels.iter().all(|&level| level == 0) {
            return;
        }
        block.levels = levels;
        block.faces = faces;
        block.raised_faces = rising;
    }

    /// The nodes a road or a prefab joins others at.
    fn ends(&self, index: usize) -> Vec<u64> {
        match &self.map.items[index].data {
            ItemData::Road { start_node, end_node, .. } => vec![*start_node, *end_node],
            ItemData::Prefab { nodes, .. } => nodes.clone(),
            _ => Vec::new(),
        }
    }

    /// Whether a road runs in a tunnel, along any of it.
    fn in_tunnel(&self, index: usize) -> bool {
        if self.tunnels.contains(&index) {
            return true;
        }
        let ItemData::Road { start_node, end_node, length, .. } = &self.map.items[index].data else { return false };
        let (Some(a), Some(b), Some((section, _))) =
            (self.map.nodes.get(start_node), self.map.nodes.get(end_node), self.network.roads.get(&index))
        else {
            return false;
        };
        let length = f64::from(*length);
        let steps = ((length / LINE_STEP).ceil() as usize).clamp(2, 200);
        let stations: Vec<([f64; 3], [f64; 2])> = (0..=steps).map(|i| frame(a, b, length, i as f64 / steps as f64, true)).collect();
        carriageways(section).into_iter().any(|(across, width)| {
            let line: Vec<[f64; 3]> = stations.iter().map(|(p, n)| [p[0] + n[0] * across, p[1], p[2] + n[1] * across]).collect();
            self.tubes.steps_inside(&line, width as f32).contains(&true)
        })
    }

    /// Along a road, about `step` apart: where it is, and either side of it, `past` beyond where its ground starts.
    fn stations(&self, index: usize, step: f64, past: f64) -> Vec<([f64; 3], [[f64; 2]; 2])> {
        let ItemData::Road { start_node, end_node, length, .. } = &self.map.items[index].data else { return Vec::new() };
        let (Some(a), Some(b), Some((section, _))) = (self.map.nodes.get(start_node), self.map.nodes.get(end_node), self.network.roads.get(&index)) else {
            return Vec::new();
        };
        let length = f64::from(*length);
        let steps = ((length / step).ceil() as usize).max(1);
        let [right, left] = section.ground_from.map(|from| from + past);
        (0..steps)
            .map(|i| {
                let (p, n) = frame(a, b, length, (i as f64 + 0.5) / steps as f64, true);
                (p, [[p[0] + n[0] * right, p[2] + n[1] * right], [p[0] - n[0] * left, p[2] - n[1] * left]])
            })
            .collect()
    }

    /// The ground a prefab has of its own, as faces.
    fn add_prefab_ground(&self, index: usize, faces: &mut Vec<Face>) {
        let ItemData::Prefab { nodes, origin, .. } = &self.map.items[index].data else { return };
        let Some(key) = self.keys.get(&index) else { return };
        let (Some(sorted), Some(prefab), Some(at)) = (self.sorted.get(key), self.prefabs.get(&key.model), nodes.first().and_then(|uid| self.map.nodes.get(uid))) else { return };
        let Some(placed) = Placed::new(prefab, *origin as usize, at) else { return };
        faces.extend(sorted.ground().map(|t| Face { t: t.map(|v| placed.world(v.map(f64::from))), kind: Kind::Quiet, near: false }));
    }

    /// An item's faces for what passes over what, a road's about `step` long.
    fn add_faces(&self, index: usize, step: f64, faces: &mut Vec<Face>) {
        let map = self.map;
        let tables = self.tables;
        let item = &map.items[index];
        match &item.data {
            ItemData::Road { start_node, end_node, length, look, .. } => {
                let (Some(a), Some(b), Some((section, _))) = (map.nodes.get(start_node), map.nodes.get(end_node), self.network.roads.get(&index)) else { return };
                let Some(table) = tables.looks.get(*look) else { return };
                let class = tables.definitions.road_looks.get(look).map_or(RoadClass::Local, |look| look.class);
                let kind = roads::road_kind(table, class);
                let near = self.classes.stubs.contains(&index) || !on_the_map(item);
                roads::faces(a, b, f64::from(*length), section, kind, step, &mut |kind, t| {
                    faces.push(Face { t, kind, near: near && kind != Kind::Quiet });
                });
            }
            ItemData::Prefab { nodes, origin, .. } => {
                let Some(key) = self.keys.get(&index) else { return };
                let (Some(sorted), Some(prefab), Some(at)) = (self.sorted.get(key), self.prefabs.get(&key.model), nodes.first().and_then(|uid| map.nodes.get(uid))) else { return };
                let origin = *origin as usize;
                let Some(placed) = Placed::new(prefab, origin, at) else { return };
                let (classes, _) = self.classes.at_prefab(index, nodes, origin, prefab.nodes.len(), self.on_node);
                let lanes = lane_classes(prefab, &sorted.reach, &classes);
                let road = !sorted.is_place && classes.iter().any(Option::is_some);
                // A face's class is its nearest lane's.
                let class_of = |t: &sort::Triangle| -> RoadClass {
                    let m: [f32; 3] = std::array::from_fn(|k| (t[0][k] + t[1][k] + t[2][k]) / 3.0);
                    let mut best = (f32::INFINITY, None);
                    for (lane, points) in sorted.lanes.iter().enumerate() {
                        let Some(class) = lanes[lane] else { continue };
                        for p in points {
                            let d = (p[0] - m[0]).powi(2) + (p[2] - m[2]).powi(2) + (3.0 * (p[1] - m[1])).powi(2);
                            if d < best.0 {
                                best = (d, Some(class));
                            }
                        }
                    }
                    best.1.unwrap_or(RoadClass::Local)
                };
                for (t, kind) in sorted.shown() {
                    let kind = match kind {
                        FaceKind::Road if road => Kind::of(class_of(t)),
                        FaceKind::Road | FaceKind::Quiet => Kind::Quiet,
                        _ => continue,
                    };
                    let near = !on_the_map(item) && kind != Kind::Quiet;
                    faces.push(Face { t: t.map(|v| placed.world(v.map(f64::from))), kind, near });
                }
            }
            _ => {}
        }
    }

    /// The points of the lanes through `items`' prefabs that a truck can drive, between two nodes with something on them
    /// (a junction's arm with no road on it is closed in the game), for an audit: each with its item.
    fn probes(&self, items: &[u32]) -> Vec<(u32, [f64; 2])> {
        let mut out = Vec::new();
        for &index in items {
            let item = &self.map.items[index as usize];
            let ItemData::Prefab { nodes, origin, .. } = &item.data else { continue };
            let Some(key) = self.keys.get(&(index as usize)) else { continue };
            let (Some(sorted), Some(prefab), Some(at)) =
                (self.sorted.get(key), self.prefabs.get(&key.model), nodes.first().and_then(|uid| self.map.nodes.get(uid)))
            else {
                continue;
            };
            let Some(placed) = Placed::new(prefab, *origin as usize, at) else { continue };
            let count = prefab.nodes.len();
            let (classes, _) = self.classes.at_prefab(index as usize, nodes, *origin as usize, count, self.on_node);
            let open = |i: usize| classes[i].is_some();
            for (lane, points) in sorted.lanes.iter().enumerate() {
                let driven = (0..count).any(|i| {
                    open(i)
                        && sorted.reach.from[lane][i].is_some()
                        && (0..count).any(|j| i != j && open(j) && sorted.reach.to[lane][j].is_some())
                });
                if driven {
                    for p in points {
                        let w = placed.world(p.map(f64::from));
                        out.push((index, [w[0], w[2]]));
                    }
                }
            }
        }
        out
    }

    /// An item's painted ground of the paints `wanted`, into `out`.
    fn painted(&self, index: usize, wanted: ground::Wanted, out: &mut Vec<Quad>) {
        let map = self.map;
        let tables = self.tables;
        let (definitions, paints) = (tables.definitions, &tables.paints);
        let item = &map.items[index];
        match &item.data {
            ItemData::Road { start_node, end_node, length, relief, .. } => {
                let (Some(a), Some(b)) = (map.nodes.get(start_node), map.nodes.get(end_node)) else { return };
                let (Some((section, _)), Some(terrain)) = (self.network.roads.get(&index), map.road_terrain.get(&item.uid)) else { return };
                let length = f64::from(*length);
                for (side, relief, inner, sign) in [
                    (&terrain.right, &relief[0], section.ground_from[0], 1.0),
                    (&terrain.left, &relief[1], section.ground_from[1], -1.0),
                ] {
                    ground::side_quads(definitions, paints, a, b, length, side, relief, inner, sign, wanted, out);
                }
            }
            ItemData::Prefab { .. } => {
                self.edges(index, &mut |side, relief, points, prefab, placed| {
                    ground::edge_quads(definitions, paints, side, relief, points, &prefab.nodes, &|v| placed.world(v), wanted, out);
                });
            }
            ItemData::Terrain { start_node, end_node, right, left, relief, research } => {
                let (Some(a), Some(b)) = (map.nodes.get(start_node), map.nodes.get(end_node)) else { return };
                let length = f64::from(research.length);
                ground::side_quads(definitions, paints, a, b, length, right, &relief[0], 0.0, 1.0, wanted, out);
                ground::side_quads(definitions, paints, a, b, length, left, &relief[1], 0.0, -1.0, wanted, out);
            }
            ItemData::BezierPatch { node, control_points, grid } => {
                if let Some(node) = map.nodes.get(node) {
                    ground::patch_quads(paints, node, control_points, grid, wanted, out);
                }
            }
            _ => {}
        }
    }

    /// Tells everything stacked at a point of `items`, to look into a place ([`Audit::looking_at`]).
    fn stack_at(&self, items: &[u32], [x, z]: [f64; 2]) {
        let inside = |ring: &[[f64; 3]]| {
            let mut odd = false;
            for i in 0..ring.len() {
                let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
                if (a[2] > z) != (b[2] > z) && x < a[0] + (z - a[2]) / (b[2] - a[2]) * (b[0] - a[0]) {
                    odd = !odd;
                }
            }
            odd
        };
        let high = |ring: &[[f64; 3]]| ring.iter().map(|p| p[1]).fold((f64::MAX, f64::MIN), |(low, high), y| (low.min(y), high.max(y)));
        for &index in items {
            let index = index as usize;
            let item = &self.map.items[index];
            let mut quads = Vec::new();
            self.painted(index, |_| true, &mut quads);
            for quad in quads.iter().filter(|quad| inside(&quad.corners)) {
                let (low, top) = high(&quad.corners);
                eprintln!("AT paint {:?} {low:.2}..{top:.2} of item {:016x}", quad.paint, item.uid);
            }
            if let ItemData::Prefab { nodes, origin, .. } = &item.data
                && let Some(key) = self.keys.get(&index)
                && let (Some(sorted), Some(prefab), Some(at)) =
                    (self.sorted.get(key), self.prefabs.get(&key.model), nodes.first().and_then(|uid| self.map.nodes.get(uid)))
                && let Some(placed) = Placed::new(prefab, *origin as usize, at)
            {
                let path = self.tables.definitions.prefab_models.get(&key.model).map_or("", String::as_str);
                let ground = sorted.ground().map(|t| (t, FaceKind::Ground));
                for (t, kind) in ground.chain(sorted.shown().map(|(t, kind)| (t, *kind))) {
                    let world = t.map(|v| placed.world(v.map(f64::from)));
                    if inside(&world) {
                        let (low, top) = high(&world);
                        eprintln!("AT face {kind:?} {low:.2}..{top:.2} of {path}");
                    }
                }
            }
        }
    }

    /// What's laid over the paving in `block`: other paints, and the lawns of prefabs.
    fn add_cover(&self, items: &[u32], block: &mut Block) {
        let paving = block.paving();
        if paving.is_empty() {
            return;
        }
        let mut found = Vec::new();
        for &index in items {
            let index = index as usize;
            let item = &self.map.items[index];
            if !paving.reaches(item.bounds.map(f64::from)) {
                continue;
            }
            self.painted(index, ground::laid_over, &mut found);
            if let ItemData::Prefab { nodes, origin, .. } = &item.data
                && let Some(key) = self.keys.get(&index)
                && let (Some(sorted), Some(prefab), Some(at)) =
                    (self.sorted.get(key), self.prefabs.get(&key.model), nodes.first().and_then(|uid| self.map.nodes.get(uid)))
                && let Some(placed) = Placed::new(prefab, *origin as usize, at)
            {
                for t in sorted.ground() {
                    let [a, b, c] = t.map(|v| placed.world(v.map(f64::from)));
                    found.push(Quad { corners: [a, b, c, c], paint: Painted::Grass });
                }
            }
            block.over.extend(found.drain(..).filter(|quad| paving.reaches(ring_box(&quad.corners))));
        }
    }

    /// The shapes and the painted ground of one item, into `block`.
    fn add_item(&self, index: usize, block: &mut Block) {
        let map = self.map;
        let tables = self.tables;
        let item = &map.items[index];
        self.painted(index, ground::drawn, &mut block.quads);
        match &item.data {
            ItemData::Road { start_node, end_node, length, look, .. } => {
                let (Some(a), Some(b)) = (map.nodes.get(start_node), map.nodes.get(end_node)) else { return };
                let length = f64::from(*length);
                if let Some((section, place)) = self.network.roads.get(&index) {
                    let class = tables.definitions.road_looks.get(look).map_or(RoadClass::Local, |look| look.class);
                    let Some(table) = tables.looks.get(*look) else { return };
                    let kind = roads::road_kind(table, class);
                    let shape = roads::shape(a, b, length, section, kind, self.tubes, self.tunnels.contains(&index), &|s| {
                        self.network.moved(place, length, s)
                    });
                    let stub = self.classes.stubs.contains(&index) || !on_the_map(item);
                    for (kind, ring) in shape.rings {
                        let near = stub && kind != Kind::Quiet;
                        block.add(Shape { kind, near, raw: true, ring });
                    }
                }
            }
            ItemData::Prefab { nodes, origin, .. } => {
                let (Some(key), Some(at)) = (self.keys.get(&index), nodes.first().and_then(|uid| map.nodes.get(uid))) else { return };
                let (Some(sorted), Some(prefab)) = (self.sorted.get(key), self.prefabs.get(&key.model)) else { return };
                let origin = *origin as usize;
                let Some(placed) = Placed::new(prefab, origin, at) else { return };
                let count = prefab.nodes.len();
                let (classes, far) = self.classes.at_prefab(index, nodes, origin, count, self.on_node);
                let lanes = lane_classes(prefab, &sorted.reach, &classes);
                let joined = classes.iter().any(Option::is_some);
                let shown_at = |node: usize| far.get(node).copied().unwrap_or(false) || classes[node] == Some(RoadClass::Rail);
                let linked: Vec<bool> = (0..sorted.lanes.len()).map(|lane| sorted.is_through(lane, &shown_at)).collect();
                let joins_far = far.iter().any(|&shown| shown);
                let reach = if sorted.is_place { self.meeting_reach(nodes, origin, prefab, sorted) } else { 0 };
                let drawing = self.drawing((key.clone(), classes, far, reach), sorted, prefab);
                // Off the game's map, or joining nothing shown from far: it only shows close up.
                let off_the_map = !on_the_map(item) || !joins_far;
                let tunnel = self.tunnels.contains(&index);
                for (kind, near, rings) in &drawing.rings {
                    // Of what's underground only its road shows.
                    let kind = match *kind {
                        Kind::Road(class) if tunnel => Kind::Tunnel(class),
                        _ if tunnel => continue,
                        other => other,
                    };
                    for ring in rings {
                        let ring: Vec<[f64; 3]> = ring.iter().map(|v| placed.world(v.map(f64::from))).collect();
                        let near = *near || (off_the_map && matches!(kind, Kind::Road(_) | Kind::Tunnel(_) | Kind::Rail));
                        block.add(Shape { kind, near, raw: false, ring });
                    }
                }
                // Lanes with no surface of the prefab's own under them run on what's painted there.
                for (lane, points) in sorted.lanes.iter().enumerate() {
                    // Tracks always run as tracks, whatever's modelled under them: their rails and sleepers aren't a
                    // surface, and their lanes run clear of the bed.
                    if lanes[lane] == Some(RoadClass::Rail) {
                        let line = points.iter().map(|p| placed.world(p.map(f64::from))).collect();
                        block.claims.push(Claim { line, class: RoadClass::Rail, near: off_the_map, band: true, tunnel: false });
                        continue;
                    }
                    if !joined {
                        continue;
                    }
                    // What runs past a filling station's pumps is no road, whatever's under it.
                    if sorted.station_road.is_some() && !linked[lane] {
                        continue;
                    }
                    // Between two shown roads a lane needs a road under it, not just a surface.
                    let under = if linked[lane] && !sorted.is_place { &sorted.on_road[lane] } else { &sorted.surfaced[lane] };
                    if under.iter().all(|&on| on) {
                        continue;
                    }
                    let nodes_reached = |list: &[Option<f32>]| list.iter().filter(|d| d.is_some()).count();
                    let through = (0..count).any(|i| {
                        sorted.reach.from[lane][i].is_some()
                            && (0..count).any(|j| i != j && sorted.reach.to[lane][j].is_some())
                    });
                    if !through && nodes_reached(&sorted.reach.from[lane]) + nodes_reached(&sorted.reach.to[lane]) > 0 {
                        continue;
                    }
                    let class = lanes[lane].unwrap_or(RoadClass::Local);
                    let (near, band) = (off_the_map, linked[lane]);
                    let mut stretch: Vec<[f64; 3]> = Vec::new();
                    for (i, (p, on)) in points.iter().zip(under).enumerate() {
                        if !*on {
                            // From the last point on its surface, so a band joins what it runs on from.
                            if stretch.is_empty() && i > 0 {
                                stretch.push(placed.world(points[i - 1].map(f64::from)));
                            }
                            stretch.push(placed.world(p.map(f64::from)));
                        } else if !stretch.is_empty() {
                            stretch.push(placed.world(p.map(f64::from)));
                            block.claims.push(Claim { line: std::mem::take(&mut stretch), class, near, band, tunnel });
                        }
                    }
                    if stretch.len() >= 2 {
                        block.claims.push(Claim { line: stretch, class, near, band, tunnel });
                    }
                }
            }
            ItemData::Model { model, node, scale } => {
                let (Some(rings), Some(at)) = (self.pools.get(model), map.nodes.get(node)) else { return };
                let (q, p, s) = (at.rotation.map(f64::from), at.position.map(f64::from), scale.map(f64::from));
                let mirrored = s[0] * s[1] * s[2] < 0.0;
                for ring in rings {
                    let mut ring: Vec<[f64; 3]> = ring
                        .iter()
                        .map(|v| {
                            let turned = rotate(q, [f64::from(v[0]) * s[0], f64::from(v[1]) * s[1], f64::from(v[2]) * s[2]]);
                            [p[0] + turned[0], p[1] + turned[1], p[2] + turned[2]]
                        })
                        .collect();
                    if mirrored {
                        ring.reverse();
                    }
                    block.add(Shape { kind: Kind::Water, near: false, raw: false, ring });
                }
            }
            _ => {}
        }
    }

    /// A prefab's terrain at each of its edges: `each` is given its corner's terrain, the relief and the edge's points.
    fn edges(&self, index: usize, each: &mut EachEdge) {
        let map = self.map;
        let item = &map.items[index];
        let ItemData::Prefab { nodes, origin, relief, .. } = &item.data else { return };
        let (Some(key), Some(at)) = (self.keys.get(&index), nodes.first().and_then(|uid| map.nodes.get(uid))) else { return };
        let (Some(sorted), Some(prefab), Some(data)) = (self.sorted.get(key), self.prefabs.get(&key.model), map.prefab_data.get(&item.uid)) else {
            return;
        };
        let origin = *origin as usize;
        let count = prefab.nodes.len();
        let Some(placed) = Placed::new(prefab, origin, at) else { return };
        for (own, corner) in data.corners.iter().enumerate().take(count) {
            let world = geometry::prefab_world_node(own, origin, count);
            let Some(relief) = relief.get(world) else { continue };
            let list = &prefab.nodes[own].terrain_points;
            let mut points: Vec<[f32; 3]> =
                list.get(sorted.variant.unwrap_or(0)).or(list.first()).map_or(Vec::new(), |list| list.iter().map(|p| p.0).collect());
            // A corner model brings its own edge, when the terrain's columns are its.
            if let Some(fit) = sorted.fitted[own]
                && points.len() != corner.terrain.grid.cols as usize + 1
                && let Some(ppd) = self.tables.corners.descriptor(self.tables.files, corner.model[0])
                && let Some(own_points) = ppd.nodes.first().and_then(|node| node.terrain_points.first())
            {
                points = own_points.iter().map(|p| fit.place(p.0)).collect();
            }
            each(&corner.terrain, relief, &points, prefab, &placed);
        }
    }
}

/// What's drawn under the lanes of a map's prefabs, every point of every placement (an audit of the tiles' own outlines).
#[derive(Debug, Default)]
pub struct Audit {
    /// Lane points by whether the prefab's a place, and what's drawn under them (`None`: nothing).
    pub counts: BTreeMap<(bool, Option<Kind>), u64>,
    /// By prefab model, the points off road and one such place.
    pub off_road: BTreeMap<String, (u64, [f64; 2])>,
    /// By prefab item, its lane points off road (with nothing there, on quiet ground), its model and a place.
    pub placements: BTreeMap<usize, (u64, u64, String, [f64; 2])>,
    /// Every lane point off road: what's there, and where.
    pub points: Vec<(Option<Kind>, [f64; 2])>,
    /// A point of the map (x, z) to tell everything stacked at, on standard error.
    pub looking_at: Option<[f64; 2]>,
}

/// The surfaces written.
#[derive(Debug, Default)]
pub struct Summary {
    pub tiles: usize,
    pub bytes: u64,
    pub service_areas: usize,
    /// The tiles with service ground in them.
    pub service_tiles: usize,
    /// The blocks with roads drawn at their real heights, and their bytes.
    pub raised_blocks: usize,
    pub raised_bytes: u64,
    pub prefab_models: usize,
    pub prefab_drawings: usize,
    pub chains: usize,
    pub ease: roads::EaseReport,
}

/// What the rest of the map takes from the surfaces.
#[derive(Default)]
pub struct Surfaces {
    pub summary: Summary,
    /// The road lines for the map seen from far (the `road` layer of the map's other tiles).
    pub road_lines: Vec<Feature>,
    /// Each prefab's ground: its faces' hull, with their heights.
    pub footprints: Vec<Vec<[f32; 3]>>,
    /// Where each shown road's terrain starts, right and left of its centre, by uid.
    pub ground_from: HashMap<u64, [f32; 2]>,
    /// The water painted at prefabs' edges, a list of quads (x, height, z) for each edge.
    pub edge_water: Vec<Vec<[[f32; 3]; 4]>>,
    /// The service areas' ground.
    pub service: Vec<service::Area>,
    /// The roads in the air, as drawn.
    pub decks: raised::Decks,
}

/// A road's carriageways: how far across from its middle each one's is, and how wide it is.
fn carriageways(section: &roads::Section) -> Vec<(f64, f64)> {
    let [right, left] = section.outer;
    match section.inner {
        Some([inner_right, inner_left]) => {
            vec![((inner_right + right) / 2.0, right - inner_right), (-(inner_left + left) / 2.0, left - inner_left)]
        }
        None => vec![((right - left) / 2.0, right + left)],
    }
}

/// The road lines from far: each road's carriageways, and through each prefab the way its lanes go between the nodes
/// with a road on them.
fn road_lines(scene: &Scene) -> Vec<RoadLine> {
    let map = scene.map;
    let definitions = scene.tables.definitions;
    let mut lines = Vec::new();
    for (&index, (section, _)) in &scene.network.roads {
        let ItemData::Road { look, start_node, end_node, length, .. } = &map.items[index].data else { continue };
        let (Some(a), Some(b), Some(road)) = (map.nodes.get(start_node), map.nodes.get(end_node), definitions.road_looks.get(look)) else {
            continue;
        };
        // Tracks show only as surfaces: the game leaves most of them off its map, so as lines they'd be bits.
        if road.class == RoadClass::Rail {
            continue;
        }
        let length = f64::from(*length);
        let steps = ((length / LINE_STEP).ceil() as usize).clamp(2, 200);
        let stations: Vec<([f64; 3], [f64; 2])> = (0..=steps).map(|i| frame(a, b, length, i as f64 / steps as f64, true)).collect();
        let tunnel_all = scene.tunnels.contains(&index);
        for (across, width) in carriageways(section) {
            let line: Vec<[f64; 3]> = stations.iter().map(|(p, n)| [p[0] + n[0] * across, p[1], p[2] + n[1] * across]).collect();
            let inside = if tunnel_all { vec![true; steps] } else { scene.tubes.steps_inside(&line, width as f32) };
            let mut start = 0;
            for step in 1..=inside.len() {
                if step == inside.len() || inside[step] != inside[start] {
                    let points = line[start..=step].iter().map(|p| [p[0] as f32, p[2] as f32]).collect();
                    lines.push(RoadLine {
                        points,
                        class: road.class,
                        width: width as f32,
                        minzoom: road_zoom(road.class),
                        tunnel: inside[start],
                    });
                    start = step;
                }
            }
        }
    }
    // The map's order, so the export is reproducible.
    lines.sort_by(|a, b| {
        let key = |line: &RoadLine| line.points.first().map_or((0.0, 0.0), |p| (p[0], p[1]));
        let (ka, kb) = (key(a), key(b));
        ka.0.total_cmp(&kb.0).then(ka.1.total_cmp(&kb.1))
    });
    let mut indexes: Vec<&usize> = scene.keys.keys().collect();
    indexes.sort_unstable();
    for &index in indexes {
        let Some((nodes, origin, model)) = prefab_nodes(&map.items[index]) else { continue };
        let Some(prefab) = scene.prefabs.get(&model) else { continue };
        let Some(placed) = nodes.first().and_then(|uid| map.nodes.get(uid)).and_then(|at| Placed::new(prefab, origin, at)) else {
            continue;
        };
        let count = prefab.nodes.len();
        let (classes, _) = scene.classes.at_prefab(index, nodes, origin, count, scene.on_node);
        let road_at = |node: usize| classes[node].is_some_and(|class| class != RoadClass::Rail);
        for from in 0..count {
            for to in 0..count {
                if from == to || !road_at(from) || !road_at(to) {
                    continue;
                }
                let Some(way) = prefab.path(from, to) else { continue };
                // Both ways on the one line: the way back only where it isn't the same road.
                if from > to && prefab.path(to, from).is_some_and(|back| close_lines(&back, &way)) {
                    continue;
                }
                let Some(class) = classes[from].max(classes[to]) else { continue };
                let height = |own: usize| f64::from(prefab.nodes[own].height);
                let points = way
                    .iter()
                    .map(|p| {
                        let world = placed.world([f64::from(p[0]), (height(from) + height(to)) / 2.0, f64::from(p[1])]);
                        [world[0] as f32, world[2] as f32]
                    })
                    .collect();
                lines.push(RoadLine { tunnel: scene.tunnels.contains(&index), ..RoadLine::junction(points, class) });
            }
        }
    }
    lines
}

/// Whether two ways through a prefab run within a road's width of each other: its middles.
fn close_lines(a: &[[f32; 2]], b: &[[f32; 2]]) -> bool {
    let (Some(x), Some(y)) = (a.get(a.len() / 2), b.get(b.len() / 2)) else { return false };
    (x[0] - y[0]).hypot(x[1] - y[1]) < 12.0
}

/// The road looks' surfaces, for what has to run on them.
pub fn road_looks(files: &GameFiles) -> Looks {
    let materials = Materials::default();
    let rules = Rules::load(files, &materials);
    Looks::load(files, &materials, &rules)
}

/// Writes the surfaces of `map` within `bounds` as tiles under `dir`: `{z}/{x}/{y}.pbf`. `progress` is told how far it
/// is, 0 to 1. Stops (and says nothing) once `stop` is set.
#[allow(clippy::too_many_arguments)]
pub fn write(
    files: &GameFiles,
    definitions: &Definitions,
    map: &Map,
    bounds: [f32; 4],
    dir: &Path,
    stop: &AtomicBool,
    audit: Option<&Mutex<Audit>>,
    progress: &mut dyn FnMut(f32),
) -> std::io::Result<Surfaces> {
    let materials = Materials::default();
    let rules = Rules::load(files, &materials);
    let paints = Paints::load(files, &materials);
    let looks = Looks::load(files, &materials, &rules);
    let corners = CornerModels::load(files);
    let pools: HashMap<u64, Vec<Own3>> = definitions
        .water_models
        .iter()
        .map(|(model, path)| (*model, water::model_water(files, &materials, path)))
        .collect();
    let tables = Tables { files, definitions, materials, rules, paints, looks, corners };
    let threads = crate::threads::for_a_build();
    // The prefabs in reach, and what sorts each.
    let overlaps = |b: &[f32; 4], margin: f32| {
        b[0] <= bounds[2] + margin && b[2] >= bounds[0] - margin && b[1] <= bounds[3] + margin && b[3] >= bounds[1] - margin
    };
    let mut keys: HashMap<usize, ModelKey> = HashMap::new();
    let mut near: Vec<usize> = Vec::new();
    let mut looks_alike: HashMap<(u64, u64), u64> = HashMap::new();
    let mut alike: HashMap<(u64, u64), u64> = HashMap::new();
    for (index, item) in map.items.iter().enumerate() {
        let ItemData::Prefab { model, variant, flags, parts, .. } = &item.data else { continue };
        if !overlaps(&item.bounds, 4000.0) {
            continue;
        }
        near.push(index);
        if flags.secret {
            continue;
        }
        let data = map.prefab_data.get(&item.uid);
        let mut corner_models = [[0; 3]; 6];
        if let Some(data) = data {
            for (slot, corner) in corner_models.iter_mut().zip(&data.corners) {
                *slot = corner.model;
            }
        }
        let look = data.map_or(0, |data| data.look);
        // Looks that sort the same (most only change a texture's colour) are one look here.
        let look = *looks_alike.entry((*model, look)).or_insert_with(|| {
            let Some(path) = definitions.prefab_models.get(model) else { return look };
            let Some(pmd) = files.read(path).ok().flatten() else { return look };
            let sorts_as = faces::look_signature(files, &tables.materials, &tables.rules, &pmd, look);
            *alike.entry((*model, sorts_as)).or_insert(look)
        });
        keys.insert(index, ModelKey { model: *model, variant: *variant, look, corners: corner_models, parts: parts.clone() });
    }
    let mut prefabs: HashMap<u64, Arc<Prefab>> = HashMap::new();
    for &index in &near {
        let Some((_, _, model)) = prefab_nodes(&map.items[index]) else { continue };
        if !prefabs.contains_key(&model)
            && let Some(path) = definitions.prefab_paths.get(&model)
            && let Some(parsed) = files.read(path).ok().flatten().and_then(|bytes| prefab::parse(&bytes).ok())
        {
            prefabs.insert(model, Arc::new(parsed));
        }
    }
    let parking = string_to_token(PARKING).unwrap_or(0);
    let mut unique: Vec<ModelKey> = keys.values().cloned().collect();
    unique.sort_unstable();
    unique.dedup();
    let total = unique.len().max(1);
    let next = AtomicUsize::new(0);
    let sorted_list: Mutex<Vec<(ModelKey, Arc<Sorted>)>> = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                loop {
                    let at = next.fetch_add(1, Ordering::Relaxed);
                    let Some(key) = unique.get(at) else { break };
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let (Some(prefab), Some(path)) = (prefabs.get(&key.model), definitions.prefab_models.get(&key.model)) else {
                        continue;
                    };
                    let sorted = Sorted::new(prefab, files, &tables.materials, &tables.rules, &tables.corners, key, path, parking);
                    sorted_list.lock().unwrap_or_else(|e| e.into_inner()).push((key.clone(), Arc::new(sorted)));
                }
            });
        }
        while next.load(Ordering::Relaxed) < total && !stop.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });
    if stop.load(Ordering::Relaxed) {
        return Ok(Surfaces::default());
    }
    progress(0.3);
    let sorted: HashMap<ModelKey, Arc<Sorted>> = sorted_list.into_inner().unwrap_or_else(|e| e.into_inner()).into_iter().collect();
    let reaches: HashMap<u64, sort::LaneReach> = prefabs.iter().map(|(model, prefab)| (*model, sort::lane_reach(prefab))).collect();
    let classes = Classes::new(map, definitions, &prefabs, &reaches, &near);
    let mut on_node: HashMap<u64, Vec<usize>> = HashMap::new();
    for &index in &near {
        if let Some((nodes, ..)) = prefab_nodes(&map.items[index]) {
            for n in nodes {
                on_node.entry(*n).or_default().push(index);
            }
        }
    }
    let on_prefab: HashSet<u64> = on_node.keys().copied().collect();
    let sections = roads::road_sections(map, definitions, &tables.looks);
    let by_index: HashMap<usize, roads::Section> = sections.iter().cloned().collect();
    let mut road_ends: HashMap<u64, Vec<(usize, bool)>> = HashMap::new();
    for (index, _) in &sections {
        if let ItemData::Road { start_node, end_node, .. } = &map.items[*index].data {
            road_ends.entry(*end_node).or_default().push((*index, true));
            road_ends.entry(*start_node).or_default().push((*index, false));
        }
    }
    let joint = |index: usize, section: &roads::Section, arrives: bool| -> [f64; 4] {
        let ItemData::Road { start_node, end_node, .. } = &map.items[index].data else { return [0.0; 4] };
        let node_uid = if arrives { *end_node } else { *start_node };
        if let Some(list) = on_node.get(&node_uid) {
            for &prefab_index in list {
                let Some((nodes, origin, model)) = prefab_nodes(&map.items[prefab_index]) else { continue };
                let (Some(key), Some(prefab)) = (keys.get(&prefab_index), prefabs.get(&model)) else { continue };
                let Some(sorted) = sorted.get(key) else { continue };
                let Some(world_at) = nodes.iter().position(|n| *n == node_uid) else { continue };
                let own = geometry::prefab_own_node(world_at, origin, prefab.nodes.len());
                let Some(node) = prefab.nodes.get(own) else { continue };
                let road = &sorted.road_faces;
                let (position, height, direction) = (node.position, node.height, node.direction);
                let [outer_right, outer_left] = section.outer;
                let side = |sign: f32, outer: f64| -> f64 {
                    road.edge(position, height, direction, sign, 0.0, outer as f32).map_or(0.0, |edge| f64::from(edge) - outer)
                };
                let median = |sign: f32, own: Option<f64>, outer: f64| -> f64 {
                    let Some(own) = own else { return 0.0 };
                    road.inner_edge(position, height, direction, sign, ((own + outer) / 2.0) as f32).map_or(0.0, |edge| f64::from(edge) - own)
                };
                let (right_sign, left_sign) = if arrives { (1.0, -1.0) } else { (-1.0, 1.0) };
                return [
                    side(right_sign, outer_right),
                    side(left_sign, outer_left),
                    median(right_sign, section.inner.map(|i| i[0]), outer_right),
                    median(left_sign, section.inner.map(|i| i[1]), outer_left),
                ];
            }
            return [0.0; 4];
        }
        match road_ends.get(&node_uid).map(Vec::as_slice) {
            Some([one, other]) => {
                let (own, there) = if one.0 == index && one.1 == arrives { (*one, *other) } else { (*other, *one) };
                by_index.get(&there.0).map_or([0.0; 4], |section_there| roads::halfway(section, own.1, section_there, there.1))
            }
            _ => [0.0; 4],
        }
    };
    let ground_from: HashMap<u64, [f32; 2]> = sections
        .iter()
        .map(|(index, section)| (map.items[*index].uid, section.ground_from.map(|from| from as f32)))
        .collect();
    let network = Network::new(&map.items, sections, &on_prefab, &joint);
    let mut tubes = Tubes::default();
    for item in &map.items {
        if let ItemData::Curve { model, start_node, end_node, length, .. } = &item.data
            && definitions.tunnel_curves.contains(model)
            && let (Some(a), Some(b)) = (map.nodes.get(start_node), map.nodes.get(end_node))
        {
            let steps = ((f64::from(*length) / LINE_STEP).ceil() as usize).clamp(2, 200);
            let points: Vec<([f64; 3], [f64; 2])> = (0..=steps).map(|i| frame(a, b, f64::from(*length), i as f64 / steps as f64, false)).collect();
            let line: Vec<[f32; 2]> = points.iter().map(|(p, _)| [p[0] as f32, p[2] as f32]).collect();
            let heights: Vec<f32> = points.iter().map(|(p, _)| p[1] as f32).collect();
            tubes.add(&line, &heights);
        }
    }
    let mut tunnels = tunnel_roads(map, definitions);
    tunnels.extend(tunnel_prefabs(map, &prefabs, &sorted, &keys, &tubes, &tunnels, &road_ends));
    progress(0.4);
    let scene = Scene {
        map,
        tables: &tables,
        prefabs: &prefabs,
        sorted: &sorted,
        keys: &keys,
        classes: &classes,
        network: &network,
        on_node: &on_node,
        tubes: &tubes,
        tunnels: &tunnels,
        road_ends: &road_ends,
        drawings: Mutex::new(HashMap::new()),
        pools: &pools,
    };
    let road_lines = road_features(road_lines(&scene));
    let mut footprints = Vec::with_capacity(keys.len());
    for (&index, key) in &keys {
        let (Some(sorted), Some(prefab)) = (sorted.get(key), prefabs.get(&key.model)) else { continue };
        let Some((nodes, origin, _)) = prefab_nodes(&map.items[index]) else { continue };
        let Some(placed) = nodes.first().and_then(|uid| map.nodes.get(uid)).and_then(|at| Placed::new(prefab, origin, at)) else {
            continue;
        };
        footprints.push(sorted.hull.iter().map(|v| placed.world(v.map(f64::from)).map(|c| c as f32)).collect());
    }
    // The terrain at prefabs' edges: land as a prefab's, water to be painted with the rest.
    let mut edge_water = Vec::new();
    for &index in keys.keys() {
        scene.edges(index, &mut |side, relief, points, prefab, placed| {
            let mut water = Vec::new();
            ground::edge_ground(definitions, &tables.paints, side, relief, points, &prefab.nodes, &|v| placed.world(v), &mut |quad, is_water| {
                let corners = quad.map(|c| c.map(|v| v as f32));
                if is_water {
                    water.push(corners);
                } else {
                    footprints.push(corners.to_vec());
                }
            });
            if !water.is_empty() {
                edge_water.push(water);
            }
        });
    }
    // Items by the blocks they reach, and by the cells they're in.
    let mut blocks: BTreeMap<(u32, u32), Vec<u32>> = BTreeMap::new();
    for (index, item) in map.items.iter().enumerate() {
        let drawn = match &item.data {
            ItemData::Road { .. } | ItemData::Prefab { .. } | ItemData::Terrain { .. } | ItemData::BezierPatch { .. } => true,
            ItemData::Model { model, .. } => pools.contains_key(model),
            _ => false,
        };
        if !drawn {
            continue;
        }
        if !overlaps(&item.bounds, 0.0) {
            continue;
        }
        if matches!(&item.data, ItemData::Road { flags, .. } | ItemData::Prefab { flags, .. } if flags.secret) {
            continue;
        }
        let [x0, y0] = tile::tile_of(item.bounds[0] as f64 - tile::REACH, item.bounds[1] as f64 - tile::REACH, BLOCK_ZOOM);
        let [x1, y1] = tile::tile_of(item.bounds[2] as f64 + tile::REACH, item.bounds[3] as f64 + tile::REACH, BLOCK_ZOOM);
        for x in x0..=x1 {
            for y in y0..=y1 {
                blocks.entry((x, y)).or_default().push(index as u32);
            }
        }
    }
    // The service areas, and which blocks each is waiting for: its ground is worked out from the paving they find, as the
    // last of them is done with it.
    let mut places_in: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
    let mut gathering = Vec::new();
    for points in service::places(&service_points(map, &near, &keys, &prefabs, parking)) {
        let reach = service::reach_of(&points);
        let [tx0, ty0] = tile::tile_of(reach[0], reach[1], BLOCK_ZOOM);
        let [tx1, ty1] = tile::tile_of(reach[2], reach[3], BLOCK_ZOOM);
        let mut left = 0;
        for tile in (tx0..=tx1).flat_map(|x| (ty0..=ty1).map(move |y| (x, y))) {
            if blocks.contains_key(&tile) {
                places_in.entry(tile).or_default().push(gathering.len() as u32);
                left += 1;
            }
        }
        gathering.push(Mutex::new(Gathering { reach, points, left, found: Vec::new() }));
    }
    let service_areas: Mutex<Vec<service::Area>> = Mutex::new(Vec::new());
    let blocks: Vec<((u32, u32), Vec<u32>)> = blocks.into_iter().collect();
    let heights = raised::heights(&scene, &blocks, threads, stop, &mut |done| progress(0.4 + 0.05 * done));
    let looking_at = audit.and_then(|audit| audit.lock().unwrap_or_else(|e| e.into_inner()).looking_at);
    let total = blocks.len().max(1);
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let budget = Budget::of(&blocks);
    let summary = Mutex::new(Summary::default());
    let failure: Mutex<Option<std::io::Error>> = Mutex::new(None);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                loop {
                    let at = next.fetch_add(1, Ordering::Relaxed);
                    let Some((tile, items)) = blocks.get(at) else { break };
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let _room = budget.take(items.len(), stop);
                    if let Some(at) = looking_at {
                        scene.stack_at(items, at);
                    }
                    let mut block = Block::new(*tile, BLOCK_ZOOM);
                    let mut spans = Vec::with_capacity(items.len());
                    for &index in items {
                        let from = block.shape_count();
                        scene.add_item(index as usize, &mut block);
                        if heights.raised.contains(&index) {
                            block.raised_shapes.extend(from as u32..block.shape_count() as u32);
                        }
                        spans.push(from..block.shape_count());
                    }
                    scene.add_cover(items, &mut block);
                    scene.add_levels(items, &heights.raised, &mut block);
                    let paving_left = block.uncover();
                    if let Some(places) = places_in.get(tile) {
                        // Roads and junctions, not the ways through a place: those are the place's.
                        let bounds = |index: usize| match &map.items[index].data {
                            ItemData::Road { .. } => true,
                            ItemData::Prefab { .. } => keys.get(&index).and_then(|key| sorted.get(key)).is_some_and(|sorted| !sorted.is_place),
                            _ => false,
                        };
                        let roads: Vec<(u32, Vec<service::Piece>)> = items
                            .iter()
                            .zip(&spans)
                            .filter(|(index, _)| bounds(**index as usize))
                            .map(|(index, span)| (*index, block.road_surface(span.clone())))
                            .filter(|(_, pieces)| !pieces.is_empty())
                            .collect();
                        spans.extend(paving_left);
                        let paving = BlockPaving::new(block.quiet_pieces(&spans), roads, tile::tile_box(tile.0, tile.1, BLOCK_ZOOM));
                        for &place in places {
                            let mut place = gathering[place as usize].lock().unwrap_or_else(|e| e.into_inner());
                            let Gathering { reach, points, left, found } = &mut *place;
                            found.push(paving.in_reach(*reach));
                            *left -= 1;
                            if *left == 0 {
                                let (points, found) = (std::mem::take(points), std::mem::take(found));
                                drop(place);
                                let (rings, roads) = service::ground(&points, &found);
                                drop(found);
                                if !rings.is_empty() {
                                    service_areas.lock().unwrap_or_else(|e| e.into_inner()).push(service::Area { points, rings, roads });
                                }
                            }
                        }
                    }
                    let mut found: Vec<(u32, [f64; 2], Option<Kind>)> = Vec::new();
                    if audit.is_some() {
                        block.probes = Some(scene.probes(items));
                    }
                    let written = block.write(dir, &mut |what, at, kind| found.push((what, at, kind)));
                    if let Some(audit) = audit {
                        let mut audit = audit.lock().unwrap_or_else(|e| e.into_inner());
                        for (what, at, kind) in found {
                            let index = what as usize;
                            let place = scene.sorted.get(&scene.keys[&index]).is_some_and(|sorted| sorted.is_place);
                            *audit.counts.entry((place, kind)).or_default() += 1;
                            if !matches!(kind, Some(Kind::Road(_) | Kind::Tunnel(_) | Kind::Rail)) {
                                audit.points.push((kind, at));
                                let path = definitions.prefab_models.get(&scene.keys[&index].model).cloned().unwrap_or_default();
                                let entry = audit.off_road.entry(path.clone()).or_insert((0, at));
                                entry.0 += 1;
                                let placement = audit.placements.entry(index).or_insert((0, 0, path, at));
                                if kind.is_none() {
                                    placement.0 += 1;
                                } else {
                                    placement.1 += 1;
                                }
                            }
                        }
                    }
                    match written {
                        Ok((tiles, bytes)) => {
                            let mut summary = summary.lock().unwrap_or_else(|e| e.into_inner());
                            summary.tiles += tiles;
                            summary.bytes += bytes;
                        }
                        Err(error) => *failure.lock().unwrap_or_else(|e| e.into_inner()) = Some(error),
                    }
                    done.fetch_add(1, Ordering::Relaxed);
                }
            });
        }
        while done.load(Ordering::Relaxed) < total && next.load(Ordering::Relaxed) < total + threads && !stop.load(Ordering::Relaxed) {
            progress(0.45 + 0.45 * done.load(Ordering::Relaxed) as f32 / total as f32);
            std::thread::sleep(std::time::Duration::from_millis(200));
            if failure.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
                break;
            }
        }
    });
    if let Some(error) = failure.into_inner().unwrap_or_else(|e| e.into_inner()) {
        return Err(error);
    }
    let mut summary = summary.into_inner().unwrap_or_else(|e| e.into_inner());
    summary.prefab_models = sorted.len();
    summary.prefab_drawings = scene.drawings.lock().unwrap_or_else(|e| e.into_inner()).len();
    summary.chains = network.chains.len();
    summary.ease = network.report();
    let service = service_areas.into_inner().unwrap_or_else(|e| e.into_inner());
    let mut decks = raised::Decks::default();
    if !stop.load(Ordering::Relaxed) {
        let rings: Vec<&Own64> = service.iter().flat_map(|area| &area.rings).collect();
        let (tiles, bytes) = tile::add_service(dir, &rings)?;
        summary.service_areas = service.len();
        summary.service_tiles = tiles;
        summary.bytes += bytes;
        let (blocks, bytes, in_the_air) = raised::write(&scene, &heights, &blocks, dir, threads, stop, &mut |done| progress(0.9 + 0.1 * done))?;
        decks = in_the_air;
        summary.raised_blocks = blocks;
        summary.raised_bytes = bytes;
        summary.bytes += bytes;
    }
    progress(1.0);
    Ok(Surfaces { summary, road_lines, footprints, ground_from, edge_water, service, decks })
}

/// Whether a quad of painted ground lies within `reach` of `p`, near enough its height.
pub(crate) fn quad_near(quad: &Quad, p: &[f64; 3], reach: f64) -> bool {
    let (x0, x1, z0, z1) = quad.corners.iter().fold((f64::MAX, f64::MIN, f64::MAX, f64::MIN), |b, c| {
        (b.0.min(c[0]), b.1.max(c[0]), b.2.min(c[2]), b.3.max(c[2]))
    });
    let (y0, y1) = quad.corners.iter().fold((f64::MAX, f64::MIN), |b, c| (b.0.min(c[1]), b.1.max(c[1])));
    p[0] >= x0 - reach && p[0] <= x1 + reach && p[2] >= z0 - reach && p[2] <= z1 + reach && p[1] >= y0 - ON_GROUND && p[1] <= y1 + ON_GROUND
}

/// A ring's box.
pub(crate) fn ring_box(ring: &[[f64; 3]]) -> [f64; 4] {
    ring.iter().fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |b, p| [b[0].min(p[0]), b[1].min(p[2]), b[2].max(p[0]), b[3].max(p[2])])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A packed grid gives each cell's things in the order they were added, as a list a cell kept them.
    #[test]
    fn a_cells_things_come_in_the_order_added() {
        let cells = Cells::new(vec![((1, 0), 7), ((0, 0), 3), ((1, 0), 2), ((0, 0), 9), ((1, 0), 5)]);
        assert_eq!(cells.at((1, 0)), &[7, 2, 5]);
        assert_eq!(cells.at((0, 0)), &[3, 9]);
        assert!(cells.at((4, 4)).is_empty());
    }

    /// Blocks run at once only while they hold no more items than the largest, which still runs (alone if need be).
    #[test]
    fn the_budget_holds_back_blocks_past_the_largest() {
        let blocks: Vec<((u32, u32), Vec<u32>)> = [10, 4, 4, 3, 9].iter().map(|&n| ((0, 0), vec![0; n])).collect();
        let budget = Budget::of(&blocks);
        let stop = AtomicBool::new(false);
        let (most, now) = (AtomicUsize::new(0), AtomicUsize::new(0));
        std::thread::scope(|scope| {
            for (_, items) in &blocks {
                let (budget, stop, most, now) = (&budget, &stop, &most, &now);
                scope.spawn(move || {
                    let _room = budget.take(items.len(), stop);
                    let held = now.fetch_add(items.len(), Ordering::SeqCst) + items.len();
                    most.fetch_max(held, Ordering::SeqCst);
                    std::thread::sleep(std::time::Duration::from_millis(20));
                    now.fetch_sub(items.len(), Ordering::SeqCst);
                });
            }
        });
        assert!(most.load(Ordering::SeqCst) <= 10, "at most the largest block's items at once");
        assert_eq!(now.load(Ordering::SeqCst), 0, "every block ran");
    }
}
