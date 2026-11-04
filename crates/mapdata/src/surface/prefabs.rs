//! A prefab's surfaces in its own space: sorted once per model, drawn once per the way it's joined on the map.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use crate::files::GameFiles;
use crate::prefab::{self, MapPoint, MapPointKind, Prefab, SpawnKind};
use crate::sea;
use crate::token::string_to_token;
use crate::world::RoadClass;

use super::faces::{Materials, model_pieces, model_pieces_with};
use super::shapes::{self, Ring, Spot};
use super::sort::{FaceKind, LaneReach, Levels, Rules, Triangle, lane_frames, lane_reach, sorted_faces, trigger_rings, under_point};
use super::{Kind, Own3};

/// The grid a prefab's outlines are worked on, units a metre.
pub const OWN_GRID: f64 = 256.0;
/// How far a road reaches past its lanes, m, where a junction doesn't say.
const CLAIM: f64 = 4.0;
/// Looked up in cells of this size, m.
const FACE_CELL: f32 = 3.0;
/// Where a road's class changes inside a junction, worked out on a lattice this fine, m.
const CLASS_LATTICE: f32 = 1.0;
/// A lane's point this close to a surface's height is on it, m.
const ON_SURFACE: f32 = 0.5;
/// Where road and rail share a surface (a level crossing), how far from the road's lanes it's the road's, m: a lane and
/// its shoulder. What's left is the track's bed.
const ROAD_OVER_RAIL: f64 = 3.5;

fn own_spot(x: f32, z: f32) -> Spot {
    Spot::new((f64::from(x) * OWN_GRID).round() as i32, (f64::from(z) * OWN_GRID).round() as i32)
}

fn triangle_ring(t: &Triangle) -> Option<Ring> {
    let ring: Ring = t.iter().map(|v| own_spot(v[0], v[2])).collect();
    (shapes::twice_area(&ring) != 0).then(|| shapes::outward(ring))
}

/// A prefab's faces, sorted, each once: what its [`FaceGrid`]s and its shown and ground faces are of.
pub type Faces = Arc<[(Triangle, FaceKind)]>;

/// Some of a list of faces (those of the kinds it's made of), indexed to look points up in: by their place in the
/// list, so the list is shared, never copied (a prefab's faces were kept three times over, gigabytes on a whole map).
pub struct FaceGrid {
    faces: Faces,
    grid: super::Cells,
}

impl FaceGrid {
    /// A grid of these triangles alone.
    pub fn new(list: Vec<Triangle>) -> Self {
        let faces: Faces = list.into_iter().map(|t| (t, FaceKind::Road)).collect();
        Self::of(&faces, |_| true)
    }

    /// A grid of `faces`' faces of the kinds `keep` says, in their order.
    pub fn of(faces: &Faces, keep: impl Fn(FaceKind) -> bool) -> Self {
        let mut added = Vec::new();
        for (i, (t, kind)) in faces.iter().enumerate() {
            if !keep(*kind) {
                continue;
            }
            let (x0, x1) = (t[0][0].min(t[1][0]).min(t[2][0]), t[0][0].max(t[1][0]).max(t[2][0]));
            let (z0, z1) = (t[0][2].min(t[1][2]).min(t[2][2]), t[0][2].max(t[1][2]).max(t[2][2]));
            for gx in (x0 / FACE_CELL).floor() as i32..=(x1 / FACE_CELL).floor() as i32 {
                for gz in (z0 / FACE_CELL).floor() as i32..=(z1 / FACE_CELL).floor() as i32 {
                    added.push(((gx, gz), i as u32));
                }
            }
        }
        Self { faces: faces.clone(), grid: super::Cells::new(added) }
    }

    fn near(&self, x: f32, z: f32) -> impl Iterator<Item = &Triangle> {
        let cell = ((x / FACE_CELL).floor() as i32, (z / FACE_CELL).floor() as i32);
        self.grid.at(cell).iter().map(|&i| &self.faces[i as usize].0)
    }

    /// Whether a face covers (x, z) at about `height`.
    pub fn covered(&self, x: f32, z: f32, height: f32) -> bool {
        self.near(x, z).any(|t| under_point(t, x, z).is_some_and(|y| (y - height).abs() <= 1.5))
    }

    /// How high the face at (x, z) nearest `height` is, if one's within `reach`.
    pub fn height(&self, x: f32, z: f32, height: f32, reach: f32) -> Option<f32> {
        self.near(x, z)
            .filter_map(|t| under_point(t, x, z))
            .filter(|y| (y - height).abs() <= reach)
            .min_by(|a, b| (a - height).abs().total_cmp(&(b - height).abs()))
    }

