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
use crate::world_picture::WorldPicture;

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

#[derive(Clone, Debug)]
pub struct SeaOptions {
    /// Size of a grid cell, metres.
    pub cell: f32,
    /// How far land is widened to seal its gaps, metres.
    pub seal: f32,
    /// The share of an enclosed region's edge that must be painted water for it to be sea.
    pub water_edge: f32,
    /// Each empty cell is what's nearest it, painted water (sea) or land (land), instead of steps 2-4: for a map
    /// that's land inland out to its edges (ATS's, 2026-10-28: its deserts between the roads, and the nothing
    /// beyond its borders, Canada, Mexico and the states not installed, all one region reaching the edge with
    /// the Pacific, all sea by the regions' rule). Its coasts and lakeshores are painted, its gaps inland aren't.
    pub nearest: bool,
    /// With [`Self::nearest`], the game's own picture of its world (world_picture.rs), which the sea is held to:
    /// empty ground is sea only joined to the picture's open sea, so a painted river or pond inland makes no sea
    /// round it, nor does the nothing south of its states (2026-10-28: sea blocks south of New Mexico, the Rio
    /// Grande's). `None`: nearest alone.
    pub world: Option<std::sync::Arc<WorldPicture>>,
    /// With [`Self::world`]: how far from the picture's sea painted water's empty ground may be and be sea, and how
    /// far out from its land its open sea starts, metres ([`WORLD_REACH`]).
    pub world_reach: f32,
}

/// How far from the game's picture's sea painted water's empty ground may be and be sea, and how far out from its
/// land its open sea starts, metres: the picture's coasts are good to about a kilometre (world_picture.rs), its
/// sea and the game's painted coasts meeting within this.
pub const WORLD_REACH: f32 = 2000.0;

