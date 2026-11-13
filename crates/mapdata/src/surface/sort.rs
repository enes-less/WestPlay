//! What each face of a prefab's model is, from the game's lists and from where its lanes run.
//!
//! A prefab's textures can't say: `stripes_01`, `sidewalk_*`, `concrete_gs_01` are sheets of bands, `cargo_01` an atlas,
//! and `substance` is a tyre property most compiled materials have lost. What holds on every map and mod is that lanes
//! run on road: road is the faces a lane runs on and the flat surface going on from there at their level, but not on to
//! terrain: the grass and the yard round a road are level with it too. What's left
//! stands apart from the lanes: a kerb's height above them sidewalk or island, higher a structure, level with them paved.
//! Physics meshes (`coll*`), fades to the terrain (`tran*`), decals and glass are never surfaces.

use std::collections::{HashMap, HashSet};

use crate::files::GameFiles;
use crate::prefab::{NavCurve, Prefab};
use crate::sii;

use super::faces::{Materials, Piece};

/// Up to this high above the lanes, a surface is a kerb's height: sidewalk or island.
const KERB: f32 = 0.45;
/// How far from a lane its height says anything about a surface, m.
const NEAR: f32 = 12.0;
const LEVEL_CELL: f32 = 6.0;
/// How far one flat face may turn from the next and still be the same surface, as the cosine (6 degrees).
const SAME_SLAB: f32 = 0.9945;
/// A lane's point this close above or below a face runs on it, m.
const ON_LANE: f32 = 0.12;
/// How far above the lanes a road surface may stand and still be road, m: below the lowest kerb (10 cm in France).
const MAY_RISE: f32 = 0.06;
const SEED_CELL: f32 = 4.0;
const TOUCH_CELL: f32 = 2.0;
/// How near a corner has to be to a side to lie on it, m.
const TOUCH: f32 = 0.015;

pub type Triangle = [[f32; 3]; 3];

/// What a face of a model shows as.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FaceKind {
    Ground,
    Water,
    /// Paved ground, a sidewalk: the map's one quiet colour.
    Quiet,
    Building,
    Road,
}

/// A lane's points in its prefab's own space, about `step` apart along its real 3D curve (the ends' positions and
/// rotations; the tangents as long as the lane): each with the road's right there, blended from the ends' as the game
/// blends them, which holds the road's bank.
pub fn lane_frames(curve: &NavCurve, step: f32) -> Vec<([f32; 3], [f32; 3])> {
    let rotate = |q: [f32; 4], v: [f64; 3]| crate::surface::rotate(q.map(f64::from), v);
    let length = f64::from(curve.length);
    let p0 = [f64::from(curve.start[0]), f64::from(curve.heights[0]), f64::from(curve.start[1])];
    let p1 = [f64::from(curve.end[0]), f64::from(curve.heights[1]), f64::from(curve.end[1])];
    let [q0, q1] = curve.rotations;
    let (m0, m1) = (rotate(q0, [0.0, 0.0, -length]), rotate(q1, [0.0, 0.0, -length]));
    let (r0, r1) = (rotate(q0, [1.0, 0.0, 0.0]), rotate(q1, [1.0, 0.0, 0.0]));
    let steps = ((curve.length / step).ceil() as usize).clamp(1, 512);
    (0..=steps)
        .map(|n| {
            let s = n as f64 / steps as f64;
            let (s2, s3) = (s * s, s * s * s);
            let (h00, h10, h01, h11) = (2.0 * s3 - 3.0 * s2 + 1.0, s3 - 2.0 * s2 + s, -2.0 * s3 + 3.0 * s2, s3 - s2);
            let at = [0, 1, 2].map(|k| (h00 * p0[k] + h10 * m0[k] + h01 * p1[k] + h11 * m1[k]) as f32);
            let right = [0, 1, 2].map(|k| (h00 * r0[k] + h01 * r1[k]) as f32);
            (at, right)
        })
        .collect()
}

