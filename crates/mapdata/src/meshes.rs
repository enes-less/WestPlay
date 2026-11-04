//! The buildings in their real shapes (owner 2026-11-02: "instead of picking the top, we will take its shape"; PLAN.md,
//! "True shapes"): the triangles the game draws of each, not an outline raised to one height. A model's are kept where
//! its shape ([`buildings::trace`]) stands on its ground, measured from that ground (a place's prefab stands on its own
//! yard), and merged on a grid of [`NEAR_CELL`]; a landmark's as they are (owner: "for important buildings, it can be
//! full detail"). And again on a grid of [`FAR_CELL`], to draw far off. A roof over lanes (what a filling station's or
//! a border crossing's model, or a place's prefab, has lifted off its ground) stays the outline it was (buildings.rs),
//! for the cab to leave out while the truck is under it; what other buildings have lifted (a stadium's stands over its
//! ground) is in their shape.
//!
//! A model's triangles are stored once (a whole map's placed would be 152 million at a metre, ETS2's; once each, 5
//! million), in a file of its own under [`MODELS`], and each placing in the file of the tile it's in. A bridge's model
//! is placed point by point as its road is drawn ([`Decks::bridge_point`]), so its triangles are in its tile's file
//! as they're drawn.
//!
//! A model's file, little endian: [`MODEL_MAGIC`], the unit its points are in (m, f32), how high it reaches over its
//! ground (m, f32), how many levels of detail (u32), each level's vertex count, index count and bytes an index (u32
//! each); then each level's vertices (x, y, z in units, i16 each) and its indices (triangles, u16 or u32), each run
//! padded to 4 bytes. A tile's, of [`FILE_ZOOM`]: [`TILE_MAGIC`], how many placings and bridge vertices (u32 each);
//! the placings ([`PLACING_BYTES`] each: the model (u32), where it is in the tile ([`PAST`] over each side, u16 each),
//! how it's turned (radians, f32), its scale (x, y, z, f32 each), how high a road over it lets it reach (cm, u16;
//! `u16::MAX`: no road), flags (u16)), by model; then the bridges' triangles' vertices (where in the tile, u16 each,
//! how high, cm, u16). Both gzipped.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use crate::buildings::{self, Building, Ground, Levels, Placement, PrefabLook, ShapePart};
use crate::files::GameFiles;
use crate::geometry::Point;
use crate::surface::raised::Decks;
use crate::surface::tile;
use crate::sector::Node;
use crate::world::{BuildingKind, BuildingModel};

/// The folder of the files, in the tiles'.
pub const FOLDER: &str = "meshes";
/// The models' folder, in it.
pub const MODELS: &str = "models";
/// The zoom of the tiles the placings are filed by: a couple of kilometres across.
pub const FILE_ZOOM: u8 = 14;
/// How far past its tile a placing or a bridge's vertex may lie, of the tile: a bridge's triangles are long.
const PAST: f64 = 0.5;
const MODEL_MAGIC: u32 = u32::from_le_bytes(*b"WPM1");
const TILE_MAGIC: u32 = u32::from_le_bytes(*b"WPT1");
const PLACING_BYTES: usize = 28;
/// Near, a model's vertices merged on a grid this fine (m): a metre (owner 2026-11-02, Tromsø's: 1.96 million
/// triangles as they are, 322,000 so, little lost to the eye). Far, on this.
const NEAR_CELL: f32 = 1.0;
const FAR_CELL: f32 = 4.0;
/// A triangle no higher than this over its ground (m) lies on it: a floor, paving, a yard.
const ON_GROUND: f32 = 0.05;
/// A bridge's piece (its triangles joined by their corners) no wider than this (m, its box's diagonal) stands as a
/// whole under a road over it: a pier's corners lie outside the road's edges, and the one model of it is as tall
/// everywhere it's placed (Tromsø's).
const PIECE_REACH: f32 = 40.0;
/// No road over a placing.
const NO_CAP: u16 = u16::MAX;

