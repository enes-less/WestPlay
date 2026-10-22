//! The road network as a route planner sees it, and WestPlay's own planner (PLAN.md: Navigation plan, B):
//! the routes offered before Go, the one picked then handed to the game's GPS.
//!
//! The graph is written beside the map's tiles when the map is built, from what the map is drawn from:
//!
//! - a road is a way between its two nodes, both ways. Which way its traffic runs isn't needed: every road
//!   leads to junctions, and a junction's lanes only lead in where traffic enters and out where it leaves,
//!   so a road can't be joined against its traffic;
//! - a junction (a prefab) is its own lanes from end to end, one way each, as the game's GPS takes them
//!   ([`Prefab::lanes`]): turns it has no lane for aren't in the graph;
//! - a ferry or train crossing joins its two ports (`def/ferry/connection`), with its time.
//!
//! The search goes from way to way, never straight back along the way it came (no U-turn on a road, no
//! leaving a junction back into it): a U-turn is only where a junction has a lane for it (a roundabout).
//! Secret roads (on the map only once driven) are left out, as the map leaves them out.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, HashMap};
use std::io::{self, Read, Write};

use crate::files::{FileError, GameFiles};
use crate::geometry;
use crate::sector::ItemData;
use crate::sii;
use crate::token::string_to_token;
use crate::world::{Definitions, Map, Prefabs, RoadClass};

/// The graph's file, beside the map's tiles.
pub const FILE: &str = "route-graph.bin";
const MAGIC: &[u8; 4] = b"WPRG";
const VERSION: u32 = 1;

/// A place the graph's ways meet: a map node.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Node {
    pub uid: u64,
    /// Metres: x east, z south.
    pub x: f32,
    pub z: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Road,
    Junction,
    Ferry,
}

/// A way from one node to another, one direction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Way {
    pub from: u32,
    pub to: u32,
    /// The map item it is part of: a road's two directions share it, as a junction's lanes do.
    pub item: u32,
    /// Metres (a crossing's: none).
    pub length: f32,
    pub class: RoadClass,
    pub kind: Kind,
    /// A crossing's time, minutes (the game's).
    pub minutes: f32,
    /// Not a road piece the player sees: a road the game doesn't show (its look's `invisible`) or a join to a
    /// ferry's boarding point made here. Routes may go along it; a waypoint never goes on it.
    pub invisible: bool,
}

pub struct Graph {
    pub nodes: Vec<Node>,
    pub ways: Vec<Way>,
    /// The ways leaving each node: `leaving[start[n]..start[n + 1]]`.
    start: Vec<u32>,
    leaving: Vec<u32>,
    by_uid: HashMap<u64, u32>,
    /// The ways arriving at each node, as `start`/`leaving` are for those leaving: made when first needed.
    arriving: std::sync::OnceLock<(Vec<u32>, Vec<u32>)>,
}

impl Graph {
    pub fn new(nodes: Vec<Node>, ways: Vec<Way>) -> Self {
        let mut counts = vec![0u32; nodes.len() + 1];
        for way in &ways {
            counts[way.from as usize + 1] += 1;
        }
        for n in 0..nodes.len() {
            counts[n + 1] += counts[n];
        }
        let mut next = counts.clone();
        let mut leaving = vec![0u32; ways.len()];
        for (index, way) in ways.iter().enumerate() {
            leaving[next[way.from as usize] as usize] = index as u32;
            next[way.from as usize] += 1;
        }
        let by_uid = nodes.iter().enumerate().map(|(index, node)| (node.uid, index as u32)).collect();
        Self { nodes, ways, start: counts, leaving, by_uid, arriving: std::sync::OnceLock::new() }
    }

    /// The node of a map node uid.
    pub fn node(&self, uid: u64) -> Option<u32> {
        self.by_uid.get(&uid).copied()
    }

    /// The node nearest `(x, z)` (metres), with a way leaving it.
    pub fn nearest(&self, x: f32, z: f32) -> Option<u32> {
        (0..self.nodes.len())
            .filter(|&n| self.start[n + 1] > self.start[n])
            .min_by(|&a, &b| {
                let distance = |n: usize| (self.nodes[n].x - x).powi(2) + (self.nodes[n].z - z).powi(2);
                distance(a).total_cmp(&distance(b))
            })
            .map(|n| n as u32)
    }

    /// Whether the node is on road pieces alone, the only place a waypoint goes (owner 2026-10-22): every way
    /// arriving at it and leaving it a road the player sees. Not a junction's edge, either side (lanes only
    /// arrive at a junction's way out, so the ways leaving alone don't tell: they took junction exits for
    /// roads), nor an invisible road.
    pub fn road_piece(&self, node: u32) -> bool {
        let piece = |&way: &u32| {
            let way = &self.ways[way as usize];
            way.kind == Kind::Road && !way.invisible
        };
        let (leaving, arriving) = (self.leaving(node), self.arriving(node));
        !leaving.is_empty() && !arriving.is_empty() && leaving.iter().all(piece) && arriving.iter().all(piece)
    }

