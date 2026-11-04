//! Roads: what the game's looks and edges say a road's cross-section is, and how roads ease where they meet something
//! of another width.
//!
//! A road's own colour is its carriageway (a template's drawn surface, a legacy road's size and shoulder); what its edge
//! models carry beside it (a hard shoulder, a sidewalk) is quiet ground. Road pieces are short: a run of them with the
//! same cross-section is a chain, and a chain's edges ease along its whole length to what they meet at its two ends.

use std::collections::{HashMap, HashSet};

use crate::files::GameFiles;
use crate::lanes;
use crate::sector::{Item, ItemData, Node};
use crate::sii;
use crate::token::string_to_token;
use crate::world::{Definitions, Map, RoadClass};

use super::faces::{Materials, Piece, model_pieces};
use super::lines::Tubes;
use super::sort::Rules;
use super::{Kind, LANE_HALF, TRACK_HALF, frame};

/// Differences no eye sees, and those no joint has: over this a road is opening into a plaza or a yard.
const EASE_FROM: f64 = 0.05;
const EASE_TO: f64 = 4.0;
/// An edge moves one metre sideways over this many metres of road, at the least over [`EASE_SHORTEST`], over no more
/// than [`EASE_MOST`] of the chain.
const EASE_SLOPE: f64 = 20.0;
const EASE_SHORTEST: f64 = 8.0;
const EASE_MOST: f64 = 0.45;
/// Where both ends of a chain want the same way and this little is left between the two eases, the edge holds rather
/// than going back and forth, m.
const HOLD_BELOW: f64 = 100.0;
/// A cross-line every this many metres along a road, before its edges are thinned.
const STATION: f64 = 2.0;
/// Road edges keep a point where it matters by this much, m.
const EDGE_TOLERANCE: f64 = 0.02;
/// A road's faces for what passes over what are about this long, m.
pub const FACE_STEP: f64 = 4.0;

/// How far a template's drawn surface reaches across (its model's x).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Span {
    from: f64,
    to: f64,
}

/// A road look's cross-section, as its definition gives it.
#[derive(Clone, Debug, Default)]
pub struct Look {
    right: Option<Span>,
    left: Option<Span>,
    offset: f64,
    /// Legacy looks: their carriageway's size, left and right, and their shoulders'.
    sizes: Option<(f64, f64)>,
    shoulders: (f64, f64),
    /// No lanes: a sidewalk laid as a road.
    footpath: bool,
    /// A look the game doesn't draw: its template's only a placeholder, its lanes are all there is of the road. Where they
    /// end on the right and on the left, out from the centre.
    lanes: Option<[f64; 2]>,
    /// Its two directions' lanes, as its definition lays them ([`lanes::directions_of`]).
    pub ways: Option<lanes::Carriageways>,
}

/// A strip of a road edge model, outward from the road's edge.
#[derive(Clone, Copy, Debug)]
struct EdgeStrip {
    from: f64,
    to: f64,
    /// Where the model ends.
    end: f64,
    top: f64,
    /// Terrain's or a fade's: not a surface.
    ground: bool,
}

/// A road's cross-section as placed: where things are across it, metres right of its centre line (left negative).
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    /// Where its own colour ends on the right and on the left (both as distances out from the centre).
    pub outer: [f64; 2],
    /// Where a median starts on the right and on the left, likewise.
    pub inner: Option<[f64; 2]>,
    /// Quiet bands beside it: from, to (across), how high.
    pub strips: Vec<[f64; 3]>,
    /// Where its terrain starts, right and left.
    pub ground_from: [f64; 2],
}

/// The game's tables a road is read with.
pub struct Looks {
    looks: HashMap<u64, Look>,
    edges: HashMap<u64, Vec<EdgeStrip>>,
}

