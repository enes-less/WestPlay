//! How a place is reached. The game's GPS and WestPlay's routes take a place as a map node, and that has to be a node a route
//! gets to. What a service area offers is reached through it (owner 2026-10-31: navigation has to take you through what the
//! map marks as a service area): a route follows the lane that runs nearest the place to whichever of its ends it comes to
//! last, so that it passes it. The lane may be the place's prefab's own, from its way in to its way out; the game's invisible
//! and private ones over the area's ground (a filling station's pumps can be islands on the paving, joined to each other and
//! to nothing else, with those lanes running past them); or a road whose surface meets that ground, whose nearest node may be
//! far from the place though it runs right by. Which end is the far one the map can't always say: beside a two-way road it's
//! the truck's way there that decides, so a place has both ([`WayIn`]) and the route planned picks.

use std::collections::{HashMap, HashSet};

use crate::geometry::{self, Point};
use crate::routing::Graph;
use crate::sector::ItemData;
use crate::surface::service::Area;
use crate::world::{Definitions, Map, RoadClass};

/// The fewest nodes that are a road network a truck can be on, rather than a bit of road cut off from the rest.
const NETWORK: usize = 200;
/// The cells lanes and service areas are looked up by, m.
const CELL: f32 = 64.0;
/// How far from where a service area offers fuel or rest a place can be that area's, m: a station's icon is the middle of
/// its pumps.
const IN_AREA: f32 = 64.0;

/// A lane of the game's own that isn't a road on the map: an invisible road, a private one.
pub struct Lane {
    pub ends: [u64; 2],
    pub line: Vec<Point>,
}

/// The node a route to a place takes, and the other end of the lane that passes the place, if it has one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WayIn {
    pub node: u64,
    pub also: Option<u64>,
}

/// A prefab's way through from one of its ends to another, as the game's GPS takes it.
pub struct Through {
    pub from: u64,
    pub to: u64,
    pub line: Vec<Point>,
}

/// A service area's ground, ready to ask.
struct Ground {
    points: Vec<Point>,
    rings: Vec<Vec<Point>>,
    bounds: [f32; 4],
    /// The roads that meet it.
    roads: Vec<Lane>,
}

impl Ground {
    fn new(area: &Area, road: &impl Fn(u32) -> Option<Lane>) -> Self {
        let rings: Vec<Vec<Point>> = area.rings.iter().map(|ring| ring.iter().map(|p| [p[0] as f32, p[2] as f32]).collect()).collect();
        let mut bounds = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for p in rings.iter().flatten() {
            bounds = [bounds[0].min(p[0]), bounds[1].min(p[1]), bounds[2].max(p[0]), bounds[3].max(p[1])];
        }
        let roads = area.roads.iter().filter_map(|&item| road(item)).collect();
        Self { points: area.points.iter().map(|p| [p[0] as f32, p[1] as f32]).collect(), rings, bounds, roads }
    }

    /// Whether `at` is on it.
    fn holds(&self, at: Point) -> bool {
        let [x0, z0, x1, z1] = self.bounds;
        if at[0] < x0 || at[0] > x1 || at[1] < z0 || at[1] > z1 {
            return false;
        }
        let mut inside = false;
        for ring in &self.rings {
            for i in 0..ring.len() {
                let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
                if (a[1] > at[1]) != (b[1] > at[1]) && at[0] < a[0] + (at[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
                    inside = !inside;
                }
            }
        }
        inside
    }

    /// How far `at` is from where it offers fuel or rest.
    fn off(&self, at: Point) -> f32 {
        self.points.iter().map(|p| distance(*p, at)).fold(f32::MAX, f32::min)
    }
}

fn distance(a: Point, b: Point) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// How near `line` comes to `at`.
fn nearest_on(line: &[Point], at: Point) -> f32 {
    line.iter().map(|p| distance(*p, at)).fold(f32::MAX, f32::min)
}

fn cell(at: Point) -> (i32, i32) {
    ((at[0] / CELL).floor() as i32, (at[1] / CELL).floor() as i32)
}

/// What a route gets to, and the lanes through the service areas.
pub struct Access<'a> {
    graph: &'a Graph,
    /// How long from the road network it's on to each node one gets to, seconds.
    times: HashMap<u32, f32>,
    lanes: Vec<Lane>,
    lanes_in: HashMap<(i32, i32), Vec<u32>>,
    grounds: Vec<Ground>,
    grounds_in: HashMap<(i32, i32), Vec<u32>>,
}

