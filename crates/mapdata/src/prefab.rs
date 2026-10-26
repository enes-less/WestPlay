//! Prefab descriptors (`.ppd`): the junctions, roundabouts and company yards placed on the map. The map
//! needs their nodes (where roads attach, to place and turn the prefab) and their map points: the
//! prefab's own small road graph and areas, as the game's map draws them; its spawn points, which mark
//! what a place offers (fuel, a service shop, a garage…); and its navigation curves, the lanes through
//! it, which the GPS route follows ([`Prefab::path`]).

use std::fmt;

use crate::reader::{OutOfBounds, Reader};

/// The prefab descriptor version of ETS2/ATS 1.5x as of 2026.
pub const SUPPORTED_VERSION: u32 = 25;
/// The oldest read: map mods' own prefabs made with an older editor (Bourges Updated's company yards, 22).
/// One older than today's is read only where its tables hold records of the sizes read here, measured by its
/// own offsets: so far only a spawn point's size was seen to differ ([`SPAWN_POINT_FIELD`]).
pub const OLDEST_VERSION: u32 = 22;

const NODE_SIZE: usize = 0x68;
/// A navigation curve: its name (a token); its flags at 8 ([`CURVE_SMALL_VEHICLES`], [`CURVE_LARGE_VEHICLES`];
/// 0x08 and 0x10 go with lanes bending right and left, the blinkers) and one more field; start and end
/// positions (x, y, z) at 0x10
/// and 0x1C; start and end rotations (quaternions, w x y z) at 0x28 and 0x38; its length at 0x48; the
/// curves it leads to (4 slots) at 0x4C and comes from at 0x5C, with their counts at 0x6C and 0x70; then
/// its semaphore, traffic rule and one more field.
const NAV_CURVE_SIZE: usize = 0x84;
/// A navigation curve's flags: cars may drive it, trucks may drive it (most lanes: both). Read from the game's
/// toll plazas (France, Italy, Austria, Poland, 2026-10-21): the booths on the left only cars, one further
/// right only trucks, the rightmost both, as the game's GPS sends a truck to the right.
const CURVE_SMALL_VEHICLES: u32 = 0x20;
const CURVE_LARGE_VEHICLES: u32 = 0x40;
const NAV_CURVE_LINK_SLOTS: usize = 4;
const MAP_POINT_SIZE: usize = 0x30;
/// Position (x, y, z), rotation (a quaternion), kind, and from version 24 one more field.
const SPAWN_POINT_SIZE: usize = 0x24;
/// The version from which a spawn point has that field, and its size before.
const SPAWN_POINT_FIELD: u32 = 24;
const OLD_SPAWN_POINT_SIZE: usize = 0x20;
/// A trigger point: its action (a token) at 4, its position (x, y, z) at 0x1C; the rest (range, links to
/// the other corners of its area) isn't read.
const TRIGGER_POINT_SIZE: usize = 0x30;
/// Lanes in and out of each prefab node (unused slots hold -1).
const NODE_LANE_SLOTS: usize = 8;
const MAP_POINT_NEIGHBOUR_SLOTS: usize = 6;

#[derive(Debug, PartialEq)]
pub enum PrefabError {
    UnsupportedVersion(u32),
    /// An older version whose tables hold records of other sizes than those read.
    OtherLayout(u32),
    OutOfBounds(OutOfBounds),
    /// A map point refers to one that doesn't exist.
    BadNeighbour {
        point: usize,
        neighbour: i32,
    },
}

impl fmt::Display for PrefabError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion(v) => {
                write!(f, "prefab version {v} (only {OLDEST_VERSION} to {SUPPORTED_VERSION} are supported)")
            }
            Self::OtherLayout(v) => write!(f, "prefab version {v} laid out in a way WestPlay doesn't read"),
            Self::OutOfBounds(error) => error.fmt(f),
            Self::BadNeighbour { point, neighbour } => write!(f, "map point {point} has neighbour {neighbour}"),
        }
    }
}