impl Looks {
    pub fn load(files: &GameFiles, materials: &Materials, rules: &Rules) -> Self {
        let mut looks = HashMap::new();
        let mut spans: HashMap<String, Option<Span>> = HashMap::new();
        for unit in sii::read_family(files, "/def/world", "road_look").unwrap_or_default() {
            let Some(token) = unit.name.strip_prefix("road.").and_then(string_to_token) else { continue };
            let number = |key: &str| unit.get(key).and_then(|value| value.parse::<f64>().ok());
            let mut span_of = |key: &str| -> Option<Span> {
                let path = unit.get(key)?.trim_matches('"');
                if let Some(known) = spans.get(path) {
                    return *known;
                }
                let span = template_span(files, materials, path);
                spans.insert(path.to_owned(), span);
                span
            };
            let lanes = unit.all("lanes_left").count() + unit.all("lanes_right").count();
            let invisible = unit.get("invisible") == Some("true");
            let look = Look {
                right: span_of("template_right"),
                left: span_of("template_left"),
                offset: number("road_offset").unwrap_or(0.0),
                sizes: number("road_size_left").zip(number("road_size_right")),
                shoulders: (number("shoulder_size_left").unwrap_or(0.0), number("shoulder_size_right").unwrap_or(0.0)),
                footpath: lanes == 0,
                lanes: invisible.then(|| lanes_reach(&unit)),
                ways: lanes::directions_of(&lanes::look_lanes(&unit)),
            };
            looks.insert(token, look);
        }
        let mut edges = HashMap::new();
        for unit in sii::read_family(files, "/def/world", "road_edge").unwrap_or_default() {
            let (Some(token), Some(model)) =
                (unit.name.strip_prefix("road_edge.").and_then(string_to_token), unit.get("model"))
            else {
                continue;
            };
            let variant = unit.get("variant_15").and_then(string_to_token);
            let pieces = model_pieces(files, materials, model.trim_matches('"'), variant, 0);
            let list = pieces
                .iter()
                .filter(|piece| !piece.part.starts_with("coll") && !piece.material.effect.contains(".decal"))
                .map(|piece| {
                    let (from, end) = (f64::from(piece.box_x.0), f64::from(piece.box_x.1));
                    let first = piece.material.textures.first().map_or("", String::as_str);
                    // A transition fades the road into the terrain: past where the solid road ends.
                    let ground = piece.part.starts_with("tran") || rules.is_ground(first);
                    EdgeStrip { from, to: if ground { from } else { end }, end, top: f64::from(piece.box_y.1), ground }
                })
                .collect();
            edges.insert(token, list);
        }
        Self { looks, edges }
    }

    pub fn get(&self, look: u64) -> Option<&Look> {
        self.looks.get(&look)
    }

    /// How far a side's edge model reaches, and its surfaces as quiet bands, from `at` going `outward` (1: right).
    fn edge(&self, token: u64, at: f64, outward: f64, strips: &mut Vec<[f64; 3]>) -> f64 {
        let mut reach: f64 = 0.0;
        for strip in self.edges.get(&token).map_or(&[][..], Vec::as_slice) {
            reach = reach.max(strip.end);
            if !strip.ground && strip.to > strip.from {
                let (a, b) = (at + outward * strip.from.max(0.0), at + outward * strip.to);
                strips.push([a.min(b), a.max(b), strip.top + 0.02]);
            }
        }
        reach
    }

    /// A road's cross-section: its look's, with the edge models `sides` names (right template's right and left, left
    /// template's right and left, at 4..8) and the sidewalks a legacy city road has (`sidewalks`, left and right, m).
    pub fn section(&self, look: &Look, sides: &[u64; 8], sidewalks: [f64; 2]) -> Option<Section> {
        let drawn = self.drawn_section(look, sides, sidewalks);
        let Some(outer) = look.lanes else { return drawn };
        // Its terrain still starts where its template says.
        let ground_from = drawn.map_or(outer, |section| section.ground_from);
        Some(Section { outer, inner: None, strips: Vec::new(), ground_from })
    }

