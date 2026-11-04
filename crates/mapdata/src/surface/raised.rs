//! The roads at their real heights. The map's tiles are flat, but the game's roads aren't: the files here have the roads
//! that stand in the air as the game has them, for the cab to draw so (ui/src/cabin/maps/raisedRoads.ts), with the ramps up
//! to them. Everything else stays on the flat map, which is whole without these files.
//!
//! Only intersections and bridges rise, with their ramps. The map has no relief: a road following the land, up a hill or
//! down it, lies on the flat map however high it is.
//!
//! A road crossing over another stands as high over it as it is in the game ([`CROSSING`]).
//!
//! A bridge is a road with no ground beside it either side, a clear height down or more, and something under it. A road's
//! terrain stops at its edges, and the game's water goes on under the land, so what's under a road says nothing by itself:
//! it's what's beside it that does. The map is flat, so a bridge can't stand as high as it is over a valley: it stands as
//! high as it is over the line between its two ends. A level bridge stays level, an arched one is as high as it arches, and
//! one from low ground to higher ground is as high as it is over that line. Water is on the map, so over water a bridge
//! stands as high as it is over the water, and its ramps take it down to the land.
//!
//! From those a road's height falls as the road does, down its ramps to the ground, and where the road doesn't fall as
//! fast it falls anyway, at [`EASE`]. A road goes into a tunnel on the ground, and gets down to it the same way. Nothing else
//! rises.
//!
//! And a bridge's model comes down as its road does, and nothing stands higher than the road over it, drawn as it is
//! ([`Decks`]): a bridge's columns are under its road.
//!
//! Written a tile of [`FILE_ZOOM`] to a file, for the cab to load and draw what's in view. A
//! file, gzipped (it comes to a third), little endian: how many vertices (u32), the height its vertices' heights are from
//! (f32), the vertex each of [`BUCKETS`] ends at (u32 each), then the vertices in threes, the lowest triangles first.
//! Drawn in that order, bucket by bucket across the files, the higher surface ends up on top with no depth written, so
//! what the map draws after them isn't cut by them. A vertex is where it is in the tile ([`PAST`] over each side, u16
//! each), how high it's drawn (cm, u16), its height in the game (dm from the file's, i16; on a side, the way out from
//! the road instead, i8 each), its kind ([`Mesh::kind`], u8) and flags (u8).
//!
//! The flat map has these roads on the ground too, for when they aren't shown: what's of them in its tiles goes by the file
//! here that has them (tile.rs), and the cab takes it off the map once it has that file, so a road isn't drawn twice. What
//! passes under them isn't theirs and stays. Over water the cab paints the water back, past the road's edges by its sides
//! ([`OVER_WATER`], known once the map's ground has been worked out: [`mark_water`]).

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use super::ground::Painted;
use super::levels::{self, Face, FaceIndex};
use super::{BLOCK_ZOOM, Kind, Scene, on_the_map, rotate, tile};
use crate::buildings::Building;
use crate::geometry::Point;
use crate::sea::contains;
use crate::sector::ItemData;
use crate::world::RoadClass;

/// The folder of the files, in the surfaces'.
pub const FOLDER: &str = "raised";
/// The zoom of the files' tiles: a couple of kilometres across.
pub(super) const FILE_ZOOM: u8 = 14;
/// A road's faces are about this long: it's the top of it that counts (0.2 m off on the tightest of ramps).
const FACE_STEP: f64 = 8.0;
/// How steeply a road's drawn height falls at the least, going away from what's in the air, of the way along: the steepest
/// of the game's own ramps (8% and over: 6 km of the 485 km coming off what's in the air in Europe).
const EASE: f32 = 0.08;
/// How far past where a road's ground starts it looks for the ground beside it, m: past the sliver of terrain the game
/// leaves along some bridges (0.2 m to 1.6 m), short of what a bank falls in.
const BESIDE: f64 = 3.0;
/// How far past where a road's ground starts it looks for water, m: past the walkways the game lays beside a bridge as
/// terrain of their own (the Golden Gate's reach 17 m from the middle of the road).
const OUT_OVER_WATER: f64 = 20.0;
/// How far over another a road is crossing it, m: Phoenix's Stack, the game's tallest, is 23 m. Further than this it's a
/// road high over a valley, or over one deep under the map, which is no intersection: it's a bridge if it's one at all.
const CROSSING: f32 = 30.0;
/// How far either way a road's end reaches across it, m: past the edges of the widest road. The corners of a road's end
/// and of a junction's arm aren't at the node, they're on that line, and what's joined there has to meet.
const AT_NODE: f64 = 16.0;
/// From this high, a raised road's side shows, m: its depth goes to nothing with its height, so it doesn't stop dead.
const SIDE_FROM: f64 = 0.1;
/// How deep a raised road's side is, m.
const SIDE: f64 = 1.5;
/// Draw order: how high one bucket is, m (under the 2.5 m that makes one surface over another).
const BUCKET: f64 = 2.0;
const BUCKETS: usize = 32;
/// How far past its tile a file's vertices may lie, of the tile: a face belongs to the file its middle is in.
const PAST: f64 = 0.0625;
/// A vertex's flags: it's on a road's side; the bottom of it; it's over water.
const ON_SIDE: u8 = 1;
const BOTTOM: u8 = 2;
const OVER_WATER: u8 = 8;
/// What a raised road passes over is looked for in cells this big, m.
const UNDER_CELL: f64 = 16.0;
/// How far apart a building's ground is tried for a road over it, m: under the narrowest of columns.
const UNDER_STEP: f32 = 2.0;
/// How far under a road what's under it stops, m: at the road's own height the two would fight for the same pixels.
const UNDER_ROAD: f32 = 0.2;

type Cell = (i32, i32);

/// The items of the map, as what a walk along its roads needs of them: `ends`, an item's nodes; `items_at`, a node's items;
/// `height`, a node's; `length`, an item's between two of its nodes; `bridge`, whether an item is one.
struct Roads<'a> {
    ends: &'a dyn Fn(u32) -> Vec<u64>,
    items_at: &'a dyn Fn(u64) -> Vec<u32>,
    height: &'a dyn Fn(u64) -> f32,
    length: &'a dyn Fn(u32, u64, u64) -> f32,
    bridge: &'a dyn Fn(u32) -> bool,
}

/// How far a bridge comes down going on from `node` through the item `first`, and how far along that is.
fn climb_along(roads: &Roads, node: u64, first: u32) -> (f32, f32) {
    let start = (roads.height)(node);
    let (mut most, mut far) = (0.0f32, 0.0f32);
    let mut seen = HashSet::new();
    let mut walking = vec![(first, node, 0.0)];
    while let Some((item, from, along)) = walking.pop() {
        if !(roads.bridge)(item) || !seen.insert(item) {
            continue;
        }
        for next in (roads.ends)(item).into_iter().filter(|&next| next != from) {
            let along = along + (roads.length)(item, from, next);
            let down = start - (roads.height)(next);
            if down > most {
                (most, far) = (down, along);
            }
            walking.extend((roads.items_at)(next).into_iter().filter(|&other| other != item).map(|other| (other, next, along)));
        }
    }
    (most, far)
}

/// How high a node of a bridge stands, `climbs` each way how far it comes down and how far along that is: over the line
/// between the two ends that are furthest down.
fn rise_of(mut climbs: Vec<(f32, f32)>) -> f32 {
    climbs.sort_by(|a, b| b.0.total_cmp(&a.0));
    match climbs[..] {
        [(most, far), (next, near), ..] if far + near > 0.0 => (most * near + next * far) / (far + near),
        _ => 0.0,
    }
}

/// How high each node stands, from what's in the air: `rises`. `ways`: where a node goes on to, and how long the way is.
fn spread(rises: &mut HashMap<u64, f32>, height: &dyn Fn(u64) -> f32, ways: &dyn Fn(u64) -> Vec<(u64, f32)>) {
    let mut walking: Vec<u64> = rises.keys().copied().collect();
    while let Some(node) = walking.pop() {
        let rise = rises[&node];
        for (next, length) in ways(node) {
            let left = rise - (height(node) - height(next)).max(EASE * length);
            if left > rises.get(&next).copied().unwrap_or(0.0) {
                rises.insert(next, left);
                walking.push(next);
            }
        }
    }
}

