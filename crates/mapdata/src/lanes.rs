//! Where roads' lanes really are. The game places roads and prefabs on node lines, and its drawn road
//! graphs follow those lines; but a node line isn't always the middle of the road. Some road looks lay
//! their lanes off their line (a motorway's three-lane stretch, where an exit lane opens beside its two
//! lanes, has its line between the exit lane and the others), and a prefab's drawn graph runs from line to
//! line, so it shows jogs the road doesn't make (where a motorway splits into its two carriageways the drawn
//! graph is a V, while the lanes run straight on). The game's own map draws its roads thick enough to hide
//! that; WestPlay's thinner roads show it.
//!
//! Prefabs' navigation curves are their lanes, exactly where the game drives them: at each prefab node,
//! the curves starting or ending there are the lanes of the road joining there. This module reads where
//! lanes are from them: at a prefab's node ([`lanes_at`], [`lanes_between`]), and for each road look, how
//! far its lanes' middle lies from its line, measured wherever roads of that look meet prefabs
//! ([`road_offsets`]).

use std::collections::HashMap;

use crate::geometry::{self, Point, PrefabPlacement};
use crate::prefab::Prefab;
use crate::sector::ItemData;
use crate::world::{Map, Prefabs};

/// A lane's width in the game, metres (its lanes lie 4.5 m apart).
pub const GAME_LANE: f32 = 4.5;
/// How much further from its node than its own lanes' half width a road's lanes may lie, in lanes: a
/// central reservation, or a road whose line is beside its lanes.
const NODE_SLACK: f32 = 2.0;

/// Where the lanes at a prefab's node `node` are, in the prefab's space: the curves starting there (into
/// the prefab) at their start, and those ending there (out of it) at their end. Nothing where any lies
/// further from the node than one road's lanes could: some prefabs' nodes list lanes that aren't the
/// road's (a border gate's node naming its booth lanes, fanning out across the gate up to 35 m from the one
/// road joining there), and the node's own line is the better guess there.
pub fn lanes_at(prefab: &Prefab, node: usize) -> Vec<Point> {
    let Some(at) = prefab.nodes.get(node) else { return Vec::new() };
    let starts = at.curves_in.iter().filter_map(|&curve| prefab.curves.get(curve).map(|curve| curve.start));
    let ends = at.curves_out.iter().filter_map(|&curve| prefab.curves.get(curve).map(|curve| curve.end));
    let lanes: Vec<Point> = starts.chain(ends).collect();
    let reach = (lanes.len() as f32 / 2.0 + NODE_SLACK) * GAME_LANE;
    let near = |p: &Point| (p[0] - at.position[0]).hypot(p[1] - at.position[1]) <= reach;
    if !lanes.iter().all(near) {
        return Vec::new();
    }
    lanes
}

/// Where the lanes at the prefab's node `from` are that lead to or come from its node `to` (by any chain of
/// curves), in the prefab's space: the lanes of the road at `from` that the way between the two takes.
/// Nothing where the node's lanes aren't plausibly one road's ([`lanes_at`]).
pub fn lanes_between(prefab: &Prefab, from: usize, to: usize) -> Vec<Point> {
    let (Some(start), Some(end)) = (prefab.nodes.get(from), prefab.nodes.get(to)) else { return Vec::new() };
    if lanes_at(prefab, from).is_empty() {
        return Vec::new();
    }
    // Lanes leaving `from` that reach `to`; lanes arriving at `from` that came from `to`.
    let reaches = |first: usize, goal: &[usize]| reachable(prefab, &[first]).iter().any(|curve| goal.contains(curve));
    let out = start
        .curves_in
        .iter()
        .filter(|&&curve| reaches(curve, &end.curves_out))
        .filter_map(|&curve| prefab.curves.get(curve).map(|curve| curve.start));
    let from_to = reachable(prefab, &end.curves_in);
    let back = start
        .curves_out
        .iter()
        .filter(|curve| from_to.contains(curve))
        .filter_map(|&curve| prefab.curves.get(curve).map(|curve| curve.end));
    out.chain(back).collect()
}