    /// How high the highest face at (x, z) is, or the nearest corner's height if none's quite there (a point on an
    /// outline can fall a rounding off its face).
    fn top(&self, x: f32, z: f32) -> f32 {
        let found = self.near(x, z).filter_map(|t| under_point(t, x, z)).reduce(f32::max);
        found.unwrap_or_else(|| {
            let distance = |v: &[f32; 3]| (v[0] - x).hypot(v[2] - z);
            self.near(x, z)
                .flatten()
                .min_by(|a, b| distance(a).total_cmp(&distance(b)))
                .map_or(0.0, |v| v[1])
        })
    }

    /// Where the surface starts on the median's side of a prefab node, half a metre in, walking in from `from`. `None`
    /// if it runs on across the centre: the prefab has no median here.
    pub fn inner_edge(&self, position: [f32; 2], height: f32, direction: [f32; 2], sign: f32, from: f32) -> Option<f32> {
        let right = [-direction[1], direction[0]];
        let at = |u: f32| {
            [position[0] + 0.5 * direction[0] + sign * u * right[0], position[1] + 0.5 * direction[1] + sign * u * right[1]]
        };
        let (mut u, mut last, mut gap) = (from, from, 0.0f32);
        while u > 0.0 {
            let p = at(u);
            if self.covered(p[0], p[1], height) {
                last = u;
                gap = 0.0;
            } else {
                gap += 0.05;
                if gap > 0.3 {
                    return Some(last);
                }
            }
            u -= 0.05;
        }
        None
    }

    /// Where the surface ends on one side of a prefab node, half a metre in: walking out from `inner` towards `outer`
    /// and on. `sign` is +1 on the node's right (with `direction` the way in), -1 on its left. `None` if there's no
    /// surface there.
    pub fn edge(&self, position: [f32; 2], height: f32, direction: [f32; 2], sign: f32, inner: f32, outer: f32) -> Option<f32> {
        let right = [-direction[1], direction[0]];
        let at = |u: f32| {
            [position[0] + 0.5 * direction[0] + sign * u * right[0], position[1] + 0.5 * direction[1] + sign * u * right[1]]
        };
        let mut start = None;
        let mut u = inner + 0.2;
        while u < outer {
            let p = at(u);
            if self.covered(p[0], p[1], height) {
                start = Some(u);
                break;
            }
            u += 0.1;
        }
        let start = start?;
        let (mut u, mut last, mut gap) = (start, start, 0.0f32);
        while u < outer + 14.0 && gap <= 0.3 {
            let p = at(u);
            if self.covered(p[0], p[1], height) {
                last = u;
                gap = 0.0;
            } else {
                gap += 0.05;
            }
            u += 0.05;
        }
        Some(last)
    }
}

/// How a prefab model's corner is fitted on one of its terrain edges: turned by `turn` about its first node `from`,
/// then moved to `to`.
#[derive(Clone, Copy, Debug)]
pub struct Fitted {
    from: [f32; 3],
    to: [f32; 3],
    turn: (f32, f32),
}

impl Fitted {
    pub fn place(&self, v: [f32; 3]) -> [f32; 3] {
        let (x, z) = (v[0] - self.from[0], v[2] - self.from[2]);
        let (cos, sin) = self.turn;
        [self.to[0] + cos * x - sin * z, self.to[1] + v[1] - self.from[1], self.to[2] + sin * x + cos * z]
    }
}

/// A corner model's two nodes laid on the two ends of the edge it's set on.
pub fn fit_corner(corner: &Prefab, edge: &[([f32; 3], [f32; 3])]) -> Option<Fitted> {
    let (first, last) = (corner.nodes.first()?, corner.nodes.get(1)?);
    let (start, end) = (edge.first()?.0, edge.last()?.0);
    let own = (last.position[1] - first.position[1]).atan2(last.position[0] - first.position[0]);
    let placed = (end[2] - start[2]).atan2(end[0] - start[0]);
    let (sin, cos) = (placed - own).sin_cos();
    Some(Fitted {
        from: [first.position[0], first.height, first.position[1]],
        to: start,
        turn: (cos, sin),
    })
}

/// What sorts a prefab's surfaces: its model, variant and look, and each corner's model, variant and look.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelKey {
    pub model: u64,
    pub variant: u64,
    pub look: u64,
    pub corners: [[u64; 3]; 6],
    /// Its additional parts.
    pub parts: Vec<u64>,
}

/// The game's definitions of prefab corners: their models' and descriptors' paths.
#[derive(Default)]
pub struct CornerModels {
    paths: HashMap<u64, (String, String)>,
    read: std::sync::Mutex<HashMap<u64, Option<Arc<Prefab>>>>,
}