    fn drawn_section(&self, look: &Look, sides: &[u64; 8], sidewalks: [f64; 2]) -> Option<Section> {
        let mut strips = Vec::new();
        if let Some((size_left, size_right)) = look.sizes {
            // The junctions a legacy road meets end in the middle of its metre's fade to the terrain.
            let outer = [look.offset + size_right + look.shoulders.1 + 0.5, look.offset + size_left + look.shoulders.0 + 0.5];
            if sidewalks[1] > 0.0 {
                strips.push([outer[0], outer[0] + sidewalks[1], 0.1]);
            }
            if sidewalks[0] > 0.0 {
                strips.push([-outer[1] - sidewalks[0], -outer[1], 0.1]);
            }
            let inner = (look.offset > 0.0).then_some([look.offset, look.offset]);
            let ground_from = [outer[0] + 0.5 + sidewalks[1], outer[1] + 0.5 + sidewalks[0]];
            return Some(Section { outer, inner, strips, ground_from });
        }
        let right = look.right?;
        let (ra, rb) = (right.from + look.offset, right.to + look.offset);
        let right_reach = self.edge(sides[4], rb, 1.0, &mut strips);
        let left_of_right = self.edge(sides[5], ra, -1.0, &mut strips);
        if let Some(left) = look.left {
            let (la, lb) = (left.from + look.offset, left.to + look.offset);
            let left_reach = self.edge(sides[6], -lb, -1.0, &mut strips);
            self.edge(sides[7], -la, 1.0, &mut strips);
            Some(Section { outer: [rb, lb], inner: Some([ra, la]), strips, ground_from: [rb + right_reach, lb + left_reach] })
        } else {
            Some(Section { outer: [rb, -ra], inner: None, strips, ground_from: [rb + right_reach, -ra + left_of_right] })
        }
    }
}

/// Where an undrawn road's lanes end on its right and left. Lanes both ways run out from the centre, each its look's
/// `lane_offsets_*` nearer it. Lanes one way lie across the line with one more on the left of them if they're odd (the
/// lanes of 0-1, 0-2 and 0-3 looks end at -2.25, ±2.25, and +2.25, -2.25, -6.75 on prefabs' nodes, all of ETS2's, ATS's
/// and ProMods'), mirrored if they're the left's.
fn lanes_reach(unit: &sii::Unit) -> [f64; 2] {
    let (right, left) = (unit.all("lanes_right").count(), unit.all("lanes_left").count());
    let lane = 2.0 * LANE_HALF;
    let rail = unit.all("lanes_right").chain(unit.all("lanes_left")).any(|lane| lane.starts_with("traffic_lane.rail"));
    let nearer = |key: &str| -> f64 {
        let text = unit.get(key).unwrap_or("");
        text.trim_matches(|c| c == '(' || c == ')').split(',').next().and_then(|first| first.trim().parse().ok()).unwrap_or(0.0)
    };
    match (right, left) {
        // A single track on a one-way look's lane.
        (_, _) if rail => [TRACK_HALF - LANE_HALF, TRACK_HALF + LANE_HALF],
        (n, 0) => [(n / 2) as f64 * lane, n.div_ceil(2) as f64 * lane],
        (0, n) => [n.div_ceil(2) as f64 * lane, (n / 2) as f64 * lane],
        (r, l) => [r as f64 * lane + nearer("lane_offsets_right"), l as f64 * lane + nearer("lane_offsets_left")],
    }
}

/// How far a template's drawn surface reaches: its `vis` parts, else (a model with none) `coll`, decals left out.
fn template_span(files: &GameFiles, materials: &Materials, path: &str) -> Option<Span> {
    let pieces = model_pieces(files, materials, path, None, 0);
    let drawn = |part: &str| -> Vec<&Piece> {
        pieces.iter().filter(|piece| piece.part == part && !piece.material.effect.contains(".decal")).collect()
    };
    let visible = drawn("vis");
    let list = if visible.is_empty() { drawn("coll") } else { visible };
    let from = list.iter().map(|piece| f64::from(piece.box_x.0)).reduce(f64::min)?;
    let to = list.iter().map(|piece| f64::from(piece.box_x.1)).reduce(f64::max)?;
    Some(Span { from, to })
}