    fn leaving(&self, node: u32) -> &[u32] {
        &self.leaving[self.start[node as usize] as usize..self.start[node as usize + 1] as usize]
    }
}

/// Builds the graph of `map` (its junctions' lanes from `prefabs`, its crossings from the game's files).
pub fn build(
    map: &Map,
    definitions: &Definitions,
    prefabs: &mut Prefabs,
    files: &GameFiles,
) -> Result<Graph, FileError> {
    build_telling(map, definitions, prefabs, files, &mut |_| {})
}

/// [`build`], telling how far it has gone through the map's items (0 to 1) as it goes.
pub fn build_telling(
    map: &Map,
    definitions: &Definitions,
    prefabs: &mut Prefabs,
    files: &GameFiles,
    progress: &mut dyn FnMut(f32),
) -> Result<Graph, FileError> {
    let mut nodes: Vec<Node> = Vec::new();
    let mut index: HashMap<u64, u32> = HashMap::new();
    let mut node = |uid: u64| -> Option<u32> {
        if let Some(&found) = index.get(&uid) {
            return Some(found);
        }
        let at = map.nodes.get(&uid)?;
        nodes.push(Node { uid, x: at.position[0], z: at.position[2] });
        index.insert(uid, nodes.len() as u32 - 1);
        Some(nodes.len() as u32 - 1)
    };
    let mut ways: Vec<Way> = Vec::new();
    // Each junction model's lanes from end to end (its own node numbers), worked out once.
    let mut movements: HashMap<u64, Vec<(usize, usize, f32)>> = HashMap::new();
    // In token order: the file comes out the same each build.
    let mut ports: BTreeMap<u64, u32> = BTreeMap::new();
    let item_count = map.items.len();
    let mut told = 0;
    for (item_index, item) in map.items.iter().enumerate() {
        // Each hundredth of the items gone through, told.
        let hundredths = item_index * 100 / item_count.max(1);
        if hundredths > told {
            told = hundredths;
            progress(item_index as f32 / item_count as f32);
        }
        let item_index = item_index as u32;
        match &item.data {
            ItemData::Road { flags, look, start_node, end_node, .. } => {
                if flags.secret {
                    continue;
                }
                let (Some(start), Some(end)) = (map.nodes.get(start_node), map.nodes.get(end_node)) else { continue };
                let length = line_length(&geometry::road_curve(start, end));
                let look = definitions.road_looks.get(look);
                let class = look.map_or(RoadClass::Local, |look| look.class);
                // Rails, and roads no vehicle may drive: never a route's.
                if class == RoadClass::Rail || look.is_some_and(|look| look.no_vehicles) {
                    continue;
                }
                let invisible = look.is_some_and(|look| look.invisible);
                let (Some(a), Some(b)) = (node(*start_node), node(*end_node)) else { continue };
                for (from, to) in [(a, b), (b, a)] {
                    ways.push(Way {
                        from,
                        to,
                        item: item_index,
                        length,
                        class,
                        kind: Kind::Road,
                        minutes: 0.0,
                        invisible,
                    });
                }
            }
            ItemData::Prefab { flags, model, nodes: ends, origin, .. } => {
                if flags.secret {
                    continue;
                }
                let lanes = match movements.get(model) {
                    Some(lanes) => lanes,
                    None => {
                        let found = prefabs.get(*model).map(junction_lanes).unwrap_or_default();
                        movements.entry(*model).or_insert(found)
                    }
                };
                let count = ends.len();
                if count == 0 {
                    continue;
                }
                // The item lists its nodes from the prefab's origin node on (geometry::prefab_world_node).
                let world = |own: usize| (own + count - (*origin as usize % count)) % count;
                for &(from, to, length) in lanes.iter() {
                    let (Some(&a), Some(&b)) = (ends.get(world(from)), ends.get(world(to))) else { continue };
                    let (Some(a), Some(b)) = (node(a), node(b)) else { continue };
                    ways.push(Way {
                        from: a,
                        to: b,
                        item: item_index,
                        length,
                        class: RoadClass::Local,
                        kind: Kind::Junction,
                        minutes: 0.0,
                        invisible: false,
                    });
                }
            }
            ItemData::Ferry { port, node: at, .. } => {
                if let Some(at) = node(*at) {
                    ports.entry(*port).or_insert(at);
                }
            }
            _ => {}
        }
    }
    // A port's boarding point is a node of its own, beside the port's roads: joined to the nearest node a
    // road or junction leads to, both ways, so a route can drive on and off.
    let mut on_road = vec![false; nodes.len()];
    for way in &ways {
        on_road[way.from as usize] = true;
        on_road[way.to as usize] = true;
    }
    let mut item = map.items.len() as u32;
    for &port in ports.values() {
        if on_road[port as usize] {
            continue;
        }
        let at = nodes[port as usize];
        let nearest = (0..nodes.len())
            .filter(|&n| on_road[n])
            .map(|n| (n, (nodes[n].x - at.x).hypot(nodes[n].z - at.z)))
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((road, length)) = nearest.filter(|&(_, length)| length <= PORT_REACH) {
            for (from, to) in [(port, road as u32), (road as u32, port)] {
                ways.push(Way {
                    from,
                    to,
                    item,
                    length,
                    class: RoadClass::Local,
                    kind: Kind::Road,
                    minutes: 0.0,
                    invisible: true,
                });
            }
            item += 1;
        }
    }
    let crossings = ferry_connections(files)?;
    let first_crossing = item;
    for (index, (from, to, minutes)) in crossings.into_iter().enumerate() {
        if let (Some(&a), Some(&b)) = (ports.get(&from), ports.get(&to)) {
            ways.push(Way {
                from: a,
                to: b,
                item: first_crossing + index as u32,
                length: 0.0,
                class: RoadClass::Local,
                kind: Kind::Ferry,
                minutes,
                invisible: false,
            });
        }
    }
    Ok(Graph::new(nodes, ways))
}