/// Brings `rises` down so that a road gets to `ends`, on the ground, as [`spread`] has it fall.
fn down_to(rises: &mut HashMap<u64, f32>, ends: Vec<u64>, height: &dyn Fn(u64) -> f32, ways: &dyn Fn(u64) -> Vec<(u64, f32)>) {
    let mut walking = ends;
    while let Some(node) = walking.pop() {
        let rise = rises.get(&node).copied().unwrap_or(0.0);
        for (next, length) in ways(node) {
            let most = rise + (height(next) - height(node)).max(EASE * length);
            if most < rises.get(&next).copied().unwrap_or(0.0) {
                rises.insert(next, most);
                walking.push(next);
            }
        }
    }
}

/// How the items of the map join, to walk along its roads.
struct Joints<'a> {
    scene: &'a Scene<'a>,
    /// Each node with an item of it, in order of the nodes.
    items: Vec<(u64, u32)>,
}

impl<'a> Joints<'a> {
    fn new(scene: &'a Scene<'a>, items: &[u32]) -> Self {
        let mut joints: Vec<(u64, u32)> = items.iter().flat_map(|&item| scene.ends(item as usize).into_iter().map(move |node| (node, item))).collect();
        joints.sort_unstable();
        Self { scene, items: joints }
    }

    fn items_at(&self, node: u64) -> Vec<u32> {
        let from = self.items.partition_point(|&(at, _)| at < node);
        self.items[from..].iter().take_while(|&&(at, _)| at == node).map(|&(_, item)| item).collect()
    }

    fn height(&self, node: u64) -> f32 {
        self.scene.map.nodes.get(&node).map_or(0.0, |node| node.position[1])
    }

    /// How long `item` is from one of its nodes to another.
    fn length(&self, item: u32, from: u64, to: u64) -> f32 {
        let map = self.scene.map;
        match &map.items[item as usize].data {
            ItemData::Road { length, .. } => *length,
            _ => match (map.nodes.get(&from), map.nodes.get(&to)) {
                (Some(a), Some(b)) => (a.position[0] - b.position[0]).hypot(a.position[2] - b.position[2]),
                _ => 0.0,
            },
        }
    }

    /// Whether `item` is a road the game doesn't show.
    fn invisible(&self, item: u32) -> bool {
        let ItemData::Road { look, .. } = &self.scene.map.items[item as usize].data else { return false };
        self.scene.tables.definitions.road_looks.get(look).is_some_and(|look| look.invisible)
    }

    /// Whether `item` is a junction: a prefab of three ends or more.
    fn junction(&self, item: u32) -> bool {
        matches!(&self.scene.map.items[item as usize].data, ItemData::Prefab { nodes, .. } if nodes.len() > 2)
    }

    /// How high `node`, of a bridge, stands. Not at all at a tunnel: a road goes into one on the ground.
    fn rise_through(&self, node: u64, bridges: &HashMap<u32, f32>, tunnels: &HashSet<u32>) -> f32 {
        let here = self.items_at(node);
        if here.iter().any(|item| tunnels.contains(item)) {
            return 0.0;
        }
        let ends = |item: u32| self.scene.ends(item as usize);
        let items_at = |node: u64| self.items_at(node);
        let height = |node: u64| self.height(node);
        let length = |item: u32, from: u64, to: u64| self.length(item, from, to);
        let bridge = |item: u32| bridges.contains_key(&item);
        let roads = Roads { ends: &ends, items_at: &items_at, height: &height, length: &length, bridge: &bridge };
        // Nothing stands higher than what's under it.
        let clear = here.iter().filter_map(|item| bridges.get(item)).fold(0.0, |most: f32, &clear| most.max(clear));
        rise_of(here.into_iter().map(|item| climb_along(&roads, node, item)).collect()).min(clear)
    }

    /// A prefab between two pieces of a bridge is a piece of it.
    fn add_prefabs(&self, air: &mut HashMap<u32, f32>, items: &[u32]) {
        let mut found = Vec::new();
        for &item in items.iter().filter(|item| !air.contains_key(item) && matches!(self.scene.map.items[**item as usize].data, ItemData::Prefab { .. })) {
            let arm = |node: u64| self.items_at(node).into_iter().filter(|other| *other != item).filter_map(|other| air.get(&other).copied()).reduce(f32::max);
            let arms: Vec<f32> = self.scene.ends(item as usize).into_iter().filter_map(arm).collect();
            if arms.len() > 1 {
                found.push((item, arms.into_iter().fold(0.0, f32::max)));
            }
        }
        air.extend(found);
    }

    /// What each node stands from, and the items that have one that rises: what crosses over a road, as high as it does,
    /// bridges, and from there down their ramps.
    fn lows(&self, crossings: &HashMap<u32, f32>, bridges: &HashMap<u32, f32>, waters: &HashMap<u32, f32>, tunnels: &HashSet<u32>, threads: usize) -> (HashMap<u64, f32>, HashSet<u32>) {
        let mut nodes: Vec<u64> = bridges.keys().flat_map(|&item| self.scene.ends(item as usize)).collect();
        nodes.sort_unstable();
        nodes.dedup();
        let found: Vec<(u64, f32)> = std::thread::scope(|scope| {
            let working: Vec<_> = nodes
                .chunks(nodes.len().div_ceil(threads).max(1))
                .map(|nodes| scope.spawn(move || nodes.iter().map(|&node| (node, self.rise_through(node, bridges, tunnels))).collect::<Vec<_>>()))
                .collect();
            working.into_iter().flat_map(|thread| thread.join().unwrap_or_default()).collect()
        });
        let mut rises: HashMap<u64, f32> = found.into_iter().filter(|&(_, rise)| rise > 0.0).collect();
        // A way into a tunnel gets there on the ground.
        let in_tunnel = |node: u64| self.items_at(node).iter().any(|item| tunnels.contains(item));
        for (&item, &clear) in crossings {
            for node in self.scene.ends(item as usize).into_iter().filter(|&node| !in_tunnel(node)) {
                let rise = rises.entry(node).or_insert(clear);
                *rise = rise.max(clear);
            }
        }
        let ways = |node: u64| {
            let mut ways = Vec::new();
            for item in self.items_at(node) {
                ways.extend(self.scene.ends(item as usize).into_iter().filter(|&next| next != node && !in_tunnel(next)).map(|next| (next, self.length(item, node, next))));
            }
            ways
        };
        spread(&mut rises, &|node| self.height(node), &ways);
        let mouths: Vec<u64> = self.items.iter().filter(|(_, item)| tunnels.contains(item)).map(|&(node, _)| node).collect();
        down_to(&mut rises, mouths.clone(), &|node| self.height(node), &ways);
        // Over water a bridge that comes down to its ends stands as high as it is, and comes down by its first
        // junction, a tunnel or the end of the road the player sees: the land behind it is as flat as the rest of the
        // map. A level one stays level: it's the land that's high there, from rim to rim.
        let mut over_water: HashMap<u64, f32> = HashMap::new();
        let ends = |item: u32| self.scene.ends(item as usize);
        let items_at = |node: u64| self.items_at(node);
        let height = |node: u64| self.height(node);
        let length = |item: u32, from: u64, to: u64| self.length(item, from, to);
        let bridge = |item: u32| bridges.contains_key(&item);
        let roads = Roads { ends: &ends, items_at: &items_at, height: &height, length: &length, bridge: &bridge };
        // How far a bridge comes down from `item` to the furthest down of its ends.
        let comes_down = |item: u32| {
            let at = |node: u64| self.items_at(node).into_iter().map(move |other| (node, other));
            self.scene.ends(item as usize).into_iter().flat_map(at).map(|(node, other)| climb_along(&roads, node, other).0).fold(0.0f32, f32::max)
        };
        for (&item, &water) in waters.iter().filter(|&(&item, _)| f64::from(comes_down(item)) >= levels::CLEAR) {
            for node in self.scene.ends(item as usize).into_iter().filter(|&node| !in_tunnel(node)) {
                let rise = over_water.entry(node).or_insert(0.0);
                *rise = rise.max(self.height(node) - water);
            }
        }
        let shown_ways = |node: u64| {
            let mut ways = Vec::new();
            for item in self.items_at(node).into_iter().filter(|&item| !self.invisible(item) && !self.junction(item)) {
                ways.extend(self.scene.ends(item as usize).into_iter().filter(|&next| next != node && !in_tunnel(next)).map(|next| (next, self.length(item, node, next))));
            }
            ways
        };
        spread(&mut over_water, &|node| self.height(node), &shown_ways);
        let ends: Vec<u64> = mouths.into_iter().chain(self.items.iter().filter(|(_, item)| self.invisible(*item) || self.junction(*item)).map(|&(node, _)| node)).collect();
        for node in &ends {
            over_water.remove(node);
        }
        down_to(&mut over_water, ends, &|node| self.height(node), &shown_ways);
        for (node, rise) in over_water {
            let most = rises.entry(node).or_insert(rise);
            *most = most.max(rise);
        }
        let raised = rises.keys().flat_map(|&node| self.items_at(node)).filter(|item| !tunnels.contains(item)).collect();
        let lows = rises.iter().map(|(&node, rise)| (node, self.height(node) - rise)).collect();
        (lows, raised)
    }
}