/// Where the lanes at the prefab's node `at` are that a way through it between `at` and its node `other`
/// drives: in at `at` heading for `other` (`entering`), or out at `at` having come from `other`; in the
/// prefab's space. Of a motorway's three lanes before its exit, going on past it: the two that carry on,
/// not the one that leaves. Nothing where the node's lanes aren't plausibly one road's ([`lanes_at`]).
pub fn lanes_driven(prefab: &Prefab, at: usize, other: usize, entering: bool) -> Vec<Point> {
    let (Some(here), Some(there)) = (prefab.nodes.get(at), prefab.nodes.get(other)) else { return Vec::new() };
    if lanes_at(prefab, at).is_empty() {
        return Vec::new();
    }
    if entering {
        let reaches = |first: usize| reachable(prefab, &[first]).iter().any(|curve| there.curves_out.contains(curve));
        here.curves_in
            .iter()
            .filter(|&&curve| reaches(curve))
            .filter_map(|&curve| prefab.curves.get(curve).map(|curve| curve.start))
            .collect()
    } else {
        let reached = reachable(prefab, &there.curves_in);
        here.curves_out
            .iter()
            .filter(|curve| reached.contains(curve))
            .filter_map(|&curve| prefab.curves.get(curve).map(|curve| curve.end))
            .collect()
    }
}

/// Every curve reachable from `first` (themselves included), following where curves lead.
fn reachable(prefab: &Prefab, first: &[usize]) -> Vec<usize> {
    let mut seen = vec![false; prefab.curves.len()];
    let mut stack: Vec<usize> = first.iter().copied().filter(|&curve| curve < seen.len()).collect();
    let mut found = Vec::new();
    while let Some(curve) = stack.pop() {
        if std::mem::replace(&mut seen[curve], true) {
            continue;
        }
        found.push(curve);
        stack.extend(prefab.curves[curve].next.iter().copied().filter(|&next| next < seen.len() && !seen[next]));
    }
    found
}

/// Where on the prefab's node `node`'s line across the road some of its lanes are, in the prefab's space:
/// the node, moved across the road to their middle (`None` without any lanes). Only across: some prefabs'
/// lanes start or stop a little along the road from their node (a weigh station's, 7 m on), and the road
/// still ends at the node.
pub fn across(prefab: &Prefab, node: usize, lanes: &[Point]) -> Option<Point> {
    let (at, lanes) = (prefab.nodes.get(node)?, middle(lanes)?);
    let length = at.direction[0].hypot(at.direction[1]);
    if length == 0.0 {
        return Some(lanes);
    }
    let way = [at.direction[0] / length, at.direction[1] / length];
    let side = right_of(way, [lanes[0] - at.position[0], lanes[1] - at.position[1]]);
    Some([at.position[0] - way[1] * side, at.position[1] + way[0] * side])
}

/// A central reservation at least this wide, metres, between a road's two directions (their inner lanes'
/// edges) makes them two carriageways, drawn apart (a motorway's: 4 m); anything narrower is one road (a
/// painted gap, a kerb: many roads' 2 m, which as the threshold split them at random, by rounding).
pub const RESERVATION: f32 = 3.0;

/// A road's two directions where a central reservation keeps them apart: each one's lanes' middle (how far
/// to the right of the road's line, metres: the left one's is negative) and how many lanes it has.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Carriageways {
    pub left: f32,
    pub left_lanes: u32,
    pub right: f32,
    pub right_lanes: u32,
}

/// The lanes at a road's end, given as how far each lies to the right of the road's line (left negative),
/// split into the road's two carriageways if a central reservation separates the lanes on either side of
/// the line ([`RESERVATION`]); `None` for one road (one-way, or two-way with nothing between).
pub fn carriageways_of(offsets: &[f32]) -> Option<Carriageways> {
    let (left, right): (Vec<f32>, Vec<f32>) = offsets.iter().partition(|&&offset| offset < 0.0);
    if left.is_empty() || right.is_empty() {
        return None;
    }
    let mean = |side: &[f32]| side.iter().sum::<f32>() / side.len() as f32;
    let (l, r) = (mean(&left), mean(&right));
    // Lanes side by side: each side's inner edge half its lanes' width from its middle.
    let gap = (r - right.len() as f32 * GAME_LANE / 2.0) - (l + left.len() as f32 * GAME_LANE / 2.0);
    (gap >= RESERVATION).then_some(Carriageways {
        left: l,
        left_lanes: left.len() as u32,
        right: r,
        right_lanes: right.len() as u32,
    })
}