/// The farthest a port's boarding point is joined to a road.
const PORT_REACH: f32 = 1000.0;

/// A junction model's lanes from each of its ends to each (its own node numbers), and their length.
fn junction_lanes(prefab: &crate::prefab::Prefab) -> Vec<(usize, usize, f32)> {
    let count = prefab.nodes.len();
    let mut lanes = Vec::new();
    for from in 0..count {
        for to in 0..count {
            if let Some((_, length)) = prefab.lanes(from, to) {
                lanes.push((from, to, length));
            }
        }
    }
    lanes
}

/// Every crossing the game's files define (`ferry_connection : conn.<from>.<to>`): its ports, and its time.
fn ferry_connections(files: &GameFiles) -> Result<Vec<(u64, u64, f32)>, FileError> {
    let mut crossings = Vec::new();
    for name in files.list("/def/ferry/connection")? {
        if !name.ends_with(".sii") {
            continue;
        }
        for unit in sii::read_units(files, &format!("/def/ferry/connection/{name}"))? {
            let Some(rest) = unit.name.strip_prefix("conn.") else { continue };
            let Some((from, to)) = rest.split_once('.') else { continue };
            if let (Some(from), Some(to)) = (string_to_token(from), string_to_token(to)) {
                crossings.push((from, to, unit.number("time").unwrap_or(60.0)));
            }
        }
    }
    Ok(crossings)
}

fn line_length(points: &[[f32; 2]]) -> f32 {
    points.windows(2).map(|pair| (pair[1][0] - pair[0][0]).hypot(pair[1][1] - pair[0][1])).sum()
}

impl Graph {
    pub fn write(&self, out: &mut impl Write) -> io::Result<()> {
        out.write_all(MAGIC)?;
        out.write_all(&VERSION.to_le_bytes())?;
        out.write_all(&(self.nodes.len() as u32).to_le_bytes())?;
        for node in &self.nodes {
            out.write_all(&node.uid.to_le_bytes())?;
            out.write_all(&node.x.to_le_bytes())?;
            out.write_all(&node.z.to_le_bytes())?;
        }
        out.write_all(&(self.ways.len() as u32).to_le_bytes())?;
        for way in &self.ways {
            for value in [way.from, way.to, way.item] {
                out.write_all(&value.to_le_bytes())?;
            }
            out.write_all(&way.length.to_le_bytes())?;
            out.write_all(&way.minutes.to_le_bytes())?;
            // A way the player doesn't see in the kind byte's top bit: a graph written before reads as all seen.
            out.write_all(&[class_byte(way.class), kind_byte(way.kind) | (u8::from(way.invisible) << 7)])?;
        }
        Ok(())
    }

    pub fn read(input: &mut impl Read) -> io::Result<Self> {
        let bad = |what: &str| io::Error::new(io::ErrorKind::InvalidData, format!("route graph: {what}"));
        let mut magic = [0u8; 4];
        input.read_exact(&mut magic)?;
        if &magic != MAGIC || read_u32(input)? != VERSION {
            return Err(bad("not one this WestPlay reads"));
        }
        let count = read_u32(input)? as usize;
        let mut nodes = Vec::with_capacity(count.min(1 << 24));
        for _ in 0..count {
            let mut uid = [0u8; 8];
            input.read_exact(&mut uid)?;
            nodes.push(Node { uid: u64::from_le_bytes(uid), x: read_f32(input)?, z: read_f32(input)? });
        }
        let count = read_u32(input)? as usize;
        let mut ways = Vec::with_capacity(count.min(1 << 26));
        for _ in 0..count {
            let (from, to, item) = (read_u32(input)?, read_u32(input)?, read_u32(input)?);
            let (length, minutes) = (read_f32(input)?, read_f32(input)?);
            let mut bytes = [0u8; 2];
            input.read_exact(&mut bytes)?;
            if from as usize >= nodes.len() || to as usize >= nodes.len() {
                return Err(bad("a way to a node it doesn't have"));
            }
            ways.push(Way {
                from,
                to,
                item,
                length,
                minutes,
                class: byte_class(bytes[0]),
                kind: byte_kind(bytes[1] & 0x7f),
                invisible: bytes[1] & 0x80 != 0,
            });
        }
        Ok(Self::new(nodes, ways))
    }
}