/// A model to draw: a building's, or a place's prefab in one of its variants.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Key {
    Model(u64),
    Prefab(u64, u64),
}

fn key_model(key: Key) -> u64 {
    match key {
        Key::Model(model) | Key::Prefab(model, _) => model,
    }
}

/// What a model's file is, and what its placings need of it.
struct Made {
    id: u32,
    shape: buildings::ModelShape,
    /// Whether its file was written: it has triangles standing on its ground.
    drawn: bool,
    /// A bridge's triangles, its own space (as the game has them), to place point by point.
    bridge: Option<Vec<[[f32; 3]; 3]>>,
}

/// A placing, as its tile's file has it.
struct Placing {
    model: u32,
    at: [u16; 2],
    yaw: f32,
    scale: [f32; 3],
    cap: u16,
}

/// A tile's file, being made: its placings, and its bridges' vertices (where in the tile, how high).
type TileFile = (Vec<Placing>, Vec<[u16; 3]>);

/// What [`write`] made.
#[derive(Debug, Default)]
pub struct Written {
    pub models: usize,
    pub placings: usize,
    pub bridge_triangles: usize,
    pub bytes: u64,
}

/// Whether (x, z) is inside `rings` (an outline and its holes), even-odd.
fn inside(rings: &[Vec<Point>], x: f32, z: f32) -> bool {
    let mut odd = false;
    for ring in rings {
        let mut j = ring.len().wrapping_sub(1);
        for (i, &a) in ring.iter().enumerate() {
            let b = ring[j];
            if (a[1] > z) != (b[1] > z) && x < (b[0] - a[0]) * (z - a[1]) / (b[1] - a[1]) + a[0] {
                odd = !odd;
            }
            j = i;
        }
    }
    odd
}

/// What of `triangles` is in `parts`, measured from its ground: what stands on it (and what's lifted off it, unless
/// `roofs` are left out: a roof over lanes), not what's on it (a floor, paving: [`ON_GROUND`]). Under it is under the
/// map.
fn standing(triangles: &[[[f32; 3]; 3]], parts: &[ShapePart], levels: &Levels, roofs: bool) -> Vec<[[f32; 3]; 3]> {
    let on_ground: Vec<&ShapePart> = parts.iter().filter(|part| roofs || part.bottom <= 0.0).collect();
    triangles
        .iter()
        .filter(|t| {
            let (x, z) = ((t[0][0] + t[1][0] + t[2][0]) / 3.0, (t[0][2] + t[1][2] + t[2][2]) / 3.0);
            on_ground.iter().any(|part| inside(&part.rings, x, z))
        })
        .map(|t| t.map(|v| [v[0], v[1] - levels.at(v[0], v[2]), v[2]]))
        .filter(|t| t.iter().any(|v| v[1] > ON_GROUND))
        .map(|t| t.map(|v| [v[0], v[1].max(0.0), v[2]]))
        .collect()
}

/// `triangles` with their vertices merged on a grid of `cell` (m): each cell's at their middle; what comes to
/// nothing, or twice, left out. `cell` 0: as they are.
fn merged(triangles: &[[[f32; 3]; 3]], cell: f32) -> Vec<[[f32; 3]; 3]> {
    if cell <= 0.0 {
        return triangles.to_vec();
    }
    let key = |v: [f32; 3]| v.map(|c| (c / cell).floor() as i32);
    let mut cells: HashMap<[i32; 3], ([f64; 3], f64)> = HashMap::new();
    for v in triangles.iter().flatten() {
        let entry = cells.entry(key(*v)).or_insert(([0.0; 3], 0.0));
        for (sum, c) in entry.0.iter_mut().zip(v) {
            *sum += f64::from(*c);
        }
        entry.1 += 1.0;
    }
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for t in triangles {
        let keys = t.map(key);
        if keys[0] == keys[1] || keys[1] == keys[2] || keys[0] == keys[2] {
            continue;
        }
        let mut sorted = keys;
        sorted.sort_unstable();
        if seen.insert(sorted) {
            out.push(keys.map(|k| {
                let (sum, count) = cells[&k];
                sum.map(|c| (c / count) as f32)
            }));
        }
    }
    out
}