/// Where the two carriageways of the road at the prefab's node `node` meet it, in the prefab's space (left
/// and right of the way into the prefab), if its lanes there are two carriageways ([`carriageways_of`]).
pub fn carriageways_at(prefab: &Prefab, node: usize) -> Option<(Point, Point)> {
    let at = prefab.nodes.get(node)?;
    let length = at.direction[0].hypot(at.direction[1]);
    if length == 0.0 {
        return None;
    }
    let way = [at.direction[0] / length, at.direction[1] / length];
    let offsets: Vec<f32> = lanes_at(prefab, node)
        .iter()
        .map(|lane| right_of(way, [lane[0] - at.position[0], lane[1] - at.position[1]]))
        .collect();
    let split = carriageways_of(&offsets)?;
    let place = |side: f32| [at.position[0] - way[1] * side, at.position[1] + way[0] * side];
    Some((place(split.left), place(split.right)))
}

/// The middle of some points (`None` without any).
pub fn middle(points: &[Point]) -> Option<Point> {
    let n = points.len() as f32;
    (!points.is_empty())
        .then(|| [points.iter().map(|p| p[0]).sum::<f32>() / n, points.iter().map(|p| p[1]).sum::<f32>() / n])
}

/// Measurements closer than this to a look's offset agree with it, metres.
pub const AGREEMENT: f32 = 0.3;

/// A road look's measured offset: how far its lanes' middle lies to the right of its line (facing the way
/// the road runs), metres, the middle of all measurements, rounded to a quarter metre; how many times it
/// was measured, and how many of those agree with it. And its carriageways, where its two directions lie
/// apart: the middle of the measurements that found them, with how many agree (the same lanes each side,
/// both middles within [`AGREEMENT`]: measurements finding one road disagree).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LookOffset {
    pub offset: f32,
    pub measured: usize,
    pub agreeing: usize,
    pub carriageways: Option<Carriageways>,
    pub carriageways_agreeing: usize,
    /// Where the look is split: whether its right carriageway (facing the way the road runs) carries
    /// traffic that way (driving on the right), as nearly all its measurements found; `None` if they
    /// don't agree.
    pub forward_right: Option<bool>,
}

/// How many of a look's measurements must agree for its offset to be used, and how many there must be (one
/// odd prefab mustn't move every road of a look).
pub const TRUSTED_SHARE: f32 = 0.9;
pub const TRUSTED_COUNT: usize = 3;

impl LookOffset {
    /// The offset to draw the look's roads at: the measured one where it was measured often enough and
    /// nearly all measurements agree, and none otherwise (nor where it's within rounding of none).
    pub fn trusted(&self) -> f32 {
        let trusted = self.measured >= TRUSTED_COUNT && self.agreeing as f32 >= TRUSTED_SHARE * self.measured as f32;
        if trusted && self.offset.abs() >= AGREEMENT { self.offset } else { 0.0 }
    }

    /// How far to the right of the way it travels a route on a road of this look runs, metres, travelling
    /// `along` the road (from its start to its end) or against it: on the middle of the carriageway carrying
    /// it where the look is drawn as two, else on the road's drawn middle ([`LookOffset::trusted`]).
    pub fn route_shift(&self, along: bool) -> f32 {
        let (forward, backward) = match (self.split(), self.forward_right) {
            (Some(split), Some(true)) => (split.right, split.left),
            (Some(split), Some(false)) => (split.left, split.right),
            _ => (self.trusted(), self.trusted()),
        };
        if along { forward } else { -backward }
    }

    /// The look's two carriageways, to draw its roads as, where it was measured often enough, nearly all
    /// measurements found the same ones, and both have as many lanes (divided roads do: a motorway's 2+2 or
    /// 3+3, a rural road's 1+1; a 1+2 road's gap is paint, and the prefabs it meets join it as one road);
    /// `None` (one road) otherwise.
    pub fn split(&self) -> Option<Carriageways> {
        let trusted =
            self.measured >= TRUSTED_COUNT && self.carriageways_agreeing as f32 >= TRUSTED_SHARE * self.measured as f32;
        self.carriageways.filter(|split| trusted && split.left_lanes == split.right_lanes)
    }
}