/// A prefab's lanes as points, to measure surfaces against.
pub struct Levels {
    points: Vec<[f32; 3]>,
    /// Each point's right on the ground (a unit vector, x and z) and how much the road rises per metre along it.
    across: Vec<([f32; 2], f32)>,
    lane: Vec<u32>,
    /// `points` up to here are lanes; road ends follow.
    lanes_end: usize,
    grid: HashMap<(i32, i32), Vec<u32>>,
}

impl Levels {
    pub fn new(prefab: &Prefab) -> Self {
        let (mut points, mut across, mut lane) = (Vec::new(), Vec::new(), Vec::new());
        for (index, curve) in prefab.curves.iter().enumerate() {
            for (p, right) in lane_frames(curve, 1.0) {
                points.push(p);
                let flat = right[0].hypot(right[2]).max(1e-6);
                across.push(([right[0] / flat, right[2] / flat], right[1] / flat));
                lane.push(index as u32);
            }
        }
        let lanes_end = points.len();
        for (index, node) in prefab.nodes.iter().enumerate() {
            points.push([node.position[0], node.height, node.position[1]]);
            across.push(([1.0, 0.0], 0.0));
            lane.push((prefab.curves.len() + index) as u32);
        }
        let mut grid: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for (i, p) in points.iter().enumerate() {
            grid.entry(((p[0] / LEVEL_CELL).floor() as i32, (p[2] / LEVEL_CELL).floor() as i32))
                .or_default()
                .push(i as u32);
        }
        Self { points, across, lane, lanes_end, grid }
    }

    /// How far `y` stands above the lanes within [`NEAR`] of (x, z): each lane taken at its nearest point (so a slope
    /// along it doesn't count) and carried across to (x, z) on its bank, the lane nearest in height kept (a junction has
    /// lanes at several heights).
    pub fn raise(&self, x: f32, y: f32, z: f32) -> Option<f32> {
        let (cx, cz) = ((x / LEVEL_CELL).floor() as i32, (z / LEVEL_CELL).floor() as i32);
        let mut lanes: HashMap<u32, (f32, f32)> = HashMap::new();
        for ix in -2..=2 {
            for iz in -2..=2 {
                for &i in self.grid.get(&(cx + ix, cz + iz)).map_or(&[][..], Vec::as_slice) {
                    let p = self.points[i as usize];
                    let d = (p[0] - x).hypot(p[2] - z);
                    if d < NEAR {
                        let entry = lanes.entry(self.lane[i as usize]).or_insert((f32::INFINITY, 0.0));
                        if d < entry.0 {
                            let (right, rise) = self.across[i as usize];
                            let lateral = (x - p[0]) * right[0] + (z - p[2]) * right[1];
                            *entry = (d, y - (p[1] + lateral * rise));
                        }
                    }
                }
            }
        }
        lanes.values().map(|lane| lane.1).min_by(|a, b| a.abs().total_cmp(&b.abs()))
    }

    /// How far `y` stands above the nearest point of all, wherever that is.
    pub fn raise_far(&self, x: f32, y: f32, z: f32) -> Option<f32> {
        self.points
            .iter()
            .min_by(|a, b| (a[0] - x).hypot(a[2] - z).total_cmp(&(b[0] - x).hypot(b[2] - z)))
            .map(|p| y - p[1])
    }
}

/// A face being sorted.
struct Face {
    t: Triangle,
    piece: usize,
    normal: [f32; 3],
    /// How much it faces up, 0..1.
    up: f32,
    /// What it is whatever its height: water, ground, a listed sidewalk.
    fixed: Option<FaceKind>,
    /// Mapped as terrain is (`.tg0.`): grass, a yard's asphalt. A road's strips never are.
    terrain: bool,
    /// Bare ground by its material (dirt, gravel, snow): a road if lanes run on it, ground otherwise.
    soft: bool,
    /// A two-layer material with ground under or over road: how much of its road layer each corner shows.
    road_share: Option<[f32; 3]>,
}