fn padded(out: &mut Vec<u8>) {
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}

/// A model's file's bytes: its levels of detail, near first.
fn model_file(levels: &[Vec<[[f32; 3]; 3]>], top: f32) -> Vec<u8> {
    let reach = levels.iter().flatten().flatten().flatten().fold(1.0f32, |most, c| most.max(c.abs()));
    let unit = (reach / f32::from(i16::MAX)).max(1.0 / 512.0);
    let mut header = Vec::new();
    let mut body = Vec::new();
    header.extend(MODEL_MAGIC.to_le_bytes());
    header.extend(unit.to_le_bytes());
    header.extend(top.to_le_bytes());
    header.extend((levels.len() as u32).to_le_bytes());
    for triangles in levels {
        let mut index: HashMap<[i16; 3], u32> = HashMap::new();
        let mut vertices: Vec<[i16; 3]> = Vec::new();
        let mut indices: Vec<u32> = Vec::with_capacity(triangles.len() * 3);
        for v in triangles.iter().flatten() {
            let q = v.map(|c| (c / unit).round().clamp(-32767.0, 32767.0) as i16);
            let next = vertices.len() as u32;
            let at = *index.entry(q).or_insert_with(|| {
                vertices.push(q);
                next
            });
            indices.push(at);
        }
        let wide = vertices.len() > usize::from(u16::MAX);
        header.extend((vertices.len() as u32).to_le_bytes());
        header.extend((indices.len() as u32).to_le_bytes());
        header.extend((if wide { 4u32 } else { 2 }).to_le_bytes());
        for v in &vertices {
            for c in v {
                body.extend(c.to_le_bytes());
            }
        }
        padded(&mut body);
        for &i in &indices {
            if wide {
                body.extend(i.to_le_bytes());
            } else {
                body.extend((i as u16).to_le_bytes());
            }
        }
        padded(&mut body);
    }
    header.extend(body);
    header
}

fn gzipped(bytes: &[u8]) -> std::io::Result<Vec<u8>> {
    use std::io::Write;
    let mut encoder = flate2::write::GzEncoder::new(Vec::with_capacity(bytes.len() / 2), flate2::Compression::default());
    encoder.write_all(bytes)?;
    encoder.finish()
}

/// Where a point of the ground is in the tile (x, y) of [`FILE_ZOOM`], as a file has it.
fn in_tile(x: f64, z: f64, tile: [u32; 2]) -> [u16; 2] {
    let [mx, my] = tile::mercator(x, z);
    let side = f64::from(1u32 << FILE_ZOOM);
    let part = |at: f64, tile: u32| ((at * side - f64::from(tile) + PAST) / (1.0 + 2.0 * PAST) * 65535.0).round().clamp(0.0, 65535.0) as u16;
    [part(mx, tile[0]), part(my, tile[1])]
}

/// A point of a model, its own space, turned by the quaternion `q` (w, x, y, z).
fn rotate(q: [f64; 4], v: [f64; 3]) -> [f64; 3] {
    let [w, x, y, z] = q;
    let t = [2.0 * (y * v[2] - z * v[1]), 2.0 * (z * v[0] - x * v[2]), 2.0 * (x * v[1] - y * v[0])];
    [v[0] + w * t[0] + y * t[2] - z * t[1], v[1] + w * t[1] + z * t[0] - x * t[2], v[2] + w * t[2] + x * t[1] - y * t[0]]
}