impl std::error::Error for PrefabError {}

impl From<OutOfBounds> for PrefabError {
    fn from(error: OutOfBounds) -> Self {
        Self::OutOfBounds(error)
    }
}

#[derive(Debug)]
pub struct Prefab {
    pub nodes: Vec<PrefabNode>,
    pub map_points: Vec<MapPoint>,
    pub spawn_points: Vec<SpawnPoint>,
    pub triggers: Vec<Trigger>,
    pub curves: Vec<NavCurve>,
}

/// A lane through a prefab, one direction, from `start` to `end` (in the prefab's own space, metres),
/// leaving and arriving along `start_direction` and `end_direction`.
#[derive(Clone, Debug, PartialEq)]
pub struct NavCurve {
    pub start: [f32; 2],
    pub end: [f32; 2],
    /// How high its start and its end are in the prefab's own space, metres (y): a junction with a road
    /// over another inside it has its lanes at both heights, its slip roads climbing between them.
    pub heights: [f32; 2],
    pub start_direction: [f32; 2],
    pub end_direction: [f32; 2],
    pub length: f32,
    /// The curves it leads on to.
    pub next: Vec<usize>,
    /// Whether trucks may drive it: a toll plaza's booths are for cars or for trucks (the game's GPS keeps a
    /// truck to its own: [`Prefab::lanes`]).
    pub trucks: bool,
}

/// A corner of an area where the game does something as the player drives in (`hud_parking`: offers
/// rest).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Trigger {
    /// In the prefab's own space, metres (x, z).
    pub position: [f32; 2],
    /// What happens there, a token.
    pub action: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpawnPoint {
    /// In the prefab's own space, metres (x, z).
    pub position: [f32; 2],
    pub kind: SpawnKind,
}

/// What a spawn point marks, by the game's own numbering (the kinds a map shows are named; trailer
/// spots, company docks and the like are `Other`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpawnKind {
    /// A fuel pump.
    Fuel,
    /// A service shop (repairs, upgrades).
    Service,
    WeighStation,
    TruckDealer,
    Hotel,
    Parking,
    /// The player's own garage.
    Garage,
    /// A garage for sale.
    GarageForSale,
    Recruitment,
    Other(u32),
}