impl CornerModels {
    pub fn load(files: &GameFiles) -> Self {
        let mut paths = HashMap::new();
        for unit in crate::sii::read_family(files, "/def/world", "prefab_corner").unwrap_or_default() {
            let Some(token) = unit.name.strip_prefix("corner.").and_then(string_to_token) else { continue };
            if let (Some(model), Some(prefab)) = (unit.get("model_desc"), unit.get("prefab_desc")) {
                paths.insert(token, (model.trim_matches('"').to_owned(), prefab.trim_matches('"').to_owned()));
            }
        }
        Self { paths, read: Default::default() }
    }

    pub fn model_path(&self, token: u64) -> Option<&str> {
        self.paths.get(&token).map(|paths| paths.0.as_str())
    }

    pub fn descriptor(&self, files: &GameFiles, token: u64) -> Option<Arc<Prefab>> {
        if let Some(known) = self.read.lock().unwrap_or_else(|e| e.into_inner()).get(&token) {
            return known.clone();
        }
        let read = self
            .paths
            .get(&token)
            .and_then(|(_, path)| prefab::parse(&files.read(path).ok()??).ok())
            .map(Arc::new);
        self.read.lock().unwrap_or_else(|e| e.into_inner()).insert(token, read.clone());
        read
    }
}

/// A prefab model's variant's index in its `.pmd`, which picks its nodes' terrain points.
pub fn variant_index(pmd: &[u8], variant: u64) -> Option<usize> {
    let word = |at: usize| Some(u32::from_le_bytes(pmd.get(at..at + 4)?.try_into().ok()?));
    let (count, table) = (word(16)? as usize, word(40)? as usize);
    (0..count).position(|i| {
        let long = |at: usize| Some(u64::from(word(at)?) | u64::from(word(at + 4)?) << 32);
        long(table + 8 * i) == Some(variant)
    })
}

/// The road through a filling station with two ends, where it's told from the lanes past its pumps, which are no road. Laid
/// along a two-way road, its ends facing opposite ways and the game's own map going straight on with no fork: each way's
/// lanes, by the nodes it goes between. Its ends turned any other way are driveways off roads outside it: none. On a
/// one-way slip road through opposite ends those lanes are all there is, and forks on the game's map are the game's: `None`.
fn station_road(prefab: &Prefab) -> Option<Vec<(usize, usize, Vec<usize>)>> {
    let road = |point: &MapPoint| matches!(point.kind, MapPointKind::Road(_));
    // Where the game's map has a graph of its own to draw, the graph for navigation only repeats it.
    let drawn = prefab.map_points.iter().any(|point| road(point) && !point.navigation_only);
    let mut graph = prefab.map_points.iter().filter(|point| road(point) && point.navigation_only != drawn);
    let forks = graph.any(|point| point.neighbours.iter().filter(|&&next| road(&prefab.map_points[next])).count() > 2);
    let ([a, b], false) = (&prefab.nodes[..], forks) else { return None };
    if !prefab.spawn_points.iter().any(|point| point.kind == SpawnKind::Fuel) {
        return None;
    }
    if a.direction[0] * b.direction[0] + a.direction[1] * b.direction[1] > -0.5 {
        return Some(Vec::new());
    }
    let ways = vec![(0, 1, prefab.road(0, 1)), (1, 0, prefab.road(1, 0))];
    ways.iter().all(|(.., lanes)| !lanes.is_empty()).then_some(ways)
}

/// A prefab's faces sorted, once per [`ModelKey`].
pub struct Sorted {
    pub is_place: bool,
    /// The road through a filling station that lies along one ([`station_road`]), by the nodes each way goes between.
    pub station_road: Option<Vec<(usize, usize, Vec<usize>)>>,
    /// Its variant's index, for its terrain points.
    pub variant: Option<usize>,
    /// Each corner's fit, by the prefab's own node.
    pub fitted: [Option<Fitted>; 6],
    /// Road faces' outlines, for each connected surface.
    road_rings: Vec<Vec<Ring>>,
    /// The lanes on each connected surface.
    lanes_on: Vec<Vec<usize>>,
    /// Its faces, each once, in the order they're sorted in ([`Self::shown`], [`Self::ground`] and the grids are of them).
    faces: Faces,
    /// Every road face, to walk along at joints.
    pub road_faces: FaceGrid,
    /// All surfaces' faces, for heights.
    every_face: FaceGrid,
    quiet: Vec<Ring>,
    building: Vec<Ring>,
    water: Vec<Ring>,
    /// Every face's hull on the ground, with their heights: the prefab's ground.
    pub hull: Vec<[f32; 3]>,
    /// How low and how high its shown faces go.
    pub heights: (f32, f32),
    pub reach: LaneReach,
    /// Each lane's points, 2 m apart.
    pub lanes: Vec<Vec<[f32; 3]>>,
    /// For each lane's point, whether the prefab has a surface under it.
    pub surfaced: Vec<Vec<bool>>,
    /// And whether that's road.
    pub on_road: Vec<Vec<bool>>,
    parking: Vec<Ring>,
}

