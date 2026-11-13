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
//!
//! The grid holds its cells in blocks, only where something is ([`SeaGrid`]): a map whose parts lie far apart
//! (1,561 x 1,389 km in a player's log) is billions of cells, nearly all of them the open sea between.

use std::collections::{HashMap, VecDeque};

use crate::geometry::Point;
use crate::world_picture::WorldPicture;

#[cfg(test)]
mod dense;

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

/// The ground of an area, as shapes on the ground plane (metres): what the tests paint ([`fill`]).
#[cfg(test)]
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

/// A block of the grid is this many cells a side ([`SeaGrid`]).
const BLOCK: usize = 64;
const BLOCK_SHIFT: u32 = 6;
const BLOCK_CELLS: usize = BLOCK * BLOCK;
/// A block that isn't made: all its cells one value ([`SeaGrid::whole`]).
const NOT_MADE: u32 = u32::MAX;
/// The most blocks made: while the sea's found, a made cell is known by its place among the made blocks' cells in 31
/// bits (the 32nd tells a block not made), 2 billion cells, many times any real map's (the game's map with ProMods
/// spans 188 million).
const MOST_MADE: usize = 1 << (31 - 2 * BLOCK_SHIFT);

/// The grid the sea is found on, in blocks of [`BLOCK`] × [`BLOCK`] cells: a block holds its cells once something's
/// painted in it or the sea's finding reaches into it ("made"); every other block is one value, whole. A whole map's
/// box is hundreds of millions of cells, and with a map far from the rest billions, nearly all of them the open sea
/// between: a player's map 1,561 × 1,389 km across, 5.4 billion cells of 20 m (their log, 2026-11),
/// was gigabytes of cells and its indices past what 32 bits hold (the review of 2026-11-11, M7). The sea found is
/// the same as with every cell held (`dense`, kept to check it against).
pub struct SeaGrid {
    /// World position of cell (0, 0)'s corner.
    pub origin: Point,
    pub cell: f32,
    pub width: usize,
    pub height: usize,
    /// Blocks across and down.
    blocks_w: usize,
    blocks_h: usize,
    /// Each block's place among the made ones, or [`NOT_MADE`].
    slots: Vec<u32>,
    /// The made blocks' cells, [`BLOCK_CELLS`] each in the order made, row by row within a block (those past the
    /// grid's edge unused).
    made: Vec<Cell>,
    /// Which block each made one is, in the order made.
    made_blocks: Vec<u32>,
    /// Every cell of each block that isn't made.
    whole: Vec<Cell>,
    pub regions: Vec<Region>,
}

/// What a cell's region came to, as it's judged (a byte a cell: a whole map is hundreds of millions): not
/// in a region yet, in one judged sea (or still being judged: taken as sea until it's done), or in one that
/// isn't.
const UNSEEN: u8 = 0;
const SEA_REGION: u8 = 1;
const LAND_REGION: u8 = 2;

/// A block not made, as the regions' flood knows it (a made cell is known by its place in [`SeaGrid::made`]).
const BLOCK_NODE: u32 = 1 << 31;