fn on_side(v: [f32; 3], a: [f32; 3], b: [f32; 3]) -> bool {
    let (d, w) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [v[0] - a[0], v[1] - a[1], v[2] - a[2]]);
    let long = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    if long < 1e-8 {
        return false;
    }
    let t = ((w[0] * d[0] + w[1] * d[1] + w[2] * d[2]) / long).clamp(0.0, 1.0);
    let e = [w[0] - t * d[0], w[1] - t * d[1], w[2] - t * d[2]];
    e[0] * e[0] + e[1] * e[1] + e[2] * e[2] <= TOUCH * TOUCH
}

/// Which faces touch: sharing a corner, or one's corner lying on the other's side (models have T-junctions, a strip's
/// fine tessellation against a road's long edge).
struct Touching {
    corners: HashMap<(i32, i32, i32), Vec<u32>>,
    sides: HashMap<(i32, i32), Vec<u32>>,
    points: HashMap<(i32, i32), Vec<FaceCorner>>,
}

/// A face and one of its corners.
type FaceCorner = (u32, [f32; 3]);

impl Touching {
    fn key(v: [f32; 3]) -> (i32, i32, i32) {
        ((v[0] * 100.0).round() as i32, (v[1] * 100.0).round() as i32, (v[2] * 100.0).round() as i32)
    }

    fn cell(x: f32, z: f32) -> (i32, i32) {
        ((x / TOUCH_CELL).floor() as i32, (z / TOUCH_CELL).floor() as i32)
    }

    fn new<'a>(faces: impl Iterator<Item = (usize, &'a Triangle)>) -> Self {
        let mut this = Self { corners: HashMap::new(), sides: HashMap::new(), points: HashMap::new() };
        for (i, t) in faces {
            for k in 0..3 {
                let v = t[k];
                this.corners.entry(Self::key(v)).or_default().push(i as u32);
                this.points.entry(Self::cell(v[0], v[2])).or_default().push((i as u32, v));
                let w = t[(k + 1) % 3];
                let steps = ((v[0] - w[0]).hypot(v[2] - w[2]) / (TOUCH_CELL * 0.5)).ceil().max(1.0) as usize;
                let mut last = None;
                for n in 0..=steps {
                    let s = n as f32 / steps as f32;
                    let cell = Self::cell(v[0] + s * (w[0] - v[0]), v[2] + s * (w[2] - v[2]));
                    if last != Some(cell) {
                        last = Some(cell);
                        this.sides.entry(cell).or_default().push(i as u32);
                    }
                }
            }
        }
        this
    }

    /// The faces touching `a`.
    fn around(&self, all: &dyn Fn(usize) -> Triangle, a: &Triangle, out: &mut Vec<u32>) {
        out.clear();
        for v in a {
            out.extend(self.corners.get(&Self::key(*v)).map_or(&[][..], Vec::as_slice));
            // Its corners on others' sides.
            for &j in self.sides.get(&Self::cell(v[0], v[2])).map_or(&[][..], Vec::as_slice) {
                let b = all(j as usize);
                if on_side(*v, b[0], b[1]) || on_side(*v, b[1], b[2]) || on_side(*v, b[2], b[0]) {
                    out.push(j);
                }
            }
        }
        // Others' corners on its sides.
        for k in 0..3 {
            let (v, w) = (a[k], a[(k + 1) % 3]);
            let steps = ((v[0] - w[0]).hypot(v[2] - w[2]) / (TOUCH_CELL * 0.5)).ceil().max(1.0) as usize;
            let mut last = None;
            for n in 0..=steps {
                let s = n as f32 / steps as f32;
                let cell = Self::cell(v[0] + s * (w[0] - v[0]), v[2] + s * (w[2] - v[2]));
                if last == Some(cell) {
                    continue;
                }
                last = Some(cell);
                for &(j, p) in self.points.get(&cell).map_or(&[][..], Vec::as_slice) {
                    if on_side(p, v, w) {
                        out.push(j);
                    }
                }
            }
        }
        out.sort_unstable();
        out.dedup();
    }
}