/// A bridge's triangles placed at `node`, scaled, as their road is drawn: each point as high over the map as it is
/// over the road nearest it ([`Decks::bridge_point`]), and a piece no wider than [`PIECE_REACH`] under a road over
/// it as a whole ([`Decks::over`]). Each with where it is (x, z) and how high it's drawn.
fn placed_bridge(triangles: &[[[f32; 3]; 3]], node: &Node, scale: [f32; 3], decks: &Decks) -> Vec<[[f32; 3]; 3]> {
    let p = node.position.map(f64::from);
    let (q, s) = (node.rotation.map(f64::from), scale.map(f64::from));
    let low = decks.low_at(node.position[0], node.position[2]).unwrap_or(node.position[1]);
    // Its pieces: triangles joined by a corner.
    let key = |v: &[f32; 3]| v.map(|c| (c * 100.0).round() as i32);
    let mut parent: Vec<usize> = (0..triangles.len()).collect();
    fn find(parent: &mut [usize], mut at: usize) -> usize {
        while parent[at] != at {
            parent[at] = parent[parent[at]];
            at = parent[at];
        }
        at
    }
    let mut first: HashMap<[i32; 3], usize> = HashMap::new();
    for (index, t) in triangles.iter().enumerate() {
        for v in t {
            let other = *first.entry(key(v)).or_insert(index);
            let (a, b) = (find(&mut parent, index), find(&mut parent, other));
            parent[a] = b;
        }
    }
    let world: Vec<[[f32; 3]; 3]> = triangles
        .iter()
        .map(|t| {
            t.map(|v| {
                let turned = rotate(q, [f64::from(v[0]) * s[0], f64::from(v[1]) * s[1], f64::from(v[2]) * s[2]]);
                let (x, z) = ((p[0] + turned[0]) as f32, (p[2] + turned[2]) as f32);
                [x, z, decks.bridge_point(x, z, (p[1] + turned[1]) as f32, Some(low))]
            })
        })
        .collect();
    let mut boxes: HashMap<usize, [f32; 4]> = HashMap::new();
    for (index, t) in world.iter().enumerate() {
        let root = find(&mut parent, index);
        let b = boxes.entry(root).or_insert([f32::MAX, f32::MAX, f32::MIN, f32::MIN]);
        for v in t {
            *b = [b[0].min(v[0]), b[1].min(v[1]), b[2].max(v[0]), b[3].max(v[1])];
        }
    }
    let tops: HashMap<usize, f32> = boxes
        .iter()
        .filter(|(_, b)| (b[2] - b[0]).hypot(b[3] - b[1]) < PIECE_REACH)
        .filter_map(|(&root, b)| decks.over(&[[b[0], b[1]], [b[2], b[1]], [b[2], b[3]], [b[0], b[3]]], node.position[1]).map(|top| (root, top)))
        .collect();
    world
        .iter()
        .enumerate()
        .filter_map(|(index, t)| {
            let root = find(&mut parent, index);
            let t = t.map(|v| [v[0], v[1], tops.get(&root).map_or(v[2], |top| v[2].min(*top))]);
            t.iter().any(|v| v[2] > 0.0).then_some(t)
        })
        .collect()
}

