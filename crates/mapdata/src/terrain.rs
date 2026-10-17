//! Terrain: the ground the game builds beside roads and junctions, and on its own (terrain items and
//! bezier patches). It's a grid of quads, each painted with one of a few materials, and water (sea,
//! lakes, rivers) is ground painted with a water material, so this is where the map finds water.

use std::collections::HashMap;

use crate::geometry::Frame;
use crate::reader::{OutOfBounds, Reader};
use crate::sector::{Relief, TerrainSide};
use crate::world::TerrainProfile;

/// One terrain grid: its materials and, for every quad, which of them it's painted with.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct QuadGrid {
    /// Material tokens (`terrain_mat.*` in `def/world/terrain_material*.sii`); the first is the main one.
    pub materials: Vec<u64>,
    /// Quads across (away from the road or terrain's centre line).
    pub rows: u16,
    /// Quads along.
    pub cols: u16,
    /// Per quad, an index into `materials`, column by column (see [`QuadGrid::material_at`]).
    pub quads: Vec<u8>,
    /// Hand-sculpted vertex offsets: (x, y) of the vertex in the grid and its offset (x, y, z), metres.
    pub offsets: Vec<([u16; 2], [f32; 3])>,
}

impl QuadGrid {
    /// The material of the quad at `row` (across, from the centre line out) and `col` (along).
    ///
    /// Quads are stored column by column: measured over the whole base map, water quads' neighbours are
    /// water far more often in that order (94% of neighbours agree) than row by row (85%).
    pub fn material_at(&self, row: u16, col: u16) -> Option<u64> {
        let index = col as usize * self.rows as usize + row as usize;
        let material = *self.quads.get(index)?;
        self.materials.get(material as usize).copied()
    }

    /// The runs of quads `wanted` picks (given row and column), per column: `(col, first row, last row)`.
    pub fn runs(&self, wanted: impl Fn(u16, u16) -> bool) -> Vec<(u16, u16, u16)> {
        let mut runs = Vec::new();
        for col in 0..self.cols {
            let mut start = None;
            for row in 0..=self.rows {
                let hit = row < self.rows && wanted(row, col);
                match (hit, start) {
                    (true, None) => start = Some(row),
                    (false, Some(first)) => {
                        runs.push((col, first, row - 1));
                        start = None;
                    }
                    _ => {}
                }
            }
        }
        runs
    }
}

/// Reads a quad grid. Normals (lighting of sculpted vertices) are stepped over.
pub(crate) fn read_quad_grid(r: &mut Reader) -> Result<QuadGrid, OutOfBounds> {
    let material_count = r.u16()?;
    let mut materials = Vec::with_capacity(material_count.into());
    for _ in 0..material_count {
        materials.push(r.u64()?);
        r.skip(2)?; // rotation
    }
    let colors = r.u16()?;
    r.skip_records(colors.into(), 4)?;
    let rows = r.u16()?;
    let cols = r.u16()?;
    let quad_count = r.count()?;
    let bytes = r.bytes((quad_count as usize).saturating_mul(4))?;
    // The main material is the low nibble of each quad's first byte.
    let quads = bytes.chunks_exact(4).map(|quad| quad[0] & 0x0f).collect();
    let offset_count = r.count()?;
    let mut offsets = Vec::with_capacity((offset_count as usize).min(r.remaining() / 0x10));
    for _ in 0..offset_count {
        let vertex = [r.u16()?, r.u16()?];
        offsets.push((vertex, [r.f32()?, r.f32()?, r.f32()?]));
    }
    let normals = r.count()?;
    r.skip_records(normals, 0x10)?;
    Ok(QuadGrid { materials, rows, cols, quads, offsets })
}

/// Row widths of a side without a terrain profile, metres, from the centre line out (the game's default
/// rows; the last repeats).
const DEFAULT_STEPS: [f32; 15] = [1.0, 1.0, 1.0, 2.0, 6.0, 6.0, 10.0, 10.0, 10.0, 20.0, 40.0, 50.0, 50.0, 50.0, 100.0];

