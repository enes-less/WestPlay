//! WestPlay's own route planner for the page (PLAN.md: Navigation plan, B): the routes to a place, offered
//! before Go (Maps' Directions). Planned on a thread of its own from the shown map's road graph
//! (westplay_mapdata::routing, `route-graph.bin` beside its tiles), read once per map; the page's request
//! `id` comes back with the routes. Each route's map nodes are kept, for handing the one picked to the
//! game's GPS.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};

use westplay_mapdata::routing::{self, Graph, Kind, Plan};

use crate::handover::Hold;

/// The most routes offered.
const ROUTES: usize = 3;
/// How far from the truck a route may start (metres): the nearest node ahead of it within this.
const START_WITHIN: f32 = 150.0;
/// How far from a road's line the truck may be and still be on it (metres): a motorway's carriageways and
/// the gap between them.
const ON_ROAD: f32 = 30.0;
/// How far a way the truck is on may turn from its heading: the cosine of 60° (a curve's chord, not a road
/// across it).
const ON_ROAD_TURN: f32 = 0.5;
/// How many places a route is tried from, the likeliest first, before none is found.
const STARTS_TRIED: usize = 4;

/// A request: where the truck is and faces, and the map node to go to; with `stops`, what stopping at each
/// on the way there would add instead (Maps' stops along the route).
struct Ask {
    id: u32,
    graph: PathBuf,
    x: f32,
    z: f32,
    /// The way the truck faces, on the ground (x east, z south).
    facing: [f32; 2],
    to: u64,
    stops: Option<Vec<u64>>,
}

/// The routes planned for a request, each as its map nodes (in order) and for the page.
pub struct Planned {
    pub id: u32,
    pub routes: Vec<Vec<u64>>,
    /// For each route's nodes, where a waypoint can hold the game to it (handover.rs), and where each is
    /// (metres: x east, z south; for the log).
    pub roads: Vec<Vec<Hold>>,
    pub positions: Vec<Vec<[f32; 2]>>,
    pub message: String,
}

pub struct Planner {
    asks: Sender<Ask>,
    planned: Receiver<Planned>,
    /// Detours worked out, for the page.
    detoured: Receiver<String>,
    /// The last request's routes, by their place in its list: the one picked is handed to the game.
    pub last: Option<Planned>,
}

impl Planner {
    pub fn start() -> Self {
        let (asks, asked) = mpsc::channel::<Ask>();
        let (answer, planned) = mpsc::channel();
        let (detour_answer, detoured) = mpsc::channel();
        std::thread::spawn(move || {
            let mut loaded: Option<(PathBuf, Arc<Graph>)> = None;
            while let Ok(first) = asked.recv() {
                // Only the newest of each kind: the page asks again as the player picks another place.
                let (mut routes, mut stops) = (None, None);
                for ask in std::iter::once(first).chain(asked.try_iter()) {
                    if ask.stops.is_some() { stops = Some(ask) } else { routes = Some(ask) }
                }
                for ask in routes.into_iter().chain(stops) {
                    if loaded.as_ref().is_none_or(|(path, _)| *path != ask.graph) {
                        loaded = read(&ask.graph).map(|graph| (ask.graph.clone(), Arc::new(graph)));
                    }
                    let sent = match &ask.stops {
                        Some(stops) => {
                            let seconds =
                                loaded.as_ref().map(|(_, graph)| detours(graph, &ask, stops)).unwrap_or_default();
                            let message = serde_json::json!({ "type": "detours", "id": ask.id, "seconds": seconds });
                            detour_answer.send(message.to_string()).is_ok()
                        }
                        None => {
                            let planned = match &loaded {
                                Some((_, graph)) => plan(graph, &ask),
                                None => Planned {
                                    id: ask.id,
                                    routes: Vec::new(),
                                    roads: Vec::new(),
                                    positions: Vec::new(),
                                    message: routes_message(ask.id, None),
                                },
                            };
                            answer.send(planned).is_ok()
                        }
                    };
                    if !sent {
                        return;
                    }
                }
            }
        });
        Self { asks, planned, detoured, last: None }
    }

