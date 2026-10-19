//! The open sea. The game builds ground only in corridors along its roads; between them is nothing, and at
//! sea the game shows its flat water surface there. Where the sea meets land the game paints the ground
//! with water, in strips reaching out from the coast. So the open sea is the empty ground beyond those
//! painted strips, and an empty gap inland between roads is not.
//!
//! On a grid over the area:
//! 1. land (terrain, roads, junctions, map areas) and painted water are drawn in;
//! 2. land is widened by [`SeaOptions::seal`], closing the gaps a corridor has where it isn't drawn
//!    (junctions' own ground), so inland gaps don't leak into the sea;
//! 3. the empty ground left falls into separate regions. A region reaching the map's edge is sea: in the
//!    game it is, the sea's surface reaching out to the horizon. (It takes in the nothing beyond the map's
//!    land borders too, which the game's data can't tell from open sea: a land edge facing nothing, out to
//!    the map's edge, is how a cliff coast looks as well.) A region enclosed by the map is sea when enough
//!    of its edge is painted water: a coast or a lakeshore is painted all along it, while an inland gap
//!    between roads only meets the ends of rivers;
//! 4. the sea grows back into the widened band, without crossing land.

use std::collections::{HashMap, VecDeque};

use crate::geometry::Point;

/// What a cell of the grid holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Cell {
    Empty,
    Land,
    /// Ground painted with water.
    Water,
    /// Open sea, filled in.
    Sea,
}

/// The ground of an area, as shapes on the ground plane (metres).
#[derive(Debug, Default)]
pub struct Ground {
    pub land: Vec<Vec<Point>>,
    pub water: Vec<Vec<Point>>,
}

#[derive(Clone, Copy, Debug)]
pub struct SeaOptions {
    /// Size of a grid cell, metres.
    pub cell: f32,
    /// How far land is widened to seal its gaps, metres.
    pub seal: f32,
    /// The share of an enclosed region's edge that must be painted water for it to be sea.
    pub water_edge: f32,
}

impl Default for SeaOptions {
    fn default() -> Self {
        Self { cell: 10.0, seal: 80.0, water_edge: 0.25 }
    }
}

/// A region of empty ground and how it was judged, for checking the classification.
#[derive(Clone, Copy, Debug)]
pub struct Region {
    pub cells: usize,
    /// Its edge's cells next to painted water, and next to (widened) land.
    pub water_edge: usize,
    pub land_edge: usize,
    /// It reaches the grid's edge.
    pub open: bool,
    pub sea: bool,
}

/// The grid the sea is found on. Kept compact: a whole game map is tens of millions of cells.
pub struct SeaGrid {
    /// World position of cell (0, 0)'s corner.
    pub origin: Point,
    pub cell: f32,
    pub width: usize,
    pub height: usize,
    pub cells: Vec<Cell>,
    pub regions: Vec<Region>,
}

/// What a cell's region came to, as it's judged (a byte a cell: a whole map is hundreds of millions): not
/// in a region yet, in one judged sea (or still being judged: taken as sea until it's done), or in one that
/// isn't.
const UNSEEN: u8 = 0;
const SEA_REGION: u8 = 1;
const LAND_REGION: u8 = 2;

impl SeaGrid {
    /// Everything the traced shapes can reach (min x, min z, max x, max z): the grid and half a cell
    /// beyond it, where [`land_shapes`] closes the land with the sea all round.
    pub fn traced_bounds(&self) -> [f32; 4] {
        let half = self.cell / 2.0;
        let [x, z] = self.origin;
        [x - half, z - half, x + self.width as f32 * self.cell + half, z + self.height as f32 * self.cell + half]
    }

    /// An empty grid over `bounds` (min x, min z, max x, max z), in cells of `cell` metres.
    pub fn new(bounds: [f32; 4], cell: f32) -> Self {
        let width = ((bounds[2] - bounds[0]) / cell).ceil() as usize;
        let height = ((bounds[3] - bounds[1]) / cell).ceil() as usize;
        Self {
            origin: [bounds[0], bounds[1]],
            cell,
            width,
            height,
            cells: vec![Cell::Empty; width * height],
            regions: Vec::new(),
        }
    }