/// What's needed of a legacy city road's flags: its sidewalks' widths, left and right, m. (Stored as 0: 8 m, 1: 4 m,
/// 2: 2 m, 3: none.)
pub fn sidewalk_widths(flags: u8) -> [f64; 2] {
    let width = |bits: u8| [8.0, 4.0, 2.0, 0.0][usize::from(bits & 3)];
    [width(flags), width(flags >> 2)]
}

/// One side's edge along a chain, as a change on the section's own.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Edge {
    pub start: f64,
    pub end: f64,
}

/// A difference a joint really has, or none.
fn clean(d: f64) -> f64 {
    if d.abs() <= EASE_FROM || d.abs() > EASE_TO { 0.0 } else { d }
}

impl Edge {
    /// How far the edge has moved `at` metres along a chain `length` long.
    pub fn at(&self, at: f64, length: f64) -> f64 {
        let (start, end) = (clean(self.start), clean(self.end));
        let long = |d: f64| if d == 0.0 { 0.0 } else { (EASE_SLOPE * d.abs()).max(EASE_SHORTEST).min(length * EASE_MOST) };
        let (start_long, end_long) = (long(start), long(end));
        let smooth = |f: f64| f * f * (3.0 - 2.0 * f);
        if start * end > 0.0 && length - start_long - end_long < HOLD_BELOW {
            return start + (end - start) * smooth((at / length).clamp(0.0, 1.0));
        }
        let fade = |d: f64, long: f64, from_end: f64| {
            if d == 0.0 { 0.0 } else { d * smooth((1.0 - from_end / long).clamp(0.0, 1.0)) }
        };
        fade(start, start_long, at) + fade(end, end_long, length - at)
    }
}

/// A run of road pieces with one cross-section.
#[derive(Clone, Debug, Default)]
pub struct Chain {
    pub length: f64,
    /// Its edges: right, left, the median's right, its left, the chain's way.
    pub edges: [Edge; 4],
}

/// How a map's joints ease, every end of every chain, each of its four edges (an audit).
#[derive(Clone, Debug, Default)]
pub struct EaseReport {
    /// Edges with nothing to ease (under 5 cm).
    pub meeting: u64,
    /// Over 4 m: a road opening into a plaza or a yard.
    pub opening: u64,
    /// Eased at the full length, 20 times the difference (8 m at the least).
    pub full: u64,
    /// Eased over the 45% of a chain too short for that, by how steep: metres sideways per metre along, up to 1:20,
    /// 1:10, 1:5, and more.
    pub short: [u64; 4],
    /// Held along a short chain wanting the same both ways.
    pub held: u64,
    /// Chains and the most edges' difference eased, m.
    pub chains: u64,
    pub most: f64,
}

/// A road piece's place on its chain.
#[derive(Clone, Copy, Debug)]
pub struct OnChain {
    pub chain: u32,
    /// How far along the chain it starts (forward) or ends.
    pub from: f64,
    /// Whether it runs the chain's way.
    pub forward: bool,
}

/// Each shown road's section and its place on a chain.
pub struct Network {
    pub chains: Vec<Chain>,
    pub roads: HashMap<usize, (Section, OnChain)>,
}

fn same_section(a: &Section, b: &Section, same_way: bool) -> bool {
    let close = |x: f64, y: f64| (x - y).abs() <= EASE_FROM;
    let flip = |s: [f64; 2]| if same_way { s } else { [s[1], s[0]] };
    let (bo, bi) = (flip(b.outer), b.inner.map(flip));
    close(a.outer[0], bo[0])
        && close(a.outer[1], bo[1])
        && match (a.inner, bi) {
            (None, None) => true,
            (Some(x), Some(y)) => close(x[0], y[0]) && close(x[1], y[1]),
            _ => false,
        }
}