    /// Asks for the routes from the truck (at `x`, `z`, facing `heading`, the game's turns) to the map node
    /// `to`, on the map in `maps`/`folder`.
    pub fn ask(&self, id: u32, maps: &Path, folder: &str, (x, z, heading): (f32, f32, f32), to: u64) {
        let Some(graph) = graph_of(&maps.join(folder)) else {
            crate::log!("routes: the map {folder} has no road graph (built before WestPlay had one)");
            return;
        };
        let turn = heading * std::f32::consts::TAU;
        let facing = [-turn.sin(), -turn.cos()];
        let _ = self.asks.send(Ask { id, graph, x, z, facing, to, stops: None });
    }

    /// Asks what stopping at each of `stops` (map nodes) on the way from the truck to `to` would add.
    pub fn ask_detours(
        &self,
        id: u32,
        maps: &Path,
        folder: &str,
        (x, z, heading): (f32, f32, f32),
        to: u64,
        stops: Vec<u64>,
    ) {
        let Some(graph) = graph_of(&maps.join(folder)) else { return };
        let turn = heading * std::f32::consts::TAU;
        let facing = [-turn.sin(), -turn.cos()];
        let _ = self.asks.send(Ask { id, graph, x, z, facing, to, stops: Some(stops) });
    }

    /// Detours worked out since last asked, for the page.
    pub fn detours(&self) -> Option<String> {
        self.detoured.try_iter().last()
    }

    /// Routes planned since last asked, for the page.
    pub fn answer(&mut self) -> Option<String> {
        let planned = self.planned.try_iter().last()?;
        let message = planned.message.clone();
        self.last = Some(planned);
        Some(message)
    }
}

/// The road graph beside the map in `folder` (the tiles its `map.json` names).
fn graph_of(folder: &Path) -> Option<PathBuf> {
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(folder.join("map.json")).ok()?).ok()?;
    let path = folder.join(manifest.get("tiles")?.as_str()?).join(routing::FILE);
    path.is_file().then_some(path)
}

fn read(path: &Path) -> Option<Graph> {
    let started = std::time::Instant::now();
    let file = std::fs::File::open(path).ok()?;
    match Graph::read(&mut std::io::BufReader::new(file)) {
        Ok(graph) => {
            crate::log!("routes: road graph read in {:.1?}: {} nodes", started.elapsed(), graph.nodes.len());
            Some(graph)
        }
        Err(e) => {
            crate::log!("routes: road graph unreadable: {e}");
            None
        }
    }
}

fn plan(graph: &Graph, ask: &Ask) -> Planned {
    let empty = |id| Planned {
        id,
        routes: Vec::new(),
        roads: Vec::new(),
        positions: Vec::new(),
        message: routes_message(id, None),
    };
    let Some(to) = graph.node(ask.to) else { return empty(ask.id) };
    let started = std::time::Instant::now();
    // From the likeliest start on: one cut off from the roads (a hidden road over the motorway, owner
    // 2026-10-22: no routes at all) reaches nothing, and the next is tried.
    let offered = starts(graph, ask)
        .into_iter()
        .take(STARTS_TRIED)
        .map(|from| graph.routes(from, to, ROUTES))
        .find(|offered| !offered.is_empty())
        .unwrap_or_default();
    crate::log!("routes: {} planned in {:.1?}", offered.len(), started.elapsed());
    let routes: Vec<Vec<u64>> = offered.iter().map(|(_, route)| nodes_of(graph, route)).collect();
    let hold = |uid: u64| match graph.node(uid) {
        Some(n) if graph.road_piece(n) => Hold::Road,
        _ => Hold::No,
    };
    let roads = routes.iter().map(|nodes| nodes.iter().map(|&uid| hold(uid)).collect()).collect();
    let positions = routes
        .iter()
        .map(|nodes| {
            nodes
                .iter()
                .map(|&uid| {
                    graph.node(uid).map_or([0.0; 2], |n| [graph.nodes[n as usize].x, graph.nodes[n as usize].z])
                })
                .collect()
        })
        .collect();
    let message = routes_message(ask.id, Some((graph, &offered)));
    Planned { id: ask.id, routes, roads, positions, message }
}