/// Writes the files of `placements`' models and placings under `dir` ([`FOLDER`]), for the models in `models` and
/// the places' prefabs in `prefab_models`, the bridges as `decks` draws their roads. Hands `outlines` what stays an
/// outline: what's lifted off its ground, as its tile feature (fitted to `decks` by the caller). `progress`: 0 to 1.
#[allow(clippy::too_many_arguments)]
pub fn write(
    placements: &[Placement],
    models: &HashMap<u64, BuildingModel>,
    prefab_models: &HashMap<u64, String>,
    files: &GameFiles,
    decks: &Decks,
    dir: &Path,
    outlines: &mut dyn FnMut(Building),
    progress: &mut dyn FnMut(f32),
) -> std::io::Result<Written> {
    let folder = dir.join(FOLDER);
    let model_dir = folder.join(MODELS);
    std::fs::create_dir_all(&model_dir)?;
    // Each model once, numbered in order (the same files for the same map).
    let mut keys: Vec<(Key, Option<[f32; 3]>)> = placements
        .iter()
        .map(|placed| match placed.prefab {
            Some(PrefabLook { variant, road }) => (Key::Prefab(placed.model, variant), Some(road)),
            None => (Key::Model(placed.model), None),
        })
        .collect();
    keys.sort_by_key(|(key, _)| *key);
    keys.dedup_by_key(|(key, _)| *key);
    let jobs: Vec<(u32, Key, Option<[f32; 3]>)> = keys.into_iter().enumerate().map(|(id, (key, road))| (id as u32, key, road)).collect();
    let failed: std::sync::Mutex<Option<std::io::Error>> = std::sync::Mutex::new(None);
    let made: HashMap<Key, Made> = buildings::on_the_threads(&jobs, |&(id, key, road)| {
        let (path, landmark, bridge, variant) = match key {
            Key::Model(model) => {
                let Some(building) = models.get(&model) else { return Vec::new() };
                (building.path.as_str(), building.kind == BuildingKind::Landmark, buildings::is_bridge(&building.path), 0)
            }
            Key::Prefab(model, variant) => {
                let Some(path) = prefab_models.get(&model) else { return Vec::new() };
                (path.as_str(), false, false, variant)
            }
        };
        let Some(pmg) = path.strip_suffix(".pmd").and_then(|stem| files.read(&format!("{stem}.pmg")).ok().flatten()) else {
            return Vec::new();
        };
        let Some(model_box) = buildings::model_box(&pmg) else { return Vec::new() };
        let seen = buildings::seen_pieces(files, path, &pmg, variant, &mut HashMap::new());
        let Some(triangles) = buildings::shown_triangles(&pmg, seen.as_deref()) else { return Vec::new() };
        let ground = match road {
            Some(road) => Ground::Own(road),
            None => Ground::Level,
        };
        if bridge {
            let shape = buildings::ModelShape { model_box, parts: Vec::new() };
            let near = merged(&triangles, if landmark { 0.0 } else { NEAR_CELL });
            return vec![(key, Made { id, shape, drawn: false, bridge: Some(near) })];
        }
        let shape = buildings::ModelShape { model_box, parts: buildings::trace(&model_box, &triangles, ground) };
        // A roof over lanes is left to its outline: a filling station's or a border crossing's, a place's.
        let roofs = !matches!(key, Key::Prefab(..)) && models.get(&key_model(key)).is_some_and(|building| building.kind != BuildingKind::Canopy);
        let kept = standing(&triangles, &shape.parts, &Levels::new(&model_box, &triangles, ground), roofs);
        let near = merged(&kept, if landmark { 0.0 } else { NEAR_CELL });
        let far = merged(&kept, FAR_CELL);
        let top = near.iter().flatten().fold(0.0f32, |most, v| most.max(v[1]));
        let drawn = !near.is_empty();
        if drawn {
            let written = gzipped(&model_file(&[near, far], top)).and_then(|bytes| std::fs::write(model_dir.join(format!("{id}.bin")), bytes));
            if let Err(error) = written {
                failed.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert(error);
            }
        }
        vec![(key, Made { id, shape, drawn, bridge: None })]
    });
    progress(0.5);
    if let Some(error) = failed.into_inner().unwrap_or_else(|e| e.into_inner()) {
        return Err(error);
    }
    let mut tiles: BTreeMap<[u32; 2], TileFile> = BTreeMap::new();
    let mut written = Written { models: made.values().filter(|made| made.drawn).count(), ..Written::default() };
    for placed in placements {
        let key = match placed.prefab {
            Some(PrefabLook { variant, .. }) => Key::Prefab(placed.model, variant),
            None => Key::Model(placed.model),
        };
        let Some(made) = made.get(&key) else { continue };
        let [x, _, z] = placed.node.position;
        if let Some(triangles) = &made.bridge {
            // Each triangle in the tile its first point is in.
            for t in placed_bridge(triangles, &placed.node, placed.scale, decks) {
                let tile = tile::tile_of(f64::from(t[0][0]), f64::from(t[0][1]), FILE_ZOOM);
                let (_, vertices) = tiles.entry(tile).or_default();
                for v in t {
                    let [u, w] = in_tile(f64::from(v[0]), f64::from(v[1]), tile);
                    vertices.push([u, w, (v[2] * 100.0).round().clamp(0.0, 65534.0) as u16]);
                }
                written.bridge_triangles += 1;
            }
            continue;
        }
        let kind = match key {
            Key::Prefab(..) => BuildingKind::Building,
            Key::Model(model) => models.get(&model).map_or(BuildingKind::Building, |building| building.kind),
        };
        // Places' prefabs: nothing where they have no parts (a filling station's price sign alone), never their box.
        if matches!(key, Key::Prefab(..)) && made.shape.parts.is_empty() {
            continue;
        }
        let placed_parts = buildings::place(&made.shape, &placed.node, placed.scale, kind, placed.uid);
        if placed_parts.is_empty() {
            continue;
        }
        let roofs_lifted = matches!(key, Key::Prefab(..)) || kind == BuildingKind::Canopy;
        let mut cap: Option<f32> = None;
        let mut standing_parts = 0;
        for part in placed_parts {
            if part.kind == BuildingKind::Canopy && roofs_lifted {
                outlines(part);
                continue;
            }
            standing_parts += 1;
            if let Some(top) = part.rings.first().and_then(|ring| decks.over(ring, part.ground)) {
                cap = Some(cap.map_or(top, |cap| cap.min(top)));
            }
        }
        if standing_parts == 0 || !made.drawn {
            continue;
        }
        let [w, _, y, _] = placed.node.rotation;
        let tile = tile::tile_of(f64::from(x), f64::from(z), FILE_ZOOM);
        tiles.entry(tile).or_default().0.push(Placing {
            model: made.id,
            at: in_tile(f64::from(x), f64::from(z), tile),
            yaw: 2.0 * y.atan2(w),
            scale: placed.scale,
            cap: cap.map_or(NO_CAP, |cap| (cap.max(0.0) * 100.0).round().min(f32::from(NO_CAP - 1)) as u16),
        });
        written.placings += 1;
    }
    progress(0.8);
    let mut list = Vec::new();
    for (tile, (mut placings, vertices)) in tiles {
        placings.sort_by_key(|placing| placing.model);
        let mut bytes = Vec::with_capacity(12 + placings.len() * PLACING_BYTES + vertices.len() * 6);
        bytes.extend(TILE_MAGIC.to_le_bytes());
        bytes.extend((placings.len() as u32).to_le_bytes());
        bytes.extend((vertices.len() as u32).to_le_bytes());
        for placing in &placings {
            bytes.extend(placing.model.to_le_bytes());
            bytes.extend(placing.at[0].to_le_bytes());
            bytes.extend(placing.at[1].to_le_bytes());
            bytes.extend(placing.yaw.to_le_bytes());
            for s in placing.scale {
                bytes.extend(s.to_le_bytes());
            }
            bytes.extend(placing.cap.to_le_bytes());
            bytes.extend(0u16.to_le_bytes());
        }
        for v in &vertices {
            for c in v {
                bytes.extend(c.to_le_bytes());
            }
        }
        padded(&mut bytes);
        let path = folder.join(tile[0].to_string());
        std::fs::create_dir_all(&path)?;
        std::fs::write(path.join(format!("{}.bin", tile[1])), gzipped(&bytes)?)?;
        list.push(tile);
    }
    std::fs::write(folder.join("index.json"), serde_json::to_vec(&list).map_err(std::io::Error::other)?)?;
    written.bytes = folder_bytes(&folder);
    progress(1.0);
    Ok(written)
}