/// Each road look's offset ([`LookOffset`]), measured at every node where a road of the look meets a prefab
/// with lanes there: the middle of those lanes, against the road's line there (a road leaves and arrives
/// along its nodes' way: [`geometry::road_curve`]). Only where the prefab has as many lanes there as the look,
/// if it ever does: where it has more (a one-lane road into a two-lane filling station or crossing), their
/// middle isn't the road's (`blke_40`, one lane 2.25 m left of its line at 1,565 of its 1,584 one-lane ends,
/// agreed at only 87% of all 1,808 with the others: drawn on its line, its junctions' roads met it 2.25 m
/// aside; owner 2026-10-22: "connections are fucked up").
pub fn road_offsets(map: &Map, prefabs: &mut Prefabs) -> HashMap<u64, LookOffset> {
    let definitions = prefabs.definitions();
    let looks: HashMap<u64, u64> = map
        .items
        .iter()
        .filter_map(|item| match &item.data {
            ItemData::Road { look, .. } => Some((item.uid, *look)),
            _ => None,
        })
        .collect();
    // Each look's measurements: at ends with as many lanes as it, and at the others.
    let mut measured: HashMap<u64, (Vec<Measurement>, Vec<Measurement>)> = HashMap::new();
    for item in &map.items {
        let ItemData::Prefab { model, nodes, origin, .. } = &item.data else { continue };
        let Some(prefab) = prefabs.get(*model) else { continue };
        let (Some(first), Some(origin_node)) =
            (nodes.first().and_then(|uid| map.nodes.get(uid)), prefab.nodes.get(*origin as usize))
        else {
            continue;
        };
        let placement = PrefabPlacement::new(origin_node, first);
        for (world, uid) in nodes.iter().enumerate() {
            let Some(node) = map.nodes.get(uid) else { continue };
            let own = geometry::prefab_own_node(world, *origin as usize, prefab.nodes.len());
            let lanes = lanes_at(prefab, own);
            if lanes.is_empty() {
                continue;
            }
            let (way, line) = (geometry::node_forward(node), geometry::node_point(node));
            let offsets: Vec<f32> = lanes
                .iter()
                .map(|&lane| {
                    let at = placement.place(lane);
                    right_of(way, [at[0] - line[0], at[1] - line[1]])
                })
                .collect();
            let offset = offsets.iter().sum::<f32>() / offsets.len() as f32;
            let split = carriageways_of(&offsets);
            // The lanes carrying traffic the way the road runs: into the prefab (its curves starting at the
            // node, listed first by lanes_at) where the road runs into it there, out of it otherwise.
            let at = &prefab.nodes[own];
            let into = {
                let (a, b) = (
                    placement.place(at.position),
                    placement.place([at.position[0] + at.direction[0], at.position[1] + at.direction[1]]),
                );
                [b[0] - a[0], b[1] - a[1]]
            };
            let entering = at.curves_in.len().min(offsets.len());
            let running_in = into[0] * way[0] + into[1] * way[1] > 0.0;
            let forward: Vec<f32> =
                if running_in { offsets[..entering].to_vec() } else { offsets[entering..].to_vec() };
            let forward_right = (!forward.is_empty()).then(|| forward.iter().sum::<f32>() > 0.0);
            for road in [node.forward_item, node.backward_item] {
                if let Some(&look) = looks.get(&road) {
                    let fits = definitions.road_looks.get(&look).is_some_and(|look| look.lanes as usize == lanes.len());
                    let (fitting, others) = measured.entry(look).or_default();
                    let measurement = Measurement { offset, split, forward_right };
                    if fits { fitting.push(measurement) } else { others.push(measurement) }
                }
            }
        }
    }
    measured
        .into_iter()
        .map(|(look, (fitting, others))| (look, summarise(if fitting.is_empty() { others } else { fitting })))
        .collect()
}

/// One look measured at one prefab's end: its lanes' middle, its carriageways if split, and whether the
/// lanes running the road's way lie right of its line.
#[derive(Clone, Copy, Debug)]
struct Measurement {
    offset: f32,
    split: Option<Carriageways>,
    forward_right: Option<bool>,
}

/// How far `v` reaches to the right of the way `way` (x east, z south).
pub fn right_of(way: Point, v: Point) -> f32 {
    v[0] * -way[1] + v[1] * way[0]
}