/// What stopping at each of `stops` adds to the trip (seconds on the map's ground; `None`: unknown).
fn detours(graph: &Graph, ask: &Ask, stops: &[u64]) -> Vec<Option<f32>> {
    let (Some(from), Some(to)) = (starts(graph, ask).first().copied(), graph.node(ask.to)) else {
        return vec![None; stops.len()];
    };
    let nodes: Vec<Option<u32>> = stops.iter().map(|&uid| graph.node(uid)).collect();
    let known: Vec<u32> = nodes.iter().flatten().copied().collect();
    let mut found = graph.detours(from, to, &known).into_iter();
    nodes.iter().map(|node| node.and_then(|_| found.next().flatten())).collect()
}

/// Where a route may start, the likeliest first: the end of each way the truck is on, going its way, the
/// nearest road first (owner 2026-10-22: the nearest node ahead was a hidden road's over the motorway, 58 m
/// off, the motorway's own 238 m on); then the nodes within reach a way leaves, those ahead of it first,
/// each nearest first; then the nearest.
fn starts(graph: &Graph, ask: &Ask) -> Vec<u32> {
    let ahead = |x: f32, z: f32| (x - ask.x) * ask.facing[0] + (z - ask.z) * ask.facing[1] > 0.0;
    let mut on: Vec<(u32, f32)> = graph
        .ways
        .iter()
        .filter(|way| way.kind != Kind::Ferry)
        .filter_map(|way| {
            let (a, b) = (graph.nodes[way.from as usize], graph.nodes[way.to as usize]);
            let (dx, dz) = (b.x - a.x, b.z - a.z);
            // Going the truck's way (within ON_ROAD_TURN of its heading: not a road across it), its end
            // still ahead.
            let length = dx.hypot(dz);
            if dx * ask.facing[0] + dz * ask.facing[1] < ON_ROAD_TURN * length || !ahead(b.x, b.z) {
                return None;
            }
            let along =
                (((ask.x - a.x) * dx + (ask.z - a.z) * dz) / (dx * dx + dz * dz).max(f32::EPSILON)).clamp(0.0, 1.0);
            let off = (a.x + along * dx - ask.x).hypot(a.z + along * dz - ask.z);
            (off <= ON_ROAD).then_some((way.to, off))
        })
        .collect();
    on.sort_by(|a, b| a.1.total_cmp(&b.1));
    // Off every road (a yard, a parking): the nodes near, ahead first.
    let leaves: std::collections::HashSet<u32> = graph.ways.iter().map(|way| way.from).collect();
    let mut near: Vec<(u32, bool, f32)> = (0..graph.nodes.len() as u32)
        .filter(|n| leaves.contains(n))
        .filter_map(|n| {
            let node = graph.nodes[n as usize];
            let distance = (node.x - ask.x).hypot(node.z - ask.z);
            (distance <= START_WITHIN).then_some((n, !ahead(node.x, node.z), distance))
        })
        .collect();
    near.sort_by(|a, b| a.1.cmp(&b.1).then(a.2.total_cmp(&b.2)));
    let mut starts: Vec<u32> = Vec::new();
    let near = near.into_iter().map(|(n, _, distance)| (n, distance));
    for n in on.into_iter().chain(near).map(|(n, _)| n).chain(graph.nearest(ask.x, ask.z)) {
        if !starts.contains(&n) {
            starts.push(n);
        }
    }
    starts
}

/// A route's map nodes, in order.
fn nodes_of(graph: &Graph, route: &routing::Route) -> Vec<u64> {
    let mut nodes = Vec::with_capacity(route.ways.len() + 1);
    if let Some(&first) = route.ways.first() {
        nodes.push(graph.nodes[graph.ways[first as usize].from as usize].uid);
    }
    nodes.extend(route.ways.iter().map(|&way| graph.nodes[graph.ways[way as usize].to as usize].uid));
    nodes
}