impl Default for SeaOptions {
    fn default() -> Self {
        Self { cell: 10.0, seal: 80.0, water_edge: 0.25, nearest: false, world: None, world_reach: WORLD_REACH }
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

    /// Whether what's at (x, z) on the ground is water: painted, or open sea.
    pub fn water_at(&self, x: f32, z: f32) -> bool {
        let (col, row) = (((x - self.origin[0]) / self.cell).floor(), ((z - self.origin[1]) / self.cell).floor());
        let inside = col >= 0.0 && row >= 0.0 && (col as usize) < self.width && (row as usize) < self.height;
        inside && matches!(self.at(col as usize, row as usize), Cell::Water | Cell::Sea)
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
        if options.nearest {
            return self.find_nearest_sea(options.world.as_deref().map(|world| (world, options.world_reach)), progress);
        }
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

impl SeaGrid {
    /// [`SeaOptions::nearest`]: each empty cell takes after the land or the painted water nearest it (in steps
    /// across the grid; of two as near, the one first in the grid), the sea being the empty ground nearest painted
    /// water. Empty ground with neither anywhere is sea. With the game's picture of its world (`world`), the sea is
    /// only what joins the picture's open sea: empty ground that's open sea in the picture (sea `reach` all
    /// round), and empty ground nearest painted water with the picture's sea within that reach, reached from it
    /// through such ground and painted water; empty ground with neither near is the picture's.
    fn find_nearest_sea(&mut self, world: Option<(&WorldPicture, f32)>, progress: &mut dyn FnMut(f32)) {
        const NONE: u8 = 0;
        const LAND: u8 = 1;
        const WATER: u8 = 2;
        let cells = self.cells.len();
        let mut nearest = vec![NONE; cells];
        let mut queue: VecDeque<u32> = VecDeque::new();
        for (index, cell) in self.cells.iter().enumerate() {
            let kind = match cell {
                Cell::Land => LAND,
                Cell::Water => WATER,
                _ => continue,
            };
            nearest[index] = kind;
            queue.push_back(index as u32);
        }
        progress(0.2);
        let every = (cells / 100).max(1);
        let mut reached = 0usize;
        while let Some(index) = queue.pop_front() {
            let index = index as usize;
            reached += 1;
            if reached.is_multiple_of(every) {
                progress(0.2 + 0.7 * reached as f32 / cells as f32);
            }
            for next in self.neighbours(index).into_iter().flatten() {
                if self.cells[next] == Cell::Empty && nearest[next] == NONE {
                    nearest[next] = nearest[index];
                    queue.push_back(next as u32);
                }
            }
        }
        let Some((world, reach)) = world else {
            for (cell, kind) in self.cells.iter_mut().zip(&nearest) {
                if *cell == Cell::Empty && *kind != LAND {
                    *cell = Cell::Sea;
                }
            }
            progress(1.0);
            return;
        };
        // What may be sea, and where the open sea starts.
        let mut may = vec![false; cells];
        let mut seeds: Vec<u32> = Vec::new();
        for (index, cell) in self.cells.iter().enumerate() {
            if *cell != Cell::Empty {
                continue;
            }
            let [x, z] = self.centre(index);
            let open = world.deep_sea(x, z, reach);
            may[index] = open
                || match nearest[index] {
                    WATER => world.sea_within(x, z, reach),
                    NONE => world.sea_at(x, z),
                    _ => false,
                };
            if open {
                seeds.push(index as u32);
            }
        }
        progress(0.95);
        // The open sea, and what joins it through ground that may be sea and painted water.
        let mut reached = vec![false; cells];
        for &seed in &seeds {
            reached[seed as usize] = true;
        }
        let mut stack = seeds;
        while let Some(index) = stack.pop() {
            for next in self.neighbours(index as usize).into_iter().flatten() {
                if !reached[next] && (may[next] || self.cells[next] == Cell::Water) {
                    reached[next] = true;
                    stack.push(next as u32);
                }
            }
        }
        for (cell, reached) in self.cells.iter_mut().zip(&reached) {
            if *cell == Cell::Empty && *reached {
                *cell = Cell::Sea;
            }
        }
        progress(1.0);
    }

    /// A cell's centre on the ground plane.
    fn centre(&self, index: usize) -> Point {
        let (x, y) = (index % self.width, index / self.width);
        [self.origin[0] + (x as f32 + 0.5) * self.cell, self.origin[1] + (y as f32 + 0.5) * self.cell]
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
    trace_land(grid, tolerance, false, progress)
}

/// [`land_shapes_telling`] for a map whose world's land goes on past its area (world_picture.rs: the cab draws
/// it there): the land at the grid's edges carried on a cell beyond it, so it meets the land drawn beyond without
/// the strip of sea a closing outline leaves (seen in the cab 2026-10-28, a faint line round the area).
pub fn land_shapes_carried_on(grid: &SeaGrid, tolerance: f32, progress: &mut dyn FnMut(f32)) -> Vec<Shape> {
    trace_land(grid, tolerance, true, progress)
}

fn trace_land(grid: &SeaGrid, tolerance: f32, carried_on: bool, progress: &mut dyn FnMut(f32)) -> Vec<Shape> {
    // The field's ring of cells round the grid: one of sea, or (carried on) one of the edge's own and one of sea.
    let pad = if carried_on { 2 } else { 1 };
    let (w, h) = (grid.width + 2 * pad, grid.height + 2 * pad);
    let every = (grid.height / 100).max(1);
    // Whether a cell is land: in the grid, by its own; carried on, a cell just beyond by the edge's nearest.
    let land = |nx: isize, ny: isize| {
        let (width, height) = (grid.width as isize, grid.height as isize);
        let reach = if carried_on { 1 } else { 0 };
        if nx < -reach || ny < -reach || nx >= width + reach || ny >= height + reach {
            return false;
        }
        grid.at(nx.clamp(0, width - 1) as usize, ny.clamp(0, height - 1) as usize) != Cell::Sea
    };
    // Land strength at each cell's centre (how many of the 3 × 3 around it are land, a byte each: a whole
    // map is tens of millions of cells), with a ring of sea around it all so every outline closes.
    let mut field = vec![0u8; w * h];
    let reach = pad as isize - 1;
    for y in -reach..grid.height as isize + reach {
        if y % every as isize == 0 {
            progress(0.4 * y.max(0) as f32 / grid.height.max(1) as f32);
        }
        for x in -reach..grid.width as isize + reach {
            let mut sum = 0;
            for (dx, dy) in [(-1, -1), (0, -1), (1, -1), (-1, 0), (0, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
                sum += land(x + dx, y + dy) as u8;
            }
            field[(y + pad as isize) as usize * w + (x + pad as isize) as usize] = sum;
        }
    }
    progress(0.4);
    let rings = trace(&field, w, h, 4.5);
    progress(0.9);
    let offset = pad as f32 - 0.5;
    let rings = rings
        .into_iter()
        .map(|ring| {
            let ring: Vec<Point> = ring
                .iter()
                .map(|p| [grid.origin[0] + (p[0] - offset) * grid.cell, grid.origin[1] + (p[1] - offset) * grid.cell])
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

pub(crate) fn contains(ring: &[Point], p: Point) -> bool {
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

    /// ATS's kind of map: the ocean to the west beyond a painted coast, and inland an empty desert between road
    /// corridors reaching the map's east edge (the nothing beyond its borders).
    #[test]
    fn by_the_nearest_the_ocean_is_sea_and_a_desert_reaching_the_edge_is_land() {
        let mut ground = Ground::default();
        ground.water.push(square(400.0, 0.0, 500.0, 2000.0)); // painted coastal water
        ground.land.push(square(500.0, 0.0, 600.0, 2000.0)); // coast road corridor
        ground.land.push(square(600.0, 900.0, 2000.0, 1000.0)); // a road east, the desert both sides of it
        ground.water.push(square(1800.0, 300.0, 1850.0, 350.0)); // a pond out in the desert
        let options = SeaOptions { nearest: true, ..SeaOptions::default() };
        let grid = fill(&ground, [0.0, 0.0, 2000.0, 2000.0], options);
        let at = |x: f32, z: f32| grid.at((x / 10.0) as usize, (z / 10.0) as usize);
        assert_eq!(at(100.0, 1000.0), Cell::Sea, "the ocean, out to the map's edge");
        assert_eq!(at(1200.0, 500.0), Cell::Empty, "the desert, inland");
        assert_eq!(at(1995.0, 1500.0), Cell::Empty, "the desert at the map's edge");
        assert_eq!(at(1830.0, 330.0), Cell::Water);
        // The regions' rule (ETS2's): all of it reaches the edge, all sea.
        let regions = fill(&ground, [0.0, 0.0, 2000.0, 2000.0], SeaOptions::default());
        assert_eq!(regions.at(120, 50), Cell::Sea);
    }

    /// The same map held to the game's picture of its world (sea west of x 300, land east): the ocean is still sea,
    /// the pond's ground and a painted river's out in the desert are not, nor is the land the picture has where
    /// the nearest rule alone made sea.
    #[test]
    fn held_to_the_games_picture_a_river_inland_makes_no_sea() {
        use crate::world_picture::{Placement, WorldPicture};
        let mut ground = Ground::default();
        ground.water.push(square(400.0, 0.0, 500.0, 2000.0)); // painted coastal water
        ground.land.push(square(500.0, 0.0, 600.0, 2000.0)); // coast road corridor
        ground.land.push(square(600.0, 900.0, 2000.0, 1000.0)); // a road east
        ground.water.push(square(1500.0, 1600.0, 2000.0, 1640.0)); // a river, out to the east edge
        // 200 pixels of 10 m: sea to x 300, land beyond (the picture's coast a little off the painted one).
        let grey: Vec<u8> = (0..200 * 200).map(|i| if i % 200 < 30 { 155 } else { 53 }).collect();
        let world = WorldPicture::from_grey(&grey, 200, Placement { origin: [0.0, 0.0], scale: 10.0 });
        let world = Some(std::sync::Arc::new(world));
        let held = SeaOptions { nearest: true, world, world_reach: 150.0, ..SeaOptions::default() };
        let grid = fill(&ground, [0.0, 0.0, 2000.0, 2000.0], held);
        let at = |x: f32, z: f32| grid.at((x / 10.0) as usize, (z / 10.0) as usize);
        assert_eq!(at(100.0, 1000.0), Cell::Sea, "the ocean");
        assert_eq!(at(350.0, 1000.0), Cell::Sea, "out to the painted coast, the picture's coast near it");
        assert_eq!(at(1800.0, 1700.0), Cell::Empty, "the river's ground out in the desert");
        let alone = fill(&ground, [0.0, 0.0, 2000.0, 2000.0], SeaOptions { nearest: true, ..SeaOptions::default() });
        let alone_at = |x: f32, z: f32| alone.at((x / 10.0) as usize, (z / 10.0) as usize);
        assert_eq!(alone_at(1800.0, 1700.0), Cell::Sea, "where the nearest rule alone made sea");
    }

    #[test]
    fn land_at_the_edges_carried_on_a_cell_or_closed_inside() {
        let mut grid = SeaGrid::new([0.0, 0.0, 100.0, 100.0], 10.0);
        grid.paint(&square(0.0, 0.0, 100.0, 100.0), Cell::Land);
        let extent = |shapes: Vec<Shape>| {
            shapes[0].outline.iter().fold([f32::MAX, f32::MIN], |e, p| [e[0].min(p[0]), e[1].max(p[0])])
        };
        let [closed_min, closed_max] = extent(land_shapes(&grid, 0.5));
        assert!(closed_min > 0.0 && closed_max < 100.0, "closed inside the grid: {closed_min}..{closed_max}");
        let [on_min, on_max] = extent(land_shapes_carried_on(&grid, 0.5, &mut |_| {}));
        // Past the area's own sea (its tiles' `sea`, half a cell beyond the grid), so no strip of it shows.
        assert!(on_min < -5.0 && on_max > 105.0, "carried on past half a cell: {on_min}..{on_max}");
    }
}