/// What each face is. Road is what lanes run on and the flat surface that goes on from there at their level; `None`
/// for faces that don't show from above as anything.
fn sort_faces(faces: &[Face], level: &Levels) -> Vec<Option<FaceKind>> {
    #[derive(Clone, Copy, PartialEq)]
    enum State {
        Unsorted,
        Is(FaceKind),
    }
    use State::{Is, Unsorted};
    let mut kinds: Vec<State> = faces.iter().map(|f| f.fixed.map_or(Unsorted, Is)).collect();
    let flat = |f: &Face| f.fixed.is_none() && f.up >= 0.8;
    // Faces lanes run on.
    let mut cells: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
    for (i, f) in faces.iter().enumerate() {
        if !flat(f) {
            continue;
        }
        let (x0, x1) = (f.t[0][0].min(f.t[1][0]).min(f.t[2][0]), f.t[0][0].max(f.t[1][0]).max(f.t[2][0]));
        let (z0, z1) = (f.t[0][2].min(f.t[1][2]).min(f.t[2][2]), f.t[0][2].max(f.t[1][2]).max(f.t[2][2]));
        for gx in (x0 / SEED_CELL).floor() as i32..=(x1 / SEED_CELL).floor() as i32 {
            for gz in (z0 / SEED_CELL).floor() as i32..=(z1 / SEED_CELL).floor() as i32 {
                cells.entry((gx, gz)).or_default().push(i as u32);
            }
        }
    }
    let middle = |f: &Face| -> [f32; 3] { std::array::from_fn(|k| (f.t[0][k] + f.t[1][k] + f.t[2][k]) / 3.0) };
    let mut queue: Vec<u32> = Vec::new();
    // A face painted with a road layer says it's road itself, at the lanes' level.
    for (i, f) in faces.iter().enumerate() {
        if kinds[i] == Unsorted && f.road_share.is_some() {
            let m = middle(f);
            if level.raise(m[0], m[1], m[2]).is_none_or(|r| r <= MAY_RISE) {
                kinds[i] = Is(FaceKind::Road);
                queue.push(i as u32);
            }
        }
    }
    for p in &level.points[..level.lanes_end] {
        let cell = ((p[0] / SEED_CELL).floor() as i32, (p[2] / SEED_CELL).floor() as i32);
        for &i in cells.get(&cell).map_or(&[][..], Vec::as_slice) {
            if kinds[i as usize] != Unsorted {
                continue;
            }
            if under_point(&faces[i as usize].t, p[0], p[2]).is_some_and(|y| (y - p[1]).abs() <= ON_LANE) {
                kinds[i as usize] = Is(FaceKind::Road);
                queue.push(i);
            }
        }
    }
    // The same flat surface going on from there, at the lanes' level.
    let touching = Touching::new(faces.iter().enumerate().filter(|(_, f)| flat(f)).map(|(i, f)| (i, &f.t)));
    let all = |i: usize| faces[i].t;
    let mut around: Vec<u32> = Vec::new();
    while let Some(i) = queue.pop() {
        let a = &faces[i as usize];
        touching.around(&all, &a.t, &mut around);
        for &j in &around {
            if kinds[j as usize] != Unsorted {
                continue;
            }
            let b = &faces[j as usize];
            if a.normal[0] * b.normal[0] + a.normal[1] * b.normal[1] + a.normal[2] * b.normal[2] < SAME_SLAB {
                continue;
            }
            // Into another material it may only go on at the lanes' level: not up on to a raised sidewalk.
            let m = middle(b);
            if a.piece != b.piece && level.raise(m[0], m[1], m[2]).is_some_and(|r| r > MAY_RISE) {
                continue;
            }
            // Terrain is road only where a lane runs on it: the grass and the yard round a road are level with it too.
            if b.terrain && !a.terrain {
                continue;
            }
            kinds[j as usize] = Is(FaceKind::Road);
            queue.push(j);
        }
    }
    for (i, f) in faces.iter().enumerate() {
        if f.soft && kinds[i] == Unsorted {
            kinds[i] = Is(FaceKind::Ground);
        }
    }
    // What's left stands apart from the lanes: each connected slab one thing, by most of its area.
    let area = |f: &Face| {
        ((f.t[1][0] - f.t[0][0]) * (f.t[2][2] - f.t[0][2]) - (f.t[2][0] - f.t[0][0]) * (f.t[1][2] - f.t[0][2])).abs() * 0.5
    };
    let mut slab_of: Vec<Option<FaceKind>> = vec![None; faces.len()];
    let mut done = vec![false; faces.len()];
    for first in 0..faces.len() {
        if done[first] || kinds[first] != Unsorted || !flat(&faces[first]) {
            continue;
        }
        let mut members = vec![first as u32];
        let mut stack = vec![first as u32];
        done[first] = true;
        while let Some(i) = stack.pop() {
            touching.around(&all, &faces[i as usize].t, &mut around);
            for &j in &around {
                if !done[j as usize] && kinds[j as usize] == Unsorted {
                    done[j as usize] = true;
                    members.push(j);
                    stack.push(j);
                }
            }
        }
        let (mut whole, mut level_up, mut tall) = (0.0f32, 0.0f32, 0.0f32);
        for &i in &members {
            let f = &faces[i as usize];
            let (m, a) = (middle(f), area(f));
            whole += a;
            level_up += a * f.up;
            let raise = level.raise(m[0], m[1], m[2]).or_else(|| level.raise_far(m[0], m[1], m[2]));
            if raise.unwrap_or(f32::INFINITY) > KERB {
                tall += a;
            }
        }
        let kind = if tall * 2.0 > whole {
            Some(FaceKind::Building)
        } else if level_up / whole.max(1e-6) < 0.97 {
            // A bank no lane runs on: the ground shows.
            None
        } else {
            Some(FaceKind::Quiet)
        };
        for &i in &members {
            slab_of[i as usize] = kind;
        }
    }
    faces
        .iter()
        .enumerate()
        .map(|(i, f)| {
            if let Is(kind) = kinds[i] {
                return (f.up >= 0.8).then_some(kind);
            }
            if flat(f) {
                return slab_of[i];
            }
            // Not flat: a roof if it stands high, else nothing that shows.
            let m = middle(f);
            let raise = level.raise(m[0], m[1], m[2]).or_else(|| level.raise_far(m[0], m[1], m[2]));
            (raise.unwrap_or(f32::INFINITY) > KERB && f.up >= 0.2).then_some(FaceKind::Building)
        })
        .collect()
}