impl Sorted {
    /// The faces that aren't ground, by what they are, for levels, in their order.
    pub fn shown(&self) -> impl Iterator<Item = &(Triangle, FaceKind)> {
        self.faces.iter().filter(|(_, kind)| *kind != FaceKind::Ground)
    }

    /// The faces that are: the lawns in a prefab, in their order.
    pub fn ground(&self) -> impl Iterator<Item = &Triangle> {
        self.faces.iter().filter(|(_, kind)| *kind == FaceKind::Ground).map(|(t, _)| t)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        prefab: &Prefab,
        files: &GameFiles,
        materials: &Materials,
        rules: &Rules,
        corners: &CornerModels,
        key: &ModelKey,
        pmd_path: &str,
        parking_action: u64,
    ) -> Self {
        let variant = files.read(pmd_path).ok().flatten().and_then(|pmd| variant_index(&pmd, key.variant));
        let mut pieces = model_pieces_with(files, materials, pmd_path, Some(key.variant), key.look, &key.parts);
        let mut fitted = [None; 6];
        for (index, [model, corner_variant, look]) in key.corners.iter().enumerate() {
            if *model == 0 {
                continue;
            }
            let Some(path) = corners.model_path(*model) else { continue };
            let (Some(node), Some(corner)) = (prefab.nodes.get(index), corners.descriptor(files, *model)) else { continue };
            let edge = node.terrain_points.get(variant.unwrap_or(0)).or(node.terrain_points.first());
            let Some(fit) = edge.and_then(|edge| fit_corner(&corner, edge)) else { continue };
            fitted[index] = Some(fit);
            for mut piece in model_pieces(files, materials, path, Some(*corner_variant), *look) {
                for t in &mut piece.triangles {
                    *t = t.map(|v| fit.place(v));
                }
                pieces.push(piece);
            }
        }
        let level = Levels::new(prefab);
        let faces: Faces = sorted_faces(&pieces, &level, rules).into();
        let of_kind = |wanted: FaceKind| -> Vec<Ring> {
            let rings: Vec<Ring> =
                faces.iter().filter(|(_, kind)| *kind == wanted).filter_map(|(t, _)| triangle_ring(t)).collect();
            shapes::union(&rings)
        };
        let road_faces: Vec<Triangle> =
            faces.iter().filter(|(_, kind)| *kind == FaceKind::Road).map(|(t, _)| *t).collect();
        let road = connected(&road_faces);
        let road_rings: Vec<Vec<Ring>> = road
            .iter()
            .map(|faces| shapes::union(&faces.iter().filter_map(triangle_ring).collect::<Vec<_>>()))
            .collect();
        let lanes: Vec<Vec<[f32; 3]>> =
            prefab.curves.iter().map(|curve| lane_frames(curve, 2.0).into_iter().map(|p| p.0).collect()).collect();
        // Which connected surface each lane runs on, and where it has none of its prefab's own under it.
        let road_grids: Vec<FaceGrid> = road.iter().map(|faces| FaceGrid::new(faces.clone())).collect();
        let mut lanes_on = vec![Vec::new(); road.len()];
        let every_face = FaceGrid::of(&faces, |kind| kind != FaceKind::Water);
        let mut surfaced = Vec::with_capacity(lanes.len());
        let mut on_road = Vec::with_capacity(lanes.len());
        for (lane, points) in lanes.iter().enumerate() {
            let mut under = Vec::with_capacity(points.len());
            let mut road_under = Vec::with_capacity(points.len());
            for p in points {
                for (surface, grid) in road_grids.iter().enumerate() {
                    if grid.height(p[0], p[2], p[1], ON_SURFACE).is_some() && lanes_on[surface].last() != Some(&lane) {
                        lanes_on[surface].push(lane);
                    }
                }
                under.push(every_face.height(p[0], p[2], p[1], ON_SURFACE).is_some());
                road_under.push(road_grids.iter().any(|grid| grid.height(p[0], p[2], p[1], ON_SURFACE).is_some()));
            }
            surfaced.push(under);
            on_road.push(road_under);
        }
        let parking: Vec<Ring> = trigger_rings(prefab, parking_action)
            .iter()
            .map(|ring| shapes::outward(ring.iter().map(|p| own_spot(p[0], p[1])).collect()))
            .filter(|ring: &Ring| ring.len() >= 3)
            .collect();
        let hull = hull_of(faces.iter().flat_map(|(t, _)| t.iter().copied()).collect());
        let heights = faces
            .iter()
            .filter(|(_, kind)| *kind != FaceKind::Ground)
            .flat_map(|(t, _)| t.iter().map(|v| v[1]))
            .fold((f32::MAX, f32::MIN), |(low, high), y| (low.min(y), high.max(y)));
        Self {
            is_place: !prefab.spawn_points.is_empty(),
            station_road: station_road(prefab),
            variant,
            fitted,
            road_faces: FaceGrid::of(&faces, |kind| kind == FaceKind::Road),
            road_rings,
            lanes_on,
            quiet: of_kind(FaceKind::Quiet),
            building: of_kind(FaceKind::Building),
            water: of_kind(FaceKind::Water),
            every_face,
            hull,
            heights,
            faces,
            reach: lane_reach(prefab),
            lanes,
            surfaced,
            on_road,
            parking,
        }
    }