/// What `nodes` have at a place among them: each is where it is (x and z), the way across the road there, and a height.
/// Nearer ones count for more, by how far it is to the road's end at the node ([`AT_NODE`]): along a road it goes evenly from
/// one to the other, all the way, and at an end it's that node's, corners and all.
fn between(nodes: &[([f64; 2], [f64; 2], f32)], v: [f64; 3]) -> f32 {
    let (mut sum, mut weight) = (0.0, 0.0);
    for &([x, z], [across_x, across_z], value) in nodes {
        let (out_x, out_z) = (v[0] - x, v[2] - z);
        let along = (out_x * across_x + out_z * across_z).clamp(-AT_NODE, AT_NODE);
        let distance = (out_x - along * across_x).hypot(out_z - along * across_z).max(0.01);
        sum += f64::from(value) / distance;
        weight += 1.0 / distance;
    }
    if weight > 0.0 { (sum / weight) as f32 } else { f32::MAX }
}

/// A face of a road in the air, by its corners: x, z, its height in the game, and how high it's drawn.
type Deck = [[f32; 4]; 3];

/// Where `deck` is at (x, z), if it's there at all: its height in the game, and how high it's drawn.
fn on_deck(deck: &Deck, x: f32, z: f32) -> Option<[f32; 2]> {
    let [a, b, c] = deck;
    let area = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
    if area.abs() < 1e-6 {
        return None;
    }
    let u = ((x - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (z - a[1])) / area;
    let v = ((b[0] - a[0]) * (z - a[1]) - (x - a[0]) * (b[1] - a[1])) / area;
    (u >= 0.0 && v >= 0.0 && u + v <= 1.0).then(|| [2, 3].map(|k| a[k] + u * (b[k] - a[k]) + v * (c[k] - a[k])))
}

/// The roads in the air, as drawn, for what stands under them to be drawn no higher.
#[derive(Default)]
pub struct Decks {
    decks: Vec<Deck>,
    /// The decks in each cell of [`UNDER_CELL`].
    cells: HashMap<Cell, Vec<u32>>,
}

impl Decks {
    /// A building as it's to be drawn. A bridge's comes down with the road nearest it: the game has it as high as the
    /// road, and the map as high as the road's drawn. Then what's under a road stops under it; what's lifted over the
    /// road (the bracing of an arch over it) is over it.
    pub fn fit(&self, building: &mut Building) {
        let Some(ring) = building.rings.first() else { return };
        if building.bridge
            && let Some(low) = self.low_near(ring)
        {
            let down = (low - building.ground).max(0.0);
            building.height -= down;
            building.base = (building.base - down).max(0.0);
        }
        if let Some(top) = self.over(ring, building.ground)
            && building.base < top
        {
            building.height = building.height.min(top);
        }
    }

    /// What the road nearest (x, z) stands from, of those a cell or so off it.
    pub fn low_at(&self, x: f32, z: f32) -> Option<f32> {
        self.low_near(&[[x, z]])
    }

    /// How high a point of a bridge's model, at (x, z) and `y` in the game, is drawn: as high over the map as it is
    /// over what the road nearest it stands from (`low`, where no road is near: what the road nearest the model's
    /// node stands from), and under any road over it that it's under in the game. Nothing under the map.
    pub fn bridge_point(&self, x: f32, z: f32, y: f32, low: Option<f32>) -> f32 {
        let Some(low) = self.low_near(&[[x, z]]).or(low) else { return 0.0 };
        let mut top = y - low;
        let cell = ((f64::from(x) / UNDER_CELL).floor() as i32, (f64::from(z) / UNDER_CELL).floor() as i32);
        for [deck_y, drawn] in self.cells.get(&cell).into_iter().flatten().filter_map(|&deck| on_deck(&self.decks[deck as usize], x, z)) {
            if y <= deck_y + 0.5 {
                top = top.min(drawn - UNDER_ROAD);
            }
        }
        top.max(0.0)
    }

    /// What the road nearest the middle of `ring` stands from, of those a cell or so off it.
    fn low_near(&self, ring: &[Point]) -> Option<f32> {
        let (mut x0, mut x1) = (f32::MAX, f32::MIN);
        let (mut z0, mut z1) = (f32::MAX, f32::MIN);
        for &[x, z] in ring {
            (x0, x1, z0, z1) = (x0.min(x), x1.max(x), z0.min(z), z1.max(z));
        }
        let (x, z) = ((x0 + x1) / 2.0, (z0 + z1) / 2.0);
        let (col, row) = ((f64::from(x) / UNDER_CELL).floor() as i32, (f64::from(z) / UNDER_CELL).floor() as i32);
        let mut nearest = (f32::MAX, None);
        let cells = (col - 1..=col + 1).flat_map(|c| (row - 1..=row + 1).map(move |r| (c, r)));
        for list in cells.filter_map(|cell| self.cells.get(&cell)) {
            for corner in list.iter().flat_map(|&deck| self.decks[deck as usize]) {
                let distance = (corner[0] - x).hypot(corner[1] - z);
                if distance < nearest.0 {
                    nearest = (distance, Some(corner[2] - corner[3]));
                }
            }
        }
        nearest.1
    }

    fn new(decks: Vec<Deck>) -> Self {
        let mut cells: HashMap<Cell, Vec<u32>> = HashMap::new();
        for (index, deck) in decks.iter().enumerate() {
            let cell = |at: f32| (f64::from(at) / UNDER_CELL).floor() as i32;
            let (x0, x1) = (deck[0][0].min(deck[1][0]).min(deck[2][0]), deck[0][0].max(deck[1][0]).max(deck[2][0]));
            let (z0, z1) = (deck[0][1].min(deck[1][1]).min(deck[2][1]), deck[0][1].max(deck[1][1]).max(deck[2][1]));
            for x in cell(x0)..=cell(x1) {
                for z in cell(z0)..=cell(z1) {
                    cells.entry((x, z)).or_default().push(index as u32);
                }
            }
        }
        Self { decks, cells }
    }

    /// How high the road over what stands on `ring` is drawn, where it's lowest over it. What stands there is placed at
    /// `ground` in the game, which is a bridge's own road for a bridge's model: only a road a clear height under that
    /// passes under it. `None` with no road over it.
    pub fn over(&self, ring: &[Point], ground: f32) -> Option<f32> {
        let (mut x0, mut x1) = (f32::MAX, f32::MIN);
        let (mut z0, mut z1) = (f32::MAX, f32::MIN);
        for &[x, z] in ring {
            (x0, x1, z0, z1) = (x0.min(x), x1.max(x), z0.min(z), z1.max(z));
        }
        let mut lowest = None;
        let mut x = x0 + UNDER_STEP / 2.0;
        while x < x1 {
            let mut z = z0 + UNDER_STEP / 2.0;
            while z < z1 {
                let cell = ((f64::from(x) / UNDER_CELL).floor() as i32, (f64::from(z) / UNDER_CELL).floor() as i32);
                if let Some(list) = self.cells.get(&cell)
                    && contains(ring, [x, z])
                {
                    for [y, drawn] in list.iter().filter_map(|&deck| on_deck(&self.decks[deck as usize], x, z)) {
                        if f64::from(ground - y) < levels::CLEAR {
                            lowest = Some(lowest.map_or(drawn - UNDER_ROAD, |low: f32| low.min(drawn - UNDER_ROAD)));
                        }
                    }
                }
                z += UNDER_STEP;
            }
            x += UNDER_STEP;
        }
        lowest
    }
}

/// A file, being made.
struct Mesh {
    tile: (u32, u32),
    base: f64,
    triangles: Vec<(f64, [[u8; Mesh::VERTEX]; 3])>,
}

impl Mesh {
    const VERTEX: usize = 10;

    /// The file of the tile (x, z) is in.
    fn of(files: &mut HashMap<(u32, u32), Mesh>, x: f64, z: f64, base: f64) -> &mut Mesh {
        let [x, y] = tile::tile_of(x, z, FILE_ZOOM);
        files.entry((x, y)).or_insert_with(|| Mesh { tile: (x, y), base, triangles: Vec::new() })
    }

    /// What the cab knows a surface by.
    fn kind(kind: Kind) -> Option<u8> {
        match kind {
            Kind::Road(RoadClass::Minor) => Some(0),
            Kind::Road(RoadClass::Local) => Some(1),
            Kind::Road(RoadClass::Trunk) => Some(2),
            Kind::Road(RoadClass::Motorway) => Some(3),
            Kind::Rail | Kind::Road(RoadClass::Rail) => Some(4),
            _ => None,
        }
    }

    fn vertex(&self, p: [f64; 3], height: f64, kind: u8, flags: u8) -> [u8; Self::VERTEX] {
        self.vertex_with(p, height, ((p[1] - self.base) * 10.0).round().clamp(-32768.0, 32767.0) as i16, kind, flags)
    }

    /// A vertex of a side, going out from the road `out` (x, z, of length 1).
    fn side_vertex(&self, p: [f64; 3], height: f64, out: [f64; 2], kind: u8, flags: u8) -> [u8; Self::VERTEX] {
        let [x, z] = out.map(|part| (part * 127.0).round() as i8 as u8);
        self.vertex_with(p, height, i16::from_le_bytes([x, z]), kind, flags)
    }

    fn vertex_with(&self, p: [f64; 3], height: f64, game: i16, kind: u8, flags: u8) -> [u8; Self::VERTEX] {
        let [x, y] = tile::mercator(p[0], p[2]);
        let side = f64::from(1u32 << FILE_ZOOM);
        let part = |at: f64, tile: u32| ((at * side - f64::from(tile) + PAST) / (1.0 + 2.0 * PAST) * 65535.0).round().clamp(0.0, 65535.0) as u16;
        let drawn = (height * 100.0).round().clamp(0.0, 65535.0) as u16;
        let mut out = [0; Self::VERTEX];
        out[0..2].copy_from_slice(&part(x, self.tile.0).to_le_bytes());
        out[2..4].copy_from_slice(&part(y, self.tile.1).to_le_bytes());
        out[4..6].copy_from_slice(&drawn.to_le_bytes());
        out[6..8].copy_from_slice(&game.to_le_bytes());
        out[8] = kind;
        out[9] = flags;
        out
    }

    fn encode(mut self) -> Vec<u8> {
        self.triangles.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut ends = [0u32; BUCKETS];
        for (height, _) in &self.triangles {
            ends[((height / BUCKET) as usize).min(BUCKETS - 1)] += 3;
        }
        let mut total = 0;
        for end in &mut ends {
            total += *end;
            *end = total;
        }
        let mut out = Vec::with_capacity(8 + 4 * BUCKETS + self.triangles.len() * 3 * Self::VERTEX);
        out.extend_from_slice(&total.to_le_bytes());
        out.extend_from_slice(&(self.base as f32).to_le_bytes());
        for end in ends {
            out.extend_from_slice(&end.to_le_bytes());
        }
        for (_, corners) in self.triangles {
            out.extend(corners.into_iter().flatten());
        }
        out
    }
}

/// The roads in `faces` that rise, all of each, with their sides: those of the block `tile`, as the files of its tiles.
/// `lows`: where each face's corners stand from: what the map's flat ground is to them, nothing at what doesn't rise.
fn meshes(tile: (u32, u32), faces: &[Face], lows: &[[f32; 3]]) -> Vec<((u32, u32), Vec<u8>)> {
    let inside = |x: f64, z: f64| tile::tile_of(x, z, BLOCK_ZOOM) == [tile.0, tile.1];
    let mut shown: Vec<(&Face, [f64; 3], u8)> = Vec::new();
    for (face, low) in faces.iter().zip(lows).filter(|(_, low)| !low.contains(&f32::MAX)) {
        let heights = [0, 1, 2].map(|i| (face.t[i][1] - f64::from(low[i])).max(0.0));
        let Some(kind) = Mesh::kind(face.kind) else { continue };
        shown.push((face, heights, kind));
    }
    // A side is along an edge only one face has, among all of them, the block's edge being no edge.
    let key = |v: [f64; 3]| ((v[0] * 10.0).round() as i64, (v[1] * 10.0).round() as i64, (v[2] * 10.0).round() as i64);
    let mut edges: HashMap<_, (u32, usize, usize)> = HashMap::new();
    for (index, (face, ..)) in shown.iter().enumerate() {
        for from in 0..3 {
            let (a, b) = (key(face.t[from]), key(face.t[(from + 1) % 3]));
            edges.entry(if a < b { (a, b) } else { (b, a) }).or_insert((0, index, from)).0 += 1;
        }
    }
    let middle = |t: &[[f64; 3]; 3]| ((t[0][0] + t[1][0] + t[2][0]) / 3.0, (t[0][2] + t[1][2] + t[2][2]) / 3.0);
    let ours: Vec<_> = shown.iter().filter(|(face, ..)| { let (x, z) = middle(&face.t); inside(x, z) }).collect();
    if ours.is_empty() {
        return Vec::new();
    }
    let base = ours.iter().map(|(face, ..)| face.t[0][1]).sum::<f64>() / ours.len() as f64;
    let mut files: HashMap<(u32, u32), Mesh> = HashMap::new();
    for (face, heights, kind) in ours {
        let (x, z) = middle(&face.t);
        let mesh = Mesh::of(&mut files, x, z, base);
        let corners = [0, 1, 2].map(|i| mesh.vertex(face.t[i], heights[i], *kind, 0));
        mesh.triangles.push(((heights[0] + heights[1] + heights[2]) / 3.0, corners));
    }
    for &(count, index, from) in edges.values() {
        let (face, heights, kind) = &shown[index];
        let to = (from + 1) % 3;
        let (a, b) = (face.t[from], face.t[to]);
        let (high_a, high_b) = (heights[from], heights[to]);
        if count != 1 || high_a.min(high_b) < SIDE_FROM || !inside((a[0] + b[0]) / 2.0, (a[2] + b[2]) / 2.0) {
            continue;
        }
        let (low_a, low_b) = ((high_a - SIDE).max(0.0), (high_b - SIDE).max(0.0));
        // Away from the face's third corner.
        let third = face.t[(to + 1) % 3];
        let (along_x, along_z) = (b[0] - a[0], b[2] - a[2]);
        let length = along_x.hypot(along_z).max(1e-6);
        let across = [along_z / length, -along_x / length];
        let side = across[0] * (third[0] - a[0]) + across[1] * (third[2] - a[2]);
        let out = if side > 0.0 { [-across[0], -across[1]] } else { across };
        let mesh = Mesh::of(&mut files, (a[0] + b[0]) / 2.0, (a[2] + b[2]) / 2.0, base);
        let top = |p, height| mesh.side_vertex(p, height, out, *kind, ON_SIDE);
        let bottom = |p, height| mesh.side_vertex(p, height, out, *kind, ON_SIDE | BOTTOM);
        let quad = [
            ((high_a + high_b + low_b) / 3.0, [top(a, high_a), top(b, high_b), bottom(b, low_b)]),
            ((high_a + low_b + low_a) / 3.0, [top(a, high_a), bottom(b, low_b), bottom(a, low_a)]),
        ];
        mesh.triangles.extend(quad);
    }
    files.into_iter().map(|(tile, mesh)| (tile, mesh.encode())).collect()
}

fn gzipped(bytes: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::with_capacity(bytes.len() / 2), flate2::Compression::default());
    encoder.write_all(bytes)?;
    encoder.finish()
}