/// How high `t` is at (x, z), if that's on it.
pub fn under_point(t: &Triangle, x: f32, z: f32) -> Option<f32> {
    let [a, b, c] = *t;
    let area = (b[0] - a[0]) * (c[2] - a[2]) - (c[0] - a[0]) * (b[2] - a[2]);
    if area.abs() < 1e-6 {
        return None;
    }
    let w0 = ((b[0] - x) * (c[2] - z) - (c[0] - x) * (b[2] - z)) / area;
    let w1 = ((c[0] - x) * (a[2] - z) - (a[0] - x) * (c[2] - z)) / area;
    let w2 = 1.0 - w0 - w1;
    (w0 >= -1e-4 && w1 >= -1e-4 && w2 >= -1e-4).then(|| w0 * a[1] + w1 * b[1] + w2 * c[1])
}

/// A triangle cut where its road layer's share crosses a half: the parts on the road's side and the rest.
fn cut_by_paint(t: Triangle, share: [f32; 3]) -> (Vec<Triangle>, Vec<Triangle>) {
    let (mut road, mut rest) = (Vec::with_capacity(4), Vec::with_capacity(4));
    for i in 0..3 {
        let j = (i + 1) % 3;
        let (a, b) = (t[i], t[j]);
        let (sa, sb) = (share[i], share[j]);
        if sa >= 0.5 {
            road.push(a);
        } else {
            rest.push(a);
        }
        if (sa >= 0.5) != (sb >= 0.5) {
            let s = (0.5 - sa) / (sb - sa);
            let crossing: [f32; 3] = std::array::from_fn(|k| a[k] + s * (b[k] - a[k]));
            road.push(crossing);
            rest.push(crossing);
        }
    }
    let fan = |polygon: Vec<[f32; 3]>| -> Vec<Triangle> {
        (1..polygon.len().saturating_sub(1)).map(|i| [polygon[0], polygon[i], polygon[i + 1]]).collect()
    };
    (fan(road), fan(rest))
}