    fn own_rings(&self, rings: &[Ring]) -> Vec<Own3> {
        rings
            .iter()
            .map(|ring| {
                ring.iter()
                    .map(|p| {
                        let (x, z) = ((f64::from(p.x) / OWN_GRID) as f32, (f64::from(p.y) / OWN_GRID) as f32);
                        [x, self.every_face.top(x, z), z]
                    })
                    .collect()
            })
            .collect()
    }

    /// Whether a lane leads from one of `usable`'s nodes to another of them.
    pub fn is_through(&self, lane: usize, usable: &dyn Fn(usize) -> bool) -> bool {
        if let Some(road) = &self.station_road {
            return road.iter().any(|(from, to, lanes)| usable(*from) && usable(*to) && lanes.contains(&lane));
        }
        let nodes = self.reach.from[lane].len();
        (0..nodes).filter(|&i| usable(i) && self.reach.from[lane][i].is_some()).any(|i| {
            (0..nodes).filter(|&j| usable(j) && self.reach.to[lane][j].is_some()).any(|j| i != j)
        })
    }

    fn lane_line(&self, lane: usize) -> Vec<[f64; 2]> {
        self.lanes[lane].iter().map(|p| [f64::from(p[0]), f64::from(p[2])]).collect()
    }

    /// A place's ways through, as bands round their lanes, by class.
    fn bands(&self, lanes: &[usize], classes: &[Option<RoadClass>], reach: f64) -> Vec<(RoadClass, Vec<Ring>)> {
        let mut by_class: BTreeMap<RoadClass, Vec<Vec<[f64; 2]>>> = BTreeMap::new();
        for &lane in lanes {
            if let Some(class) = classes[lane].filter(|class| *class != RoadClass::Rail) {
                by_class.entry(class).or_default().push(self.lane_line(lane));
            }
        }
        by_class.into_iter().map(|(class, lines)| (class, shapes::around_lines(&lines, reach, OWN_GRID))).collect()
    }

    /// How far past a node's outermost lane the road on it reaches on one side, given how far that road reaches from the
    /// node's centre: `sign` +1 on the node's right.
    pub fn past_lanes(&self, prefab: &Prefab, node: usize, sign: f32, edge: f64) -> Option<f64> {
        self.outermost_lane(&prefab.nodes[node], sign).map(|lane| edge - f64::from(lane))
    }

    /// How far a node's outermost lane lies from its centre on one side.
    fn outermost_lane(&self, node: &prefab::PrefabNode, sign: f32) -> Option<f32> {
        let right = [-node.direction[1], node.direction[0]];
        let mut lane: Option<f32> = None;
        for &c in node.curves_in.iter().chain(&node.curves_out) {
            let Some(list) = self.lanes.get(c) else { continue };
            for p in [list.first(), list.last()].into_iter().flatten() {
                let (dx, dz) = (p[0] - node.position[0], p[2] - node.position[1]);
                if dx.hypot(dz) < 12.0 {
                    let u = sign * (dx * right[0] + dz * right[1]);
                    lane = Some(lane.map_or(u, |l: f32| l.max(u)));
                }
            }
        }
        lane
    }