/// `{"type":"routes","id":3,"routes":[{"plan":"fastest","length":…,"seconds":…,"line":[[x,z],…]}]}` (its
/// line through its nodes, metres; length in metres and time in seconds on the map's ground: the page
/// gives them in the game's scale).
fn routes_message(id: u32, planned: Option<(&Graph, &Vec<(Plan, routing::Route)>)>) -> String {
    let routes: Vec<serde_json::Value> = planned
        .map(|(graph, offered)| {
            offered
                .iter()
                .map(|(plan, route)| {
                    let mut line: Vec<[f32; 2]> = Vec::new();
                    if let Some(&first) = route.ways.first() {
                        let node = graph.nodes[graph.ways[first as usize].from as usize];
                        line.push([node.x, node.z]);
                    }
                    for &way in &route.ways {
                        let node = graph.nodes[graph.ways[way as usize].to as usize];
                        line.push([node.x, node.z]);
                    }
                    serde_json::json!({
                        "plan": plan_name(*plan),
                        "length": route.length,
                        "seconds": route.seconds,
                        "line": line,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    serde_json::json!({ "type": "routes", "id": id, "routes": routes }).to_string()
}

fn plan_name(plan: Plan) -> &'static str {
    match plan {
        Plan::Fastest => "fastest",
        Plan::Shortest => "shortest",
        Plan::SmallRoads => "smallRoads",
        Plan::Other => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use westplay_mapdata::routing::{Kind, Node, Way};
    use westplay_mapdata::world::RoadClass;

    #[test]
    fn a_route_starts_ahead_of_the_truck_and_comes_as_its_nodes() {
        // A road west to east: 0 at x=0, 1 at x=100, 2 at x=200. The truck at x=90 facing east.
        let nodes = (0..3).map(|n| Node { uid: 100 + n, x: n as f32 * 100.0, z: 0.0 }).collect();
        let road = |from, to, item| Way {
            from,
            to,
            item,
            length: 100.0,
            class: RoadClass::Local,
            kind: Kind::Road,
            minutes: 0.0,
            invisible: false,
        };
        let graph = Graph::new(nodes, vec![road(0, 1, 0), road(1, 0, 0), road(1, 2, 1), road(2, 1, 1)]);
        // East: heading 0.75 turns (0 north, counter-clockwise).
        let turn = 0.75 * std::f32::consts::TAU;
        let ask = Ask {
            id: 1,
            graph: PathBuf::new(),
            x: 90.0,
            z: 0.0,
            facing: [-turn.sin(), -turn.cos()],
            to: 102,
            stops: None,
        };
        assert_eq!(starts(&graph, &ask).first(), Some(&1));
        let planned = plan(&graph, &ask);
        assert_eq!(planned.routes, vec![vec![101, 102]]);
        let message: serde_json::Value = serde_json::from_str(&planned.message).unwrap();
        assert_eq!(message["routes"][0]["plan"], "fastest");
        assert_eq!(message["routes"][0]["line"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn a_route_starts_on_the_road_the_truck_is_on_not_one_over_it() {
        // A motorway west to east, 0 at x=0 and 1 at x=400; a road over it, north to south, 2 at z=-200 and 3
        // at z=200 (x=50), cut off from it; the destination 4 on the motorway at x=800. The truck at x=10 on
        // the motorway, facing east: the overpass's node 3 (51 m) is nearer than the motorway's 1 (390 m).
        let nodes = vec![
            Node { uid: 100, x: 0.0, z: 0.0 },
            Node { uid: 101, x: 400.0, z: 0.0 },
            Node { uid: 102, x: 50.0, z: -200.0 },
            Node { uid: 103, x: 50.0, z: 50.0 },
            Node { uid: 104, x: 800.0, z: 0.0 },
        ];
        let road = |from, to, item, length| Way {
            from,
            to,
            item,
            length,
            class: RoadClass::Local,
            kind: Kind::Road,
            minutes: 0.0,
            invisible: false,
        };
        let ways = vec![
            road(0, 1, 0, 400.0),
            road(1, 0, 0, 400.0),
            road(1, 4, 1, 400.0),
            road(4, 1, 1, 400.0),
            road(2, 3, 2, 250.0),
            road(3, 2, 2, 250.0),
        ];
        let graph = Graph::new(nodes, ways);
        let turn = 0.75 * std::f32::consts::TAU;
        let ask = Ask {
            id: 1,
            graph: PathBuf::new(),
            x: 10.0,
            z: 0.0,
            facing: [-turn.sin(), -turn.cos()],
            to: 104,
            stops: None,
        };
        assert_eq!(starts(&graph, &ask).first(), Some(&1));
        assert_eq!(plan(&graph, &ask).routes, vec![vec![101, 104]]);
        // Off every road (a yard beside the overpass's node 3): the nodes near, nearest first; 3 reaches
        // nothing, so the route comes from the next, the motorway's 0.
        let ask = Ask { x: 50.0, z: 40.0, ..ask };
        assert_eq!(starts(&graph, &ask)[..2], [3, 0]);
        assert_eq!(plan(&graph, &ask).routes, vec![vec![100, 101, 104]]);
    }

    /// A route as `examples/route_dump` writes the game's: after `route <n> …` and `waypoints …`, a node a
    /// line (uid in hex, x, z).
    fn recorded_route(file: &str, number: u32) -> Vec<(u64, f32, f32)> {
        let text = std::fs::read_to_string(file).expect("the recorded routes");
        let mut lines = text.lines().skip_while(|line| !line.starts_with(&format!("route {number} "))).skip(2);
        let mut nodes = Vec::new();
        for line in lines.by_ref() {
            if line.starts_with("route ") {
                break;
            }
            let mut words = line.split_whitespace();
            let (Some(uid), Some(x), Some(z)) = (words.next(), words.next(), words.next()) else { continue };
            nodes.push((u64::from_str_radix(uid, 16).unwrap(), x.parse().unwrap(), z.parse().unwrap()));
        }
        nodes
    }

    /// Replays a Go on a player's map against the route their game planned (owner 2026-10-22: a border
    /// crossing, and no routes at all on a toll plaza's approach). `WESTPLAY_GRAPH`: the map's
    /// `route-graph.bin`; `WESTPLAY_ROUTES`: the game's routes as `examples/route_dump` wrote them;
    /// `WESTPLAY_CASE`: `<route number> <truck x> <truck z> <destination uid hex>`, the truck heading along
    /// the game's route from it.
    #[test]
    #[ignore = "reads a player's map and recorded routes"]
    fn replays_a_go_on_a_players_map() {
        let var = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("{name}"));
        let graph = read(Path::new(&var("WESTPLAY_GRAPH"))).expect("the graph");
        let case = var("WESTPLAY_CASE");
        let words: Vec<&str> = case.split_whitespace().collect();
        let number: u32 = words[0].parse().unwrap();
        let (x, z): (f32, f32) = (words[1].parse().unwrap(), words[2].parse().unwrap());
        let to = u64::from_str_radix(words[3], 16).unwrap();
        let game = recorded_route(&var("WESTPLAY_ROUTES"), number);
        // The truck's heading: along the game's route from its first node to one 50 m on.
        let first = game[0];
        let on = game.iter().find(|node| (node.1 - first.1).hypot(node.2 - first.2) > 50.0).unwrap();
        let length = (on.1 - first.1).hypot(on.2 - first.2);
        let facing = [(on.1 - first.1) / length, (on.2 - first.2) / length];
        let ask = Ask { id: 1, graph: PathBuf::new(), x, z, facing, to, stops: None };
        for &n in starts(&graph, &ask).iter().take(3) {
            let node = graph.nodes[n as usize];
            println!("start candidate {:x} ({:.0}, {:.0})", node.uid, node.x, node.z);
        }
        let planned = plan(&graph, &ask);
        println!("{} routes", planned.routes.len());
        assert!(!planned.routes.is_empty(), "no routes");
        // Handed over: the game's route against the fastest, step by step as the game would answer, its
        // route as recorded (no waypoints: what it plans for the destination alone).
        let mut handover =
            crate::handover::Handover::new(planned.routes[0].clone(), planned.roads[0].clone(), &planned.positions[0]);
        let game: Vec<u64> = game.iter().map(|node| node.0).collect();
        let step = handover.read(&game);
        println!("first step: {step:?}");
        // The route picked from a little before where the game leaves it.
        if let crate::handover::Step::Hold { parts, .. } = step {
            for at in parts.saturating_sub(4)..planned.routes[0].len() {
                let uid = planned.routes[0][at];
                let n = graph.node(uid).unwrap();
                let node = graph.nodes[n as usize];
                let taken = if game.contains(&uid) { "game's too" } else { "" };
                println!("  picked {at}: {uid:x} ({:.0}, {:.0}) {:?} {taken}", node.x, node.z, planned.roads[0][at]);
            }
        }
        for &via in handover.handed() {
            let n = graph.node(via).unwrap();
            let node = graph.nodes[n as usize];
            println!("waypoint {via:x} ({:.0}, {:.0}): road piece {}", node.x, node.z, graph.road_piece(n));
            assert!(graph.road_piece(n), "a waypoint off a road piece");
        }
    }
}