/// How far from the centre line each of a side's `rows + 1` row edges is: the profile's steps (the last
/// repeating), the outermost cut at the side's `size`.
pub fn row_edges(steps: &[f32], size: f32, rows: u16) -> Vec<f32> {
    let steps = if steps.is_empty() { &DEFAULT_STEPS[..] } else { steps };
    let mut edges = Vec::with_capacity(rows as usize + 1);
    let mut distance = 0.0;
    edges.push(0.0);
    for row in 0..rows as usize {
        distance = (distance + steps[row.min(steps.len() - 1)]).min(size.max(distance));
        edges.push(distance);
    }
    edges
}

/// The ground of one terrain side: where its vertices are and how high, as the game builds it.
pub struct SideGround {
    pub rows: u16,
    pub cols: u16,
    /// Column by column, `rows + 1` per column (from the centre line out): x, height, z.
    pub vertices: Vec<[f32; 3]>,
}

impl SideGround {
    pub fn vertex(&self, col: u16, row: u16) -> [f32; 3] {
        self.vertices[col as usize * (self.rows as usize + 1) + row as usize]
    }
}

/// Builds a terrain side's ground. `frames` are its curve's `cols + 1` frames; the side starts `inner`
/// metres from the curve, on its right (`sign` 1) or left (-1). Each vertex sits at its row edge, as high
/// as the curve plus the profile's height there (times the coefficient) plus the side's height offset,
/// unless it was sculpted: then the grid stores its position, relative to `origin` (the strip's start
/// node), in world axes.
pub fn side_ground(
    frames: &[Frame],
    inner: f32,
    sign: f32,
    side: &TerrainSide,
    relief: &Relief,
    profile: Option<&TerrainProfile>,
    origin: [f32; 3],
) -> SideGround {
    let grid = &side.grid;
    let steps = profile.map_or(&[][..], |p| &p.steps);
    let heights = profile.map_or(&[][..], |p| &p.heights);
    let edges = row_edges(steps, side.size, grid.rows);
    let profile_height = |row: usize| heights.get(row).or(heights.last()).copied().unwrap_or(0.0) * relief.coefficient;
    let mut vertices = Vec::with_capacity((grid.cols as usize + 1) * (grid.rows as usize + 1));
    for frame in frames.iter().take(grid.cols as usize + 1) {
        for (row, &edge) in edges.iter().enumerate() {
            let d = sign * (inner + edge);
            vertices.push([
                frame.point[0] + frame.right[0] * d,
                frame.height + profile_height(row) + relief.height_offset,
                frame.point[1] + frame.right[1] * d,
            ]);
        }
    }
    let rows = grid.rows as usize + 1;
    for &([col, row], position) in &grid.offsets {
        if let Some(vertex) = vertices.get_mut(col as usize * rows + row as usize)
            && (row as usize) < rows
        {
            *vertex = [origin[0] + position[0], origin[1] + position[1], origin[2] + position[2]];
        }
    }
    SideGround { rows: grid.rows, cols: grid.cols, vertices }
}

/// A grid corner: (column, row), from 0 to `cols` and `rows`.
pub type Corner = (u16, u16);