fn gunzipped(bytes: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(bytes).read_to_end(&mut out)?;
    Ok(out)
}

/// How far `at` (x, y, z) is over the nearest of `surfaces` a clear height or more under it.
fn clear_over(at: [f64; 3], surfaces: &FaceIndex) -> Option<f32> {
    let [x, y, z] = at;
    surfaces.heights(x, z).filter(|height| y - height >= levels::CLEAR).reduce(f64::max).map(|height| (y - height) as f32)
}

/// How far `at` (x, y, z) is over the lowest of `roads` a clear height or more under it: the top of a stack of them stands
/// on the ground, not on the road next under it.
fn clear_over_roads(at: [f64; 3], roads: &FaceIndex) -> Option<f32> {
    let [x, y, z] = at;
    roads.heights(x, z).filter(|height| y - height >= levels::CLEAR).reduce(f64::min).map(|height| (y - height) as f32)
}

/// How far a road at `at` is in the air: with `sides`, the places either side of it, off `ground` as far as a clear height
/// down, over what's of `surfaces` under it.
fn in_the_air_at(at: [f64; 3], sides: [[f64; 2]; 2], ground: &FaceIndex, surfaces: &FaceIndex) -> Option<f32> {
    let beside = |[x, z]: [f64; 2]| ground.heights(x, z).any(|height| at[1] - height < levels::CLEAR);
    if beside(sides[0]) || beside(sides[1]) { None } else { clear_over(at, surfaces) }
}