fn read_u32(input: &mut impl Read) -> io::Result<u32> {
    let mut bytes = [0u8; 4];
    input.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_f32(input: &mut impl Read) -> io::Result<f32> {
    Ok(f32::from_bits(read_u32(input)?))
}

fn class_byte(class: RoadClass) -> u8 {
    match class {
        RoadClass::Minor => 0,
        RoadClass::Local => 1,
        RoadClass::Trunk => 2,
        RoadClass::Motorway => 3,
        RoadClass::Rail => 4,
    }
}

fn byte_class(byte: u8) -> RoadClass {
    match byte {
        0 => RoadClass::Minor,
        2 => RoadClass::Trunk,
        3 => RoadClass::Motorway,
        4 => RoadClass::Rail,
        _ => RoadClass::Local,
    }
}

fn kind_byte(kind: Kind) -> u8 {
    match kind {
        Kind::Road => 0,
        Kind::Junction => 1,
        Kind::Ferry => 2,
    }
}

fn byte_kind(byte: u8) -> Kind {
    match byte {
        1 => Kind::Junction,
        2 => Kind::Ferry,
        _ => Kind::Road,
    }
}

/// What a route is planned for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plan {
    /// The quickest, by each road's usual speed.
    Fastest,
    /// The fewest metres.
    Shortest,
    /// Quick, keeping off motorways and expressways where it can.
    SmallRoads,
    /// Another way, as quick as can be on other roads than the routes already offered.
    Other,
}

/// A route planned: its ways in order, how long it is (metres, crossings not counted) and its time
/// (seconds at each road's usual speed, crossings' own time added).
#[derive(Clone, Debug, PartialEq)]
pub struct Route {
    pub ways: Vec<u32>,
    pub length: f32,
    pub seconds: f32,
}

/// A road's usual speed, metres a second (on the map's ground: the game's own scale applies to both).
fn speed(way: &Way) -> f32 {
    let kmh = match (way.kind, way.class) {
        (Kind::Junction, _) => 30.0,
        (_, RoadClass::Motorway) => 85.0,
        (_, RoadClass::Trunk) => 75.0,
        (_, RoadClass::Local) => 55.0,
        (_, RoadClass::Minor) => 35.0,
        (_, RoadClass::Rail) => 1.0,
    };
    kmh / 3.6
}

/// The fastest any way is driven: what a route's time can't beat in a straight line.
const TOP_SPEED: f32 = 85.0 / 3.6;

/// A crossing's time on the map's ground (seconds): the game's minutes over the game's own scale.
fn crossing_seconds(way: &Way) -> f32 {
    way.minutes * 60.0 / GAME_SCALE
}

/// About how much faster the game's clock runs than the map's ground is driven (between towns): its
/// crossing times are in its clock.
const GAME_SCALE: f32 = 17.0;

fn seconds(way: &Way) -> f32 {
    if way.kind == Kind::Ferry { crossing_seconds(way) } else { way.length / speed(way) }
}

impl Plan {
    fn cost(self, way: &Way) -> f32 {
        match self {
            Plan::Fastest | Plan::Other => seconds(way),
            Plan::Shortest if way.kind == Kind::Ferry => crossing_seconds(way) * TOP_SPEED,
            Plan::Shortest => way.length,
            Plan::SmallRoads => {
                let keep_off = match way.class {
                    RoadClass::Motorway if way.kind == Kind::Road => 1.8,
                    RoadClass::Trunk if way.kind == Kind::Road => 1.4,
                    _ => 1.0,
                };
                seconds(way) * keep_off
            }
        }
    }

    /// What a route can't cost less than over a straight `distance` (metres): A*'s estimate.
    fn least(self, distance: f32) -> f32 {
        match self {
            Plan::Shortest => distance,
            Plan::Fastest | Plan::SmallRoads | Plan::Other => distance / TOP_SPEED,
        }
    }
}