    pub fn at(&self, x: usize, y: usize) -> Cell {
        self.cells[y * self.width + x]
    }

    /// Fills a polygon (even-odd) with `value`, at cell centres.
    pub fn paint(&mut self, ring: &[Point], value: Cell) {
        self.paint_rings(&[ring], value);
    }

    /// Fills a polygon of several rings (an outline and its holes, even-odd: the holes left as they are)
    /// with `value`, at cell centres.
    pub fn paint_rings<R: AsRef<[Point]>>(&mut self, rings: &[R], value: Cell) {
        let runs = self.runs(rings);
        self.fill(&runs, value);
    }

    /// The cells a polygon of several rings covers ([`Self::paint_rings`]), as runs of them along its rows
    /// (indices into [`Self::cells`]): found without changing the grid, so many at once, and filled after.
    pub fn runs<R: AsRef<[Point]>>(&self, rings: &[R]) -> Vec<std::ops::Range<usize>> {
        let mut runs = Vec::new();
        let to_grid = |p: &Point| [(p[0] - self.origin[0]) / self.cell, (p[1] - self.origin[1]) / self.cell];
        let rings: Vec<Vec<[f32; 2]>> = rings
            .iter()
            .map(AsRef::as_ref)
            .filter(|ring| ring.len() >= 3)
            .map(|ring| ring.iter().map(to_grid).collect())
            .collect();
        if rings.is_empty() {
            return runs;
        }
        let (min_y, max_y) =
            rings.iter().flatten().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p[1]), hi.max(p[1])));
        let first_row = (min_y - 0.5).ceil().max(0.0) as usize;
        let last_row = ((max_y - 0.5).floor() as isize).min(self.height as isize - 1);
        let mut crossings = Vec::new();
        for row in first_row as isize..=last_row {
            let y = row as f32 + 0.5;
            crossings.clear();
            for points in &rings {
                for i in 0..points.len() {
                    let (a, b) = (points[i], points[(i + 1) % points.len()]);
                    if (a[1] <= y) != (b[1] <= y) {
                        crossings.push(a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0]));
                    }
                }
            }
            crossings.sort_by(f32::total_cmp);
            for pair in crossings.chunks_exact(2) {
                let start = (pair[0] - 0.5).ceil().max(0.0) as usize;
                let end = ((pair[1] - 0.5).floor() as isize).min(self.width as isize - 1);
                if end >= start as isize {
                    let at = row as usize * self.width;
                    runs.push(at + start..at + end as usize + 1);
                }
            }
        }
        runs
    }

    /// Fills the runs of cells ([`Self::runs`]) with `value`.
    pub fn fill(&mut self, runs: &[std::ops::Range<usize>], value: Cell) {
        for run in runs {
            self.cells[run.clone()].fill(value);
        }
    }

    /// The four neighbours of a cell (`None` off the grid).
    fn neighbours(&self, index: usize) -> [Option<usize>; 4] {
        let (x, y) = (index % self.width, index / self.width);
        [
            (x > 0).then(|| index - 1),
            (x + 1 < self.width).then(|| index + 1),
            (y > 0).then(|| index - self.width),
            (y + 1 < self.height).then(|| index + self.width),
        ]
    }

    /// Finds the open sea among the empty cells, once land and painted water are painted in (steps 2-4
    /// above): marks it [`Cell::Sea`] and records the regions it judged.
    pub fn find_sea(&mut self, options: SeaOptions) {
        self.find_sea_telling(options, &mut |_| {});
    }

    /// [`Self::find_sea`], telling `progress` how far it is, 0 to 1 (its pass over every cell, the long one,
    /// as it goes).
    pub fn find_sea_telling(&mut self, options: SeaOptions, progress: &mut dyn FnMut(f32)) {
        let cells = self.cells.len();
        let every = (cells / 100).max(1);
        // 2. Steps from the nearest land, up to the seal distance (one byte each: the seal is a few cells).
        let seal = ((options.seal / self.cell).ceil() as u32).min(250) as u8;
        let mut distance = vec![u8::MAX; cells];
        let mut queue: VecDeque<u32> = VecDeque::new();
        for (index, cell) in self.cells.iter().enumerate() {
            if *cell == Cell::Land {
                distance[index] = 0;
                queue.push_back(index as u32);
            }
        }
        while let Some(index) = queue.pop_front() {
            let index = index as usize;
            if distance[index] >= seal {
                continue;
            }
            for next in self.neighbours(index).into_iter().flatten() {
                if self.cells[next] == Cell::Empty && distance[next] == u8::MAX {
                    distance[next] = distance[index] + 1;
                    queue.push_back(next as u32);
                }
            }
        }
        let free = |grid: &SeaGrid, index: usize| grid.cells[index] == Cell::Empty && distance[index] > seal;
        progress(0.25);

        // 3. Regions of empty ground beyond the seal, judged by their edges: each region's cells taken as sea as
        //    it's flooded, and flooded again as not sea if it isn't (inland gaps: small; the open sea, most of
        //    the grid, is flooded once).
        let mut region_of = vec![UNSEEN; cells];
        for start in 0..cells {
            if start % every == 0 {
                progress(0.25 + 0.6 * start as f32 / cells as f32);
            }
            if !free(self, start) || region_of[start] != UNSEEN {
                continue;
            }
            let mut region = Region { cells: 0, water_edge: 0, land_edge: 0, open: false, sea: false };
            region_of[start] = SEA_REGION;
            queue.push_back(start as u32);
            while let Some(index) = queue.pop_front() {
                region.cells += 1;
                let (x, y) = (index as usize % self.width, index as usize / self.width);
                if x == 0 || y == 0 || x + 1 == self.width || y + 1 == self.height {
                    region.open = true;
                }
                for next in self.neighbours(index as usize).into_iter().flatten() {
                    if free(self, next) {
                        if region_of[next] == UNSEEN {
                            region_of[next] = SEA_REGION;
                            queue.push_back(next as u32);
                        }
                    } else if self.cells[next] == Cell::Water {
                        region.water_edge += 1;
                    } else {
                        region.land_edge += 1;
                    }
                }
            }
            let edge = region.water_edge + region.land_edge;
            region.sea = region.open || (edge > 0 && region.water_edge as f32 >= options.water_edge * edge as f32);
            if !region.sea {
                region_of[start] = LAND_REGION;
                queue.push_back(start as u32);
                while let Some(index) = queue.pop_front() {
                    for next in self.neighbours(index as usize).into_iter().flatten() {
                        if region_of[next] == SEA_REGION && free(self, next) {
                            region_of[next] = LAND_REGION;
                            queue.push_back(next as u32);
                        }
                    }
                }
            }
            self.regions.push(region);
        }
        drop(distance);
        progress(0.85);

        // 4. Sea, grown back across the band it was sealed off by (no further: through a gap in the land,
        //    the band leads inland).
        let mut steps = vec![u8::MAX; cells];
        for index in 0..cells {
            if region_of[index] == SEA_REGION {
                self.cells[index] = Cell::Sea;
                steps[index] = 0;
            }
        }
        // Grown from the sea's cells next to empty ground, in their order: the others have nothing to grow
        // into (the same sea, without most of the open sea's cells queued at once: a whole map's are hundreds
        // of megabytes of queue).
        let grows =
            |index: usize| self.neighbours(index).into_iter().flatten().any(|next| self.cells[next] == Cell::Empty);
        let seeds = (0..cells).filter(|&index| self.cells[index] == Cell::Sea && steps[index] == 0 && grows(index));
        queue.extend(seeds.map(|index| index as u32));
        drop(region_of);
        while let Some(index) = queue.pop_front() {
            let index = index as usize;
            if steps[index] > seal {
                continue;
            }
            for next in self.neighbours(index).into_iter().flatten() {
                if self.cells[next] == Cell::Empty {
                    self.cells[next] = Cell::Sea;
                    steps[next] = steps[index] + 1;
                    queue.push_back(next as u32);
                }
            }
        }
        progress(1.0);
    }
}