fn summarise(measurements: Vec<Measurement>) -> LookOffset {
    let quarter = |value: f32| (value * 4.0).round() / 4.0;
    let middle_of = |mut values: Vec<f32>| {
        values.sort_by(f32::total_cmp);
        values[values.len() / 2]
    };
    let offset = quarter(middle_of(measurements.iter().map(|m| m.offset).collect()));
    let agreeing = measurements.iter().filter(|m| (m.offset - offset).abs() < AGREEMENT).count();
    let found: Vec<Carriageways> = measurements.iter().filter_map(|m| m.split).collect();
    // Which side carries the road's way, where split: as nearly all those measurements found.
    let sides: Vec<bool> = measurements.iter().filter(|m| m.split.is_some()).filter_map(|m| m.forward_right).collect();
    let right = sides.iter().filter(|&&right| right).count();
    let forward_right = [true, false].into_iter().find(|&side| {
        let agreeing = if side { right } else { sides.len() - right };
        !sides.is_empty() && agreeing as f32 >= TRUSTED_SHARE * sides.len() as f32
    });
    let carriageways = (!found.is_empty()).then(|| {
        let lanes = |count: fn(&Carriageways) -> u32| {
            let mut counts: Vec<u32> = found.iter().map(count).collect();
            counts.sort_unstable();
            counts[counts.len() / 2]
        };
        Carriageways {
            left: quarter(middle_of(found.iter().map(|c| c.left).collect())),
            left_lanes: lanes(|c| c.left_lanes),
            right: quarter(middle_of(found.iter().map(|c| c.right).collect())),
            right_lanes: lanes(|c| c.right_lanes),
        }
    });
    let carriageways_agreeing = carriageways.map_or(0, |middle| {
        found
            .iter()
            .filter(|c| {
                c.left_lanes == middle.left_lanes
                    && c.right_lanes == middle.right_lanes
                    && (c.left - middle.left).abs() < AGREEMENT
                    && (c.right - middle.right).abs() < AGREEMENT
            })
            .count()
    });
    LookOffset { offset, measured: measurements.len(), agreeing, carriageways, carriageways_agreeing, forward_right }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prefab::{NavCurve, PrefabNode};

    fn node(position: Point, direction: Point, curves_in: Vec<usize>, curves_out: Vec<usize>) -> PrefabNode {
        PrefabNode { height: 0.0, position, direction, lanes: 0, curves_in, curves_out }
    }

    fn lane(start: Point, end: Point, next: Vec<usize>) -> NavCurve {
        let way = [(end[0] - start[0]) / 15.0, (end[1] - start[1]) / 15.0];
        NavCurve {
            heights: [0.0, 0.0],
            start,
            end,
            start_direction: way,
            end_direction: way,
            length: 15.0,
            next,
            trucks: true,
        }
    }

    /// The game's motorway split (it_hw2_fork): a two-way motorway at node 0, two carriageways at nodes 1
    /// and 2 (6.5 m to either side), every lane straight.
    fn split() -> Prefab {
        Prefab {
            nodes: vec![
                node([0.0, 0.0], [0.0, 1.0], vec![1, 0], vec![2, 3]),
                node([6.5, 15.0], [0.0, -1.0], vec![2, 3], vec![]),
                node([-6.5, 15.0], [0.0, -1.0], vec![], vec![1, 0]),
            ],
            map_points: Vec::new(),
            spawn_points: Vec::new(),
            triggers: Vec::new(),
            curves: vec![
                lane([-8.75, 0.0], [-8.75, 15.0], vec![]),
                lane([-4.25, 0.0], [-4.25, 15.0], vec![]),
                lane([4.25, 15.0], [4.25, 0.0], vec![]),
                lane([8.75, 15.0], [8.75, 0.0], vec![]),
            ],
        }
    }

    #[test]
    fn finds_the_lanes_at_a_node_and_those_on_the_way_to_another() {
        let prefab = split();
        assert_eq!(middle(&lanes_at(&prefab, 0)), Some([0.0, 0.0]));
        // The way from the motorway to each carriageway takes that carriageway's lanes: straight on.
        assert_eq!(middle(&lanes_between(&prefab, 0, 1)), Some([6.5, 0.0]));
        assert_eq!(middle(&lanes_between(&prefab, 0, 2)), Some([-6.5, 0.0]));
        assert_eq!(middle(&lanes_between(&prefab, 1, 0)), Some([6.5, 15.0]));
        // No lanes lead from one carriageway to the other.
        assert!(lanes_between(&prefab, 1, 2).is_empty());
        // Across the road at the node only: lanes starting a little on from it still end the road there.
        let mut late = split();
        late.curves[0].start = [-8.75, 7.0];
        late.curves[1].start = [-4.25, 7.0];
        assert_eq!(across(&late, 0, &lanes_between(&late, 0, 2)), Some([-6.5, 0.0]));
    }

    #[test]
    fn lanes_spread_wider_than_a_road_are_not_its_lanes() {
        // A border gate's node: one lane beside it, the other listed across the gate, 44.75 m from it.
        let mut gate = split();
        gate.nodes[1] = node([23.5, 15.0], [0.0, -1.0], vec![2, 3], vec![]);
        gate.curves[3] = lane([-21.25, 15.0], [-21.25, 0.0], vec![]);
        gate.curves[2] = lane([21.25, 15.0], [21.25, 0.0], vec![]);
        assert!(lanes_at(&gate, 1).is_empty());
        assert!(lanes_between(&gate, 1, 0).is_empty());
        // A motorway's four lanes with their central reservation (17.5 m) are a road's.
        assert_eq!(lanes_at(&split(), 0).len(), 4);
    }

    #[test]
    fn a_look_is_trusted_where_its_measurements_agree() {
        let one = |offsets: Vec<f32>| {
            summarise(
                offsets.into_iter().map(|offset| Measurement { offset, split: None, forward_right: None }).collect(),
            )
        };
        let offset = one(vec![-2.25, -2.24, -2.26, -2.25, -2.25, -2.25, -2.25, -2.25, -2.25, 1.0]);
        assert_eq!((offset.offset, offset.measured, offset.agreeing), (-2.25, 10, 9));
        assert_eq!(offset.trusted(), -2.25);
        assert_eq!(offset.split(), None);
        // Spread out: not trusted; on its line: none; measured too few times: not trusted.
        assert_eq!(one(vec![-2.25, 0.0, 2.25, 4.5]).trusted(), 0.0);
        assert_eq!(one(vec![0.1, -0.1, 0.0]).trusted(), 0.0);
        assert_eq!(one(vec![-2.25, -2.25]).trusted(), 0.0);
    }

    #[test]
    fn a_central_reservation_makes_two_carriageways() {
        // The game's two-way motorway: two lanes each way, 4.25 and 8.75 m out (a 4 m reservation).
        let motorway = carriageways_of(&[-8.75, -4.25, 4.25, 8.75]);
        assert_eq!(motorway, Some(Carriageways { left: -6.5, left_lanes: 2, right: 6.5, right_lanes: 2 }));
        // A two-way street (nothing between its lanes), and one-way roads (lanes either side of their line,
        // or a lane more on one side): one road.
        assert_eq!(carriageways_of(&[-2.25, 2.25]), None);
        assert_eq!(carriageways_of(&[-2.25, 2.25, 6.75]), None);
        assert_eq!(carriageways_of(&[2.25, 6.75]), None);
        // At the motorway split's node, the carriageways where its lanes are: facing into the prefab (south,
        // +z), east (+x) is on the left.
        assert_eq!(carriageways_at(&split(), 0), Some(([6.5, 0.0], [-6.5, 0.0])));
        assert_eq!(carriageways_at(&split(), 1), None);
    }

    #[test]
    fn a_look_is_split_only_where_its_measurements_agree() {
        let measured = |split, forward_right| Measurement { offset: 0.0, split, forward_right };
        let found = Some(Carriageways { left: -6.5, left_lanes: 2, right: 6.5, right_lanes: 2 });
        let all: Vec<_> = (0..10).map(|_| measured(found, Some(true))).collect();
        let look = summarise(all.clone());
        assert_eq!(look.split(), found);
        // Driving on the right: the route runs on the right carriageway either way it goes.
        assert_eq!(look.forward_right, Some(true));
        assert_eq!((look.route_shift(true), look.route_shift(false)), (6.5, 6.5));
        // Uneven (a 1+2 road with a painted gap): not split.
        let uneven = Some(Carriageways { left: -6.75, left_lanes: 1, right: 4.5, right_lanes: 2 });
        assert_eq!(summarise((0..10).map(|_| measured(uneven, Some(true))).collect()).split(), None);
        // Found at only some of its ends: not split, and the route on the road's middle.
        let some: Vec<_> = all.iter().enumerate().map(|(i, &m)| if i < 3 { measured(None, None) } else { m }).collect();
        let look = summarise(some);
        assert_eq!(look.split(), None);
        assert_eq!(look.route_shift(true), 0.0);
        // Sides that don't agree: which carriageway is unknown.
        let mixed: Vec<_> = (0..10).map(|i| measured(found, Some(i % 2 == 0))).collect();
        assert_eq!(summarise(mixed).forward_right, None);
    }

    #[test]
    fn measures_to_the_right_of_the_way_the_road_runs() {
        // Heading north (-z), east is to the right.
        assert_eq!(right_of([0.0, -1.0], [2.0, 0.0]), 2.0);
        assert_eq!(right_of([0.0, -1.0], [-2.0, 0.0]), -2.0);
    }
}