/// The outlines of the regions of a grid's quads that are `inside`, as polygons of grid corners: each an
/// outline and its holes. Quads that touch only at a corner are separate regions. Outlines go round the
/// way the quads' own corners do ((c, r), (c + 1, r), (c + 1, r + 1), (c, r + 1)), holes the other way.
pub fn region_outlines(rows: u16, cols: u16, inside: impl Fn(u16, u16) -> bool) -> Vec<Vec<Vec<Corner>>> {
    let at = |col: i32, row: i32| {
        col >= 0 && row >= 0 && col < i32::from(cols) && row < i32::from(rows) && inside(row as u16, col as u16)
    };
    // Every quad's sides that face out of the region, going round as its corners do: the region on
    // their left (+ cross product, with x the column and y the row).
    let mut edges: Vec<(Corner, Corner)> = Vec::new();
    for col in 0..cols {
        for row in 0..rows {
            if !inside(row, col) {
                continue;
            }
            let (c, r) = (i32::from(col), i32::from(row));
            if !at(c, r - 1) {
                edges.push(((col, row), (col + 1, row)));
            }
            if !at(c + 1, r) {
                edges.push(((col + 1, row), (col + 1, row + 1)));
            }
            if !at(c, r + 1) {
                edges.push(((col + 1, row + 1), (col, row + 1)));
            }
            if !at(c - 1, r) {
                edges.push(((col, row + 1), (col, row)));
            }
        }
    }
    let mut starting: HashMap<Corner, Vec<usize>> = HashMap::new();
    for (index, &(from, _)) in edges.iter().enumerate() {
        starting.entry(from).or_default().push(index);
    }
    let direction = |(a, b): (Corner, Corner)| (i32::from(b.0) - i32::from(a.0), i32::from(b.1) - i32::from(a.1));
    // At a corner where several sides go on, the one turning furthest towards the region first (so quads
    // touching only at a corner aren't joined), then straight on, then away.
    let turn = |from: (i32, i32), to: (i32, i32)| match from.0 * to.1 - from.1 * to.0 {
        cross if cross > 0 => 0,
        0 => 1,
        _ => 2,
    };
    let mut used = vec![false; edges.len()];
    let mut rings: Vec<Vec<Corner>> = Vec::new();
    for first in 0..edges.len() {
        if used[first] {
            continue;
        }
        used[first] = true;
        let mut ring = vec![edges[first].0];
        let mut current = first;
        loop {
            let at_corner = edges[current].1;
            let heading = direction(edges[current]);
            let next = starting[&at_corner]
                .iter()
                .copied()
                .filter(|&edge| !used[edge] || edge == first)
                .min_by_key(|&edge| turn(heading, direction(edges[edge])));
            match next {
                Some(edge) if edge == first => break,
                Some(edge) => {
                    ring.push(at_corner);
                    used[edge] = true;
                    current = edge;
                }
                None => break,
            }
        }
        rings.push(ring);
    }

    // Outlines (positive area, going round as the quads do) and holes, each hole in the smallest outline
    // around it.
    let area = |ring: &[Corner]| -> i64 {
        (0..ring.len())
            .map(|i| {
                let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
                i64::from(a.0) * i64::from(b.1) - i64::from(b.0) * i64::from(a.1)
            })
            .sum()
    };
    let (outlines, holes): (Vec<_>, Vec<_>) = rings.into_iter().partition(|ring| area(ring) > 0);
    let mut polygons: Vec<Vec<Vec<Corner>>> = outlines.into_iter().map(|outline| vec![outline]).collect();
    for hole in holes {
        // A point inside the hole: the middle of the quad just right of its first side.
        let (a, b) = (hole[0], hole[1 % hole.len()]);
        let (dx, dy) = (f32::from(b.0) - f32::from(a.0), f32::from(b.1) - f32::from(a.1));
        let point =
            ((f32::from(a.0) + f32::from(b.0)) / 2.0 + dy * 0.5, (f32::from(a.1) + f32::from(b.1)) / 2.0 - dx * 0.5);
        let around =
            polygons.iter_mut().filter(|polygon| contains(&polygon[0], point)).min_by_key(|polygon| area(&polygon[0]));
        if let Some(polygon) = around {
            polygon.push(hole);
        }
    }
    polygons
}