/// Finds the open sea in `bounds` (min x, min z, max x, max z) around `ground`.
pub fn fill(ground: &Ground, bounds: [f32; 4], options: SeaOptions) -> SeaGrid {
    let mut grid = SeaGrid::new(bounds, options.cell);
    for ring in &ground.land {
        grid.paint(ring, Cell::Land);
    }
    for ring in &ground.water {
        grid.paint(ring, Cell::Water);
    }
    grid.find_sea(options);
    grid
}

/// A polygon: its outline and the holes in it (the sea inside the land: lakes, bays closed off).
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub outline: Vec<Point>,
    pub holes: Vec<Vec<Point>>,
}

/// The land (everything the sea doesn't fill) as smooth shapes: its cells, lightly blurred (so the coast
/// runs smoothly rather than in cell steps), traced at half strength, with points dropped where they add
/// less than `tolerance` metres. Beyond the grid is sea: the game's world lies in it, so the map draws the
/// sea as its ground, and land over it.
pub fn land_shapes(grid: &SeaGrid, tolerance: f32) -> Vec<Shape> {
    land_shapes_telling(grid, tolerance, &mut |_| {})
}

/// [`land_shapes`], telling `progress` how far it is, 0 to 1: the land's strength row by row, then the
/// outlines.
pub fn land_shapes_telling(grid: &SeaGrid, tolerance: f32, progress: &mut dyn FnMut(f32)) -> Vec<Shape> {
    let (w, h) = (grid.width + 2, grid.height + 2);
    let every = (grid.height / 100).max(1);
    // Land strength at each cell's centre (how many of the 3 × 3 around it are land, a byte each: a whole
    // map is tens of millions of cells), with a ring of sea around the grid so every outline closes.
    let mut field = vec![0u8; w * h];
    for y in 0..grid.height {
        if y % every == 0 {
            progress(0.4 * y as f32 / grid.height.max(1) as f32);
        }
        for x in 0..grid.width {
            let mut sum = 0;
            for (dx, dy) in [(-1, -1), (0, -1), (1, -1), (-1, 0), (0, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
                let (nx, ny) = (x as isize + dx, y as isize + dy);
                if nx >= 0 && ny >= 0 && (nx as usize) < grid.width && (ny as usize) < grid.height {
                    sum += (grid.at(nx as usize, ny as usize) != Cell::Sea) as u8;
                }
            }
            field[(y + 1) * w + x + 1] = sum;
        }
    }
    progress(0.4);
    let rings = trace(&field, w, h, 4.5);
    progress(0.9);
    let rings = rings
        .into_iter()
        .map(|ring| {
            let ring: Vec<Point> = ring
                .iter()
                .map(|p| [grid.origin[0] + (p[0] - 0.5) * grid.cell, grid.origin[1] + (p[1] - 0.5) * grid.cell])
                .collect();
            simplify_ring(&ring, tolerance)
        })
        .filter(|ring| ring.len() >= 3)
        .collect();
    let shapes = group_holes(rings);
    progress(1.0);
    shapes
}

/// Marching squares over `field` (`w` × `h` values, at whole coordinates) at `level`: closed rings, in
/// field coordinates. Each runs with the inside (above `level`) on the same side, so outlines and holes
/// wind opposite ways.
pub(crate) fn trace<T: Copy + Into<f32>>(field: &[T], w: usize, h: usize, level: f32) -> Vec<Vec<Point>> {
    // A crossing is identified by the edge it's on: (x, y, vertical).
    type Edge = (usize, usize, bool);
    let value = |x: usize, y: usize| field[y * w + x].into();
    let crossing = |(x, y, vertical): Edge| -> Point {
        let (x2, y2) = if vertical { (x, y + 1) } else { (x + 1, y) };
        let (a, b) = (value(x, y), value(x2, y2));
        let t = ((level - a) / (b - a)).clamp(0.0, 1.0);
        [x as f32 + t * (x2 as f32 - x as f32), y as f32 + t * (y2 as f32 - y as f32)]
    };
    let mut next: HashMap<Edge, Edge> = HashMap::new();
    for y in 0..h - 1 {
        for x in 0..w - 1 {
            // Corners clockwise from the top left, and the edges between them in the same order.
            let corners = [value(x, y), value(x + 1, y), value(x + 1, y + 1), value(x, y + 1)];
            let edges: [Edge; 4] = [(x, y, false), (x + 1, y, true), (x, y + 1, false), (x, y, true)];
            let inside = corners.map(|v| v > level);
            // Walking clockwise round the square: leaving the inside is an exit, entering it an entry.
            let mut exits = Vec::new();
            let mut entries = Vec::new();
            for i in 0..4 {
                match (inside[i], inside[(i + 1) % 4]) {
                    (true, false) => exits.push(i),
                    (false, true) => entries.push(i),
                    _ => {}
                }
            }
            match exits.len() {
                1 => {
                    next.insert(edges[exits[0]], edges[entries[0]]);
                }
                2 => {
                    // A saddle: the centre decides whether the inside corners join across it.
                    let centre = corners.iter().sum::<f32>() / 4.0 > level;
                    for &exit in &exits {
                        // Joined: each exit meets the entry after it (cutting off the outside corner
                        // between); apart: the entry before it (cutting off its inside corner).
                        let entry = if centre { (exit + 1) % 4 } else { (exit + 3) % 4 };
                        next.insert(edges[exit], edges[entry]);
                    }
                }
                _ => {}
            }
        }
    }
    let mut rings = Vec::new();
    // In a fixed order, so the same grid always traces the same rings (the export is reproducible).
    let mut starts: Vec<Edge> = next.keys().copied().collect();
    starts.sort_unstable();
    for start in starts {
        if !next.contains_key(&start) {
            continue; // already part of a ring
        }
        let mut ring = Vec::new();
        let mut edge = start;
        while let Some(following) = next.remove(&edge) {
            ring.push(crossing(edge));
            edge = following;
        }
        rings.push(ring);
    }
    rings
}

/// Drops points of a closed ring that lie within `tolerance` of the line through their neighbours
/// (Douglas-Peucker, run on the ring split at its farthest-apart points).
pub(crate) fn simplify_ring(ring: &[Point], tolerance: f32) -> Vec<Point> {
    if ring.len() < 4 {
        return ring.to_vec();
    }
    let far = (1..ring.len())
        .max_by(|&a, &b| distance2(ring[0], ring[a]).total_cmp(&distance2(ring[0], ring[b])))
        .unwrap_or(1);
    let mut first = simplify_line(&ring[..=far], tolerance);
    let mut second: Vec<Point> = ring[far..].to_vec();
    second.push(ring[0]);
    let second = simplify_line(&second, tolerance);
    first.pop();
    first.extend(&second[..second.len() - 1]);
    first
}

fn simplify_line(line: &[Point], tolerance: f32) -> Vec<Point> {
    let mut keep = vec![false; line.len()];
    keep[0] = true;
    keep[line.len() - 1] = true;
    let mut stack = vec![(0, line.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        let (mut worst, mut index) = (0.0, 0);
        for i in a + 1..b {
            let d = distance_to_segment(line[i], line[a], line[b]);
            if d > worst {
                (worst, index) = (d, i);
            }
        }
        if worst > tolerance {
            keep[index] = true;
            stack.push((a, index));
            stack.push((index, b));
        }
    }
    line.iter().zip(keep).filter(|(_, k)| *k).map(|(p, _)| *p).collect()
}

fn distance2(a: Point, b: Point) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)
}