impl Graph {
    /// The best route from node `from` to node `to` for `plan`, each way's cost multiplied by `penalty`'s
    /// for it (ways a route already takes, to find another).
    pub fn route(&self, from: u32, to: u32, plan: Plan, penalty: &HashMap<u32, f32>) -> Option<Route> {
        let goal = self.nodes.get(to as usize)?;
        let estimate = |node: u32| {
            let at = &self.nodes[node as usize];
            plan.least((at.x - goal.x).hypot(at.z - goal.z))
        };
        let cost = |way: u32| plan.cost(&self.ways[way as usize]) * penalty.get(&way).copied().unwrap_or(1.0);
        // Searched way by way: each reached at its end, by the cost to there, and the way before it.
        let mut best: HashMap<u32, (f32, Option<u32>)> = HashMap::new();
        let mut queue = BinaryHeap::new();
        for &way in self.leaving(from) {
            let spent = cost(way);
            best.insert(way, (spent, None));
            queue.push(Reverse((Order(spent + estimate(self.ways[way as usize].to)), way)));
        }
        let mut last = None;
        while let Some(Reverse((_, way))) = queue.pop() {
            let (spent, _) = best[&way];
            let here = self.ways[way as usize];
            if here.to == to {
                last = Some(way);
                break;
            }
            for &next in self.leaving(here.to) {
                let then = self.ways[next as usize];
                // Never straight back along the way it came, nor back into the junction it left.
                if then.item == here.item {
                    continue;
                }
                let total = spent + cost(next);
                if best.get(&next).is_none_or(|&(known, _)| total < known) {
                    best.insert(next, (total, Some(way)));
                    queue.push(Reverse((Order(total + estimate(then.to)), next)));
                }
            }
        }
        let mut ways = vec![last?];
        while let Some(&(_, Some(before))) = best.get(ways.last()?) {
            ways.push(before);
        }
        ways.reverse();
        let length = ways.iter().map(|&way| self.ways[way as usize].length).sum();
        let seconds = ways.iter().map(|&way| seconds(&self.ways[way as usize])).sum();
        Some(Route { ways, length, seconds })
    }

    /// Up to `most` routes from `from` to `to` worth offering, the fastest first: the fastest, the shortest,
    /// the one keeping to small roads, then others found by making the fastest's roads dearer. A route much
    /// like one already offered (sharing most of its length) isn't offered again; nor is one found that way
    /// that takes far longer than the fastest.
    pub fn routes(&self, from: u32, to: u32, most: usize) -> Vec<(Plan, Route)> {
        let none = HashMap::new();
        let mut offered: Vec<(Plan, Route)> = Vec::new();
        let Some(fastest) = self.route(from, to, Plan::Fastest, &none) else { return offered };
        offered.push((Plan::Fastest, fastest));
        for plan in [Plan::Shortest, Plan::SmallRoads] {
            if offered.len() >= most {
                break;
            }
            if let Some(route) = self.route(from, to, plan, &none)
                && offered.iter().all(|(_, known)| self.shared(&route, known) < SAME)
            {
                offered.push((plan, route));
            }
        }
        // Others: each round, the roads of every route offered dearer, the fastest found again.
        let mut penalty: HashMap<u32, f32> = HashMap::new();
        for _ in 0..3 {
            if offered.len() >= most {
                break;
            }
            for (_, route) in &offered {
                for &way in &route.ways {
                    *penalty.entry(way).or_insert(1.0) *= DEARER;
                }
            }
            let Some(route) = self.route(from, to, Plan::Fastest, &penalty).map(|route| self.straightened(route))
            else {
                break;
            };
            let quickest = offered[0].1.seconds;
            if route.seconds <= quickest * LONGEST && offered.iter().all(|(_, known)| self.shared(&route, known) < SAME)
            {
                offered.push((Plan::Other, route));
            }
        }
        offered
    }

    /// Another route, found with the offered routes' roads made dearer, can leave a motorway only to come back
    /// onto it a little further on: it dodged a dearer piece of motorway, it didn't go another way (owner
    /// 2026-10-21: off at an interchange west of Frankfurt, over the roads beside it and back on, where the
    /// motorway goes straight through). Each stretch off a motorway that comes back onto one within
    /// [`HOP_WITHIN`] is planned again at the roads' own costs, and the quicker way kept.
    fn straightened(&self, route: Route) -> Route {
        let none = HashMap::new();
        let motorway = |way: u32| {
            let way = &self.ways[way as usize];
            way.kind == Kind::Road && way.class == RoadClass::Motorway
        };
        let took = &route.ways;
        let mut ways = Vec::with_capacity(took.len());
        let mut at = 0;
        while at < took.len() {
            ways.push(took[at]);
            let hop = (motorway(took[at]) && took.get(at + 1).is_some_and(|&next| !motorway(next)))
                .then(|| (at + 1..took.len()).find(|&back| motorway(took[back])))
                .flatten();
            let Some(back) = hop else {
                at += 1;
                continue;
            };
            let (left, back_on) = (self.ways[took[at] as usize], self.ways[took[back] as usize]);
            let (start, end) = (&self.nodes[left.to as usize], &self.nodes[back_on.from as usize]);
            let off = &took[at + 1..back];
            let off_seconds: f32 = off.iter().map(|&way| seconds(&self.ways[way as usize])).sum();
            // Planned again: kept if quicker, and if it neither turns back along the motorway it left nor
            // comes onto the one it joins from ahead.
            let again = ((start.x - end.x).hypot(start.z - end.z) <= HOP_WITHIN)
                .then(|| self.route(left.to, back_on.from, Plan::Fastest, &none))
                .flatten()
                .filter(|again| {
                    again.seconds < off_seconds
                        && self.ways[again.ways[0] as usize].item != left.item
                        && self.ways[again.ways[again.ways.len() - 1] as usize].item != back_on.item
                });
            ways.extend_from_slice(again.as_ref().map_or(off, |again| &again.ways));
            at = back;
        }
        let length = ways.iter().map(|&way| self.ways[way as usize].length).sum();
        let seconds = ways.iter().map(|&way| seconds(&self.ways[way as usize])).sum();
        Route { ways, length, seconds }
    }