/// Whether `point` is inside a ring of grid corners (even-odd).
fn contains(ring: &[Corner], (x, y): (f32, f32)) -> bool {
    let mut inside = false;
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
        let (ax, ay, bx, by) = (f32::from(a.0), f32::from(a.1), f32::from(b.0), f32::from(b.1));
        if (ay > y) != (by > y) && x < ax + (y - ay) / (by - ay) * (bx - ax) {
            inside = !inside;
        }
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outlines_regions_of_quads_with_their_holes() {
        // A 3x3 ring of water round a dry middle: one outline of 8 corners... and a hole.
        let ring = region_outlines(3, 3, |row, col| !(row == 1 && col == 1));
        assert_eq!(ring.len(), 1);
        assert_eq!(ring[0].len(), 2, "an outline and its hole");
        assert_eq!(ring[0][0].len(), 12, "the outline's corners: 3 a side, round 4 sides");
        assert_eq!(ring[0][1].len(), 4);
        // A band two quads wide along four columns: one outline, its long sides through every column's corner.
        let band = region_outlines(5, 4, |row, _| (1..3).contains(&row));
        assert_eq!(band.len(), 1);
        assert_eq!(
            band[0],
            [vec![(0, 1), (1, 1), (2, 1), (3, 1), (4, 1), (4, 2), (4, 3), (3, 3), (2, 3), (1, 3), (0, 3), (0, 2)]]
        );
        // Quads touching only at a corner: two regions.
        assert_eq!(region_outlines(2, 2, |row, col| row == col).len(), 2);
        assert!(region_outlines(2, 2, |_, _| false).is_empty());
    }

    #[test]
    fn rows_follow_the_profile_and_stop_at_the_side_edge() {
        assert_eq!(row_edges(&[], 10.0, 5), [0.0, 1.0, 2.0, 3.0, 5.0, 10.0]);
        // 1+1+1+2+6+6+10+10+10+20+40+50+50+50+100 = 357, then 100 m rows.
        assert_eq!(row_edges(&[], 500.0, 17).last(), Some(&500.0));
        assert_eq!(row_edges(&[40.0], 100.0, 3), [0.0, 40.0, 80.0, 100.0]);
    }

    #[test]
    fn ground_follows_the_profile_unless_sculpted() {
        let frames: Vec<Frame> =
            (0..=1).map(|i| Frame { point: [0.0, -10.0 * i as f32], right: [1.0, 0.0], height: 5.0 }).collect();
        let side = TerrainSide {
            size: 3.0,
            grid: QuadGrid {
                rows: 2,
                cols: 1,
                quads: vec![0, 0],
                offsets: vec![([1, 2], [3.0, -20.0, -10.0])],
                ..Default::default()
            },
        };
        let profile = TerrainProfile { steps: vec![1.0, 2.0], heights: vec![0.0, 1.0, 4.0] };
        let relief = Relief { profile: 1, coefficient: -2.0, height_offset: 0.5 };
        let ground = side_ground(&frames, 0.0, 1.0, &side, &relief, Some(&profile), [0.0, 5.0, 0.0]);
        assert_eq!(ground.vertex(0, 0), [0.0, 5.5, 0.0]);
        assert_eq!(ground.vertex(0, 1), [1.0, 5.0 - 2.0 + 0.5, 0.0]);
        assert_eq!(ground.vertex(0, 2), [3.0, 5.0 - 8.0 + 0.5, 0.0]);
        // Sculpted: the stored position, from the start node.
        assert_eq!(ground.vertex(1, 2), [3.0, -15.0, -10.0]);
        // On the left the rows run the other way.
        let left = side_ground(&frames, 2.0, -1.0, &side, &relief, Some(&profile), [0.0, 5.0, 0.0]);
        assert_eq!(left.vertex(0, 1)[0], -3.0);
    }

    #[test]
    fn reads_materials_and_each_quads_main_material() {
        let mut data = Vec::new();
        data.extend(2u16.to_le_bytes());
        for token in [9u64, 42] {
            data.extend(token.to_le_bytes());
            data.extend(0u16.to_le_bytes());
        }
        data.extend(1u16.to_le_bytes());
        data.extend([255, 255, 255, 0]);
        data.extend(2u16.to_le_bytes()); // rows
        data.extend(1u16.to_le_bytes()); // cols
        data.extend(2u32.to_le_bytes());
        data.extend([0x10, 0, 0, 0, 0x01, 0, 0, 0]);
        data.extend(0u32.to_le_bytes());
        data.extend(0u32.to_le_bytes());
        let grid = read_quad_grid(&mut Reader::new(&data)).unwrap();
        assert_eq!((grid.rows, grid.cols, grid.quads.as_slice()), (2, 1, &[0, 1][..]));
        assert_eq!((grid.material_at(0, 0), grid.material_at(1, 0)), (Some(9), Some(42)));
        assert_eq!(grid.material_at(2, 0), None);
    }

    #[test]
    fn finds_runs_of_a_material_in_each_column() {
        // Two columns of three rows, stored column by column: water (0) at rows 1-2, then at row 0.
        let grid =
            QuadGrid { materials: vec![7, 3], rows: 3, cols: 2, quads: vec![1, 0, 0, 0, 1, 1], ..Default::default() };
        assert_eq!(grid.runs(|row, col| grid.material_at(row, col) == Some(7)), [(0, 1, 2), (1, 0, 0)]);
        assert_eq!(grid.runs(|row, col| grid.material_at(row, col) == Some(3)), [(0, 0, 0), (1, 1, 2)]);
    }
}