impl Network {
    /// How the chains' edges ease.
    pub fn report(&self) -> EaseReport {
        let mut report = EaseReport { chains: self.chains.len() as u64, ..EaseReport::default() };
        for chain in &self.chains {
            for edge in &chain.edges {
                let (start, end) = (clean(edge.start), clean(edge.end));
                let long = |d: f64| (EASE_SLOPE * d.abs()).max(EASE_SHORTEST).min(chain.length * EASE_MOST);
                let held = start * end > 0.0 && chain.length - long(start) - long(end) < HOLD_BELOW;
                for (raw, d) in [(edge.start, start), (edge.end, end)] {
                    if raw.abs() <= EASE_FROM {
                        report.meeting += 1;
                    } else if d == 0.0 {
                        report.opening += 1;
                    } else if held {
                        report.held += 1;
                    } else {
                        report.most = report.most.max(d.abs());
                        let wanted = (EASE_SLOPE * d.abs()).max(EASE_SHORTEST);
                        if long(d) >= wanted {
                            report.full += 1;
                        } else {
                            let slope = d.abs() / long(d);
                            let bucket = if slope <= 0.05 {
                                0
                            } else if slope <= 0.1 {
                                1
                            } else if slope <= 0.2 {
                                2
                            } else {
                                3
                            };
                            report.short[bucket] += 1;
                        }
                    }
                }
            }
        }
        report
    }

    /// Chains roads in `roads` (item indexes with their sections), then sets each chain's edges: `joint` says how far
    /// a road's end has to move to meet what's on its node (right, left, median right, median left, its own way round),
    /// for an end where something else than a road is met. `on_prefab` says which nodes have a prefab.
    pub fn new(
        items: &[Item],
        roads: Vec<(usize, Section)>,
        on_prefab: &HashSet<u64>,
        joint: &(dyn Fn(usize, &Section, bool) -> [f64; 4] + Sync),
    ) -> Self {
        let ends_of = |index: usize| match &items[index].data {
            ItemData::Road { start_node, end_node, .. } => (*start_node, *end_node),
            _ => (0, 0),
        };
        let length_of = |index: usize| match &items[index].data {
            ItemData::Road { length, .. } => f64::from(*length),
            _ => 0.0,
        };
        let sections: HashMap<usize, Section> = roads.into_iter().collect();
        let mut ends: HashMap<u64, Vec<(usize, bool)>> = HashMap::new();
        let mut order: Vec<usize> = sections.keys().copied().collect();
        order.sort_unstable();
        for &index in &order {
            let (start, end) = ends_of(index);
            ends.entry(end).or_default().push((index, true));
            ends.entry(start).or_default().push((index, false));
        }
        // What carries on from a road's end at `node`, if it's the same road going on.
        let next = |node: u64, from: (usize, bool)| -> Option<(usize, bool)> {
            if on_prefab.contains(&node) {
                return None;
            }
            let list = ends.get(&node)?;
            if list.len() != 2 {
                return None;
            }
            let other = if list[0] == from { list[1] } else { list[0] };
            let same_way = from.1 != other.1;
            same_section(&sections[&from.0], &sections[&other.0], same_way).then_some(other)
        };
        let mut placed: HashMap<usize, (u32, f64, bool)> = HashMap::new();
        let mut chains: Vec<Chain> = Vec::new();
        for &first in &order {
            if placed.contains_key(&first) {
                continue;
            }
            // Walk back to where the chain starts, then along it. (A loop of road ends where it began.)
            let (mut at, mut forward) = (first, true);
            let mut seen = HashSet::from([first]);
            loop {
                let (start, end) = ends_of(at);
                let (node, this_end) = if forward { (start, (at, false)) } else { (end, (at, true)) };
                let Some(previous) = next(node, this_end) else { break };
                if !seen.insert(previous.0) {
                    break;
                }
                at = previous.0;
                // Leaving it by its end means it runs the way we're walking back against... it arrives here.
                forward = previous.1;
            }
            let chain = chains.len() as u32;
            let mut along = 0.0;
            let mut list = Vec::new();
            loop {
                placed.insert(at, (chain, along, forward));
                list.push((at, forward));
                along += length_of(at);
                let (start, end) = ends_of(at);
                let (node, this_end) = if forward { (end, (at, true)) } else { (start, (at, false)) };
                let Some(following) = next(node, this_end) else { break };
                if placed.contains_key(&following.0) {
                    break;
                }
                at = following.0;
                forward = !following.1;
            }
            let swap = |d: [f64; 4], forward: bool| if forward { d } else { [d[1], d[0], d[3], d[2]] };
            let (head, head_way) = list[0];
            let (tail, tail_way) = list[list.len() - 1];
            let at_start = swap(joint(head, &sections[&head], !head_way), head_way);
            let at_end = swap(joint(tail, &sections[&tail], tail_way), tail_way);
            let edges = std::array::from_fn(|k| Edge { start: at_start[k], end: at_end[k] });
            chains.push(Chain { length: along.max(1e-3), edges });
        }
        let roads = sections
            .into_iter()
            .map(|(index, section)| {
                let (chain, from, forward) = placed[&index];
                (index, (section, OnChain { chain, from, forward }))
            })
            .collect();
        Self { chains, roads }
    }