    /// How much of `a`'s length `b` shares (0 to 1).
    fn shared(&self, a: &Route, b: &Route) -> f32 {
        let theirs: std::collections::HashSet<u32> = b.ways.iter().map(|&way| self.ways[way as usize].item).collect();
        let total: f32 = a.ways.iter().map(|&way| self.ways[way as usize].length).sum();
        if total <= 0.0 {
            return 1.0;
        }
        let common: f32 = a
            .ways
            .iter()
            .filter(|&&way| theirs.contains(&self.ways[way as usize].item))
            .map(|&way| self.ways[way as usize].length)
            .sum();
        common / total
    }
}

impl Graph {
    /// The fastest time (seconds) from node `from` to every node within `limit` seconds, or, with
    /// `backwards`, from every node within `limit` to `from`. By node (a quick estimate: U-turns on a road
    /// aren't ruled out as [`Graph::route`] rules them out).
    pub fn times(&self, from: u32, limit: f32, backwards: bool) -> HashMap<u32, f32> {
        let mut best: HashMap<u32, f32> = HashMap::from([(from, 0.0)]);
        let mut queue = BinaryHeap::from([Reverse((Order(0.0), from))]);
        while let Some(Reverse((Order(spent), node))) = queue.pop() {
            if best.get(&node).is_some_and(|&known| spent > known) {
                continue;
            }
            let ways = if backwards { self.arriving(node) } else { self.leaving(node) };
            for &way in ways {
                let way = self.ways[way as usize];
                let next = if backwards { way.from } else { way.to };
                let total = spent + seconds(&way);
                if total <= limit && best.get(&next).is_none_or(|&known| total < known) {
                    best.insert(next, total);
                    queue.push(Reverse((Order(total), next)));
                }
            }
        }
        best
    }

    /// The ways arriving at `node`.
    fn arriving(&self, node: u32) -> &[u32] {
        let (start, arriving) = self.arriving.get_or_init(|| {
            let mut start = vec![0u32; self.nodes.len() + 1];
            for way in &self.ways {
                start[way.to as usize + 1] += 1;
            }
            for n in 0..self.nodes.len() {
                start[n + 1] += start[n];
            }
            let mut next = start.clone();
            let mut arriving = vec![0u32; self.ways.len()];
            for (index, way) in self.ways.iter().enumerate() {
                arriving[next[way.to as usize] as usize] = index as u32;
                next[way.to as usize] += 1;
            }
            (start, arriving)
        });
        &arriving[start[node as usize] as usize..start[node as usize + 1] as usize]
    }

    /// How much longer (seconds) going from `from` to `to` takes by way of each of `stops`: there, and on
    /// from there, against straight there (`None`: a stop too far off to be worth it, or not reachable).
    pub fn detours(&self, from: u32, to: u32, stops: &[u32]) -> Vec<Option<f32>> {
        let Some(straight) = self.route(from, to, Plan::Fastest, &HashMap::new()).map(|route| route.seconds) else {
            return vec![None; stops.len()];
        };
        // A stop more than half the trip again out of the way isn't one to offer a time for.
        let limit = straight * 1.5;
        let (out, back) = (self.times(from, limit, false), self.times(to, limit, true));
        stops.iter().map(|stop| Some((out.get(stop)? + back.get(stop)? - straight).max(0.0))).collect()
    }
}

/// Routes sharing more than this of their length are the same route, as offered.
const SAME: f32 = 0.7;
/// How much dearer a road an offered route takes becomes, looking for another.
const DEARER: f32 = 1.4;
/// The longest another route may take, against the fastest.
const LONGEST: f32 = 1.4;
/// How far (metres, straight) another route may leave a motorway and come back onto one and still be planned
/// again for that stretch at the roads' own costs ([`Graph::straightened`]).
const HOP_WITHIN: f32 = 3000.0;

/// An `f32` ordered for the queue (costs are never NaN).
#[derive(Clone, Copy, PartialEq)]
struct Order(f32);

impl Eq for Order {}

impl PartialOrd for Order {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Order {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(uid: u64, x: f32, z: f32) -> Node {
        Node { uid, x, z }
    }

    fn road(from: u32, to: u32, item: u32, length: f32, class: RoadClass) -> [Way; 2] {
        let way = |from, to| Way { from, to, item, length, class, kind: Kind::Road, minutes: 0.0, invisible: false };
        [way(from, to), way(to, from)]
    }