/// What lies next to a piece of empty ground beyond the seal, as the regions' flood sees it.
enum Next {
    /// More of it: a made cell, or a block not made ([`BLOCK_NODE`]).
    Free(u32),
    Water,
    /// Land, or empty ground within the seal.
    Other,
}

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
        Self::sized([bounds[0], bounds[1]], cell, width, height)
    }

    /// An empty grid of `width` × `height` cells of `cell` metres from `origin` (its first cell's corner): one whose
    /// cells are something else's, as many (a picture's pixels: a size worked out from its bounds can come out a cell
    /// more, the review of 2026-11-11, minor 25).
    pub fn sized(origin: Point, cell: f32, width: usize, height: usize) -> Self {
        let (blocks_w, blocks_h) = (width.div_ceil(BLOCK), height.div_ceil(BLOCK));
        Self {
            origin,
            cell,
            width,
            height,
            blocks_w,
            blocks_h,
            slots: vec![NOT_MADE; blocks_w * blocks_h],
            made: Vec::new(),
            made_blocks: Vec::new(),
            whole: vec![Cell::Empty; blocks_w * blocks_h],
            regions: Vec::new(),
        }
    }

    /// The block cell (x, y) is in.
    fn block(&self, x: usize, y: usize) -> usize {
        (y >> BLOCK_SHIFT) * self.blocks_w + (x >> BLOCK_SHIFT)
    }

    /// Cell (x, y)'s place within its block.
    fn local(x: usize, y: usize) -> usize {
        ((y & (BLOCK - 1)) << BLOCK_SHIFT) | (x & (BLOCK - 1))
    }

    /// A block's first cell, x and y.
    fn corner(&self, block: usize) -> (usize, usize) {
        ((block % self.blocks_w) << BLOCK_SHIFT, (block / self.blocks_w) << BLOCK_SHIFT)
    }

    /// How many of a block's cells are on the grid, across and down.
    fn block_size(&self, block: usize) -> (usize, usize) {
        let (x, y) = self.corner(block);
        ((self.width - x).min(BLOCK), (self.height - y).min(BLOCK))
    }

    /// Cell (x, y)'s place in [`Self::made`], its block made if it isn't (each cell its whole value).
    fn made_at(&mut self, x: usize, y: usize) -> usize {
        let block = self.block(x, y);
        self.make(block) * BLOCK_CELLS + Self::local(x, y)
    }

    /// Makes `block` hold its cells (each its whole value), if it doesn't; its place among the made ones.
    fn make(&mut self, block: usize) -> usize {
        if self.slots[block] == NOT_MADE {
            assert!(
                self.made_blocks.len() < MOST_MADE,
                "the map's too big to find its sea in: more than {} cells of ground",
                MOST_MADE * BLOCK_CELLS
            );
            self.slots[block] = self.made_blocks.len() as u32;
            self.made_blocks.push(block as u32);
            self.made.extend(std::iter::repeat_n(self.whole[block], BLOCK_CELLS));
        }
        self.slots[block] as usize
    }

    /// A made cell's x and y, by its place in [`Self::made`].
    fn made_xy(&self, at: usize) -> (usize, usize) {
        let (x, y) = self.corner(self.made_blocks[at / BLOCK_CELLS] as usize);
        let local = at % BLOCK_CELLS;
        (x + (local & (BLOCK - 1)), y + (local >> BLOCK_SHIFT))
    }

    /// Whether a made cell is on the grid (a block at its right or bottom edge has cells past it, never used).
    fn on_grid(&self, at: usize) -> bool {
        let (x, y) = self.made_xy(at);
        x < self.width && y < self.height
    }

    /// Cell (x, y)'s place in [`Self::made`], if its block's made.
    fn made_place(&self, x: usize, y: usize) -> Option<usize> {
        let slot = self.slots[self.block(x, y)];
        (slot != NOT_MADE).then(|| slot as usize * BLOCK_CELLS + Self::local(x, y))
    }

    pub fn at(&self, x: usize, y: usize) -> Cell {
        let block = self.block(x, y);
        match self.slots[block] {
            NOT_MADE => self.whole[block],
            slot => self.made[slot as usize * BLOCK_CELLS + Self::local(x, y)],
        }
    }

    /// Sets cell (x, y) to `value`.
    pub fn set(&mut self, x: usize, y: usize, value: Cell) {
        let block = self.block(x, y);
        if self.slots[block] == NOT_MADE && self.whole[block] == value {
            return;
        }
        let at = self.made_at(x, y);
        self.made[at] = value;
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
    /// (each `y × width + x`, from its first cell to past its last): found without changing the grid, so many at
    /// once, and filled after.
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

    /// Fills the runs of cells ([`Self::runs`]) with `value`: the blocks they cross made, but where the block's
    /// whole value is already `value`.
    pub fn fill(&mut self, runs: &[std::ops::Range<usize>], value: Cell) {
        for run in runs {
            let y = run.start / self.width;
            let (mut x, end) = (run.start % self.width, run.end - y * self.width);
            while x < end {
                let block = self.block(x, y);
                let stop = (((x >> BLOCK_SHIFT) + 1) << BLOCK_SHIFT).min(end);
                if self.slots[block] != NOT_MADE || self.whole[block] != value {
                    let at = self.made_at(x, y);
                    self.made[at..at + (stop - x)].fill(value);
                }
                x = stop;
            }
        }
    }

    /// The four neighbours of cell (x, y) (`None` off the grid).
    fn neighbours(&self, x: usize, y: usize) -> [Option<(usize, usize)>; 4] {
        [
            (x > 0).then(|| (x - 1, y)),
            (x + 1 < self.width).then(|| (x + 1, y)),
            (y > 0).then(|| (x, y - 1)),
            (y + 1 < self.height).then(|| (x, y + 1)),
        ]
    }

    /// Finds the open sea among the empty cells, once land and painted water are painted in (steps 2-4
    /// above): marks it [`Cell::Sea`] and records the regions it judged (the tests': the map tells its progress).
    #[cfg(test)]
    pub fn find_sea(&mut self, options: SeaOptions) {
        self.find_sea_telling(options, &mut |_| {});
    }

    /// [`Self::find_sea`], telling `progress` how far it is, 0 to 1 (its passes over the made cells, as it goes).
    ///
    /// A block not made is empty ground (nothing's painted in it) beyond the seal: one the seal reaches into is made
    /// as it does. So it's one piece of a region, all of it: blocks not made next to each other are in the same one,
    /// and nothing but empty ground beyond the seal lies next to them (else they'd be made), but painted water or land
    /// and its band in a made block beside, counted as their edge cell by cell, as before. The sea's growing back
    /// makes a block it grows into.
    pub fn find_sea_telling(&mut self, options: SeaOptions, progress: &mut dyn FnMut(f32)) {
        if options.nearest {
            return self.find_nearest_sea(options.world.as_deref().map(|world| (world, options.world_reach)), progress);
        }
        // 2. Steps from the nearest land, up to the seal distance (one byte each: the seal is a few cells).
        let seal = ((options.seal / self.cell).ceil() as u32).min(250) as u8;
        let mut distance = vec![u8::MAX; self.made.len()];
        let mut queue: VecDeque<u32> = VecDeque::new();
        for slot in 0..self.made_blocks.len() {
            let (w, h) = self.block_size(self.made_blocks[slot] as usize);
            for local in (0..h).flat_map(|y| (0..w).map(move |x| (y << BLOCK_SHIFT) | x)) {
                let at = slot * BLOCK_CELLS + local;
                if self.made[at] == Cell::Land {
                    distance[at] = 0;
                    queue.push_back(at as u32);
                }
            }
        }
        while let Some(at) = queue.pop_front() {
            let at = at as usize;
            if distance[at] >= seal {
                continue;
            }
            let (x, y) = self.made_xy(at);
            for (nx, ny) in self.neighbours(x, y).into_iter().flatten() {
                // Empty ground the seal reaches into, in a block not made: made, to hold its distance.
                let next = self.made_at(nx, ny);
                distance.resize(self.made.len(), u8::MAX);
                if self.made[next] == Cell::Empty && distance[next] == u8::MAX {
                    distance[next] = distance[at] + 1;
                    queue.push_back(next as u32);
                }
            }
        }
        progress(0.25);

        // 3. Regions of empty ground beyond the seal, judged by their edges: each region's cells taken as sea as
        //    it's flooded, and flooded again as not sea if it isn't (inland gaps: small; the open sea, most of
        //    the grid, is flooded once). Their order: the made blocks', then the blocks not made (the same regions,
        //    the same verdicts, whatever order they're found in).
        let free = |grid: &SeaGrid, at: usize| grid.made[at] == Cell::Empty && distance[at] > seal;
        let mut region_of = vec![UNSEEN; self.made.len()];
        let mut block_region = vec![UNSEEN; self.slots.len()];
        let starts = (0..self.made.len() as u32)
            .filter(|&at| self.on_grid(at as usize))
            .chain((0..self.slots.len()).filter(|&block| self.slots[block] == NOT_MADE).map(|b| BLOCK_NODE | b as u32));
        let every = (self.made.len() / 100).max(1);
        let label = |region_of: &mut Vec<u8>, block_region: &mut Vec<u8>, node: u32, value: u8| {
            if node & BLOCK_NODE != 0 {
                block_region[(node & !BLOCK_NODE) as usize] = value;
            } else {
                region_of[node as usize] = value;
            }
        };
        let label_of = |region_of: &Vec<u8>, block_region: &Vec<u8>, node: u32| {
            if node & BLOCK_NODE != 0 { block_region[(node & !BLOCK_NODE) as usize] } else { region_of[node as usize] }
        };
        let mut nexts: Vec<Next> = Vec::new();
        let mut regions: Vec<Region> = Vec::new();
        for (index, start) in starts.enumerate() {
            if index % every == 0 {
                progress(0.25 + 0.6 * (index as f32 / self.made.len().max(1) as f32).min(1.0));
            }
            if start & BLOCK_NODE == 0 && !free(self, start as usize) {
                continue;
            }
            if label_of(&region_of, &block_region, start) != UNSEEN {
                continue;
            }
            let mut region = Region { cells: 0, water_edge: 0, land_edge: 0, open: false, sea: false };
            label(&mut region_of, &mut block_region, start, SEA_REGION);
            queue.push_back(start);
            while let Some(node) = queue.pop_front() {
                let (cells, open) = self.extent_of(node);
                region.cells += cells;
                region.open |= open;
                self.nexts(node, &|at| free(self, at), &mut nexts);
                for next in nexts.drain(..) {
                    match next {
                        Next::Free(next) => {
                            if label_of(&region_of, &block_region, next) == UNSEEN {
                                label(&mut region_of, &mut block_region, next, SEA_REGION);
                                queue.push_back(next);
                            }
                        }
                        Next::Water => region.water_edge += 1,
                        Next::Other => region.land_edge += 1,
                    }
                }
            }
            let edge = region.water_edge + region.land_edge;
            region.sea = region.open || (edge > 0 && region.water_edge as f32 >= options.water_edge * edge as f32);
            if !region.sea {
                label(&mut region_of, &mut block_region, start, LAND_REGION);
                queue.push_back(start);
                while let Some(node) = queue.pop_front() {
                    self.nexts(node, &|at| free(self, at), &mut nexts);
                    for next in nexts.drain(..) {
                        if let Next::Free(next) = next
                            && label_of(&region_of, &block_region, next) == SEA_REGION
                        {
                            label(&mut region_of, &mut block_region, next, LAND_REGION);
                            queue.push_back(next);
                        }
                    }
                }
            }
            regions.push(region);
        }
        self.regions.extend(regions);
        drop(distance);
        progress(0.85);

        // 4. Sea, grown back across the band it was sealed off by (no further: through a gap in the land,
        //    the band leads inland).
        let mut steps = vec![u8::MAX; self.made.len()];
        for at in 0..self.made.len() {
            if region_of[at] == SEA_REGION {
                self.made[at] = Cell::Sea;
                steps[at] = 0;
            }
        }
        for ((whole, slot), region) in self.whole.iter_mut().zip(&self.slots).zip(&block_region) {
            if *slot == NOT_MADE && *region == SEA_REGION {
                *whole = Cell::Sea;
            }
        }
        drop(region_of);
        drop(block_region);
        // Grown from the sea's cells next to empty ground: the made ones, then the empty ground next to a block not
        // made that's sea, a step out from it (the same sea as from every cell in order: each cell's steps are its
        // fewest from the sea, however it's reached).
        let grows = |grid: &SeaGrid, at: usize| {
            let (x, y) = grid.made_xy(at);
            grid.neighbours(x, y).into_iter().flatten().any(|(nx, ny)| grid.at(nx, ny) == Cell::Empty)
        };
        let seeds: Vec<u32> = (0..self.made.len())
            .filter(|&at| self.on_grid(at) && self.made[at] == Cell::Sea && steps[at] == 0 && grows(self, at))
            .map(|at| at as u32)
            .collect();
        queue.extend(seeds);
        let mut beside: Vec<u32> = Vec::new();
        for at in 0..self.made.len() {
            if self.on_grid(at) && self.made[at] == Cell::Empty {
                let (x, y) = self.made_xy(at);
                let by_sea = self.neighbours(x, y).into_iter().flatten().any(|(nx, ny)| {
                    let block = self.block(nx, ny);
                    self.slots[block] == NOT_MADE && self.whole[block] == Cell::Sea
                });
                if by_sea {
                    beside.push(at as u32);
                }
            }
        }
        for at in beside {
            self.made[at as usize] = Cell::Sea;
            steps[at as usize] = 1;
            queue.push_back(at);
        }
        while let Some(at) = queue.pop_front() {
            let at = at as usize;
            if steps[at] > seal {
                continue;
            }
            let (x, y) = self.made_xy(at);
            for (nx, ny) in self.neighbours(x, y).into_iter().flatten() {
                if self.at(nx, ny) == Cell::Empty {
                    let next = self.made_at(nx, ny);
                    steps.resize(self.made.len(), u8::MAX);
                    self.made[next] = Cell::Sea;
                    steps[next] = steps[at] + 1;
                    queue.push_back(next as u32);
                }
            }
        }
        progress(1.0);
    }

    /// How many cells a node of the regions' flood is (a made cell, a block not made), and whether it reaches the
    /// grid's edge.
    fn extent_of(&self, node: u32) -> (usize, bool) {
        if node & BLOCK_NODE != 0 {
            let block = (node & !BLOCK_NODE) as usize;
            let ((x, y), (w, h)) = (self.corner(block), self.block_size(block));
            (w * h, x == 0 || y == 0 || x + w == self.width || y + h == self.height)
        } else {
            let (x, y) = self.made_xy(node as usize);
            (1, x == 0 || y == 0 || x + 1 == self.width || y + 1 == self.height)
        }
    }

    /// What lies next to a node of the regions' flood, into `out`: for a made cell, each neighbour; for a block not
    /// made, each block not made beside it, and each made cell along its sides, once for each of its own cells
    /// beside it. `free`: whether a made cell is empty ground beyond the seal.
    fn nexts(&self, node: u32, free: &dyn Fn(usize) -> bool, out: &mut Vec<Next>) {
        let what = |at: usize| {
            if free(at) {
                Next::Free(at as u32)
            } else if self.made[at] == Cell::Water {
                Next::Water
            } else {
                Next::Other
            }
        };
        if node & BLOCK_NODE == 0 {
            let (x, y) = self.made_xy(node as usize);
            for (nx, ny) in self.neighbours(x, y).into_iter().flatten() {
                out.push(match self.made_place(nx, ny) {
                    Some(at) => what(at),
                    None => Next::Free(BLOCK_NODE | self.block(nx, ny) as u32),
                });
            }
            return;
        }
        let block = (node & !BLOCK_NODE) as usize;
        let ((x0, y0), (w, h)) = (self.corner(block), self.block_size(block));
        let (bx, by) = (block % self.blocks_w, block / self.blocks_w);
        // Each side: the block beside it, if there's one, and the cells along it (those beside its own).
        let sides = [
            (bx > 0).then(|| (block - 1, (0..h).map(|i| (x0 - 1, y0 + i)).collect::<Vec<_>>())),
            (bx + 1 < self.blocks_w).then(|| (block + 1, (0..h).map(|i| (x0 + w, y0 + i)).collect())),
            (by > 0).then(|| (block - self.blocks_w, (0..w).map(|i| (x0 + i, y0 - 1)).collect())),
            (by + 1 < self.blocks_h).then(|| (block + self.blocks_w, (0..w).map(|i| (x0 + i, y0 + h)).collect())),
        ];
        for (beside, cells) in sides.into_iter().flatten() {
            if self.slots[beside] == NOT_MADE {
                out.push(Next::Free(BLOCK_NODE | beside as u32));
            } else {
                for (cx, cy) in cells {
                    if let Some(at) = self.made_place(cx, cy) {
                        out.push(what(at));
                    }
                }
            }
        }
    }
}