/// How high the water is at `side`, beside a road at `at`, if nothing's over it there and it's a clear height down.
fn water_beside(at: [f64; 3], [x, z]: [f64; 2], ground: &FaceIndex, water: &FaceIndex) -> Option<f64> {
    let top = ground.heights(x, z).reduce(f64::max)?;
    (at[1] - top >= levels::CLEAR && water.heights(x, z).any(|height| (height - top).abs() < 0.01)).then_some(top)
}

/// What [`in_the_air`] finds of a block.
#[derive(Default)]
struct Found {
    /// What crosses over a road, with how far over it.
    crossings: Vec<(u32, f32)>,
    /// Bridges, with how far at the most they are over what's under them.
    bridges: Vec<(u32, f32)>,
    /// Bridges with water next under them, with the water's height.
    waters: Vec<(u32, f32)>,
    tunnels: Vec<u32>,
}

/// Of the block `tile`'s `items`, what crosses over a road, what's a bridge and what's in a tunnel. What's off the game's
/// map is no surface here: scenery roads run under the land.
fn in_the_air(scene: &Scene, tile: (u32, u32), items: &[u32]) -> Found {
    let (mut found, mut shown, mut quads) = (Found::default(), Vec::new(), Vec::new());
    let (mut ground, mut roads, mut water) = (Vec::new(), Vec::new(), Vec::new());
    for &item in items {
        if scene.in_tunnel(item as usize) {
            found.tunnels.push(item);
            continue;
        }
        if !on_the_map(&scene.map.items[item as usize]) {
            continue;
        }
        quads.clear();
        scene.painted(item as usize, |_| true, &mut quads);
        for quad in &quads {
            let [a, b, c, d] = quad.corners;
            let faces = [[a, b, c], [a, c, d]].map(|t| Face { t, kind: Kind::Quiet, near: false });
            ground.extend(faces);
            if matches!(quad.paint, Painted::Water) {
                water.extend(faces);
            }
        }
        scene.add_prefab_ground(item as usize, &mut ground);
        let mut part = Vec::new();
        scene.add_faces(item as usize, FACE_STEP, &mut part);
        roads.extend(part.iter().copied().filter(|face| Mesh::kind(face.kind).is_some()));
        ground.extend(part.into_iter().filter(|face| face.kind == Kind::Quiet));
        shown.push(item);
    }
    // The ground's and the roads' faces together, indexed in place.
    let (on_ground, on_roads, on_surfaces, on_water) = (FaceIndex::new(&ground), FaceIndex::new(&roads), FaceIndex::both(&ground, &roads), FaceIndex::new(&water));
    let inside = |x: f64, z: f64| tile::tile_of(x, z, BLOCK_ZOOM) == [tile.0, tile.1];
    for item in shown {
        let index = item as usize;
        let (mut crossing, mut bridge, mut waters): (Vec<f32>, Vec<f32>, Vec<f32>) = (Vec::new(), Vec::new(), Vec::new());
        let mut over_roads = |at: [f64; 3], bridge: &mut Vec<f32>| {
            if let Some(clear) = clear_over_roads(at, &on_roads) {
                if clear <= CROSSING { crossing.push(clear) } else { bridge.push(clear) }
            }
        };
        if matches!(scene.map.items[index].data, ItemData::Road { .. }) {
            let wide = scene.stations(index, FACE_STEP, OUT_OVER_WATER);
            for ((at, sides), (_, wide)) in scene.stations(index, FACE_STEP, BESIDE).into_iter().zip(wide).filter(|((at, _), _)| inside(at[0], at[2])) {
                over_roads(at, &mut bridge);
                bridge.extend(in_the_air_at(at, sides, &on_ground, &on_surfaces));
                if let [Some(a), Some(b)] = wide.map(|side| water_beside(at, side, &on_ground, &on_water)) {
                    bridge.push((at[1] - a.min(b)) as f32);
                    waters.push(a.min(b) as f32);
                }
            }
        } else {
            let mut part = Vec::new();
            scene.add_faces(index, FACE_STEP, &mut part);
            for [a, b, c] in part.iter().filter(|face| Mesh::kind(face.kind).is_some()).map(|face| face.t) {
                let middle = [0, 1, 2].map(|k| (a[k] + b[k] + c[k]) / 3.0);
                if inside(middle[0], middle[2]) {
                    over_roads(middle, &mut bridge);
                }
            }
        }
        found.crossings.extend(crossing.into_iter().reduce(f32::max).map(|clear| (item, clear)));
        found.bridges.extend(bridge.into_iter().reduce(f32::max).map(|clear| (item, clear)));
        found.waters.extend(waters.into_iter().reduce(f32::min).map(|height| (item, height)));
    }
    found
}

/// What's worked out of the roads for the files.
pub(super) struct Heights {
    /// What each node stands from.
    stands: HashMap<u64, f32>,
    /// The items that rise.
    pub(super) raised: HashSet<u32>,
    /// What's in the air, of those that rise or not.
    air: HashMap<u32, f32>,
    tunnels: HashSet<u32>,
}