fn distance_to_segment(p: Point, a: Point, b: Point) -> f32 {
    let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
    let length2 = dx * dx + dz * dz;
    let t = if length2 > 0.0 { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dz) / length2).clamp(0.0, 1.0) } else { 0.0 };
    distance2(p, [a[0] + t * dx, a[1] + t * dz]).sqrt()
}

/// Twice the ring's signed area: its sign tells which way it winds.
pub(crate) fn signed_area(ring: &[Point]) -> f32 {
    (0..ring.len())
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum()
}

fn contains(ring: &[Point], p: Point) -> bool {
    let mut inside = false;
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
            inside = !inside;
        }
    }
    inside
}

/// Puts each hole (the rings winding the other way) into the smallest outline around it.
pub(crate) fn group_holes(rings: Vec<Vec<Point>>) -> Vec<Shape> {
    let (outlines, holes): (Vec<_>, Vec<_>) = rings.into_iter().partition(|ring| signed_area(ring) > 0.0);
    let mut shapes: Vec<Shape> = outlines.into_iter().map(|outline| Shape { outline, holes: Vec::new() }).collect();
    for hole in holes {
        let owner = shapes
            .iter()
            .enumerate()
            .filter(|(_, shape)| contains(&shape.outline, hole[0]))
            .min_by(|(_, a), (_, b)| signed_area(&a.outline).abs().total_cmp(&signed_area(&b.outline).abs()))
            .map(|(index, _)| index);
        if let Some(index) = owner {
            shapes[index].holes.push(hole);
        }
    }
    shapes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x0: f32, z0: f32, x1: f32, z1: f32) -> Vec<Point> {
        vec![[x0, z0], [x1, z0], [x1, z1], [x0, z1]]
    }

    #[test]
    fn traces_the_land_as_outlines_with_holes() {
        // A land square 40 × 40 cells in the sea, with a lake 10 × 10 in the middle; and land at the grid's
        // edge, closed off by the sea beyond it.
        let mut grid = SeaGrid::new([0.0, 0.0, 600.0, 600.0], 10.0);
        for y in 0..60 {
            for x in 0..60 {
                let island = (10..50).contains(&x) && (10..50).contains(&y);
                let lake = (25..35).contains(&x) && (25..35).contains(&y);
                let edge = x < 4 && y < 4;
                grid.cells[y * grid.width + x] = if (island && !lake) || edge { Cell::Land } else { Cell::Sea };
            }
        }
        let mut shapes = land_shapes(&grid, 1.0);
        assert_eq!(shapes.len(), 2);
        shapes.sort_by_key(|shape| shape.holes.len());
        let (edge, island) = (&shapes[0], &shapes[1]);
        assert!(edge.holes.is_empty() && contains(&edge.outline, [15.0, 15.0]));
        assert_eq!(island.holes.len(), 1);
        // Corners rounded a little by the blur; straight sides simplified to few points.
        assert!(island.outline.len() <= 16, "{} points", island.outline.len());
        let xs: Vec<f32> = island.outline.iter().map(|p| p[0]).collect();
        let (min_x, max_x) = (xs.iter().copied().fold(f32::MAX, f32::min), xs.iter().copied().fold(f32::MIN, f32::max));
        assert!((min_x - 100.0).abs() < 6.0 && (max_x - 500.0).abs() < 6.0, "{min_x} {max_x}");
        assert!(contains(&island.holes[0], [300.0, 300.0]));
    }

    #[test]
    fn fills_polygons_at_cell_centres() {
        let mut grid = SeaGrid::new([0.0, 0.0, 100.0, 100.0], 10.0);
        grid.paint(&square(10.0, 10.0, 30.0, 20.0), Cell::Land);
        let land: Vec<(usize, usize)> =
            (0..10).flat_map(|y| (0..10).map(move |x| (x, y))).filter(|&(x, y)| grid.at(x, y) == Cell::Land).collect();
        assert_eq!(land, [(1, 1), (2, 1)]);
    }

    /// A coast along z = 500: land south of it, its painted water strip north of it, the open sea beyond;
    /// and inland, a gap closed in by roads, with a river ending at its edge.
    #[test]
    fn fills_the_sea_beyond_painted_water_but_not_gaps_inland() {
        let mut ground = Ground::default();
        ground.land.push(square(0.0, 500.0, 2000.0, 600.0)); // coast road corridor
        ground.water.push(square(0.0, 300.0, 2000.0, 500.0)); // painted coastal water
        // An inland gap (600..1400 x 600..1400) walled by corridors, with a river ending in it.
        ground.land.push(square(400.0, 600.0, 600.0, 2000.0));
        ground.land.push(square(1400.0, 600.0, 1600.0, 2000.0));
        ground.land.push(square(400.0, 1400.0, 1600.0, 1600.0));
        ground.water.push(square(900.0, 1350.0, 960.0, 1400.0));
        let grid = fill(&ground, [0.0, 0.0, 2000.0, 2000.0], SeaOptions::default());
        let at = |x: f32, z: f32| grid.at((x / 10.0) as usize, (z / 10.0) as usize);
        assert_eq!(at(1000.0, 100.0), Cell::Sea, "open sea");
        assert_eq!(at(1000.0, 295.0), Cell::Sea, "grown back up to the painted water");
        assert_eq!(at(1000.0, 1000.0), Cell::Empty, "inland gap");
        assert_eq!(at(1000.0, 550.0), Cell::Land);
    }
}