impl<'a> Access<'a> {
    pub fn new(map: &Map, definitions: &Definitions, graph: &'a Graph, areas: &[Area]) -> Self {
        let mut lanes = Vec::new();
        for item in &map.items {
            let ItemData::Road { flags, look, start_node, end_node, .. } = &item.data else { continue };
            let Some(look) = definitions.road_looks.get(look) else { continue };
            if flags.secret || look.class == RoadClass::Rail || !(look.invisible || look.no_vehicles) {
                continue;
            }
            let (Some(start), Some(end)) = (map.nodes.get(start_node), map.nodes.get(end_node)) else { continue };
            lanes.push(Lane { ends: [*start_node, *end_node], line: geometry::road_curve(start, end) });
        }
        let road = |item: u32| {
            let ItemData::Road { start_node, end_node, .. } = &map.items.get(item as usize)?.data else { return None };
            let (start, end) = (map.nodes.get(start_node)?, map.nodes.get(end_node)?);
            Some(Lane { ends: [*start_node, *end_node], line: geometry::road_curve(start, end) })
        };
        Self::of(graph, NETWORK, lanes, areas, &road)
    }

    fn of(graph: &'a Graph, network: usize, lanes: Vec<Lane>, areas: &[Area], road: &impl Fn(u32) -> Option<Lane>) -> Self {
        let mut times: HashMap<u32, f32> = HashMap::new();
        for hub in graph.networks(network) {
            for (node, time) in graph.times(hub, f32::MAX, false) {
                times.entry(node).or_insert(time);
            }
        }
        let mut lanes_in: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for (index, lane) in lanes.iter().enumerate() {
            let cells: HashSet<(i32, i32)> = lane.line.iter().map(|p| cell(*p)).collect();
            for at in cells {
                lanes_in.entry(at).or_default().push(index as u32);
            }
        }
        let grounds: Vec<Ground> = areas.iter().map(|area| Ground::new(area, road)).collect();
        let mut grounds_in: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for (index, ground) in grounds.iter().enumerate() {
            let (x0, z0) = cell([ground.bounds[0], ground.bounds[1]]);
            let (x1, z1) = cell([ground.bounds[2], ground.bounds[3]]);
            for at in (x0..=x1).flat_map(|x| (z0..=z1).map(move |z| (x, z))) {
                grounds_in.entry(at).or_default().push(index as u32);
            }
        }
        Self { graph, times, lanes, lanes_in, grounds, grounds_in }
    }

    /// How long from its road network to the map node `uid`, seconds, if a route gets there.
    fn time(&self, uid: u64) -> Option<f32> {
        self.times.get(&self.graph.node(uid)?).copied()
    }

    /// Whether a route gets to the map node `uid` from a road network.
    pub fn reaches(&self, uid: u64) -> bool {
        self.time(uid).is_some()
    }