    /// How far each of a road's four edges has moved `s` (0 to 1) along it, the road's own way round.
    pub fn moved(&self, place: &OnChain, length: f64, s: f64) -> [f64; 4] {
        let chain = &self.chains[place.chain as usize];
        let at = place.from + if place.forward { s * length } else { (1.0 - s) * length };
        let d: [f64; 4] = std::array::from_fn(|k| chain.edges[k].at(at, chain.length));
        if place.forward { d } else { [d[1], d[0], d[3], d[2]] }
    }
}

/// How far a road's ends have to move where two looks meet on a node with no prefab: each goes halfway. `None` when the
/// node isn't one of those.
pub fn halfway(
    own: &Section,
    arrives: bool,
    other: &Section,
    other_arrives: bool,
) -> [f64; 4] {
    let same_way = arrives != other_arrives;
    let flip = |s: [f64; 2]| if same_way { s } else { [s[1], s[0]] };
    let outer = flip(other.outer);
    let d = [(outer[0] - own.outer[0]) / 2.0, (outer[1] - own.outer[1]) / 2.0];
    let m = match (own.inner, other.inner.map(flip)) {
        (Some(a), Some(b)) => [(b[0] - a[0]) / 2.0, (b[1] - a[1]) / 2.0],
        _ => [0.0, 0.0],
    };
    [d[0], d[1], m[0], m[1]]
}

/// A road's surfaces as triangles along it, a stretch about `step` long each, for what passes over what.
pub fn faces(a: &Node, b: &Node, length: f64, section: &Section, kind: Kind, step: f64, out: &mut dyn FnMut(Kind, [[f64; 3]; 3])) {
    let steps = ((length / step).ceil() as usize).clamp(1, 100);
    let stations: Vec<([f64; 3], [f64; 2])> = (0..=steps).map(|i| frame(a, b, length, i as f64 / steps as f64, true)).collect();
    let [right, left] = section.outer;
    let mut bands: Vec<(Kind, f64, f64)> = match section.inner {
        Some([inner_right, inner_left]) => vec![(kind, inner_right, right), (kind, -left, -inner_left)],
        None => vec![(kind, -left, right)],
    };
    bands.extend(section.strips.iter().map(|strip| (Kind::Quiet, strip[0], strip[1])));
    for (kind, from, to) in bands {
        for pair in stations.windows(2) {
            let ((p, n), (q, m)) = (pair[0], pair[1]);
            let corner = |o: [f64; 3], r: [f64; 2], d: f64| [o[0] + r[0] * d, o[1], o[2] + r[1] * d];
            let (w, x, y, z) = (corner(p, n, from), corner(q, m, from), corner(q, m, to), corner(p, n, to));
            out(kind, [w, x, y]);
            out(kind, [w, y, z]);
        }
    }
}