/// The files of the block `tile`, of its `items`, and the roads of it in the air as drawn, into `decks`.
fn block(scene: &Scene, heights: &Heights, tile: (u32, u32), items: &[u32], decks: &Mutex<Vec<Deck>>) -> Vec<((u32, u32), Vec<u8>)> {
    let Heights { stands, raised, air, tunnels } = heights;
    let over = |item: &u32| raised.contains(item) || air.contains_key(item);
    if !items.iter().any(over) {
        return Vec::new();
    }
    let inside = |t: &[[f64; 3]; 3]| tile::tile_of((t[0][0] + t[1][0] + t[2][0]) / 3.0, (t[0][2] + t[1][2] + t[2][2]) / 3.0, BLOCK_ZOOM) == [tile.0, tile.1];
    let (mut faces, mut lows, mut found) = (Vec::new(), Vec::new(), Vec::new());
    for &item in items.iter().filter(|item| !tunnels.contains(item)) {
        let mut part = Vec::new();
        scene.add_faces(item as usize, FACE_STEP, &mut part);
        part.retain(|face| Mesh::kind(face.kind).is_some());
        // The height of what stays on the flat map isn't anybody's.
        let nodes: Vec<([f64; 2], [f64; 2], f32)> = if raised.contains(&item) {
            let map = scene.map;
            let at = |uid: u64| {
                let node = map.nodes.get(&uid)?;
                let at = node.position;
                let right = rotate(node.rotation.map(f64::from), [1.0, 0.0, 0.0]);
                let long = right[0].hypot(right[2]).max(1e-9);
                Some(([f64::from(at[0]), f64::from(at[2])], [right[0] / long, right[2] / long], stands.get(&uid).copied().unwrap_or(at[1])))
            };
            scene.ends(item as usize).into_iter().filter_map(at).collect()
        } else {
            Vec::new()
        };
        let from = lows.len();
        lows.extend(part.iter().map(|face| face.t.map(|v| between(&nodes, v))));
        if over(&item) {
            let corner = |v: [f64; 3], low: f32| [v[0] as f32, v[2] as f32, v[1] as f32, (v[1] - f64::from(low)).max(0.0) as f32];
            found.extend(part.iter().zip(&lows[from..]).filter(|(face, _)| inside(&face.t)).map(|(face, low)| [0, 1, 2].map(|i| corner(face.t[i], low[i]))));
        }
        faces.append(&mut part);
    }
    decks.lock().unwrap_or_else(|e| e.into_inner()).append(&mut found);
    meshes(tile, &faces, &lows)
}

/// `work` for each of `blocks`, on `threads`, saying how far it's got.
fn each_block(blocks: &[((u32, u32), Vec<u32>)], threads: usize, stop: &AtomicBool, progress: &mut dyn FnMut(f32), work: &(dyn Fn((u32, u32), &[u32]) + Sync)) {
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let budget = super::Budget::of(blocks);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                while let Some((tile, items)) = blocks.get(next.fetch_add(1, Ordering::Relaxed)) {
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let _room = budget.take(items.len(), stop);
                    work(*tile, items);
                    done.fetch_add(1, Ordering::Relaxed);
                }
            });
        }
        while done.load(Ordering::Relaxed) < blocks.len() && !stop.load(Ordering::Relaxed) {
            progress(done.load(Ordering::Relaxed) as f32 / blocks.len() as f32);
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    });
}

/// What rises of what the map draws, by block.
pub(super) fn heights(scene: &Scene, blocks: &[((u32, u32), Vec<u32>)], threads: usize, stop: &AtomicBool, progress: &mut dyn FnMut(f32)) -> Heights {
    let mut items: Vec<u32> = blocks.iter().flat_map(|(_, items)| items.iter().copied()).collect();
    items.sort_unstable();
    items.dedup();
    let joints = Joints::new(scene, &items);
    let gathered: Mutex<Found> = Mutex::new(Found::default());
    each_block(blocks, threads, stop, progress, &|tile, items| {
        let found = in_the_air(scene, tile, items);
        let mut gathered = gathered.lock().unwrap_or_else(|e| e.into_inner());
        gathered.crossings.extend(found.crossings);
        gathered.bridges.extend(found.bridges);
        gathered.waters.extend(found.waters);
        gathered.tunnels.extend(found.tunnels);
    });
    let gathered = gathered.into_inner().unwrap_or_else(|e| e.into_inner());
    let most = |found: Vec<(u32, f32)>| {
        let mut all: HashMap<u32, f32> = HashMap::new();
        for (item, clear) in found {
            let height = all.entry(item).or_insert(clear);
            *height = height.max(clear);
        }
        all
    };
    let (crossings, mut bridges) = (most(gathered.crossings), most(gathered.bridges));
    let mut waters: HashMap<u32, f32> = HashMap::new();
    for (item, height) in gathered.waters {
        let lowest = waters.entry(item).or_insert(height);
        *lowest = lowest.min(height);
    }
    let tunnels: HashSet<u32> = gathered.tunnels.into_iter().collect();
    joints.add_prefabs(&mut bridges, &items);
    let (stands, raised) = joints.lows(&crossings, &bridges, &waters, &tunnels, threads);
    let air: HashMap<u32, f32> = crossings.iter().chain(&bridges).map(|(&item, &clear)| (item, clear)).collect();
    Heights { stands, raised, air, tunnels }
}

/// Writes the files of what rises under `dir`, and their list. `blocks`: what the map draws, by block: how many files,
/// their bytes, and the roads in the air.
pub(super) fn write(scene: &Scene, heights: &Heights, blocks: &[((u32, u32), Vec<u32>)], dir: &Path, threads: usize, stop: &AtomicBool, progress: &mut dyn FnMut(f32)) -> std::io::Result<(usize, u64, Decks)> {
    let written: Mutex<(Vec<(u32, u32)>, u64)> = Mutex::new((Vec::new(), 0));
    let failure: Mutex<Option<std::io::Error>> = Mutex::new(None);
    let dir = dir.join(FOLDER);
    let decks: Mutex<Vec<Deck>> = Mutex::new(Vec::new());
    each_block(blocks, threads, stop, progress, &|tile, items| {
        for (tile, bytes) in block(scene, heights, tile, items, &decks) {
            let folder = dir.join(tile.0.to_string());
            let put = gzipped(&bytes).and_then(|bytes| {
                std::fs::create_dir_all(&folder)?;
                std::fs::write(folder.join(format!("{}.bin", tile.1)), &bytes)?;
                Ok(bytes.len())
            });
            match put {
                Ok(size) => {
                    let mut written = written.lock().unwrap_or_else(|e| e.into_inner());
                    written.0.push(tile);
                    written.1 += size as u64;
                }
                Err(error) => *failure.lock().unwrap_or_else(|e| e.into_inner()) = Some(error),
            }
        }
    });
    if let Some(error) = failure.into_inner().unwrap_or_else(|e| e.into_inner()) {
        return Err(error);
    }
    let (mut list, bytes) = written.into_inner().unwrap_or_else(|e| e.into_inner());
    list.sort_unstable();
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("index.json"), serde_json::to_vec(&list).map_err(std::io::Error::other)?)?;
    Ok((list.len(), bytes, Decks::new(decks.into_inner().unwrap_or_else(|e| e.into_inner()))))
}