    /// The surfaces for one way of being joined: each lane's class (`None`: it joins nothing shown), each node's class
    /// (`None`: nothing's on it), which nodes have a road shown on them from far (the rest of the road may be left out
    /// there), how far a place's ways through reach past their lanes.
    pub fn draw(&self, classes: &[Option<RoadClass>], at_nodes: &[Option<RoadClass>], far: &[bool], reach: f64, prefab: &Prefab) -> Drawing {
        let mut drawing = Drawing::default();
        let everywhere = |i: usize| !self.is_place || at_nodes.get(i).copied().flatten().is_some();
        let usable = |i: usize| far.get(i).copied().unwrap_or(false);
        let through_near: Vec<usize> = (0..self.lanes.len()).filter(|&l| self.is_through(l, &everywhere)).collect();
        let through_far: Vec<usize> = (0..self.lanes.len()).filter(|&l| self.is_through(l, &usable)).collect();
        let arms: Vec<usize> = (0..self.lanes.len()).filter(|l| !through_far.contains(l)).collect();
        let lines = |lanes: &[usize]| -> Vec<Vec<[f64; 2]>> { lanes.iter().map(|&l| self.lane_line(l)).collect() };
        let mut quiet = self.quiet.clone();
        let mut road_near: Vec<(RoadClass, Vec<Ring>)> = Vec::new();
        let mut road_far: Vec<(RoadClass, Vec<Ring>)> = Vec::new();
        // Nothing on any of its nodes: decorative, a filling station's pump islands. It has no road.
        let floors_only = self.is_place || at_nodes.iter().all(Option::is_none);
        // How far the road reaches past its lanes here, measured where it comes in at the shown nodes.
        let beside = if arms.is_empty() || through_far.is_empty() { CLAIM } else { self.beside_lanes(prefab, far) };
        let far_cut = if !self.is_place && !arms.is_empty() && !through_far.is_empty() {
            let arm_area = shapes::around_lines(&lines(&arms), beside + 3.0, OWN_GRID);
            let keep = shapes::around_lines(&lines(&through_far), beside, OWN_GRID);
            shapes::difference(&arm_area, &keep)
        } else {
            Vec::new()
        };
        if floors_only {
            // A place's floors are its own, whatever runs on them; its ways through are drawn as the road they carry on.
            quiet.extend(self.road_rings.iter().flatten().cloned());
            if self.is_place {
                road_near = self.bands(&through_near, classes, reach);
                road_far = self.bands(&through_far, classes, reach);
            }
        }
        // A parking ring is only a rough outline of the lot: where it reaches a way through, that stays road.
        let parking = if self.parking.is_empty() {
            Vec::new()
        } else {
            let every_node = vec![true; prefab.nodes.len()];
            let ways = shapes::around_lines(&lines(&through_near), self.beside_lanes(prefab, &every_node), OWN_GRID);
            shapes::difference(&self.parking, &ways)
        };
        for (surface, rings) in self.road_rings.iter().enumerate() {
            if floors_only {
                break;
            }
            let mut whole = rings.clone();
            if !parking.is_empty() {
                quiet.extend(shapes::intersect(&whole, &parking));
                whole = shapes::difference(&whole, &parking);
            }
            // A level crossing: the road keeps what's round its lanes, the rest of the surface is the tracks' bed.
            let of = |lane: &usize| classes.get(*lane).copied().flatten();
            let road_lanes: Vec<usize> =
                self.lanes_on[surface].iter().filter(|lane| of(lane).is_some_and(|c| c != RoadClass::Rail)).copied().collect();
            if road_lanes.len() < self.lanes_on[surface].len() && !road_lanes.is_empty() {
                let reach = shapes::around_lines(&lines(&road_lanes), ROAD_OVER_RAIL, OWN_GRID);
                quiet.extend(shapes::difference(&whole, &reach));
                whole = shapes::intersect(&whole, &reach);
            }
            let far_rings = shapes::difference(&whole, &far_cut);
            for (class, part) in self.by_class(surface, &whole, classes) {
                road_near.push((class, part));
            }
            for (class, part) in self.by_class(surface, &far_rings, classes) {
                road_far.push((class, part));
            }
        }
        let differs = road_near != road_far;
        for (class, rings) in road_far {
            drawing.rings.push((Kind::of(class), false, self.own_rings(&rings)));
        }
        if differs {
            for (class, rings) in road_near {
                drawing.rings.push((Kind::of(class), true, self.own_rings(&rings)));
            }
        }
        drawing.rings.push((Kind::Quiet, false, self.own_rings(&shapes::union(&quiet))));
        drawing.rings.push((Kind::Building, false, self.own_rings(&self.building)));
        drawing.rings.push((Kind::Water, false, self.own_rings(&self.water)));
        drawing.rings.retain(|(_, _, rings)| !rings.is_empty());
        drawing
    }