/// What a road draws, along its stations.
pub struct RoadShape {
    /// Rings of world points (x, y, z): what each is, whether it's a band of its carriageway.
    pub rings: Vec<(Kind, Vec<[f64; 3]>)>,
}

/// The kind a road's carriageway is.
pub fn road_kind(look: &Look, class: RoadClass) -> Kind {
    if look.footpath {
        Kind::Quiet
    } else if class == RoadClass::Rail {
        Kind::Rail
    } else {
        Kind::Road(class)
    }
}

/// A road's outlines between two nodes: its carriageway to its eased edges (the stretches in a tunnel apart), what those
/// edges leave behind as quiet ground, and its strips.
#[allow(clippy::too_many_arguments)]
pub fn shape(
    a: &Node,
    b: &Node,
    length: f64,
    section: &Section,
    kind: Kind,
    tubes: &Tubes,
    tunnel: bool,
    moved: &dyn Fn(f64) -> [f64; 4],
) -> RoadShape {
    let segments = ((length / STATION).ceil() as usize).clamp(4, 400);
    let stations: Vec<([f64; 3], [f64; 2])> =
        (0..=segments).map(|i| frame(a, b, length, i as f64 / segments as f64, true)).collect();
    let offsets: Vec<[f64; 4]> = (0..=segments).map(|i| moved(i as f64 / segments as f64)).collect();
    let [outer_right, outer_left] = section.outer;
    let inner = section.inner.unwrap_or([0.0, 0.0]);
    let right_edge = |i: usize| outer_right + offsets[i][0];
    let left_edge = |i: usize| -(outer_left + offsets[i][1]);
    let median_right = |i: usize| inner[0] + offsets[i][2];
    let median_left = |i: usize| -(inner[1] + offsets[i][3]);
    let stretch = |start: usize, end: usize, from: &dyn Fn(usize) -> f64, to: &dyn Fn(usize) -> f64, lift: f64| -> Vec<[f64; 3]> {
        let side = |across: &dyn Fn(usize) -> f64| -> Vec<[f64; 3]> {
            let line: Vec<[f64; 3]> = (start..=end)
                .map(|i| {
                    let (p, n) = stations[i];
                    let d = across(i);
                    [p[0] + n[0] * d, p[1] + lift, p[2] + n[1] * d]
                })
                .collect();
            super::shapes::thin_line(&line, |p| [p[0], p[2]], EDGE_TOLERANCE)
        };
        let mut ring = side(from);
        let mut back = side(to);
        back.reverse();
        ring.extend(back);
        ring
    };
    let band = |from: &dyn Fn(usize) -> f64, to: &dyn Fn(usize) -> f64, lift: f64| stretch(0, segments, from, to, lift);
    // Which stretches of it run in a tunnel.
    let in_tunnel: Vec<bool> = match kind {
        Kind::Road(_) if tunnel => vec![true; segments],
        Kind::Road(_) if !tubes.is_empty() => {
            let line: Vec<[f64; 3]> = stations.iter().map(|station| station.0).collect();
            tubes.steps_inside(&line, (outer_right + outer_left) as f32)
        }
        _ => vec![false; segments],
    };
    let mut rings = Vec::new();
    let lift = 0.02;
    let mut start = 0;
    for step in 1..=segments {
        if step == segments || in_tunnel[step] != in_tunnel[start] {
            let kind = match kind {
                Kind::Road(class) if in_tunnel[start] => Kind::Tunnel(class),
                other => other,
            };
            if section.inner.is_some() {
                rings.push((kind, stretch(start, step, &median_right, &right_edge, lift)));
                rings.push((kind, stretch(start, step, &left_edge, &median_left, lift)));
            } else {
                rings.push((kind, stretch(start, step, &left_edge, &right_edge, lift)));
            }
            start = step;
        }
    }
    // Where an edge eases in to the road's own width, the surface it leaves is still there: quiet ground.
    let eased = |edge: usize| offsets.iter().any(|o| o[edge] < -EASE_FROM);
    if eased(0) {
        rings.push((Kind::Quiet, band(&right_edge, &|_| outer_right, lift)));
    }
    if eased(1) {
        rings.push((Kind::Quiet, band(&|_| -outer_left, &left_edge, lift)));
    }
    if section.inner.is_some() {
        if offsets.iter().any(|o| o[2] > EASE_FROM) {
            rings.push((Kind::Quiet, band(&|_| inner[0], &median_right, lift)));
        }
        if offsets.iter().any(|o| o[3] > EASE_FROM) {
            rings.push((Kind::Quiet, band(&median_left, &|_| -inner[1], lift)));
        }
    }
    for strip in &section.strips {
        rings.push((Kind::Quiet, band(&|_| strip[0], &|_| strip[1], strip[2])));
    }
    RoadShape { rings }
}

