//! Ground the game paints: terrain items, the terrain beside roads, a prefab's terrain at its edges, bezier patches.
//! Most of it is plain land; what's painted paved is quiet ground, and road where lanes run through it (map mods build
//! whole junctions of painted terrain with invisible lanes over them), unless something else is laid over it: the game
//! paints paving under lawns.

use std::collections::HashMap;

use crate::files::GameFiles;
use crate::geometry;
use crate::prefab::PrefabNode;
use crate::sector::{Node, Relief, TerrainSide};
use crate::sii;
use crate::terrain::{self, QuadGrid};
use crate::token::string_to_token;
use crate::world::{Definitions, TerrainProfile};

use super::faces::Materials;
use super::sort::is_paved;
use super::{frame, rotate};

/// What a terrain material is, as far as the map's concerned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Painted {
    /// Not generated at all.
    Unseen,
    Water,
    /// Asphalt, concrete, tiles: its substance says.
    Paved,
    /// Bare, or says nothing: a road where lanes run through it, otherwise plain ground.
    Bare,
    Grass,
}

/// Terrain materials by token.
#[derive(Default)]
pub struct Paints {
    kinds: HashMap<u64, Painted>,
}

impl Paints {
    pub fn load(files: &GameFiles, materials: &Materials) -> Self {
        let mut kinds = HashMap::new();
        for unit in sii::read_family_or_note(files, "/def/world", "terrain_material") {
            let (Some(token), Some(path)) =
                (unit.name.strip_prefix("terrain_mat.").and_then(string_to_token), unit.get("path"))
            else {
                continue;
            };
            let material = materials.get(files, path.trim_matches('"'));
            let kind = if material.unseen {
                Painted::Unseen
            } else if material.effect.starts_with("eut2.water") {
                Painted::Water
            } else if is_paved(&material.substance) {
                Painted::Paved
            } else if material.substance == "grass" {
                Painted::Grass
            } else {
                Painted::Bare
            };
            kinds.insert(token, kind);
        }
        Self { kinds }
    }

    fn of(&self, grid: &QuadGrid, row: u16, col: u16) -> Painted {
        grid.material_at(row, col).and_then(|material| self.kinds.get(&material)).copied().unwrap_or(Painted::Bare)
    }
}

/// Which paints are wanted of a piece of ground.
pub type Wanted = fn(Painted) -> bool;

/// What's drawn or claimed (not plain grass, not water, which [`crate::area`] draws).
pub fn drawn(paint: Painted) -> bool {
    matches!(paint, Painted::Paved | Painted::Bare)
}

/// What hides the paving under it.
pub fn laid_over(paint: Painted) -> bool {
    !matches!(paint, Painted::Paved | Painted::Unseen)
}

/// A quad of painted ground: its corners (x, y, z) round it.
#[derive(Clone, Copy, Debug)]
pub struct Quad {
    pub corners: [[f64; 3]; 4],
    pub paint: Painted,
}

/// The quads of a grid `wanted`, its vertices given by `vertex` (column, row).
fn quads_of(paints: &Paints, grid: &QuadGrid, wanted: Wanted, vertex: &dyn Fn(usize, usize) -> [f64; 3], out: &mut Vec<Quad>) {
    for col in 0..grid.cols {
        for row in 0..grid.rows {
            let paint = paints.of(grid, row, col);
            if wanted(paint) {
                let (c, r) = (col as usize, row as usize);
                out.push(Quad { corners: [vertex(c, r), vertex(c + 1, r), vertex(c + 1, r + 1), vertex(c, r + 1)], paint });
            }
        }
    }
}

/// Whether a grid has anything `wanted`.
fn anything(paints: &Paints, grid: &QuadGrid, wanted: Wanted) -> bool {
    (0..grid.cols).any(|col| (0..grid.rows).any(|row| wanted(paints.of(grid, row, col))))
}

/// The steps and heights of `relief`'s profile.
fn profile<'a>(definitions: &'a Definitions, relief: &Relief) -> (&'a [f32], &'a [f32]) {
    let profile: Option<&TerrainProfile> = definitions.terrain_profiles.get(&relief.profile);
    (profile.map_or(&[][..], |p| &p.steps), profile.map_or(&[][..], |p| &p.heights))
}