impl SpawnKind {
    fn from_raw(raw: u32) -> Self {
        match raw {
            3 => Self::Fuel,
            4 => Self::Service,
            6 => Self::WeighStation,
            7 => Self::TruckDealer,
            8 => Self::Hotel,
            10 => Self::Parking,
            14 => Self::Garage,
            15 => Self::GarageForSale,
            16 => Self::Recruitment,
            other => Self::Other(other),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PrefabNode {
    /// In the prefab's own space, metres (x, z).
    pub position: [f32; 2],
    /// How high it is in the prefab's own space, metres (y).
    pub height: f32,
    /// Direction of the road at the node, in the prefab's own space: the way into the prefab.
    pub direction: [f32; 2],
    /// Lanes in and out at the node.
    pub lanes: u32,
    /// The navigation curves that start at the node (into the prefab) and end at it (out of it).
    pub curves_in: Vec<usize>,
    pub curves_out: Vec<usize>,
}

#[derive(Clone, Debug)]
pub struct MapPoint {
    pub position: [f32; 2],
    pub kind: MapPointKind,
    /// Extra width between the two directions, metres (a central reservation).
    pub lane_offset: f32,
    /// For area points: its colour (0 road, 1 light, 2 dark, 3 green, as map areas) and whether the game
    /// draws it over roads (WestPlay's map doesn't: it would hide the roads through a filling station).
    pub area_color: u8,
    pub area_over_roads: bool,
    /// Part of the prefab's navigation graph: not drawn where the prefab has a drawn graph of its own
    /// (it then repeats it), but drawn where it hasn't (the game's map draws it then).
    pub navigation_only: bool,
    /// The prefab node this point is at, if any (its lane count applies to [`MapPointKind::Road`] `None`).
    pub control_node: Option<usize>,
    pub neighbours: Vec<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MapPointKind {
    /// A road point with this many lanes in total, or `None` when it takes its node's.
    Road(Option<u32>),
    /// A corner of an area polygon (a car park, a yard, grass).
    Area,
    /// A point marking a place in the prefab (where the game's map puts its icon), linked to nothing.
    Marker,
}

pub fn parse(data: &[u8]) -> Result<Prefab, PrefabError> {
    let mut r = Reader::new(data);
    let version = r.u32()?;
    if !(OLDEST_VERSION..=SUPPORTED_VERSION).contains(&version) {
        return Err(PrefabError::UnsupportedVersion(version));
    }
    let node_count = r.u32()?;
    let curve_count = r.u32()?;
    r.skip(4 * 2)?; // signs, semaphores
    let spawn_point_count = r.u32()?;
    r.skip(4 * 2)?; // terrain points and variants
    let map_point_count = r.u32()?;
    let trigger_count = r.u32()?;
    r.skip(4 * 2)?; // intersections and one more count
    let node_offset = r.u32()? as usize;
    let curve_offset = r.u32()? as usize;
    let sign_offset = r.u32()? as usize;
    r.skip(4)?; // semaphores
    let spawn_point_offset = r.u32()? as usize;
    let terrain_point_offset = r.u32()? as usize;
    r.skip(4 * 2)?;
    let map_point_offset = r.u32()? as usize;
    let trigger_offset = r.u32()? as usize;
    let intersection_offset = r.u32()? as usize;
    let spawn_point_size = if version < SPAWN_POINT_FIELD { OLD_SPAWN_POINT_SIZE } else { SPAWN_POINT_SIZE };
    // An older one only if each table it's read from holds records of the size read (each runs to where the
    // next one starts).
    if version < SUPPORTED_VERSION {
        let tables = [
            (node_count, node_offset, curve_offset, NODE_SIZE),
            (curve_count, curve_offset, sign_offset, NAV_CURVE_SIZE),
            (spawn_point_count, spawn_point_offset, terrain_point_offset, spawn_point_size),
            (map_point_count, map_point_offset, trigger_offset, MAP_POINT_SIZE),
            (trigger_count, trigger_offset, intersection_offset, TRIGGER_POINT_SIZE),
        ];
        if tables.iter().any(|&(count, start, end, size)| end.checked_sub(start) != Some(count as usize * size)) {
            return Err(PrefabError::OtherLayout(version));
        }
    }

    let mut nodes = Vec::with_capacity(room_for(node_count, data, NODE_SIZE));
    for i in 0..node_count as usize {
        let mut r = Reader::at(data, node_offset + i * NODE_SIZE + 0x10);
        let x = r.f32()?;
        let y = r.f32()?;
        let z = r.f32()?;
        let dx = r.f32()?;
        r.skip(4)?;
        let dz = r.f32()?;
        let mut slots = [[0i32; NODE_LANE_SLOTS]; 2];
        for slot in slots.iter_mut().flatten() {
            *slot = r.i32()?;
        }
        let curves = |slots: &[i32]| -> Vec<usize> {
            slots
                .iter()
                .filter(|&&curve| curve >= 0 && (curve as u32) < curve_count)
                .map(|&curve| curve as usize)
                .collect()
        };
        let lanes = slots.iter().flatten().filter(|&&slot| slot != -1).count() as u32;
        nodes.push(PrefabNode {
            position: [x, z],
            height: y,
            direction: [dx, dz],
            lanes,
            curves_in: curves(&slots[0]),
            curves_out: curves(&slots[1]),
        });
    }

    let mut map_points = Vec::with_capacity(room_for(map_point_count, data, MAP_POINT_SIZE));
    for i in 0..map_point_count as usize {
        let mut r = Reader::at(data, map_point_offset + i * MAP_POINT_SIZE);
        let [_, look, color, _, control, nav, _, _] = r.bytes(8)?.try_into().unwrap();
        let x = r.f32()?;
        r.skip(4)?;
        let z = r.f32()?;
        let mut slots = [0i32; MAP_POINT_NEIGHBOUR_SLOTS];
        for slot in &mut slots {
            *slot = r.i32()?;
        }
        let count = (r.u32()? as usize).min(MAP_POINT_NEIGHBOUR_SLOTS);
        let mut neighbours = Vec::with_capacity(count);
        for &neighbour in &slots[..count] {
            if neighbour < 0 || neighbour as u32 >= map_point_count {
                return Err(PrefabError::BadNeighbour { point: i, neighbour });
            }
            neighbours.push(neighbour as usize);
        }
        let kind = match look & 0x0f {
            // Linked to nothing: no road or area, but a place marked (where the game's map puts an icon).
            _ if neighbours.is_empty() => MapPointKind::Marker,
            13 => MapPointKind::Area,
            14 => MapPointKind::Road(None),
            lanes => MapPointKind::Road(Some(lane_count(lanes))),
        };
        let lane_offset = match look >> 4 {
            1 => 1.0,
            2 => 2.0,
            3 => 5.0,
            4 => 10.0,
            5 => 15.0,
            6 => 20.0,
            7 => 25.0,
            _ => 0.0,
        };
        let control_node = (control != 0 && control.is_power_of_two()).then(|| control.trailing_zeros() as usize);
        map_points.push(MapPoint {
            position: [x, z],
            kind,
            lane_offset,
            area_color: area_color(color),
            area_over_roads: color & 0x01 != 0,
            navigation_only: nav & 0x02 != 0,
            control_node,
            neighbours,
        });
    }
    let mut spawn_points = Vec::with_capacity(room_for(spawn_point_count, data, spawn_point_size));
    for i in 0..spawn_point_count as usize {
        let mut r = Reader::at(data, spawn_point_offset + i * spawn_point_size);
        let x = r.f32()?;
        r.skip(4)?;
        let z = r.f32()?;
        r.skip(4 * 4)?; // rotation
        spawn_points.push(SpawnPoint { position: [x, z], kind: SpawnKind::from_raw(r.u32()?) });
    }
    let mut triggers = Vec::with_capacity(room_for(trigger_count, data, TRIGGER_POINT_SIZE));
    for i in 0..trigger_count as usize {
        let mut r = Reader::at(data, trigger_offset + i * TRIGGER_POINT_SIZE + 4);
        let action = r.u64()?;
        let mut r = Reader::at(data, trigger_offset + i * TRIGGER_POINT_SIZE + 0x1C);
        let x = r.f32()?;
        r.skip(4)?;
        triggers.push(Trigger { position: [x, r.f32()?], action });
    }
    let mut curves = Vec::with_capacity(room_for(curve_count, data, NAV_CURVE_SIZE));
    for i in 0..curve_count as usize {
        let flags = Reader::at(data, curve_offset + i * NAV_CURVE_SIZE + 0x08).u32()?;
        // Neither cars nor trucks marked: anyone may.
        let trucks = flags & CURVE_LARGE_VEHICLES != 0 || flags & (CURVE_SMALL_VEHICLES | CURVE_LARGE_VEHICLES) == 0;
        let mut r = Reader::at(data, curve_offset + i * NAV_CURVE_SIZE + 0x10);
        let mut point = || -> Result<([f32; 2], f32), OutOfBounds> {
            let (x, y) = (r.f32()?, r.f32()?);
            Ok(([x, r.f32()?], y))
        };
        let ((start, start_height), (end, end_height)) = (point()?, point()?);
        let mut rotation =
            || -> Result<[f32; 2], OutOfBounds> { Ok(forward([r.f32()?, r.f32()?, r.f32()?, r.f32()?])) };
        let (start_direction, end_direction) = (rotation()?, rotation()?);
        let length = r.f32()?;
        let mut slots = [0i32; NAV_CURVE_LINK_SLOTS];
        for slot in &mut slots {
            *slot = r.i32()?;
        }
        let mut r = Reader::at(data, curve_offset + i * NAV_CURVE_SIZE + 0x6C);
        let count = (r.u32()? as usize).min(NAV_CURVE_LINK_SLOTS);
        let next = slots[..count]
            .iter()
            .filter(|&&curve| curve >= 0 && (curve as u32) < curve_count)
            .map(|&curve| curve as usize)
            .collect();
        curves.push(NavCurve {
            start,
            end,
            heights: [start_height, end_height],
            start_direction,
            end_direction,
            length,
            next,
            trucks,
        });
    }
    Ok(Prefab { nodes, map_points, spawn_points, triggers, curves })
}

/// The way a rotation (a quaternion, w x y z) faces on the ground: the game's forward, (0, 0, -1), turned
/// by it, made a unit vector across the ground (x, z).
fn forward([w, x, y, z]: [f32; 4]) -> [f32; 2] {
    let (fx, fz) = (-2.0 * (x * z + w * y), -(1.0 - 2.0 * (x * x + y * y)));
    let length = fx.hypot(fz);
    if length > 0.0 { [fx / length, fz / length] } else { [0.0, -1.0] }
}

/// About a point every this many metres along a navigation curve.
const PATH_STEP: f32 = 4.0;

/// How many records of `size` bytes to reserve room for when a header says `count`: no more than the file
/// has room for (a damaged count would otherwise reserve gigabytes, which aborts rather than erring; the
/// records past the file's end then fail to read, as an error).
fn room_for(count: u32, data: &[u8], size: usize) -> usize {
    (count as usize).min(data.len() / size)
}

impl Prefab {
    /// The lanes of the way through the prefab from its node `from` to its node `to`, as the game's GPS
    /// takes it for a truck: the shortest chain of its navigation curves (one way each) starting at `from`
    /// and ending at `to` on lanes trucks may drive (a toll plaza's truck booths, not its cars' straight
    /// ahead: owner 2026-10-21), else on any; and its length (metres). `None` if no lane leads there.
    pub fn lanes(&self, from: usize, to: usize) -> Option<(Vec<usize>, f32)> {
        self.chain(from, to, true).or_else(|| self.chain(from, to, false))
    }

    /// The shortest chain of curves from node `from` to node `to` ([`Prefab::lanes`]), on lanes trucks may
    /// drive only if `trucks`.
    fn chain(&self, from: usize, to: usize, trucks: bool) -> Option<(Vec<usize>, f32)> {
        use std::cmp::Reverse;
        use std::collections::BinaryHeap;
        let (from, to) = (self.nodes.get(from)?, self.nodes.get(to)?);
        let allowed = |curve: usize| !trucks || self.curves[curve].trucks;
        // Dijkstra over the curves: each reached at its end, by the length of the chain to there, and the
        // curve before it.
        let mut reached: Vec<Option<(f32, Option<usize>)>> = vec![None; self.curves.len()];
        let mut queue = BinaryHeap::new();
        for &curve in from.curves_in.iter().filter(|&&curve| allowed(curve)) {
            let length = self.curves[curve].length;
            if reached[curve].is_none_or(|(best, _)| length < best) {
                reached[curve] = Some((length, None));
                queue.push(Reverse((length.to_bits(), curve)));
            }
        }
        let mut last = None;
        while let Some(Reverse((length, curve))) = queue.pop() {
            // Lengths are never negative, so their bits order as they do.
            let length = f32::from_bits(length);
            if reached[curve].is_some_and(|(best, _)| length > best) {
                continue;
            }
            if to.curves_out.contains(&curve) {
                last = Some((curve, length));
                break;
            }
            for &next in self.curves[curve].next.iter().filter(|&&next| allowed(next)) {
                let total = length + self.curves[next].length;
                if reached[next].is_none_or(|(best, _)| total < best) {
                    reached[next] = Some((total, Some(curve)));
                    queue.push(Reverse((total.to_bits(), next)));
                }
            }
        }
        let (last, length) = last?;
        let mut chain = vec![last];
        while let Some((_, Some(before))) = reached[*chain.last()?] {
            chain.push(before);
        }
        chain.reverse();
        Some((chain, length))
    }

    /// The way through the prefab from its node `from` to its node `to` ([`Prefab::lanes`]), as points in
    /// the prefab's own space (each curve the game's road curve between its ends).
    pub fn path(&self, from: usize, to: usize) -> Option<Vec<[f32; 2]>> {
        let (chain, _) = self.lanes(from, to)?;
        let mut points = vec![self.curves[chain[0]].start];
        for &curve in &chain {
            let curve = &self.curves[curve];
            let chord = (curve.end[0] - curve.start[0]).hypot(curve.end[1] - curve.start[1]);
            let segments = ((curve.length.max(chord) / PATH_STEP).ceil() as usize).clamp(1, 64);
            points.extend((1..=segments).map(|i| {
                let s = i as f32 / segments as f32;
                hermite(curve.start, curve.start_direction, curve.end, curve.end_direction, chord, s)
            }));
        }
        Some(points)
    }

    /// Whether its lanes run round in a loop: a roundabout (a junction's lanes otherwise only lead on,
    /// from its ends to its ends).
    pub fn has_ring(&self) -> bool {
        // Depth first, a curve met again while still on the way from it closes a loop.
        #[derive(Clone, Copy, PartialEq)]
        enum Seen {
            Not,
            OnTheWay,
            Done,
        }
        let mut seen = vec![Seen::Not; self.curves.len()];
        for first in 0..self.curves.len() {
            if seen[first] != Seen::Not {
                continue;
            }
            let mut stack = vec![(first, 0)];
            seen[first] = Seen::OnTheWay;
            while let Some(&mut (curve, ref mut next)) = stack.last_mut() {
                match self.curves[curve].next.get(*next).copied() {
                    Some(on) => {
                        *next += 1;
                        match seen.get(on).copied() {
                            Some(Seen::OnTheWay) => return true,
                            Some(Seen::Not) => {
                                seen[on] = Seen::OnTheWay;
                                stack.push((on, 0));
                            }
                            _ => {}
                        }
                    }
                    None => {
                        seen[curve] = Seen::Done;
                        stack.pop();
                    }
                }
            }
        }
        false
    }
}

impl NavCurve {
    /// `steps` + 1 points evenly along the curve, from its start to its end, each with how high it is
    /// (climbing evenly from its start's height to its end's).
    pub fn points(&self, steps: usize) -> Vec<([f32; 2], f32)> {
        let steps = steps.max(1);
        (0..=steps)
            .map(|step| {
                let s = step as f32 / steps as f32;
                let at = hermite(self.start, self.start_direction, self.end, self.end_direction, self.length, s);
                (at, self.heights[0] + (self.heights[1] - self.heights[0]) * s)
            })
            .collect()
    }
}

/// The game's curve between `p0` and `p1`, leaving along `f0` and arriving along `f1` with tangents
/// `length` long, at `s` (0..1) along it.
fn hermite(p0: [f32; 2], f0: [f32; 2], p1: [f32; 2], f1: [f32; 2], length: f32, s: f32) -> [f32; 2] {
    let (s2, s3) = (s * s, s * s * s);
    let (h00, h10, h01, h11) = (2.0 * s3 - 3.0 * s2 + 1.0, s3 - 2.0 * s2 + s, -2.0 * s3 + 3.0 * s2, s3 - s2);
    [
        h00 * p0[0] + h10 * f0[0] * length + h01 * p1[0] + h11 * f1[0] * length,
        h00 * p0[1] + h10 * f0[1] * length + h01 * p1[1] + h11 * f1[1] * length,
    ]
}

/// An area point's colour, in map areas' numbering (1 light, 2 dark, 3 green), from its colour flags.
fn area_color(flags: u8) -> u8 {
    if flags & 0x02 != 0 {
        1
    } else if flags & 0x04 != 0 {
        2
    } else if flags & 0x08 != 0 {
        3
    } else {
        1
    }
}

/// The lanes of a road map point (both directions), from its look's low four bits.
fn lane_count(look: u8) -> u32 {
    match look {
        0 => 1,
        1 => 2,
        2 => 4,
        3 => 6,
        4 => 8,
        5 => 5,
        6 => 7,
        8 => 3,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_past_the_files_room_are_an_error_not_an_abort() {
        // A header saying four billion of everything, in a file of a few bytes: no gigabytes reserved, and
        // the first record past the end is an error.
        let mut data = SUPPORTED_VERSION.to_le_bytes().to_vec();
        data.extend([u32::MAX.to_le_bytes(); 20].concat());
        assert!(parse(&data).is_err());
        assert_eq!(room_for(u32::MAX, &data, NODE_SIZE), data.len() / NODE_SIZE);
        assert_eq!(room_for(2, &[0; 1000], NODE_SIZE), 2);
    }

    /// A descriptor of `version` with one filling station's spawn point in a table of records `size` bytes
    /// long, and nothing else.
    fn one_spawn_point(version: u32, size: usize) -> Vec<u8> {
        const HEADER: u32 = 0x60;
        let mut data = version.to_le_bytes().to_vec();
        for count in [0u32, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0] {
            data.extend(count.to_le_bytes());
        }
        let after = HEADER + size as u32;
        // Nodes, curves, signs, semaphores and spawn points start at the header's end; the rest after the point.
        for offset in [HEADER, HEADER, HEADER, HEADER, HEADER, after, after, after, after, after, after, after] {
            data.extend(offset.to_le_bytes());
        }
        let mut point = vec![0u8; size];
        point[0x1C..0x20].copy_from_slice(&3u32.to_le_bytes()); // after its place and rotation, its kind
        data.extend(point);
        data
    }

    #[test]
    fn reads_an_older_descriptor_laid_out_as_read() {
        // Before 24 a spawn point is 0x20 bytes, from 24 on 0x24 (Bourges Updated's company yards are 22).
        for (version, size) in [(22, 0x20), (23, 0x20), (24, 0x24), (SUPPORTED_VERSION, 0x24)] {
            let prefab = parse(&one_spawn_point(version, size)).unwrap();
            assert_eq!(prefab.spawn_points[0].kind, SpawnKind::Fuel, "{version}");
        }
        // An older one whose tables hold records of other sizes than those read: refused, not misread.
        assert_eq!(parse(&one_spawn_point(22, 0x24)).unwrap_err(), PrefabError::OtherLayout(22));
        assert_eq!(parse(&one_spawn_point(21, 0x20)).unwrap_err(), PrefabError::UnsupportedVersion(21));
    }

    fn node(curves_in: Vec<usize>, curves_out: Vec<usize>) -> PrefabNode {
        PrefabNode { height: 0.0, position: [0.0, 0.0], direction: [0.0, -1.0], lanes: 2, curves_in, curves_out }
    }

    fn curve(start: [f32; 2], end: [f32; 2], next: Vec<usize>) -> NavCurve {
        let length = (end[0] - start[0]).hypot(end[1] - start[1]);
        let direction = [(end[0] - start[0]) / length, (end[1] - start[1]) / length];
        NavCurve {
            heights: [0.0, 0.0],
            start,
            end,
            start_direction: direction,
            end_direction: direction,
            length,
            next,
            trucks: true,
        }
    }

    #[test]
    fn a_truck_takes_the_truck_booth_not_the_cars_straight_ahead() {
        // A toll plaza from node 0 (z 50) to node 1 (z -50): straight ahead a cars' booth (x 0), to the right
        // a longer way through the trucks' booth (x 10).
        let cars = || NavCurve { trucks: false, ..curve([0.0, 50.0], [0.0, -50.0], vec![]) };
        let prefab = |curves: Vec<NavCurve>, nodes| Prefab {
            nodes,
            map_points: Vec::new(),
            spawn_points: Vec::new(),
            triggers: Vec::new(),
            curves,
        };
        let plaza = prefab(
            vec![
                cars(),                                      // 0: the cars' booth
                curve([0.0, 50.0], [10.0, 20.0], vec![2]),   // 1: over to the right
                curve([10.0, 20.0], [10.0, -20.0], vec![3]), // 2: the trucks' booth
                curve([10.0, -20.0], [0.0, -50.0], vec![]),  // 3: back
            ],
            vec![node(vec![0, 1], vec![]), node(vec![], vec![0, 3])],
        );
        assert_eq!(plaza.lanes(0, 1).map(|(chain, _)| chain), Some(vec![1, 2, 3]));
        // A way only cars may drive is still a way, when it's the only one.
        let only_cars = prefab(vec![cars()], vec![node(vec![0], vec![]), node(vec![], vec![0])]);
        assert_eq!(only_cars.lanes(0, 1).map(|(chain, _)| chain), Some(vec![0]));
    }

    #[test]
    fn finds_the_way_through_along_the_lanes_one_way_round() {
        // A roundabout's ring, one way round (0 → 1 → 2 → 3 → 0), entered from node 0 and left at node 1,
        // which lies "behind" the entry: the way there is three quarters of the ring, not back against it.
        let ring = [[0.0, 10.0], [10.0, 0.0], [0.0, -10.0], [-10.0, 0.0]];
        let curves = vec![
            curve([0.0, 30.0], ring[0], vec![1]), // 0: in from node 0
            curve(ring[0], ring[1], vec![2]),
            curve(ring[1], ring[2], vec![3]),
            curve(ring[2], ring[3], vec![4, 6]),
            curve(ring[3], ring[0], vec![1, 5]),
            curve(ring[0], [0.0, 30.0], vec![]),  // 5: out at node 0
            curve(ring[3], [-30.0, 0.0], vec![]), // 6: out at node 1
        ];
        let prefab = Prefab {
            nodes: vec![node(vec![0], vec![5]), node(vec![], vec![6])],
            map_points: Vec::new(),
            spawn_points: Vec::new(),
            triggers: Vec::new(),
            curves,
        };
        let path = prefab.path(0, 1).expect("a way");
        assert_eq!(path.first(), Some(&[0.0, 30.0]));
        assert_eq!(path.last(), Some(&[-30.0, 0.0]));
        // Round by the ring's other three corners.
        for corner in [ring[1], ring[2], ring[3]] {
            assert!(path.iter().any(|p| (p[0] - corner[0]).hypot(p[1] - corner[1]) < 0.01), "{corner:?}");
        }
        // No lane leads back into the prefab from where it ends.
        assert_eq!(prefab.path(1, 0), None);
    }

    #[test]
    fn turns_a_rotation_into_the_way_it_faces() {
        let half = std::f32::consts::FRAC_1_SQRT_2;
        // Turned a quarter left about the vertical: facing -x.
        let [x, z] = forward([half, 0.0, half, 0.0]);
        assert!((x + 1.0).abs() < 1e-6 && z.abs() < 1e-6);
        // Unturned: the game's forward, -z.
        assert_eq!(forward([1.0, 0.0, 0.0, 0.0]), [0.0, -1.0]);
    }
}