    fn lane(from: u32, to: u32, item: u32) -> Way {
        Way {
            from,
            to,
            item,
            length: 20.0,
            class: RoadClass::Local,
            kind: Kind::Junction,
            minutes: 0.0,
            invisible: false,
        }
    }

    /// 0 —motorway (long way round, via 1)— 2, and 0 —local road (straight)— 2.
    fn two_ways() -> Graph {
        let nodes = vec![node(10, 0.0, 0.0), node(11, 5000.0, 4000.0), node(12, 10_000.0, 0.0)];
        let mut ways = Vec::new();
        ways.extend(road(0, 1, 0, 6400.0, RoadClass::Motorway));
        ways.extend(road(1, 2, 1, 6400.0, RoadClass::Motorway));
        ways.extend(road(0, 2, 2, 10_000.0, RoadClass::Local));
        Graph::new(nodes, ways)
    }

    #[test]
    fn plans_by_time_by_length_and_by_small_roads() {
        let graph = two_ways();
        let none = HashMap::new();
        let item = |route: &Route| route.ways.iter().map(|&w| graph.ways[w as usize].item).collect::<Vec<_>>();
        // The motorway: 12.8 km at 85 is quicker than 10 km at 55.
        assert_eq!(item(&graph.route(0, 2, Plan::Fastest, &none).unwrap()), [0, 1]);
        assert_eq!(item(&graph.route(0, 2, Plan::Shortest, &none).unwrap()), [2]);
        assert_eq!(item(&graph.route(0, 2, Plan::SmallRoads, &none).unwrap()), [2]);
        // Offered: the fastest first, then the other (shortest and small roads are one route).
        let offered = graph.routes(0, 2, 3);
        assert_eq!(offered.iter().map(|(plan, _)| *plan).collect::<Vec<_>>(), [Plan::Fastest, Plan::Shortest]);
    }

    #[test]
    fn another_route_never_leaves_a_motorway_only_to_come_back_onto_it() {
        // A motorway 5 → 0 → 1 → 2 → 6 (500 m pieces); beside its middle kilometre, an exit at 0 onto a local
        // road 3—4 (700 m) and back on at 2: slower than the motorway (51 s against 42 s), quicker than it made
        // dearer for another route (59 s).
        let nodes = vec![
            node(10, 0.0, 0.0),
            node(11, 500.0, 0.0),
            node(12, 1000.0, 0.0),
            node(13, 100.0, 300.0),
            node(14, 900.0, 300.0),
            node(15, -500.0, 0.0),
            node(16, 1500.0, 0.0),
        ];
        let mut ways = Vec::new();
        ways.extend(road(5, 0, 5, 500.0, RoadClass::Motorway));
        ways.extend(road(0, 1, 0, 500.0, RoadClass::Motorway));
        ways.extend(road(1, 2, 1, 500.0, RoadClass::Motorway));
        ways.extend(road(2, 6, 6, 500.0, RoadClass::Motorway));
        ways.push(lane(0, 3, 2));
        ways.extend(road(3, 4, 3, 700.0, RoadClass::Local));
        ways.push(lane(4, 2, 4));
        let graph = Graph::new(nodes, ways);
        let item = |route: &Route| route.ways.iter().map(|&w| graph.ways[w as usize].item).collect::<Vec<_>>();
        let fastest = graph.route(5, 6, Plan::Fastest, &HashMap::new()).unwrap();
        assert_eq!(item(&fastest), [5, 0, 1, 6]);
        // The motorway made dearer: the search hops off and back on…
        let dearer = fastest.ways.iter().map(|&way| (way, DEARER)).collect();
        let dodging = graph.route(5, 6, Plan::Fastest, &dearer).unwrap();
        assert_eq!(item(&dodging), [5, 2, 3, 4, 6]);
        // …and straightened, it stays on.
        assert_eq!(graph.straightened(dodging), fastest);
        // So no other route is offered for it.
        assert!(graph.routes(5, 6, 3).iter().all(|(plan, _)| *plan != Plan::Other));
    }

    #[test]
    fn takes_only_the_turns_a_junction_has_lanes_for_and_never_turns_back_on_a_road() {
        // A road 0—1 into a junction at 1 whose only lane goes on to 2; 2—3 a road on. 1—4 is a road the
        // junction has no lane to (a one-way street out of it).
        let nodes = (0..5).map(|n| node(n, n as f32 * 100.0, 0.0)).collect();
        let mut ways = Vec::new();
        ways.extend(road(0, 1, 0, 100.0, RoadClass::Local));
        ways.push(lane(1, 2, 1));
        ways.extend(road(2, 3, 2, 100.0, RoadClass::Local));
        ways.extend(road(1, 4, 3, 50.0, RoadClass::Local));
        let graph = Graph::new(nodes, ways);
        let none = HashMap::new();
        assert!(graph.route(0, 3, Plan::Shortest, &none).is_some());
        // From 3 back to 0 there's no lane through the junction the other way.
        assert!(graph.route(3, 0, Plan::Shortest, &none).is_none());
        // 0 to 4 is straight along at the junction's node: allowed (no junction lane needed between roads).
        assert!(graph.route(0, 4, Plan::Shortest, &none).is_some());
        // Nor can a route turn round on a road to come back: 0 to 1 and back to 0.
        let there_and_back = graph.route(1, 0, Plan::Shortest, &none).unwrap();
        assert_eq!(there_and_back.ways.len(), 1);
    }