fn folder_bytes(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() => folder_bytes(&entry.path()),
            _ => entry.metadata().map_or(0, |data| data.len()),
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x0: f32, z0: f32, x1: f32, z1: f32) -> Vec<Point> {
        vec![[x0, z0], [x1, z0], [x1, z1], [x0, z1]]
    }

    #[test]
    fn what_stands_on_its_ground_is_kept_and_what_lies_on_it_isnt() {
        let part = ShapePart { rings: vec![square(0.0, 0.0, 10.0, 10.0)], bottom: 0.0, top: 8.0 };
        let wall = [[1.0, 0.0, 1.0], [9.0, 0.0, 1.0], [9.0, 8.0, 1.0]];
        let floor = [[1.0, 0.0, 1.0], [9.0, 0.0, 1.0], [9.0, 0.0, 9.0]];
        let outside = [[20.0, 0.0, 20.0], [29.0, 0.0, 20.0], [29.0, 8.0, 20.0]];
        let below = [[1.0, -3.0, 2.0], [9.0, -3.0, 2.0], [9.0, 2.0, 2.0]];
        let model_box = buildings::ModelBox { min: [0.0, -3.0, 0.0], max: [30.0, 8.0, 30.0] };
        let levels = Levels::new(&model_box, &[], Ground::Level);
        let kept = standing(&[wall, floor, outside, below], &[part], &levels, false);
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0], wall);
        // Under the ground, it's at the ground.
        assert_eq!(kept[1][0][1], 0.0);
        assert_eq!(kept[1][2][1], 2.0);
    }

    #[test]
    fn a_roof_over_lanes_is_left_to_its_outline() {
        let roof = ShapePart { rings: vec![square(0.0, 0.0, 10.0, 10.0)], bottom: 4.5, top: 6.0 };
        let face = [[1.0, 5.0, 1.0], [9.0, 5.0, 1.0], [9.0, 6.0, 1.0]];
        let model_box = buildings::ModelBox { min: [0.0, 0.0, 0.0], max: [10.0, 6.0, 10.0] };
        let levels = Levels::new(&model_box, &[], Ground::Level);
        assert!(standing(&[face], std::slice::from_ref(&roof), &levels, false).is_empty());
        // A stadium's stands over its ground: in its shape.
        assert_eq!(standing(&[face], &[roof], &levels, true).len(), 1);
    }

    #[test]
    fn merging_keeps_what_stays_a_triangle_once() {
        let a = [[0.0, 0.0, 0.0], [4.0, 0.0, 0.0], [4.0, 4.0, 0.0]];
        let tiny = [[0.1, 0.1, 0.1], [0.2, 0.1, 0.1], [0.2, 0.2, 0.1]];
        let kept = merged(&[a, a, tiny], 1.0);
        assert_eq!(kept.len(), 1);
        assert_eq!(merged(&[a, tiny], 0.0).len(), 2);
    }

    #[test]
    fn a_model_file_reads_back_as_written() {
        let t = [[0.0, 0.0, 0.0], [4.0, 0.0, 0.0], [4.0, 4.0, 0.0]];
        let bytes = model_file(&[vec![t, t], vec![t]], 4.0);
        let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        assert_eq!(word(0), MODEL_MAGIC);
        assert_eq!(word(12), 2);
        // Near: 3 vertices, 6 indices, 2 bytes each; far: 3, 3, 2.
        assert_eq!([word(16), word(20), word(24), word(28), word(32), word(36)], [3, 6, 2, 3, 3, 2]);
        let unit = f32::from_le_bytes(bytes[4..8].try_into().unwrap());
        let x = i16::from_le_bytes(bytes[40 + 6..40 + 8].try_into().unwrap());
        assert!((f32::from(x) * unit - 4.0).abs() < 0.01);
    }
}