impl SeaGrid {
    /// [`SeaOptions::nearest`]: each empty cell takes after the land or the painted water nearest it (in steps
    /// across the grid; of two as near, the one first in the grid), the sea being the empty ground nearest painted
    /// water. Empty ground with neither anywhere is sea. With the game's picture of its world (`world`), the sea is
    /// only what joins the picture's open sea: empty ground that's open sea in the picture (sea `reach` all
    /// round), and empty ground nearest painted water with the picture's sea within that reach, reached from it
    /// through such ground and painted water; empty ground with neither near is the picture's.
    ///
    /// Every cell takes after something here, so every block is made, and the cells are gone through in the grid's
    /// order, row by row: the same nearest as with every cell held.
    fn find_nearest_sea(&mut self, world: Option<(&WorldPicture, f32)>, progress: &mut dyn FnMut(f32)) {
        const NONE: u8 = 0;
        const LAND: u8 = 1;
        const WATER: u8 = 2;
        for block in 0..self.slots.len() {
            self.make(block);
        }
        let (width, height, blocks_w) = (self.width, self.height, self.blocks_w);
        let order = move || (0..height).flat_map(move |y| (0..width).map(move |x| (x, y)));
        // (The blocks' places copied: a byte for every thousand cells.)
        let slots = self.slots.clone();
        let place = move |x: usize, y: usize| {
            slots[(y >> BLOCK_SHIFT) * blocks_w + (x >> BLOCK_SHIFT)] as usize * BLOCK_CELLS + Self::local(x, y)
        };
        let cells = width * height;
        let mut nearest = vec![NONE; self.made.len()];
        let mut queue: VecDeque<u32> = VecDeque::new();
        for (x, y) in order() {
            let at = place(x, y);
            let kind = match self.made[at] {
                Cell::Land => LAND,
                Cell::Water => WATER,
                _ => continue,
            };
            nearest[at] = kind;
            queue.push_back(at as u32);
        }
        progress(0.2);
        let every = (cells / 100).max(1);
        let mut reached = 0usize;
        while let Some(at) = queue.pop_front() {
            let at = at as usize;
            reached += 1;
            if reached.is_multiple_of(every) {
                progress(0.2 + 0.7 * reached as f32 / cells as f32);
            }
            let (x, y) = self.made_xy(at);
            for (nx, ny) in self.neighbours(x, y).into_iter().flatten() {
                let next = place(nx, ny);
                if self.made[next] == Cell::Empty && nearest[next] == NONE {
                    nearest[next] = nearest[at];
                    queue.push_back(next as u32);
                }
            }
        }
        let Some((world, reach)) = world else {
            for (x, y) in order() {
                let at = place(x, y);
                if self.made[at] == Cell::Empty && nearest[at] != LAND {
                    self.made[at] = Cell::Sea;
                }
            }
            progress(1.0);
            return;
        };
        // What may be sea, and where the open sea starts.
        let mut may = vec![false; self.made.len()];
        let mut seeds: Vec<u32> = Vec::new();
        for (x, y) in order() {
            let at = place(x, y);
            if self.made[at] != Cell::Empty {
                continue;
            }
            let [cx, cz] = self.centre(x, y);
            let open = world.deep_sea(cx, cz, reach);
            may[at] = open
                || match nearest[at] {
                    WATER => world.sea_within(cx, cz, reach),
                    NONE => world.sea_at(cx, cz),
                    _ => false,
                };
            if open {
                seeds.push(at as u32);
            }
        }
        progress(0.95);
        // The open sea, and what joins it through ground that may be sea and painted water.
        let mut reached = vec![false; self.made.len()];
        for &seed in &seeds {
            reached[seed as usize] = true;
        }
        let mut stack = seeds;
        while let Some(at) = stack.pop() {
            let (x, y) = self.made_xy(at as usize);
            for (nx, ny) in self.neighbours(x, y).into_iter().flatten() {
                let next = place(nx, ny);
                if !reached[next] && (may[next] || self.made[next] == Cell::Water) {
                    reached[next] = true;
                    stack.push(next as u32);
                }
            }
        }
        for (x, y) in order() {
            let at = place(x, y);
            if self.made[at] == Cell::Empty && reached[at] {
                self.made[at] = Cell::Sea;
            }
        }
        progress(1.0);
    }