/// The game's lists a surface is looked up in.
pub struct Rules {
    /// Textures of terrain that isn't paved.
    pub ground_textures: HashSet<String>,
    pub sidewalk_textures: HashSet<String>,
}

/// Whether a substance is something paved.
pub fn is_paved(substance: &str) -> bool {
    substance.starts_with("road") || substance == "concrete"
}

impl Rules {
    pub fn load(files: &GameFiles, materials: &Materials) -> Self {
        let mut sidewalk_textures = HashSet::new();
        for unit in sii::read_family_or_note(files, "/def/world", "road_sidewalk") {
            if let Some(path) = unit.get("path") {
                sidewalk_textures.extend(materials.get(files, path.trim_matches('"')).textures.iter().cloned());
            }
        }
        let mut ground_textures = HashSet::new();
        for unit in sii::read_family_or_note(files, "/def/world", "terrain_material") {
            let Some(path) = unit.get("path") else { continue };
            let material = materials.get(files, path.trim_matches('"'));
            if material.unseen || material.effect.starts_with("eut2.water") || is_paved(&material.substance) {
                continue;
            }
            ground_textures.extend(material.textures.first().cloned());
        }
        Self { ground_textures, sidewalk_textures }
    }

    /// Whether a texture is ground: listed terrain, or the stand-in a transition's terrain side has.
    pub fn is_ground(&self, texture: &str) -> bool {
        self.ground_textures.contains(texture) || texture.contains("ter_trans")
    }
}

/// Whether a piece never shows as a surface: physics, a fade into the terrain, a decal, a light, glass.
pub fn never_surface(piece: &Piece) -> bool {
    let effect = &piece.material.effect;
    piece.part.starts_with("coll")
        || piece.part.starts_with("tran")
        || effect.contains(".decal")
        || effect.contains("lightmap")
        || effect.contains(".a.")
}