/// A terrain side's quads between two nodes: columns along their curve at equal steps, rows outward from `inner`
/// metres off it, on its right (`sign` 1) or left (-1).
#[allow(clippy::too_many_arguments)]
pub fn side_quads(
    definitions: &Definitions,
    paints: &Paints,
    a: &Node,
    b: &Node,
    length: f64,
    side: &TerrainSide,
    relief: &Relief,
    inner: f64,
    sign: f64,
    wanted: Wanted,
    out: &mut Vec<Quad>,
) {
    let grid = &side.grid;
    if grid.rows == 0 || grid.cols == 0 || !anything(paints, grid, wanted) {
        return;
    }
    let (steps, heights) = profile(definitions, relief);
    let edges = terrain::row_edges(steps, side.size, grid.rows);
    let cols = grid.cols as usize;
    let frames: Vec<([f64; 3], [f64; 2])> = (0..=cols).map(|i| frame(a, b, length, i as f64 / cols as f64, true)).collect();
    let rows = grid.rows as usize + 1;
    let mut moved: HashMap<(usize, usize), [f64; 3]> = HashMap::new();
    for &([col, row], offset) in &grid.offsets {
        let at = [0, 1, 2].map(|k| f64::from(a.position[k]) + f64::from(offset[k]));
        moved.insert((col as usize, row as usize), at);
    }
    let vertex = |c: usize, r: usize| -> [f64; 3] {
        if let Some(v) = moved.get(&(c, r)) {
            return *v;
        }
        let (p, n) = frames[c];
        let d = sign * (inner + f64::from(edges[r.min(rows - 1)]));
        let height = heights.get(r).or(heights.last()).copied().unwrap_or(0.0);
        let rise = f64::from(height) * f64::from(relief.coefficient) + f64::from(relief.height_offset);
        [p[0] + n[0] * d, p[1] + rise, p[2] + n[1] * d]
    };
    quads_of(paints, grid, wanted, &vertex, out);
}

/// A bezier patch's quads: its rows run along its x, its columns along its z.
pub fn patch_quads(paints: &Paints, node: &Node, control_points: &[[[f32; 3]; 4]; 4], grid: &QuadGrid, wanted: Wanted, out: &mut Vec<Quad>) {
    if grid.rows == 0 || grid.cols == 0 || !anything(paints, grid, wanted) {
        return;
    }
    let (q, p) = (node.rotation.map(f64::from), node.position.map(f64::from));
    let vertex = |c: usize, r: usize| -> [f64; 3] {
        let v = geometry::bezier_patch_point(control_points, r as f32 / grid.rows as f32, c as f32 / grid.cols as f32);
        let o = rotate(q, v.map(f64::from));
        [p[0] + o[0], p[1] + o[1], p[2] + o[2]]
    };
    quads_of(paints, grid, wanted, &vertex, out);
}

/// Where a point of a prefab's terrain edge lies on one of its nodes' cross-lines, if it does: which way along that
/// line leads out, in the prefab's own space.
fn out_along_node(nodes: &[PrefabNode], p: [f32; 3]) -> Option<[f32; 2]> {
    let mut best: Option<(f32, [f32; 2])> = None;
    for node in nodes {
        let (dx, dz) = (node.direction[0], node.direction[1]);
        let (ox, oz) = (p[0] - node.position[0], p[2] - node.position[1]);
        if (ox * dx + oz * dz).abs() < 0.2 {
            let across = [-dz, dx];
            let s = ox * across[0] + oz * across[1];
            if best.is_none_or(|b| s.abs() < b.0) {
                best = Some((s.abs(), [across[0] * s.signum(), across[1] * s.signum()]));
            }
        }
    }
    best.map(|b| b.1)
}