    /// Cell (x, y)'s centre on the ground plane.
    fn centre(&self, x: usize, y: usize) -> Point {
        [self.origin[0] + (x as f32 + 0.5) * self.cell, self.origin[1] + (y as f32 + 0.5) * self.cell]
    }
}

/// Finds the open sea in `bounds` (min x, min z, max x, max z) around `ground` (the tests' grid).
#[cfg(test)]
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
/// sea as its ground, and land over it. The tests' (the map tells its progress: [`land_shapes_telling`]).
#[cfg(test)]
pub fn land_shapes(grid: &SeaGrid, tolerance: f32) -> Vec<Shape> {
    land_shapes_telling(grid, tolerance, &mut |_| {})
}

/// [`land_shapes`], telling `progress` how far it is, 0 to 1: the land's strength block by block, then the
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

/// The land's outlines, traced where they can be: round the blocks with land in them, the made ones and those not made
/// that aren't sea. A square of the field crosses its level only where land and sea are both within its corners'
/// 3 × 3, so within two cells of a block with land.
fn trace_land(grid: &SeaGrid, tolerance: f32, carried_on: bool, progress: &mut dyn FnMut(f32)) -> Vec<Shape> {
    // The field's ring of cells round the grid: one of sea, or (carried on) one of the edge's own and one of sea.
    let pad = if carried_on { 2 } else { 1 };
    let (w, h) = (grid.width + 2 * pad, grid.height + 2 * pad);
    // Whether a cell is land: in the grid, by its own; carried on, a cell just beyond by the edge's nearest.
    let land = |nx: isize, ny: isize| {
        let (width, height) = (grid.width as isize, grid.height as isize);
        let reach = if carried_on { 1 } else { 0 };
        if nx < -reach || ny < -reach || nx >= width + reach || ny >= height + reach {
            return false;
        }
        grid.at(nx.clamp(0, width - 1) as usize, ny.clamp(0, height - 1) as usize) != Cell::Sea
    };
    // Land strength at each cell's centre (how many of the 3 × 3 around it are land), with a ring of sea around it
    // all so every outline closes: at field point (fx, fy), the cell (fx − pad, fy − pad).
    let (pad, w, h) = (pad as isize, w as isize, h as isize);
    let mut links = Links::new();
    let blocks = grid.slots.len();
    for block in 0..blocks {
        if block % (blocks / 100).max(1) == 0 {
            progress(0.4 * block as f32 / blocks.max(1) as f32);
        }
        if grid.slots[block] == NOT_MADE && grid.whole[block] == Cell::Sea {
            continue;
        }
        // The squares any of whose corners' 3 × 3 reach into the block.
        let (x0, y0) = grid.corner(block);
        let (x0, y0) = (x0 as isize, y0 as isize);
        let (fx0, fx1) = ((x0 + pad - 2).max(0), (x0 + pad + BLOCK as isize).min(w - 2));
        let (fy0, fy1) = ((y0 + pad - 2).max(0), (y0 + pad + BLOCK as isize).min(h - 2));
        if fx0 > fx1 || fy0 > fy1 {
            continue;
        }
        // Their corners' land around them, then their strength.
        let (cx0, cy0) = (fx0 - pad - 1, fy0 - pad - 1);
        let (cw, ch) = (fx1 + 1 - fx0 + 3, fy1 + 1 - fy0 + 3);
        let lands: Vec<u8> =
            (0..ch).flat_map(|j| (0..cw).map(move |i| (i, j))).map(|(i, j)| land(cx0 + i, cy0 + j) as u8).collect();
        let (sw, sh) = (fx1 + 2 - fx0, fy1 + 2 - fy0);
        let strength: Vec<u8> = (0..sh)
            .flat_map(|j| (0..sw).map(move |i| (i, j)))
            .map(|(i, j)| {
                let (fx, fy) = (fx0 + i, fy0 + j);
                if fx < 1 || fy < 1 || fx >= w - 1 || fy >= h - 1 {
                    return 0;
                }
                let mut sum = 0;
                for (dx, dy) in [(-1, -1), (0, -1), (1, -1), (-1, 0), (0, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
                    sum += lands[((j + 1 + dy) * cw + (i + 1 + dx)) as usize];
                }
                sum
            })
            .collect();
        let at = |fx: isize, fy: isize| f32::from(strength[((fy - fy0) * sw + (fx - fx0)) as usize]);
        for fy in fy0..=fy1 {
            for fx in fx0..=fx1 {
                let corners = [at(fx, fy), at(fx + 1, fy), at(fx + 1, fy + 1), at(fx, fy + 1)];
                link_square(&mut links, fx as usize, fy as usize, corners, 4.5);
            }
        }
    }
    progress(0.4);
    let rings = rings_of(links);
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

/// A crossing of a traced field's level, by the edge it's on: (x, y, vertical).
type Edge = (usize, usize, bool);
/// The crossings found: each one's next round its ring, and where it is.
type Links = HashMap<Edge, (Edge, Point)>;

/// Marching squares over `field` (`w` × `h` values, at whole coordinates) at `level`: closed rings, in
/// field coordinates. Each runs with the inside (above `level`) on the same side, so outlines and holes
/// wind opposite ways.
pub(crate) fn trace<T: Copy + Into<f32>>(field: &[T], w: usize, h: usize, level: f32) -> Vec<Vec<Point>> {
    let value = |x: usize, y: usize| field[y * w + x].into();
    let mut links = Links::new();
    for y in 0..h - 1 {
        for x in 0..w - 1 {
            // Corners clockwise from the top left.
            let corners = [value(x, y), value(x + 1, y), value(x + 1, y + 1), value(x, y + 1)];
            link_square(&mut links, x, y, corners, level);
        }
    }
    rings_of(links)
}

/// The crossings of the square with its top left at (x, y), its `corners` clockwise from there: each one it leaves
/// the inside by, linked to the one it enters it by next, with where it crosses.
fn link_square(links: &mut Links, x: usize, y: usize, corners: [f32; 4], level: f32) {
    // The edges between the corners, in the same order, each with the values at its start and its end.
    let edges: [Edge; 4] = [(x, y, false), (x + 1, y, true), (x, y + 1, false), (x, y, true)];
    let ends = [(corners[0], corners[1]), (corners[1], corners[2]), (corners[3], corners[2]), (corners[0], corners[3])];
    let crossing = |i: usize| -> Point {
        let (x, y, vertical) = edges[i];
        let (x2, y2) = if vertical { (x, y + 1) } else { (x + 1, y) };
        let (a, b) = ends[i];
        let t = ((level - a) / (b - a)).clamp(0.0, 1.0);
        [x as f32 + t * (x2 as f32 - x as f32), y as f32 + t * (y2 as f32 - y as f32)]
    };
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
            links.insert(edges[exits[0]], (edges[entries[0]], crossing(exits[0])));
        }
        2 => {
            // A saddle: the centre decides whether the inside corners join across it.
            let centre = corners.iter().sum::<f32>() / 4.0 > level;
            for &exit in &exits {
                // Joined: each exit meets the entry after it (cutting off the outside corner
                // between); apart: the entry before it (cutting off its inside corner).
                let entry = if centre { (exit + 1) % 4 } else { (exit + 3) % 4 };
                links.insert(edges[exit], (edges[entry], crossing(exit)));
            }
        }
        _ => {}
    }
}