/// The sections of `map`'s roads, by item index, with their looks' classes.
pub fn road_sections(map: &Map, definitions: &Definitions, looks: &Looks) -> Vec<(usize, Section)> {
    map.items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let ItemData::Road { look, sides, city_sidewalks, flags, .. } = &item.data else { return None };
            // Hidden from the game's map, a road is still there: only a secret one has to be found first.
            if flags.secret || !definitions.road_looks.contains_key(look) {
                return None;
            }
            let table = looks.get(*look)?;
            let sidewalks = city_sidewalks.map_or([0.0, 0.0], sidewalk_widths);
            Some((index, looks.section(table, sides, sidewalks)?))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_edge_eases_over_twenty_times_its_difference() {
        let edge = Edge { start: 1.0, end: 0.0 };
        assert!((edge.at(0.0, 500.0) - 1.0).abs() < 1e-9);
        assert!((edge.at(10.0, 500.0) - 0.5).abs() < 1e-9);
        assert_eq!(edge.at(20.0, 500.0), 0.0);
    }

    #[test]
    fn differences_no_joint_has_are_left_alone() {
        assert_eq!(Edge { start: 0.04, end: 0.0 }.at(0.0, 100.0), 0.0);
        assert_eq!(Edge { start: 5.0, end: 0.0 }.at(0.0, 100.0), 0.0);
    }

    #[test]
    fn a_short_chain_wanting_the_same_both_ways_holds() {
        let edge = Edge { start: 1.0, end: 1.0 };
        assert!((edge.at(30.0, 60.0) - 1.0).abs() < 1e-9);
        // On a long one it comes back to the road's own width between.
        assert_eq!(edge.at(250.0, 500.0), 0.0);
    }

    fn look(attributes: &[(&str, &str)]) -> sii::Unit {
        sii::Unit {
            class: "road_look".into(),
            name: "road.test".into(),
            attributes: attributes.iter().map(|(key, value)| ((*key).to_owned(), (*value).to_owned())).collect(),
        }
    }

    #[test]
    fn an_undrawn_roads_lanes_are_where_the_game_has_them() {
        let right = ("lanes_right", "traffic_lane.slow_road");
        let left = ("lanes_left", "traffic_lane.slow_road");
        assert_eq!(lanes_reach(&look(&[right])), [0.0, 4.5]);
        assert_eq!(lanes_reach(&look(&[right, right])), [4.5, 4.5]);
        assert_eq!(lanes_reach(&look(&[right, right, right])), [4.5, 9.0]);
        assert_eq!(lanes_reach(&look(&[left])), [4.5, 0.0]);
        assert_eq!(lanes_reach(&look(&[left, right])), [4.5, 4.5]);
        let narrow = [left, right, ("lane_offsets_left", "(-1.5, 0.0)"), ("lane_offsets_right", "(-1.5, 0.0)")];
        assert_eq!(lanes_reach(&look(&narrow)), [3.0, 3.0]);
    }

    #[test]
    fn sidewalks_are_read_as_the_game_stores_them() {
        assert_eq!(sidewalk_widths(0b0100), [8.0, 4.0]);
        assert_eq!(sidewalk_widths(0b1011), [0.0, 2.0]);
    }
}