/// A model's faces sorted: triangles by what they show as. A two-layer road face is cut where its paint says.
pub fn sorted_faces(pieces: &[Piece], level: &Levels, rules: &Rules) -> Vec<(Triangle, FaceKind)> {
    let mut faces: Vec<Face> = Vec::new();
    for (index, piece) in pieces.iter().enumerate() {
        if never_surface(piece) {
            continue;
        }
        let material = &piece.material;
        let first = material.textures.first().map_or("", String::as_str);
        // Vertex paint blends the two layers: its alpha is how much of the second shows (on every two-layer face a lane
        // runs on, the road layer shows in full).
        let ground_layer = (material.effect.contains(".weight.weight.") && material.textures.len() >= 2)
            .then(|| (rules.is_ground(&material.textures[0]), rules.is_ground(&material.textures[1])));
        // Only a paved second layer is a road's: `grass + dirt` strips beside sidewalks are ground on both.
        let bare = matches!(material.substance.as_str(), "grass" | "dirt" | "gravel" | "snow");
        let mixed = matches!(ground_layer, Some((true, false) | (false, true))) && !bare;
        // A material says what it is before its first texture does.
        let fixed = if material.effect.starts_with("eut2.water") {
            Some(FaceKind::Water)
        } else if mixed {
            None
        } else if material.substance == "grass" || (material.substance.is_empty() && rules.is_ground(first)) {
            Some(FaceKind::Ground)
        } else if rules.sidewalk_textures.contains(first) {
            Some(FaceKind::Quiet)
        } else {
            None
        };
        let soft = matches!(material.substance.as_str(), "dirt" | "gravel" | "snow");
        for (t, paint) in piece.triangles.iter().zip(&piece.paint) {
            let (a, b, c) = (t[0], t[1], t[2]);
            let road_share = match ground_layer {
                Some((true, false)) if mixed => Some(*paint),
                Some((false, true)) if mixed => Some(paint.map(|alpha| 1.0 - alpha)),
                _ => None,
            };
            let fixed = if road_share.is_some_and(|share| share.iter().all(|&v| v < 0.5)) {
                Some(FaceKind::Ground)
            } else {
                fixed
            };
            let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
            let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
            let long = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if long < 1e-6 {
                continue;
            }
            let sign = if n[1] < 0.0 { -1.0 } else { 1.0 };
            faces.push(Face {
                t: *t,
                piece: index,
                normal: [sign * n[0] / long, sign * n[1] / long, sign * n[2] / long],
                up: n[1].abs() / long,
                fixed,
                terrain: material.effect.contains(".tg0."),
                soft,
                road_share,
            });
        }
    }
    let kinds = sort_faces(&faces, level);
    let mut out = Vec::with_capacity(faces.len());
    for (face, kind) in faces.iter().zip(kinds) {
        let Some(kind) = kind else { continue };
        match (kind, face.road_share) {
            (FaceKind::Road, Some(share)) => {
                let (road, rest) = cut_by_paint(face.t, share);
                out.extend(road.into_iter().map(|t| (t, FaceKind::Road)));
                out.extend(rest.into_iter().map(|t| (t, FaceKind::Ground)));
            }
            _ => out.push((face.t, kind)),
        }
    }
    out
}

/// The ways through a prefab's lanes: for each lane the distance to it from each node's way in, and from it to each
/// node's way out.
pub struct LaneReach {
    pub from: Vec<Vec<Option<f32>>>,
    pub to: Vec<Vec<Option<f32>>>,
}

pub fn lane_reach(prefab: &Prefab) -> LaneReach {
    let (curves, nodes) = (prefab.curves.len(), prefab.nodes.len());
    let mut from = vec![vec![None; nodes]; curves];
    let mut to: Vec<Vec<Option<f32>>> = vec![vec![None; nodes]; curves];
    let mut before: Vec<Vec<usize>> = vec![Vec::new(); curves];
    for (c, curve) in prefab.curves.iter().enumerate() {
        for &n in &curve.next {
            if n < curves {
                before[n].push(c);
            }
        }
    }
    for (node, data) in prefab.nodes.iter().enumerate() {
        let mut queue: Vec<usize> = Vec::new();
        for &c in &data.curves_in {
            from[c][node] = Some(0.0);
            queue.push(c);
        }
        while let Some(c) = queue.pop() {
            let reached = from[c][node].unwrap_or(0.0) + prefab.curves[c].length;
            for &n in &prefab.curves[c].next {
                if n < curves && from[n][node].is_none_or(|d: f32| reached < d) {
                    from[n][node] = Some(reached);
                    queue.push(n);
                }
            }
        }
        let mut queue: Vec<usize> = Vec::new();
        for &c in &data.curves_out {
            to[c][node] = Some(0.0);
            queue.push(c);
        }
        while let Some(c) = queue.pop() {
            let reached = to[c][node].unwrap_or(0.0) + prefab.curves[c].length;
            for &b in &before[c] {
                if to[b][node].is_none_or(|d: f32| reached < d) {
                    to[b][node] = Some(reached);
                    queue.push(b);
                }
            }
        }
    }
    LaneReach { from, to }
}