/// The rings the crossings make, each followed round from its first crossing (in a fixed order, so the same field
/// always traces the same rings: the export is reproducible).
fn rings_of(mut links: Links) -> Vec<Vec<Point>> {
    let mut rings = Vec::new();
    let mut starts: Vec<Edge> = links.keys().copied().collect();
    starts.sort_unstable();
    for start in starts {
        if !links.contains_key(&start) {
            continue; // already part of a ring
        }
        let mut ring = Vec::new();
        let mut edge = start;
        while let Some((following, at)) = links.remove(&edge) {
            ring.push(at);
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

/// Twice the ring's signed area: its sign tells which way it winds. Worked out in f64 from its first point: in f32
/// from the map's middle, each term of a ring 100 km out is about 1e10 m², where f32's steps are 1024, so a ring under
/// about 100 m across wound either way by chance (a lake filled in as land, an island a hole: the review of
/// 2026-11-11, M11).
pub(crate) fn signed_area(ring: &[Point]) -> f64 {
    let Some(&[x0, z0]) = ring.first() else { return 0.0 };
    let from = |p: Point| (f64::from(p[0]) - f64::from(x0), f64::from(p[1]) - f64::from(z0));
    (0..ring.len())
        .map(|i| {
            let ((ax, az), (bx, bz)) = (from(ring[i]), from(ring[(i + 1) % ring.len()]));
            ax * bz - bx * az
        })
        .sum()
}

/// Whether `p` is inside `ring` (even-odd).
pub(crate) fn contains(ring: &[Point], p: Point) -> bool {
    contains_by(ring.len(), |i| ring[i], p)
}

/// [`contains`] of a ring of `count` points, the `i`th `point(i)`: a ring kept as something else (a terrain's grid
/// corners) asked the same way.
pub(crate) fn contains_by(count: usize, point: impl Fn(usize) -> Point, p: Point) -> bool {
    let mut inside = false;
    for i in 0..count {
        let (a, b) = (point(i), point((i + 1) % count));
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

    /// A ring a few metres across, far from the map's middle, winds the way it's drawn, and has its own area: in f32
    /// from the middle its sign was chance.
    #[test]
    fn a_small_ring_far_out_winds_the_way_its_drawn() {
        for (x, z) in [(0.0, 0.0), (120_000.0, -95_000.0), (1_500_000.0, 1_350_000.0)] {
            let ring: Vec<Point> = vec![[x, z], [x + 3.0, z], [x + 3.0, z + 2.0], [x, z + 2.0]];
            assert_eq!(signed_area(&ring), 12.0, "{x} {z}");
            let back: Vec<Point> = ring.iter().rev().copied().collect();
            assert_eq!(signed_area(&back), -12.0, "{x} {z}");
            // A hole in it, wound the other way: its outline's.
            let hole: Vec<Point> = vec![[x + 1.0, z + 0.5], [x + 1.0, z + 1.5], [x + 2.0, z + 1.5], [x + 2.0, z + 0.5]];
            let shapes = group_holes(vec![ring.clone(), hole]);
            assert_eq!((shapes.len(), shapes[0].holes.len()), (1, 1), "{x} {z}");
        }
    }

    /// A small generator of the numbers for the maps below: the same maps every run.
    struct Random(u64);

    impl Random {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn within(&mut self, lo: f32, hi: f32) -> f32 {
            lo + (hi - lo) * (self.next() % 1_000_000) as f32 / 1_000_000.0
        }
    }

    /// A map of `size` metres a side, its ground in a few clusters with wide empty ground between (blocks never
    /// made), some reaching its edges: roads' corridors as thin strips and rings (inland gaps closed in), squares of
    /// land, coasts painted with water, ponds, slanted pieces.
    fn random_ground(random: &mut Random, size: f32) -> Ground {
        let mut ground = Ground::default();
        let clusters = 2 + random.next() % 3;
        for _ in 0..clusters {
            let (cx, cz) = (random.within(0.0, size), random.within(0.0, size));
            let reach = random.within(500.0, 2500.0);
            for _ in 0..(10 + random.next() % 30) {
                let (x, z) = (cx + random.within(-reach, reach), cz + random.within(-reach, reach));
                let (w, h) = (random.within(15.0, 400.0), random.within(15.0, 400.0));
                match random.next() % 6 {
                    0 => ground.land.push(square(x, z, x + w, z + 30.0)),
                    1 => ground.land.push(square(x, z, x + 30.0, z + h)),
                    2 => {
                        // A ring of corridors closing in a gap.
                        ground.land.push(square(x, z, x + w + 60.0, z + 30.0));
                        ground.land.push(square(x, z + h + 30.0, x + w + 60.0, z + h + 60.0));
                        ground.land.push(square(x, z, x + 30.0, z + h + 60.0));
                        ground.land.push(square(x + w + 30.0, z, x + w + 60.0, z + h + 60.0));
                    }
                    3 => {
                        // A coast: land, its painted water strip beside it.
                        ground.land.push(square(x, z, x + w, z + 40.0));
                        ground.water.push(square(x, z - h, x + w, z));
                    }
                    4 => ground.water.push(square(x, z, x + w / 4.0, z + h / 4.0)),
                    _ => ground.land.push(vec![[x, z], [x + w, z + h / 3.0], [x + w / 2.0, z + h]]),
                }
            }
        }
        // Ground at the edges, now and then.
        if random.next().is_multiple_of(2) {
            ground.land.push(square(-50.0, size * 0.4, 300.0, size * 0.45));
            ground.water.push(square(size * 0.6, size - 200.0, size * 0.7, size + 50.0));
        }
        ground
    }

    /// The grid kept in blocks finds the same sea, the same regions and traces the same land as the grid holding
    /// every cell, on maps with wide empty ground between their parts (blocks never made), blocks cut by the grid's
    /// edges, with the regions' rule and the nearest's (and held to a picture of the world).
    #[test]
    fn the_grid_in_blocks_finds_what_the_whole_grid_finds() {
        use crate::world_picture::{Placement, WorldPicture};
        let mut random = Random(0x9E37_79B9_7F4A_7C15);
        for map in 0..14 {
            // Sizes that aren't whole blocks, and a cell that isn't 10 m now and then.
            let size = 9_000.0 + (map as f32) * 777.0;
            let ground = random_ground(&mut random, size);
            let bounds = [0.0, 0.0, size, size + 333.0];
            let cell = if map % 3 == 2 { 20.0 } else { 10.0 };
            let grey: Vec<u8> = (0..120 * 120)
                .map(|i| if (i % 120) as f32 * 120.0 < size * 0.3 || (i / 120) % 17 == 0 { 155 } else { 53 })
                .collect();
            let placement = Placement { origin: [-500.0, -500.0], scale: size / 100.0 };
            let world = WorldPicture::from_grey(&grey, 120, placement);
            let world = Some(std::sync::Arc::new(world));
            let rules = [
                SeaOptions { cell, ..SeaOptions::default() },
                SeaOptions { cell, seal: 40.0, water_edge: 0.1, ..SeaOptions::default() },
                SeaOptions { cell, nearest: true, ..SeaOptions::default() },
                SeaOptions { cell, nearest: true, world, world_reach: 300.0, ..SeaOptions::default() },
            ];
            for (rule, options) in rules.into_iter().enumerate() {
                let whole = dense::fill(&ground, bounds, options.clone());
                let blocks = fill(&ground, bounds, options);
                assert_eq!((blocks.width, blocks.height), (whole.width, whole.height));
                for y in 0..whole.height {
                    for x in 0..whole.width {
                        assert_eq!(blocks.at(x, y), whole.at(x, y), "map {map}, rule {rule}: cell {x} {y}");
                    }
                }
                let key = |r: &Region| (r.cells, r.water_edge, r.land_edge, r.open, r.sea);
                let mut found: Vec<_> = blocks.regions.iter().map(key).collect();
                let mut before: Vec<_> = whole.regions.iter().map(key).collect();
                found.sort_unstable();
                before.sort_unstable();
                assert_eq!(found, before, "map {map}, rule {rule}: regions");
                assert_eq!(land_shapes(&blocks, 3.0), dense::land_shapes(&whole, 3.0), "map {map}, rule {rule}");
                assert_eq!(
                    land_shapes_carried_on(&blocks, 3.0, &mut |_| {}),
                    dense::land_shapes_carried_on(&whole, 3.0, &mut |_| {}),
                    "map {map}, rule {rule}: carried on"
                );
                if rule < 2 {
                    assert!(blocks.made_blocks.len() < blocks.slots.len(), "map {map}: some blocks never made");
                }
            }
        }
    }

    /// A map whose parts lie far apart (a player's: 1,561 x 1,389 km) holds the blocks round its
    /// parts alone, its sea found between them all the same.
    #[test]
    fn parts_far_apart_hold_the_blocks_round_them_alone() {
        let mut ground = Ground::default();
        // Two islands 1,500 km apart, each with a coast painted along its west side.
        for (x, z) in [(10_000.0, 10_000.0), (1_500_000.0, 1_350_000.0)] {
            ground.water.push(square(x - 500.0, z, x, z + 2000.0));
            ground.land.push(square(x, z, x + 2000.0, z + 2000.0));
        }
        let bounds = [0.0, 0.0, 1_561_000.0, 1_389_000.0];
        let grid = fill(&ground, bounds, SeaOptions { cell: 20.0, ..SeaOptions::default() });
        assert_eq!(grid.width * grid.height, 78_050 * 69_450, "5.4 billion cells");
        assert!(grid.made.len() < 50_000_000, "{} cells held", grid.made.len());
        let at = |x: f32, z: f32| grid.at((x / 20.0) as usize, (z / 20.0) as usize);
        assert_eq!(at(11_000.0, 11_000.0), Cell::Land);
        assert_eq!(at(10_000.0 - 300.0, 11_000.0), Cell::Water);
        assert_eq!(at(700_000.0, 700_000.0), Cell::Sea, "the sea between");
        assert_eq!(at(1_501_000.0, 1_351_000.0), Cell::Land);
        assert_eq!(at(1_560_990.0, 1_388_990.0), Cell::Sea);
        let shapes = land_shapes(&grid, 5.0);
        assert_eq!(shapes.len(), 2, "each island's land");
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
                grid.set(x, y, if (island && !lake) || edge { Cell::Land } else { Cell::Sea });
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