    /// A connected surface cut up by the classes of the lanes on it: each part's lanes' class, its border halfway
    /// between the lanes of the two classes.
    fn by_class(&self, surface: usize, rings: &[Ring], classes: &[Option<RoadClass>]) -> Vec<(RoadClass, Vec<Ring>)> {
        if rings.is_empty() {
            return Vec::new();
        }
        let of = |lane: &usize| classes.get(*lane).copied().flatten();
        let mut here: Vec<RoadClass> = self.lanes_on[surface].iter().filter_map(of).collect();
        // Rail only where nothing else runs.
        if here.iter().any(|class| *class != RoadClass::Rail) {
            here.retain(|class| *class != RoadClass::Rail);
        }
        here.sort_unstable();
        here.dedup();
        let Some(&lowest) = here.first() else {
            // No lane on it joins anything shown: as much a road as what's round it.
            let class = classes.iter().flatten().copied().filter(|c| *c != RoadClass::Rail).max();
            return vec![(class.unwrap_or(RoadClass::Local), rings.to_vec())];
        };
        if here.len() == 1 {
            return vec![(lowest, rings.to_vec())];
        }
        let points_of = |wanted: &dyn Fn(RoadClass) -> bool| -> Vec<[f32; 2]> {
            self.lanes_on[surface]
                .iter()
                .filter(|lane| of(lane).is_some_and(wanted))
                .flat_map(|&lane| self.lanes[lane].iter().map(|p| [p[0], p[2]]))
                .collect()
        };
        let mut parts = Vec::new();
        let mut left = rings.to_vec();
        // Highest first: each class takes what's nearer its lanes than any lower class's.
        for &class in here.iter().rev() {
            if class == lowest {
                parts.push((class, left.clone()));
                break;
            }
            let own = points_of(&|c| c == class);
            let rest = points_of(&|c| c < class);
            let region = nearer_than(&left, &own, &rest);
            parts.push((class, shapes::intersect(&left, &region)));
            left = shapes::difference(&left, &region);
        }
        parts
    }

    /// How far the road reaches past its outermost lanes at the shown nodes, m.
    fn beside_lanes(&self, prefab: &Prefab, far: &[bool]) -> f64 {
        let mut widest: f32 = 0.0;
        for (i, node) in prefab.nodes.iter().enumerate() {
            if !far.get(i).copied().unwrap_or(false) {
                continue;
            }
            for sign in [1.0f32, -1.0] {
                if let (Some(lane), Some(edge)) = (
                    self.outermost_lane(node, sign),
                    self.road_faces.edge(node.position, node.height, node.direction, sign, 0.0, 12.0),
                ) {
                    widest = widest.max(edge - lane);
                }
            }
        }
        if widest > 0.5 { f64::from(widest) + 0.05 } else { CLAIM }
    }
}

/// The area nearer `own` than `rest` over `rings`' box, traced at [`CLASS_LATTICE`].
fn nearer_than(rings: &[Ring], own: &[[f32; 2]], rest: &[[f32; 2]]) -> Vec<Ring> {
    let grid = OWN_GRID as f32;
    let spots = rings.iter().flatten();
    let (mut x0, mut z0, mut x1, mut z1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in spots {
        let (x, z) = (p.x as f32 / grid, p.y as f32 / grid);
        (x0, z0, x1, z1) = (x0.min(x), z0.min(z), x1.max(x), z1.max(z));
    }
    let (x0, z0) = (x0 - 2.0 * CLASS_LATTICE, z0 - 2.0 * CLASS_LATTICE);
    let (w, h) = (((x1 - x0) / CLASS_LATTICE).ceil() as usize + 3, ((z1 - z0) / CLASS_LATTICE).ceil() as usize + 3);
    let nearest = |list: &[[f32; 2]], x: f32, z: f32| {
        list.iter().map(|p| (p[0] - x).hypot(p[1] - z)).fold(f32::INFINITY, f32::min)
    };
    let mut field = vec![-1.0f32; w * h];
    for j in 1..h - 1 {
        for i in 1..w - 1 {
            let (x, z) = (x0 + i as f32 * CLASS_LATTICE, z0 + j as f32 * CLASS_LATTICE);
            field[j * w + i] = nearest(rest, x, z) - nearest(own, x, z);
        }
    }
    sea::trace(&field, w, h, 0.0)
        .into_iter()
        .map(|ring| {
            let ring: Ring = ring
                .iter()
                .map(|p| own_spot(x0 + p[0] * CLASS_LATTICE, z0 + p[1] * CLASS_LATTICE))
                .collect();
            ring
        })
        .collect::<Vec<Ring>>()
}

/// The convex hull of `points` on the ground, each keeping its height.
fn hull_of(mut points: Vec<[f32; 3]>) -> Vec<[f32; 3]> {
    points.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[2].total_cmp(&b[2])));
    points.dedup_by(|a, b| a[0] == b[0] && a[2] == b[2]);
    if points.len() < 3 {
        return points;
    }
    let turn = |o: [f32; 3], a: [f32; 3], b: [f32; 3]| (a[0] - o[0]) * (b[2] - o[2]) - (a[2] - o[2]) * (b[0] - o[0]);
    let mut hull: Vec<[f32; 3]> = Vec::with_capacity(points.len());
    for pass in 0..2 {
        let start = hull.len();
        for index in 0..points.len() {
            let p = points[if pass == 0 { index } else { points.len() - 1 - index }];
            while hull.len() >= start + 2 && turn(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0 {
                hull.pop();
            }
            hull.push(p);
        }
        hull.pop();
    }
    hull
}