    /// The service area `at` is in.
    fn ground_at(&self, at: Point) -> Option<&Ground> {
        let (x, z) = cell(at);
        let reach = (IN_AREA / CELL).ceil() as i32;
        let mut near: Vec<u32> = (x - reach..=x + reach)
            .flat_map(|x| (z - reach..=z + reach).map(move |z| (x, z)))
            .flat_map(|at| self.grounds_in.get(&at).into_iter().flatten().copied())
            .collect();
        near.sort_unstable();
        near.dedup();
        near.iter()
            .map(|&ground| &self.grounds[ground as usize])
            .map(|ground| (ground, ground.off(at)))
            .filter(|&(ground, off)| off <= IN_AREA || ground.holds(at))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(ground, _)| ground)
    }

    /// The way a route takes to `at`, which a service area offers: along the lane that runs nearest it, of the prefab it's in
    /// (`through`) or over its ground; or to where the prefab joins the road (`ends`, those a route gets to), if that's nearer
    /// still.
    pub fn way_in(&self, at: Point, ends: &[(u64, Point)], through: &[Through]) -> Option<WayIn> {
        let own = through
            .iter()
            .filter(|lane| self.reaches(lane.from) && self.reaches(lane.to))
            .map(|lane| (nearest_on(&lane.line, at), WayIn { node: lane.to, also: Some(lane.from) }));
        let joined = ends.iter().map(|&(node, there)| (distance(at, there), WayIn { node, also: None }));
        own.chain(joined).chain(self.lane_past(at)).min_by(|a, b| a.0.total_cmp(&b.0)).map(|(_, way)| way)
    }

    /// How near the lane that runs nearest `at` over its service area's ground, or the road that does of those meeting it,
    /// comes, a route getting to it, and its ends, the one a route from its road network comes to last first. `None`: `at`
    /// isn't in a service area, or nothing runs by.
    pub fn lane_past(&self, at: Point) -> Option<(f32, WayIn)> {
        let ground = self.ground_at(at)?;
        let (x0, z0) = cell([ground.bounds[0], ground.bounds[1]]);
        let (x1, z1) = cell([ground.bounds[2], ground.bounds[3]]);
        let mut lanes: Vec<u32> = (x0..=x1)
            .flat_map(|x| (z0..=z1).map(move |z| (x, z)))
            .flat_map(|at| self.lanes_in.get(&at).into_iter().flatten().copied())
            .collect();
        lanes.sort_unstable();
        lanes.dedup();
        let mut on_it: Vec<(f32, &Lane)> = lanes
            .iter()
            .map(|&lane| &self.lanes[lane as usize])
            .filter_map(|lane| {
                let nearest = lane.line.iter().filter(|p| ground.holds(**p)).map(|p| distance(*p, at)).min_by(f32::total_cmp)?;
                Some((nearest, lane))
            })
            .collect();
        on_it.extend(ground.roads.iter().map(|road| (nearest_on(&road.line, at), road)));
        on_it.sort_by(|a, b| a.0.total_cmp(&b.0));
        on_it.iter().find_map(|&(nearest, lane)| {
            let [start, end] = lane.ends;
            let way = match (self.time(start), self.time(end)) {
                (Some(a), Some(b)) if b >= a => WayIn { node: end, also: Some(start) },
                (Some(_), Some(_)) => WayIn { node: start, also: Some(end) },
                (Some(_), None) => WayIn { node: start, also: None },
                (None, Some(_)) => WayIn { node: end, also: None },
                (None, None) => return None,
            };
            Some((nearest, way))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::{Kind, Node, Way};

    fn road(from: u32, to: u32, item: u32, class: RoadClass, invisible: bool) -> [Way; 2] {
        let length = 100.0;
        let way = |from, to| Way { from, to, item, length, class, kind: Kind::Road, minutes: 0.0, invisible, private: false };
        [way(from, to), way(to, from)]
    }

    fn lane(from: u32, to: u32, item: u32) -> Way {
        Way { from, to, item, length: 100.0, class: RoadClass::Local, kind: Kind::Junction, minutes: 0.0, invisible: false, private: false }
    }

    /// A motorway east 0 → 1 → 2 and round again, its exit 1 → 3, a lane 3 — 4 — 5 over the paving at z 100..200, and its way
    /// back on 5 → 2, with a road 6 — 7 cut off from it all.
    fn station() -> Graph {
        let at = |uid: u64, x: f32, z: f32| Node { uid, x, z };
        let nodes = vec![
            at(10, 0.0, 0.0),
            at(11, 400.0, 0.0),
            at(12, 1000.0, 0.0),
            at(13, 500.0, 150.0),
            at(14, 700.0, 150.0),
            at(15, 900.0, 150.0),
            at(16, 700.0, 180.0),
            at(17, 700.0, 190.0),
        ];
        let mut ways = vec![lane(0, 1, 0), lane(1, 2, 1), lane(2, 0, 2), lane(1, 3, 3), lane(5, 2, 4)];
        ways.extend(road(3, 4, 5, RoadClass::Minor, true));
        ways.extend(road(4, 5, 6, RoadClass::Minor, true));
        ways.extend(road(6, 7, 7, RoadClass::Local, false));
        Graph::new(nodes, ways)
    }

    fn paving() -> Area {
        Area {
            points: vec![[650.0, 170.0]],
            rings: vec![vec![[450.0, 0.0, 100.0], [950.0, 0.0, 100.0], [950.0, 0.0, 200.0], [450.0, 0.0, 200.0]]],
            roads: Vec::new(),
        }
    }

    /// `Access` with no road items to look up.
    fn access<'a>(graph: &'a Graph, lanes: Vec<Lane>, areas: &[Area]) -> Access<'a> {
        Access::of(graph, 3, lanes, areas, &|_| None)
    }

    fn lanes() -> Vec<Lane> {
        vec![
            Lane { ends: [13, 14], line: vec![[500.0, 150.0], [600.0, 150.0], [700.0, 150.0]] },
            Lane { ends: [14, 15], line: vec![[700.0, 150.0], [800.0, 150.0], [900.0, 150.0]] },
        ]
    }

    #[test]
    fn a_station_of_islands_is_reached_along_the_lane_past_it_to_the_lanes_far_end() {
        let graph = station();
        let access = access(&graph, lanes(), &[paving()]);
        // Nearest the first piece of the lane: the end of it a route comes to last, past the pumps.
        let way = |at| access.lane_past(at).map(|(_, way)| way);
        assert_eq!(way([650.0, 170.0]), Some(WayIn { node: 14, also: Some(13) }));
        assert_eq!(way([850.0, 170.0]), Some(WayIn { node: 15, also: Some(14) }));
    }

    #[test]
    fn what_a_service_area_offers_is_reached_along_the_nearest_lane_of_its_prefab_or_its_ground() {
        let graph = station();
        let bare = access(&graph, Vec::new(), &[paving()]);
        let access = access(&graph, lanes(), &[paving()]);
        // Its prefab's way through, 3 to 5, runs right past it: to the way out, past the lanes over the ground.
        let through = [Through { from: 13, to: 15, line: vec![[500.0, 170.0], [700.0, 170.0], [900.0, 170.0]] }];
        let ends = [(13, [500.0, 150.0]), (15, [900.0, 150.0])];
        assert_eq!(access.way_in([700.0, 170.0], &ends, &through), Some(WayIn { node: 15, also: Some(13) }));
        // A way through from what a route doesn't get to is none.
        let cut_off = [Through { from: 16, to: 15, line: through[0].line.clone() }];
        assert_eq!(access.way_in([700.0, 175.0], &ends, &cut_off), Some(WayIn { node: 14, also: Some(13) }));
        // No lane over its ground, a prefab with no way through: where it joins the road.
        assert_eq!(bare.way_in([520.0, 170.0], &ends, &[]), Some(WayIn { node: 13, also: None }));
    }

    #[test]
    fn a_road_that_meets_the_ground_is_gone_along_past_the_place_whose_nearest_node_is_far() {
        let graph = station();
        let road = |item: u32| (item == 7).then(|| Lane { ends: [14, 15], line: vec![[700.0, 90.0], [800.0, 90.0], [900.0, 90.0]] });
        let area = Area { roads: vec![7], ..paving() };
        let access = Access::of(&graph, 3, Vec::new(), &[area], &road);
        assert_eq!(access.lane_past([800.0, 120.0]).map(|(_, way)| way), Some(WayIn { node: 15, also: Some(14) }));
    }

    #[test]
    fn it_only_goes_by_the_lanes_on_the_places_own_ground() {
        let graph = station();
        // A lane off the paving, nearer than the one on it.
        let mut with_other = lanes();
        with_other.insert(0, Lane { ends: [16, 17], line: vec![[640.0, 95.0], [660.0, 95.0]] });
        let access = access(&graph, with_other, &[paving()]);
        assert_eq!(access.lane_past([650.0, 110.0]).map(|(_, way)| way.node), Some(14));
        // No service area there: nothing to go by.
        assert_eq!(access.lane_past([5000.0, 5000.0]), None);
    }

    #[test]
    fn what_a_route_gets_to_is_a_road_networks() {
        let graph = station();
        let reach = access(&graph, Vec::new(), &[]);
        assert!(reach.reaches(10) && reach.reaches(14) && !reach.reaches(16));
        assert!(!reach.reaches(99));
        // A lane cut off from it all isn't one to go by.
        let cut_off = vec![Lane { ends: [16, 17], line: vec![[640.0, 170.0], [660.0, 170.0]] }];
        assert_eq!(access(&graph, cut_off, &[paving()]).lane_past([650.0, 170.0]), None);
    }
}