/// Marks in the files under `dir` what's over water (`water`: by a place on the ground, x and z).
pub fn mark_water(dir: &Path, water: &dyn Fn(f32, f32) -> bool) -> std::io::Result<()> {
    let dir = dir.join(FOLDER);
    let list: Vec<(u32, u32)> = serde_json::from_slice(&std::fs::read(dir.join("index.json"))?).map_err(std::io::Error::other)?;
    let units = |vertex: &[u8], at: usize| f64::from(u16::from_le_bytes([vertex[at], vertex[at + 1]])) / 65535.0 * (1.0 + 2.0 * PAST) - PAST;
    let tiles = f64::from(1u32 << FILE_ZOOM);
    for (x, y) in list {
        let path = dir.join(x.to_string()).join(format!("{y}.bin"));
        let mut bytes = gunzipped(&std::fs::read(&path)?)?;
        let mut marked = false;
        for triangle in bytes[8 + 4 * BUCKETS..].chunks_exact_mut(3 * Mesh::VERTEX) {
            let middle = |at: usize| (0..3).map(|i| units(&triangle[i * Mesh::VERTEX..], at)).sum::<f64>() / 3.0;
            let [gx, gz] = tile::ground_of((f64::from(x) + middle(0)) / tiles, (f64::from(y) + middle(2)) / tiles);
            if water(gx as f32, gz as f32) {
                for corner in triangle.chunks_exact_mut(Mesh::VERTEX) {
                    corner[9] |= OVER_WATER;
                }
                marked = true;
            }
        }
        if marked {
            std::fs::write(&path, gzipped(&bytes)?)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A straight strip of road along `x` from `from` to `to` at height `y(x)`, 8 m wide round `z`.
    fn along_x(from: f64, to: f64, z: f64, y: impl Fn(f64) -> f64) -> Vec<Face> {
        let mut faces = Vec::new();
        let mut x = from;
        while x < to {
            let (a, b) = (x, x + 10.0);
            let (near, far) = ([a, y(a), z - 4.0], [b, y(b), z - 4.0]);
            let (near2, far2) = ([a, y(a), z + 4.0], [b, y(b), z + 4.0]);
            faces.push(Face { t: [near, far, far2], kind: Kind::Road(RoadClass::Trunk), near: false });
            faces.push(Face { t: [near, far2, near2], kind: Kind::Road(RoadClass::Trunk), near: false });
            x = b;
        }
        faces
    }

    #[test]
    fn all_of_a_road_that_rises_is_in_its_file_and_nothing_of_one_that_does_not() {
        // The flat map's copy of a road that rises goes, so what's still on the ground of it has to be here too.
        let (at, faces) = ((1020.0, 1000.0), along_x(1000.0, 1040.0, 1000.0, |_| 5.0));
        let [x, y] = tile::tile_of(at.0, at.1, BLOCK_ZOOM);
        let count = |low: f32| {
            let lows = vec![[low; 3]; faces.len()];
            let files = meshes((x, y), &faces, &lows);
            files.iter().map(|(_, bytes)| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize).sum::<usize>()
        };
        assert_eq!(count(5.0), faces.len() * 3);
        assert_eq!(count(f32::MAX), 0);
    }

    /// `faces`' one file, at the tile of `at`: those above 1 m drawn as high as they are, the rest on the flat map.
    fn file(faces: &[Face], at: (f64, f64)) -> Vec<u8> {
        let [x, y] = tile::tile_of(at.0, at.1, BLOCK_ZOOM);
        let [fx, fy] = tile::tile_of(at.0, at.1, FILE_ZOOM);
        let lows: Vec<[f32; 3]> = faces.iter().map(|face| if face.t[0][1] > 1.0 { [0.0; 3] } else { [f32::MAX; 3] }).collect();
        meshes((x, y), faces, &lows).into_iter().find(|(tile, _)| *tile == (fx, fy)).expect("something rises").1
    }

    /// A road of `heights.len()` nodes 100 m apart, the items between them numbered from 0, those of `bridge` a bridge: how
    /// high `node` stands.
    fn walked(heights: &[f32], bridge: &[u32], node: usize) -> f32 {
        let (last, count) = (heights.len() - 1, heights.len() as u32 - 1);
        let ends = |item: u32| vec![u64::from(item), u64::from(item) + 1];
        let items_at = |node: u64| (0..count).filter(|&item| node == u64::from(item) || node == u64::from(item) + 1).collect::<Vec<_>>();
        let height = |node: u64| heights[node as usize];
        let length = |_: u32, _: u64, _: u64| 100.0;
        let is_bridge = |item: u32| bridge.contains(&item);
        let roads = Roads { ends: &ends, items_at: &items_at, height: &height, length: &length, bridge: &is_bridge };
        let ways = [(node > 0).then(|| node as u32 - 1), (node < last).then_some(node as u32)];
        rise_of(ways.iter().flatten().map(|&item| climb_along(&roads, node as u64, item)).collect())
    }

    #[test]
    fn a_bridge_stands_as_high_as_it_is_over_the_line_between_its_ends() {
        // From 0 m to 20 m, 40 m up: nearer one end than the other.
        assert_eq!(walked(&[0.0, 40.0, 40.0, 20.0], &[0, 1, 2], 1), 33.0 + 1.0 / 3.0);
        // Level, across a valley.
        assert_eq!(walked(&[54.0; 7], &[0, 1, 2, 3, 4, 5], 3), 0.0);
        // Straight up a hillside.
        assert_eq!(walked(&[0.0, 10.0, 20.0, 30.0, 40.0], &[0, 1, 2, 3], 2), 0.0);
        // It ends where it ends, however far the road goes on down.
        assert_eq!(walked(&[0.0, 10.0, 20.0, 30.0, 20.0, 10.0, 0.0], &[2, 3], 3), 10.0);
    }

    #[test]
    fn an_arched_bridge_is_as_high_as_it_arches() {
        // The game's own, on the E6 by Storslett: 5.0, 5.7, 8.8, 5.4, 4.9.
        assert!((walked(&[5.0, 5.7, 8.8, 5.4, 4.9], &[0, 1, 2, 3], 2) - 3.85).abs() < 0.01);
    }

    #[test]
    fn a_bridge_of_any_length_comes_down_at_its_ends() {
        // Two kilometres of bridge, 30 m up in the middle.
        let mut heights: Vec<f32> = (0..=10).map(|i| i as f32 * 3.0).collect();
        heights.extend((0..10).rev().map(|i| i as f32 * 3.0));
        let bridge: Vec<u32> = (0..20).collect();
        assert_eq!(walked(&heights, &bridge, 10), 30.0);
        // Across a sag in it too.
        assert_eq!(walked(&[0.0, 20.0, 15.0, 20.0, 0.0], &[0, 1, 2, 3], 1), 20.0);
    }

    /// `rises` spread along a road of nodes 100 m apart at `heights`.
    fn spread_along(heights: &[f32], rises: &[(u64, f32)]) -> Vec<f32> {
        let mut rises: HashMap<u64, f32> = rises.iter().copied().collect();
        let last = heights.len() as u64 - 1;
        let ways = |node: u64| [node.checked_sub(1), (node < last).then_some(node + 1)].into_iter().flatten().map(|next| (next, 100.0)).collect();
        spread(&mut rises, &|node| heights[node as usize], &ways);
        (0..=last).map(|node| rises.get(&node).copied().unwrap_or(0.0)).collect()
    }

    #[test]
    fn a_road_s_height_falls_as_it_does_down_its_ramps() {
        // Ramps at 10%: to the ground and no further, though the road goes on down.
        assert_eq!(spread_along(&[100.0, 110.0, 120.0, 110.0, 100.0, 90.0, 80.0], &[(2, 15.0)]), [0.0, 5.0, 15.0, 5.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn a_road_stays_on_the_ground_away_from_what_s_in_the_air() {
        // Level, and up a hill, it falls at the least it falls: 8 m in the 100 m.
        assert_eq!(spread_along(&[50.0; 5], &[(0, 20.0)]), [20.0, 12.0, 4.0, 0.0, 0.0]);
        assert_eq!(spread_along(&[50.0, 60.0, 70.0, 80.0], &[(0, 10.0)]), [10.0, 2.0, 0.0, 0.0]);
    }

    #[test]
    fn a_road_gets_down_to_a_tunnel_as_it_gets_down_from_anything_else() {
        // 20 m up 100 m from the mouth, level: it has room for 8 m there, 16 m the next, and from there on it's as it was.
        let mut rises: HashMap<u64, f32> = [(1, 20.0), (2, 20.0), (3, 12.0), (4, 4.0)].into_iter().collect();
        let ways = |node: u64| [node.checked_sub(1), (node < 4).then_some(node + 1)].into_iter().flatten().map(|next| (next, 100.0)).collect();
        down_to(&mut rises, vec![0], &|_| 50.0, &ways);
        assert_eq!((1..=4).map(|node| rises[&node]).collect::<Vec<_>>(), [8.0, 16.0, 12.0, 4.0]);
    }

    #[test]
    fn a_road_goes_evenly_from_one_node_to_the_next_and_its_ends_meet_what_s_joined_there() {
        // A road along x, its ends across it.
        let nodes = [([0.0, 0.0], [0.0, 1.0], 10.0), ([100.0, 0.0], [0.0, 1.0], 20.0)];
        assert_eq!(between(&nodes, [50.0, 5.0, 0.0]), 15.0);
        // No stretch held at a node's own and the rest made up in the middle: a piece 45 m long would step.
        assert!((between(&nodes, [25.0, 5.0, 0.0]) - 12.5).abs() < 0.01);
        assert!((between(&nodes, [75.0, 5.0, 0.0]) - 17.5).abs() < 0.01);
        // A corner of the road's end, a road's width off the node: the node's, as the junction beyond has it.
        assert!((between(&nodes, [0.0, 5.0, 12.0]) - 10.0).abs() < 0.01);
        assert!((between(&nodes, [100.0, 5.0, -12.0]) - 20.0).abs() < 0.01);
    }

    #[test]
    fn a_road_is_in_the_air_with_no_ground_beside_it_and_something_under_it() {
        let at = [5.0, 8.0, 0.0];
        let sides = [[5.0, 7.0], [5.0, -7.0]];
        let air = |ground: &[Face], under: &[Face]| in_the_air_at(at, sides, &FaceIndex::new(ground), &FaceIndex::new(under));
        let water = along_x(-20.0, 30.0, 0.0, |_| 0.0);
        let bank = |y: f64| along_x(-20.0, 30.0, 7.0, move |_| y);
        // Water under a bridge, and nothing else about.
        assert_eq!(air(&[], &water), Some(8.0));
        // The water going on under the land: the ground beside the road says it's on it.
        assert_eq!(air(&bank(8.0), &water), None);
        // Ground beside it that falls away less than a clear height: a low bank.
        assert_eq!(air(&bank(6.0), &water), None);
        // The ground far down, and over the road: a cutting isn't the air, and a valley is.
        assert_eq!(air(&bank(2.0), &water), Some(8.0));
        assert_eq!(air(&bank(20.0), &water), None);
        // Nothing under it.
        assert_eq!(air(&[], &[]), None);
    }

    #[test]
    fn what_s_under_is_the_nearest_a_clear_height_down() {
        let road = along_x(-20.0, 30.0, 0.0, |_| 7.0);
        let water = along_x(-20.0, 30.0, 0.0, |_| 0.0);
        let deck = along_x(-20.0, 30.0, 0.0, |_| 8.0);
        let faces: Vec<Face> = road.iter().chain(&water).chain(&deck).copied().collect();
        assert_eq!(clear_over([5.0, 12.0, 0.0], &FaceIndex::new(&faces)), Some(4.0));
        assert_eq!(clear_over([5.0, 9.0, 0.0], &FaceIndex::new(&faces)), Some(9.0));
        // Roads under roads: down to the lowest of them.
        assert_eq!(clear_over_roads([5.0, 12.0, 0.0], &FaceIndex::new(&faces)), Some(12.0));
    }

    #[test]
    fn what_stands_under_a_road_is_no_higher_than_it_is_drawn() {
        // A deck 20 m long along x, at 30 m in the game, drawn from 10 m to 14 m.
        let corner = |x: f32, z: f32| [x, z, 30.0, 10.0 + (x / 5.0).max(0.0)];
        let decks = Decks::new(vec![[corner(0.0, -5.0), corner(20.0, -5.0), corner(20.0, 5.0)], [corner(0.0, -5.0), corner(20.0, 5.0), corner(0.0, 5.0)]]);
        let square = |x: f32, z: f32| vec![[x - 3.0, z - 3.0], [x + 3.0, z - 3.0], [x + 3.0, z + 3.0], [x - 3.0, z + 3.0]];
        // A column under it, just under the road where it's lowest over it.
        let top = decks.over(&square(10.0, 0.0), 0.0).unwrap();
        assert!((top - 11.4).abs() < 0.3, "{top}");
        // Past the road's edge, nothing over it.
        assert_eq!(decks.over(&square(10.0, 20.0), 0.0), None);
        // A bridge's model is placed on its road, and is no higher than the road either.
        assert!(decks.over(&square(10.0, 0.0), 30.0).is_some());
        // Up a clear height over the road: the road passes under it.
        assert_eq!(decks.over(&square(10.0, 0.0), 40.0), None);
    }

    #[test]
    fn a_bridge_s_model_comes_down_with_its_road() {
        // A level road 54 m up in the game, on the flat map, and a pier beside it, standing in the valley.
        let corner = |x: f32, z: f32| [x, z, 54.0, 0.0];
        let decks = Decks::new(vec![[corner(0.0, -5.0), corner(20.0, -5.0), corner(20.0, 5.0)]]);
        let ring = vec![[8.0, 7.0], [12.0, 7.0], [12.0, 11.0], [8.0, 11.0]];
        let pier = |bridge: bool| Building { rings: vec![ring.clone()], height: 59.0, base: 0.0, kind: crate::world::BuildingKind::Landmark, uid: 1, ground: -5.0, bridge };
        let mut part = pier(true);
        decks.fit(&mut part);
        assert_eq!(part.height, 0.0);
        // Anything else beside the road stays as it is.
        let mut part = pier(false);
        decks.fit(&mut part);
        assert_eq!(part.height, 59.0);
        // A tower on a bridge's own road, 3 m up on the map and in its model: as far over the road as in the game.
        let corner = |x: f32, z: f32| [x, z, 9.0, 3.0];
        let decks = Decks::new(vec![[corner(0.0, -5.0), corner(20.0, -5.0), corner(20.0, 5.0)]]);
        let mut tower = Building { ground: 6.0, height: 19.0, ..pier(true) };
        decks.fit(&mut tower);
        assert_eq!(tower.height, 19.0);
    }

    #[test]
    fn the_files_are_lowest_first_and_their_buckets_say_where_each_ends() {
        let mut faces = along_x(-600.0, 600.0, 0.0, |_| 0.0);
        faces.extend(along_x(-600.0, 600.0, 10.0, |_| 7.0));
        let bytes = file(&faces, (0.0, 10.0));
        let vertices = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
        assert_eq!(bytes.len(), 8 + 4 * BUCKETS + vertices * Mesh::VERTEX);
        assert_eq!(vertices % 3, 0);
        let last_end = u32::from_le_bytes(bytes[8 + 4 * (BUCKETS - 1)..8 + 4 * BUCKETS].try_into().unwrap()) as usize;
        assert_eq!(last_end, vertices);
        let heights: Vec<u16> = bytes[8 + 4 * BUCKETS..]
            .chunks(Mesh::VERTEX * 3)
            .map(|t| [0, 1, 2].map(|i| u16::from_le_bytes([t[i * Mesh::VERTEX + 4], t[i * Mesh::VERTEX + 5]])).iter().copied().max().unwrap())
            .collect();
        assert!(heights.iter().any(|&height| height == 700));
        let flags: HashSet<u8> = bytes[8 + 4 * BUCKETS..].chunks(Mesh::VERTEX).map(|v| v[9]).collect();
        assert_eq!(flags, HashSet::from([0, ON_SIDE, ON_SIDE | BOTTOM]));
    }

    #[test]
    fn what_rises_over_water_is_marked_as_being_over_it() {
        let dir = std::env::temp_dir().join(format!("westplay-raised-{}", std::process::id()));
        let mut faces = along_x(-600.0, 600.0, 0.0, |_| 0.0);
        faces.extend(along_x(-600.0, 600.0, 40.0, |_| 7.0));
        let tile = tile::tile_of(0.0, 40.0, FILE_ZOOM);
        let folder = dir.join(FOLDER).join(tile[0].to_string());
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(format!("{}.bin", tile[1])), gzipped(&file(&faces, (0.0, 40.0))).unwrap()).unwrap();
        std::fs::write(dir.join(FOLDER).join("index.json"), serde_json::to_vec(&[(tile[0], tile[1])]).unwrap()).unwrap();
        mark_water(&dir, &|x, _| x > 50.0).unwrap();
        let bytes = gunzipped(&std::fs::read(folder.join(format!("{}.bin", tile[1]))).unwrap()).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        let triangles: Vec<&[u8]> = bytes[8 + 4 * BUCKETS..].chunks(3 * Mesh::VERTEX).collect();
        assert!(triangles.iter().any(|t| t[9] & OVER_WATER != 0));
        assert!(triangles.iter().any(|t| t[9] & OVER_WATER == 0));
    }
}