/// The directions a prefab's terrain leaves its edge in, in the prefab's own space, point by point: its ends along the
/// cross-lines of the nodes they're on, the rest blended between them along the edge (measured on the game's stored
/// boxes: 9 in 10 sides stay inside them; a mean of the two sides' normals, 7 in 10). An end on no node goes square.
fn edge_directions(points: &[[f32; 3]], nodes: &[PrefabNode]) -> Vec<[f32; 2]> {
    let square = |i: usize| -> [f32; 2] {
        let normal = |a: [f32; 3], b: [f32; 3]| {
            let (dx, dz) = (b[0] - a[0], b[2] - a[2]);
            let long = dx.hypot(dz).max(1e-6);
            [-dz / long, dx / long]
        };
        let before = normal(points[i.saturating_sub(1)], points[i.max(1).min(points.len() - 1)]);
        let after = normal(points[i.min(points.len() - 2)], points[(i + 1).min(points.len() - 1)]);
        let (x, z) = (before[0] + after[0], before[1] + after[1]);
        let long = x.hypot(z).max(1e-6);
        [x / long, z / long]
    };
    let last = points.len() - 1;
    let (first_way, last_way) = (out_along_node(nodes, points[0]), out_along_node(nodes, points[last]));
    let lengths: Vec<f32> = points
        .windows(2)
        .scan(0.0, |along, pair| {
            *along += (pair[1][0] - pair[0][0]).hypot(pair[1][2] - pair[0][2]);
            Some(*along)
        })
        .collect();
    let whole = lengths.last().copied().unwrap_or(1.0).max(1e-6);
    (0..points.len())
        .map(|i| match (first_way, last_way) {
            (Some(a), Some(b)) => {
                let s = if i == 0 { 0.0 } else { lengths[i - 1] / whole };
                let (x, z) = (a[0] * (1.0 - s) + b[0] * s, a[1] * (1.0 - s) + b[1] * s);
                let long = x.hypot(z).max(1e-6);
                [x / long, z / long]
            }
            (Some(a), None) if i == 0 => a,
            (None, Some(b)) if i == last => b,
            _ => square(i),
        })
        .collect()
}

/// A prefab's terrain at one edge, in its own space: columns between `points`, rows outward.
#[allow(clippy::too_many_arguments)]
pub fn edge_quads(
    definitions: &Definitions,
    paints: &Paints,
    side: &TerrainSide,
    relief: &Relief,
    points: &[[f32; 3]],
    nodes: &[PrefabNode],
    place: &dyn Fn([f64; 3]) -> [f64; 3],
    wanted: Wanted,
    out: &mut Vec<Quad>,
) {
    let grid = &side.grid;
    if !anything(paints, grid, wanted) {
        return;
    }
    let Some(vertex) = edge_vertices(definitions, side, relief, points, nodes, place) else { return };
    quads_of(paints, grid, wanted, &vertex, out);
}

/// Every quad of a prefab's terrain at one edge that's there at all: `each` is told its corners and whether it's water.
#[allow(clippy::too_many_arguments)]
pub fn edge_ground(
    definitions: &Definitions,
    paints: &Paints,
    side: &TerrainSide,
    relief: &Relief,
    points: &[[f32; 3]],
    nodes: &[PrefabNode],
    place: &dyn Fn([f64; 3]) -> [f64; 3],
    each: &mut dyn FnMut([[f64; 3]; 4], bool),
) {
    let grid = &side.grid;
    let Some(vertex) = edge_vertices(definitions, side, relief, points, nodes, place) else { return };
    for col in 0..grid.cols {
        for row in 0..grid.rows {
            let paint = paints.of(grid, row, col);
            if paint != Painted::Unseen {
                let (c, r) = (col as usize, row as usize);
                each([vertex(c, r), vertex(c + 1, r), vertex(c + 1, r + 1), vertex(c, r + 1)], paint == Painted::Water);
            }
        }
    }
}

/// Where the vertices of a prefab's terrain at one edge are, by column and row; `None` if its columns aren't between
/// `points`.
fn edge_vertices<'a>(
    definitions: &Definitions,
    side: &TerrainSide,
    relief: &'a Relief,
    points: &'a [[f32; 3]],
    nodes: &[PrefabNode],
    place: &'a dyn Fn([f64; 3]) -> [f64; 3],
) -> Option<impl Fn(usize, usize) -> [f64; 3] + 'a> {
    let grid = &side.grid;
    if grid.rows == 0 || grid.cols == 0 || points.len() != grid.cols as usize + 1 {
        return None;
    }
    let (steps, heights) = profile(definitions, relief);
    let heights = heights.to_vec();
    let edges = terrain::row_edges(steps, side.size, grid.rows);
    let ways = edge_directions(points, nodes);
    Some(move |c: usize, r: usize| -> [f64; 3] {
        let (p, way) = (points[c], ways[c]);
        let d = edges[r.min(edges.len() - 1)];
        let height = heights.get(r).or(heights.last()).copied().unwrap_or(0.0) * relief.coefficient;
        place([f64::from(p[0] + way[0] * d), f64::from(p[1] + height), f64::from(p[2] + way[1] * d)])
    })
}