/// Faces in groups that touch.
fn connected(faces: &[Triangle]) -> Vec<Vec<Triangle>> {
    // Corners a centimetre apart are one corner.
    let key = |v: &[f32; 3]| ((v[0] * 50.0).round() as i32, (v[1] * 50.0).round() as i32, (v[2] * 50.0).round() as i32);
    let mut parent: Vec<usize> = (0..faces.len()).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut seen: HashMap<(i32, i32, i32), usize> = HashMap::new();
    for (i, t) in faces.iter().enumerate() {
        for v in t {
            if let Some(&other) = seen.get(&key(v)) {
                let (a, b) = (root(&mut parent, i), root(&mut parent, other));
                parent[a] = b;
            } else {
                seen.insert(key(v), i);
            }
        }
    }
    let mut groups: HashMap<usize, Vec<Triangle>> = HashMap::new();
    for (i, t) in faces.iter().enumerate() {
        groups.entry(root(&mut parent, i)).or_default().push(*t);
    }
    let mut list: Vec<(usize, Vec<Triangle>)> = groups.into_iter().collect();
    list.sort_by_key(|(group, _)| *group);
    list.into_iter().map(|(_, faces)| faces).collect()
}

/// A prefab's outlines for one way of being joined: what each is, whether it shows only close up, and its rings in its
/// own space (holes the other way round).
#[derive(Default)]
pub struct Drawing {
    pub rings: Vec<(Kind, bool, Vec<Own3>)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prefab::{NavCurve, PrefabNode, SpawnPoint};

    fn node(curves_in: Vec<usize>, curves_out: Vec<usize>) -> PrefabNode {
        PrefabNode { height: 0.0, position: [0.0, 0.0], direction: [0.0, -1.0], rise: 0.0, lanes: 2, curves_in, curves_out, terrain_points: Vec::new() }
    }

    fn lane(start: [f32; 2], end: [f32; 2], next: Vec<usize>) -> NavCurve {
        let length = (end[0] - start[0]).hypot(end[1] - start[1]);
        let direction = [(end[0] - start[0]) / length, (end[1] - start[1]) / length];
        let rotations = [[1.0, 0.0, 0.0, 0.0]; 2];
        NavCurve { start, end, heights: [0.0, 0.0], start_direction: direction, end_direction: direction, rotations, length, next, trucks: true }
    }

    fn point(neighbours: Vec<usize>) -> MapPoint {
        MapPoint {
            position: [0.0, 0.0],
            kind: MapPointKind::Road(None),
            lane_offset: 0.0,
            area_color: 0,
            area_over_roads: false,
            navigation_only: false,
            control_node: None,
            neighbours,
        }
    }

    /// A station along a road: a lane each way, and one off the first past the pumps and back on to it.
    fn station(kind: SpawnKind, map_points: Vec<MapPoint>) -> Prefab {
        Prefab {
            nodes: vec![node(vec![0], vec![1, 3]), PrefabNode { direction: [0.0, 1.0], ..node(vec![1], vec![2, 4]) }],
            map_points,
            spawn_points: vec![SpawnPoint { position: [0.0, 0.0], kind }],
            triggers: Vec::new(),
            curves: vec![
                lane([0.0, 0.0], [0.0, 40.0], vec![2, 3]),
                lane([4.0, 100.0], [4.0, 0.0], vec![]),
                lane([0.0, 40.0], [0.0, 100.0], vec![]),
                lane([0.0, 40.0], [30.0, 60.0], vec![4]),
                lane([30.0, 60.0], [0.0, 100.0], vec![]),
            ],
        }
    }

    #[test]
    fn a_filling_station_along_a_road_has_that_road_and_not_its_way_past_the_pumps() {
        let straight = || vec![point(vec![1]), point(vec![0])];
        assert_eq!(station_road(&station(SpawnKind::Fuel, straight())), Some(vec![(0, 1, vec![0, 2]), (1, 0, vec![1])]));
        // Not a place of another kind.
        assert_eq!(station_road(&station(SpawnKind::Other(99), straight())), None);
        // Nor where the game's map forks through it.
        let forks = vec![point(vec![1, 2, 3]), point(vec![0]), point(vec![0]), point(vec![0])];
        assert_eq!(station_road(&station(SpawnKind::Fuel, forks)), None);
        // Nor on a one-way slip road: its lanes past the pumps are all there is.
        let mut one_way = station(SpawnKind::Fuel, straight());
        one_way.nodes[1].curves_in.clear();
        assert_eq!(station_road(&one_way), None);
        // Driveways off a road outside it, side by side or round a corner: nothing in it is a road.
        for direction in [[0.0, -1.0], [1.0, 0.0]] {
            let mut driveways = station(SpawnKind::Fuel, straight());
            driveways.nodes[1].direction = direction;
            assert_eq!(station_road(&driveways), Some(Vec::new()));
        }
    }
}