    #[test]
    fn a_crossing_is_taken_by_its_time() {
        let nodes = vec![node(1, 0.0, 0.0), node(2, 1000.0, 0.0), node(3, 0.0, 30_000.0), node(4, 1000.0, 30_000.0)];
        let mut ways = Vec::new();
        ways.extend(road(0, 1, 0, 1000.0, RoadClass::Local));
        ways.push(Way {
            from: 1,
            to: 3,
            item: 9,
            length: 0.0,
            class: RoadClass::Local,
            kind: Kind::Ferry,
            minutes: 90.0,
            invisible: false,
        });
        ways.extend(road(3, 2, 1, 1000.0, RoadClass::Local));
        let graph = Graph::new(nodes, ways);
        let route = graph.route(0, 2, Plan::Fastest, &HashMap::new()).unwrap();
        assert_eq!(route.ways.len(), 3);
        assert!((route.seconds - (1000.0 / (55.0 / 3.6) * 2.0 + 90.0 * 60.0 / GAME_SCALE)).abs() < 0.1);
    }

    #[test]
    fn a_stop_beside_the_way_adds_little_one_off_it_more() {
        // A road 0 → 1 → 2 east, 1 km each, and a side road 1 → 3 (2 km off and back).
        let nodes = vec![node(1, 0.0, 0.0), node(2, 1000.0, 0.0), node(3, 2000.0, 0.0), node(4, 1000.0, 2000.0)];
        let mut ways = Vec::new();
        ways.extend(road(0, 1, 0, 1000.0, RoadClass::Local));
        ways.extend(road(1, 2, 1, 1000.0, RoadClass::Local));
        ways.extend(road(1, 3, 2, 2000.0, RoadClass::Local));
        let graph = Graph::new(nodes, ways);
        let detours = graph.detours(0, 2, &[1, 3]);
        assert_eq!(detours[0], Some(0.0));
        // There and back: 4 km more at 55 km/h.
        assert!((detours[1].unwrap() - 4000.0 / (55.0 / 3.6)).abs() < 0.5);
    }

    #[test]
    fn its_file_reads_back_the_same() {
        let graph = two_ways();
        let mut bytes = Vec::new();
        graph.write(&mut bytes).unwrap();
        let read = Graph::read(&mut bytes.as_slice()).unwrap();
        assert_eq!(read.nodes, graph.nodes);
        assert_eq!(read.ways, graph.ways);
        assert_eq!(read.node(12), Some(2));
        assert!(Graph::read(&mut &b"WPRG\x09\x00\x00\x00"[..]).is_err());
        // An invisible road reads back as one; a graph written before the mark reads as all seen.
        let mut hidden = two_ways();
        hidden.ways[0].invisible = true;
        let mut bytes = Vec::new();
        hidden.write(&mut bytes).unwrap();
        assert_eq!(Graph::read(&mut bytes.as_slice()).unwrap().ways, hidden.ways);
    }

    #[test]
    fn a_waypoint_goes_only_on_road_pieces() {
        // West to east: road 0-1, road 1-2, a junction's lanes 2 → 3 (in) and 4 → 5 (out of another), road
        // 5-6, road 6-7, an invisible road 7-8, road 8-9.
        let nodes = (0..10).map(|n| node(100 + n, n as f32 * 100.0, 0.0)).collect();
        let mut ways = Vec::new();
        ways.extend(road(0, 1, 0, 100.0, RoadClass::Local));
        ways.extend(road(1, 2, 1, 100.0, RoadClass::Local));
        ways.push(lane(2, 3, 2));
        ways.push(lane(4, 5, 3));
        ways.extend(road(5, 6, 4, 100.0, RoadClass::Local));
        ways.extend(road(6, 7, 5, 100.0, RoadClass::Local));
        let mut invisible = road(7, 8, 6, 100.0, RoadClass::Minor);
        for way in &mut invisible {
            way.invisible = true;
        }
        ways.extend(invisible);
        ways.extend(road(8, 9, 7, 100.0, RoadClass::Local));
        let graph = Graph::new(nodes, ways);
        // Between two roads: yes.
        assert!(graph.road_piece(1) && graph.road_piece(6));
        // A junction's way in (a lane leaves it), and its way out (a lane arrives, only the road leaves): no.
        assert!(!graph.road_piece(2) && !graph.road_piece(5));
        // Beside an invisible road, either side: no; inside a junction: no.
        assert!(!graph.road_piece(7) && !graph.road_piece(8));
        assert!(!graph.road_piece(3));
    }
}