impl LaneReach {
    /// Whether a lane leads from one of a prefab's nodes to the other, either way.
    pub fn linked(&self, prefab: &Prefab, a: usize, b: usize) -> bool {
        prefab.nodes[b].curves_out.iter().any(|&c| self.from[c][a].is_some())
            || prefab.nodes[a].curves_out.iter().any(|&c| self.from[c][b].is_some())
    }
}

/// A prefab's areas of one trigger action (`hud_parking`), each a ring of corners in its own space (x, z).
pub fn trigger_rings(prefab: &Prefab, action: u64) -> Vec<Vec<[f32; 2]>> {
    let triggers = &prefab.triggers;
    let mut seen = vec![false; triggers.len()];
    let mut rings = Vec::new();
    for first in 0..triggers.len() {
        if seen[first] || triggers[first].action != action {
            continue;
        }
        let mut ring = Vec::new();
        let (mut at, mut before) = (first, usize::MAX);
        loop {
            seen[at] = true;
            ring.push(triggers[at].position);
            let next = triggers[at]
                .links
                .iter()
                .filter(|&&link| link >= 0 && (link as usize) < triggers.len() && link as usize != before)
                .map(|&link| link as usize)
                .find(|&link| !seen[link]);
            match next {
                Some(next) => {
                    before = at;
                    at = next;
                }
                None => break,
            }
        }
        rings.push(ring);
    }
    rings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_painted_face_is_cut_where_its_road_layer_ends() {
        let t = [[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 0.0, 2.0]];
        let (road, rest) = cut_by_paint(t, [1.0, 0.0, 0.0]);
        let area = |list: &[Triangle]| -> f32 {
            list.iter()
                .map(|t| ((t[1][0] - t[0][0]) * (t[2][2] - t[0][2]) - (t[2][0] - t[0][0]) * (t[1][2] - t[0][2])).abs() / 2.0)
                .sum()
        };
        assert!((area(&road) - 0.5).abs() < 1e-5, "{}", area(&road));
        assert!((area(&rest) - 1.5).abs() < 1e-5, "{}", area(&rest));
    }

    #[test]
    fn road_goes_on_over_a_shoulder_but_not_on_to_terrain_at_its_level() {
        // A lane along x, on a strip of road, with a shoulder of another material one side and grass the other, all level.
        let points: Vec<[f32; 3]> = (0..=40).map(|x| [x as f32, 0.0, 0.0]).collect();
        let count = points.len();
        let mut grid: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for (i, p) in points.iter().enumerate() {
            grid.entry(((p[0] / LEVEL_CELL).floor() as i32, (p[2] / LEVEL_CELL).floor() as i32)).or_default().push(i as u32);
        }
        let level = Levels { points, across: vec![([0.0, 1.0], 0.0); count], lane: vec![0; count], lanes_end: count, grid };
        let band = |piece: usize, terrain: bool, z0: f32, z1: f32| {
            let (a, b, c, d) = ([0.0, 0.0, z0], [40.0, 0.0, z0], [40.0, 0.0, z1], [0.0, 0.0, z1]);
            [[a, b, c], [a, c, d]].map(|t| Face { t, piece, normal: [0.0, 1.0, 0.0], up: 1.0, fixed: None, terrain, soft: false, road_share: None })
        };
        let faces: Vec<Face> = [band(0, false, -4.0, 4.0), band(1, false, 4.0, 7.0), band(2, true, -30.0, -4.0)].into_iter().flatten().collect();
        let kinds = sort_faces(&faces, &level);
        assert_eq!(kinds[..4], [Some(FaceKind::Road); 4], "{kinds:?}");
        assert!(kinds[4..].iter().all(|kind| *kind != Some(FaceKind::Road)), "{kinds:?}");
    }

    #[test]
    fn height_on_a_face_is_found_inside_it_only() {
        let t = [[0.0, 0.0, 0.0], [10.0, 1.0, 0.0], [0.0, 0.0, 10.0]];
        assert!((under_point(&t, 5.0, 2.0).unwrap() - 0.5).abs() < 1e-5);
        assert_eq!(under_point(&t, 11.0, 2.0), None);
    }
}
