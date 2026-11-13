//! The map's buildings as blocks to raise (PLAN.md: Navigation plan, D): each placed building model's shape
//! from above, on the ground where the game stands it, in parts of their own heights.
//!
//! A building is a model item (sector item type 5) whose model's file is a building's (`world::building_kind`):
//! 244,825 on the base game's map with every DLC, 4,837 models (2026-10-22). Each model's shape is worked out
//! once from its `.pmg`'s triangles ([`model_shape`]): laid on a grid seen from above, each cell its top and
//! where the geometry hanging down from it ends; cells of like heights traced as outlines ([`ShapePart`]), each
//! raised from its bottom to its top (a tower's shaft from the ground, the sphere on it from where it starts; a canopy from
//! its roof's underside). Placed as the item is: turned as its node is (the yaw of its rotation), scaled as
//! the item is (a negative scale mirroring it). A model whose triangles can't be read is its box (the `.pmg`
//! header's: within 1.7–2.9 m of the game's own bounds of `flats_close_01_be_lux` at four places in Berlin).

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::area;
use crate::files::GameFiles;
use crate::geometry::{Point, PrefabPlacement, join_lines, road_curve};
use crate::pmd::Pmd;
use crate::pmg::Pmg;
use crate::sea::{contains, simplify_ring};
use crate::sector::{ItemData, Node};
use crate::terrain::region_outlines;
use crate::world::{BuildingKind, BuildingModel, Map, Prefabs, StructureSet};

/// Smaller than this on the ground (m², or on its narrow side, m) or lower than this (m): a prop, not a
/// building (a car charger at a filling station, a booth's sign).
const LEAST_AREA: f32 = 12.0;
const LEAST_SIDE: f32 = 2.5;
const LEAST_HEIGHT: f32 = 2.5;
/// A canopy as a box (its triangles unread): a roof, raised as a slab this thick under its top.
const CANOPY_THICKNESS: f32 = 1.2;
/// The grid a model's shape is traced on: cells this size (m), larger for a model too big for this many a side;
/// half a cell out from the model's box, so a wall on its edge falls in a cell's middle (an outline then at most
/// half a cell wider than the model).
const CELL: f32 = 0.5;
const MOST_CELLS: usize = 512;
/// The most half cells a model's edge is walked in: a million (250 km or more), past any real model's; a longer edge
/// can only be a damaged model's ([`Grid::cover`]).
const MOST_EDGE_STEPS: f32 = 1_000_000.0;
/// Lower than this (m): ground clutter (a yard's paving, kerbs), not the building.
const LEAST_TOP: f32 = 1.5;
/// Geometry reaching down to this (m) stands on the ground (a wall, a pillar); higher, it's over open ground
/// (a canopy's roof, a tower's sphere), if open ground under it joins the outside.
const CLEARANCE: f32 = 2.5;
/// A cell's heights in steps of this (m; more for a model taller than this many steps), and how big a gap
/// (m) under the geometry hanging from a cell's top ends it (a canopy's roof over the pumps under it).
const HEIGHT_STEP: f32 = 0.5;
const HEIGHT_STEPS: u32 = 64;
const GAP: f32 = 0.75;
/// A part smaller than this (m²) is left out (a chimney, a pillar, a bit of trim at a height of its own); of a
/// model's parts, only this many of the largest are kept.
const LEAST_PART: f32 = 8.0;
const MOST_PARTS: usize = 6;
/// A roof over open ground walled along this much of its edge is a building, not a canopy (a garage's hall
/// with its doors open; a filling station's roof has its pillars only).
const WALLED_SHARE: f32 = 0.5;
/// How far an outline may stray from the cells' staircase (m).
const OUTLINE_TOLERANCE: f32 = 0.6;

/// A model's box: its least and greatest corners, metres, its own axes (x right, y up, z back).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelBox {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

/// A model's box from its `.pmg` (the model's `.pmd` path with `.pmg` for `.pmd`); `None` if it isn't a
/// `.pmg` this reads ([`Pmg`]).
pub fn model_box(pmg: &[u8]) -> Option<ModelBox> {
    let (min, max) = Pmg::read(pmg)?.model_box()?;
    let sane = min.iter().chain(&max).all(|v| v.is_finite()) && (0..3).all(|axis| min[axis] <= max[axis]);
    sane.then_some(ModelBox { min, max })
}

/// A model's locator: a named place in it where something else is fitted (a truck interior's `int_rearwall`,
/// where the rear wall the player chose goes).
#[derive(Clone, Debug, PartialEq)]
pub struct Locator {
    pub name: String,
    /// Metres, the model's own axes (x right, y up, z back).
    pub position: [f32; 3],
    /// How what's fitted there is turned: a quaternion, w first.
    pub rotation: [f32; 4],
}

/// Every part's locators of a `.pmg`, in its order ([`Pmg::locator`]); `None` if it isn't a `.pmg` this reads.
pub fn model_locators(pmg: &[u8]) -> Option<Vec<Locator>> {
    let model = Pmg::read(pmg)?;
    let mut locators = Vec::new();
    for part in 0..model.part_count()? {
        for index in model.part(part)?.locators {
            let locator = model.locator(index)?;
            let name = crate::token::token_to_string(locator.name)?;
            let (position, rotation) = (locator.position, locator.rotation);
            if position.iter().chain(&rotation).any(|v| !v.is_finite()) {
                return None;
            }
            locators.push(Locator { name, position, rotation });
        }
    }
    Some(locators)
}

/// A model's triangles (each three corners: x, y, z, metres, its own axes); `None` if it isn't a model this reads, or
/// its pieces' records aren't in the file (a piece or triangle that doesn't read is left out).
pub fn model_triangles(pmg: &[u8]) -> Option<Vec<[[f32; 3]; 3]>> {
    shown_triangles(pmg, None)
}

/// [`model_triangles`] of only the pieces `shown` (by [`visible_pieces`]), or all.
pub fn shown_triangles(pmg: &[u8], shown: Option<&[bool]>) -> Option<Vec<[[f32; 3]; 3]>> {
    let model = Pmg::read(pmg)?;
    let mut triangles = Vec::new();
    for index in 0..model.piece_count()? {
        if shown.is_some_and(|shown| !shown.get(index).copied().unwrap_or(true)) {
            continue;
        }
        // A piece, or a triangle, that doesn't read is left out and the rest drawn, as surface/faces.rs does (it gave up
        // the whole model, a building gone for one damaged piece: the review of 2026-11-11, M13; no real model has one,
        // pmg_survey). Its triangles only as far as the file holds its indices (its count is the file's word).
        let Some(piece) = model.piece(index) else { continue };
        if piece.stride < 12 {
            continue;
        }
        triangles.extend(model.triangles(&piece));
    }
    Some(triangles)
}

/// Which of a `.pmg`'s pieces `variant` (a token: the item's) shows, by its `.pmd`: each part's `visible`
/// attribute in that variant. `None` if the descriptor doesn't read, or has no such variant (then all are).
pub fn visible_pieces(pmd: &[u8], pmg: &[u8], variant: u64) -> Option<Vec<bool>> {
    let pmd = Pmd::read(pmd)?;
    let chosen = pmd.variant(variant)?;
    let visible_token = crate::token::string_to_token("visible")?;
    let model = Pmg::read(pmg)?;
    if model.part_count()? != pmd.parts() {
        return None;
    }
    let pieces = model.piece_count()?;
    let mut shown = vec![true; pieces];
    for part in 0..pmd.parts() {
        if pmd.part_visible(part, chosen, visible_token) != Some(false) {
            continue;
        }
        let range = model.part(part)?.pieces;
        let (start, end) = (range.start, range.end.min(pieces));
        if start < end {
            shown[start..end].fill(false);
        }
    }
    Some(shown)
}

/// Effects whose pieces aren't drawn: a collision mesh (`eut2.none`), a shadow's (`eut2.shadowonly`). A
/// prefab's reach under its whole yard and down its sides (`garage_sc`'s to 6 m below it).
const UNSEEN_EFFECTS: [&str; 2] = ["eut2.none", "eut2.shadowonly"];

/// A `.pmd`'s materials, its first look's: their `.mat` paths.
pub fn pmd_materials(pmd: &[u8]) -> Vec<String> {
    Pmd::read(pmd).map_or_else(Vec::new, |pmd| pmd.materials(None))
}

/// Whether a `.mat`'s effect (`effect : "eut2.none.rfx" {`, or `material : "…"` in the older form) is one
/// whose pieces aren't drawn ([`UNSEEN_EFFECTS`]).
pub fn unseen_material(mat: &str) -> bool {
    let line = mat.lines().map(str::trim_start).find(|line| line.starts_with("effect") || line.starts_with("material"));
    let Some(effect) = line.and_then(|line| line.split('"').nth(1)) else { return false };
    UNSEEN_EFFECTS
        .iter()
        .any(|unseen| effect == *unseen || effect.strip_prefix(unseen).is_some_and(|rest| rest.starts_with('.')))
}

/// Whether each material isn't drawn ([`unseen_material`]), by its path: kept for a whole map's models (prefabs share
/// their materials), from any thread. (Each model had a cache of its own, which kept nothing: the review of
/// 2026-11-11, M10.)
#[derive(Default)]
pub struct UnseenMaterials(std::sync::Mutex<HashMap<String, bool>>);

impl UnseenMaterials {
    /// Whether the material at `path` isn't drawn (read once; one that can't be read is drawn).
    fn unseen(&self, files: &GameFiles, path: String) -> bool {
        if let Some(&known) = self.0.lock().unwrap_or_else(|e| e.into_inner()).get(&path) {
            return known;
        }
        // Read outside the lock: the answer is the file's, whichever thread finds it first.
        let read = files.read(&path).ok().flatten();
        let unseen = read.is_some_and(|mat| unseen_material(&String::from_utf8_lossy(&mat)));
        self.0.lock().unwrap_or_else(|e| e.into_inner()).insert(path, unseen);
        unseen
    }
}

/// Which of a prefab model's pieces are seen in its item's `variant`: those the variant shows
/// ([`visible_pieces`]; all if its `.pmd` at `pmd_path` doesn't say) and drawn ([`UNSEEN_EFFECTS`]; each
/// material's answer kept in `unseen`). `None` if the `.pmg` doesn't read.
pub fn seen_pieces(
    files: &GameFiles,
    pmd_path: &str,
    pmg: &[u8],
    variant: u64,
    unseen: &UnseenMaterials,
) -> Option<Vec<bool>> {
    let model = Pmg::read(pmg)?;
    let pieces = model.piece_count()?;
    let pmd = files.read(pmd_path).ok().flatten().unwrap_or_default();
    let mut seen = visible_pieces(&pmd, pmg, variant).unwrap_or_else(|| vec![true; pieces]);
    let unseen: Vec<bool> = pmd_materials(&pmd).into_iter().map(|path| unseen.unseen(files, path)).collect();
    for (piece, seen) in seen.iter_mut().enumerate() {
        let material = model.material(piece);
        if material.and_then(|material| unseen.get(material)).copied().unwrap_or(false) {
            *seen = false;
        }
    }
    Some(seen)
}

/// A part of a model's shape: its outline and holes (x, z, metres, its own axes), and how low and how high
/// it is (y, metres).
#[derive(Clone, Debug, PartialEq)]
pub struct ShapePart {
    pub rings: Vec<Vec<Point>>,
    pub bottom: f32,
    pub top: f32,
}

/// A model's shape: its box, and its parts from above (none if its triangles couldn't be read: the box).
#[derive(Clone, Debug, PartialEq)]
pub struct ModelShape {
    pub model_box: ModelBox,
    pub parts: Vec<ShapePart>,
}

/// A model's shape from its `.pmg`; `None` if it isn't a `.pmg` this reads.
pub fn model_shape(pmg: &[u8]) -> Option<ModelShape> {
    shape_over(pmg, Ground::Level)
}

/// Whether a model is a bridge or a piece of one, of those drawn as buildings: the game files them among its
/// landmarks (`/model2/landmark/no/alta_bridge_01_poc.pmd`, `.../chemnitz_viaduct_01_ger.pmd`,
/// `.../kiel_overpass_01_de.pmd`; ATS's `panorama/ca_san_francisco/ca_sf_ggb_pillar_1.pmd`, the Golden Gate's tower,
/// and `ca_sf_ggb_abutment_1.pmd`).
pub fn is_bridge(path: &str) -> bool {
    path.rsplit('/').next().is_some_and(|file| ["bridge", "viaduct", "overpass", "pillar", "abutment"].iter().any(|name| file.contains(name)))
}

fn shape_over(pmg: &[u8], ground: Ground) -> Option<ModelShape> {
    let model_box = model_box(pmg)?;
    let parts = model_triangles(pmg).map(|triangles| trace(&model_box, &triangles, ground)).unwrap_or_default();
    Some(ModelShape { model_box, parts })
}

/// A prefab model's shape from its `.pmg`, only the pieces `shown` (its item's variant: [`seen_pieces`]), on
/// its own ground found from `road` ([`Ground::Own`]); `None` if it isn't a `.pmg` this reads.
pub fn prefab_shape(pmg: &[u8], shown: Option<&[bool]>, road: [f32; 3]) -> Option<ModelShape> {
    let model_box = model_box(pmg)?;
    let parts = shown_triangles(pmg, shown)
        .map(|triangles| trace(&model_box, &triangles, Ground::Own(road)))
        .unwrap_or_default();
    Some(ModelShape { model_box, parts })
}

/// Where a model's heights are measured from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ground {
    /// The model's own 0: a building's model stands on it.
    Level,
    /// Each cell's own ground: a prefab's model holds its ground (its yard, its roads, the slope it's on, above
    /// or below its 0), so a cell is measured from the lowest of what's bare there (nothing higher than
    /// [`LEAST_TOP`] over it: paving, a road, grass) where that joins the ground where a road enters it (the
    /// point given: x, y, z, its own space), or from the nearest such cell's (under a building, a roof); and a
    /// part thinner than [`LEAST_THICKNESS`] is a fence or a wall round a yard, not a building.
    Own([f32; 3]),
}

/// A part thinner than this (m: twice its area over its outline's length, about its width) is left out of a
/// prefab's shape: a fence, a wall, a hedge, a barrier; and a lifted one of a bridge's: a cable.
const LEAST_THICKNESS: f32 = 1.5;
/// Of a prefab's parts, this many of the largest are kept (a service area's shop, workshop, canopies…).
const MOST_PREFAB_PARTS: usize = 12;
/// The most a prefab's ground rises or falls from a cell to the next (m): a kerb, a slope; not a roof's edge.
const GROUND_STEP: f32 = 1.0;
/// What's narrower than twice this (m) in a prefab is a fence, a wall, a hedge: left out.
const THIN_REACH: f32 = 1.0;
/// How far from where the road enters a prefab its ground is looked for (m).
const ROAD_REACH: f32 = 6.0;

/// A model's grid from above: cells `cell` m a side from (`x0`, `z0`), `cols` × `rows`.
#[derive(Clone, Copy)]
struct Grid {
    cell: f32,
    x0: f32,
    z0: f32,
    cols: usize,
    rows: usize,
}

impl Grid {
    /// The grid a model's shape is traced on.
    fn over(model_box: &ModelBox) -> Self {
        let span = (model_box.max[0] - model_box.min[0]).max(model_box.max[2] - model_box.min[2]);
        let cell = CELL.max(span / MOST_CELLS as f32);
        let (x0, z0) = (model_box.min[0] - cell / 2.0, model_box.min[2] - cell / 2.0);
        let cols = ((model_box.max[0] - x0) / cell).floor() as usize + 1;
        let rows = ((model_box.max[2] - z0) / cell).floor() as usize + 1;
        Grid { cell, x0, z0, cols, rows }
    }

    /// Where the segment from `p` to `q` (x, z) is on the grid, as shares of its length (first, last); `None` if it
    /// never is. The grid's edge is taken a cell further out all round, so no point on it is missed by a hair.
    fn crossing(&self, p: [f32; 2], q: [f32; 2]) -> Option<(f32, f32)> {
        let low = [self.x0 - self.cell, self.z0 - self.cell];
        let high = [self.x0 + (self.cols + 1) as f32 * self.cell, self.z0 + (self.rows + 1) as f32 * self.cell];
        let (mut enter, mut exit) = (0.0f32, 1.0f32);
        for axis in 0..2 {
            let along = q[axis] - p[axis];
            if along == 0.0 {
                if p[axis] < low[axis] || p[axis] > high[axis] {
                    return None;
                }
                continue;
            }
            let (a, b) = ((low[axis] - p[axis]) / along, (high[axis] - p[axis]) / along);
            enter = enter.max(a.min(b));
            exit = exit.min(a.max(b));
        }
        (enter <= exit).then_some((enter, exit))
    }

    fn cell_of(&self, x: f32, z: f32) -> Option<usize> {
        let (col, row) = (((x - self.x0) / self.cell).floor(), ((z - self.z0) / self.cell).floor());
        (col >= 0.0 && row >= 0.0 && col < self.cols as f32 && row < self.rows as f32)
            .then(|| row as usize * self.cols + col as usize)
    }

    /// Each triangle's heights over the cells it covers, to `mark` (cell, lowest, highest).
    fn cover(&self, triangles: &[[[f32; 3]; 3]], mark: &mut dyn FnMut(usize, f32, f32)) {
        let (cell, x0, z0) = (self.cell, self.x0, self.z0);
        let mut heights: Vec<(usize, f32)> = Vec::new();
        for [a, b, c] in triangles {
            let (low_y, high_y) = (a[1].min(b[1]).min(c[1]), a[1].max(b[1]).max(c[1]));
            // Steep (a wall, a pillar's side: its face more than 60° from level): every cell it crosses, its
            // whole height there taken as its height overall; on a bridge, as low and as high as it is there: a
            // cable falling away isn't as high as its top all along. Its edges seen from above, a point every half
            // cell.
            let normal = [
                (b[1] - a[1]) * (c[2] - a[2]) - (b[2] - a[2]) * (c[1] - a[1]),
                (b[2] - a[2]) * (c[0] - a[0]) - (b[0] - a[0]) * (c[2] - a[2]),
                (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]),
            ];
            let length = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
            let steep = normal[1].abs() < 0.5 * length;
            for (p, q) in [(a, b), (b, c), (c, a)] {
                // Only the points of the edge on the grid (a damaged model's vertex 1e38 away made the walk to it
                // endless: the review of 2026-11-11, M6, the fuzzing of 2026-11-12).
                let Some((enter, exit)) = self.crossing([p[0], p[2]], [q[0], q[2]]) else { continue };
                let span = (((q[0] - p[0]).hypot(q[2] - p[2])) / (cell * 0.5)).ceil().max(1.0);
                let mut at = |t: f32| {
                    if let Some(i) = self.cell_of(p[0] + (q[0] - p[0]) * t, p[2] + (q[2] - p[2]) * t) {
                        heights.push((i, p[1] + (q[1] - p[1]) * t));
                    }
                };
                if span <= MOST_EDGE_STEPS {
                    // The stretch on the grid, its points where they always were (each a half cell along the whole
                    // edge from its start): an edge on the grid, every one.
                    let steps = span as usize;
                    let first = ((enter * span).floor().max(0.0) as usize).saturating_sub(1);
                    let last = ((exit * span).ceil() as usize).saturating_add(1).min(steps);
                    for k in first..=last {
                        at(k as f32 / steps as f32);
                    }
                } else {
                    // Longer than any model (a damaged one's): too long for its share on the grid to be told in
                    // halves of a cell along it, so the stretch on the grid is walked a half cell at a time.
                    let length = (exit - enter) * (q[0] - p[0]).hypot(q[2] - p[2]);
                    let steps = (length / (cell * 0.5)).ceil().clamp(1.0, MOST_EDGE_STEPS) as usize;
                    for k in 0..=steps {
                        at(enter + (exit - enter) * (k as f32 / steps as f32));
                    }
                }
            }
            // Its face from above: every cell whose middle it covers, at its height there (a roof), or its
            // whole height (steep).
            let area = (b[0] - a[0]) * (c[2] - a[2]) - (c[0] - a[0]) * (b[2] - a[2]);
            let (min_x, max_x) = (a[0].min(b[0]).min(c[0]), a[0].max(b[0]).max(c[0]));
            let (min_z, max_z) = (a[2].min(b[2]).min(c[2]), a[2].max(b[2]).max(c[2]));
            let first_col = (((min_x - x0) / cell).floor().max(0.0)) as usize;
            let first_row = (((min_z - z0) / cell).floor().max(0.0)) as usize;
            let last_col = (((max_x - x0) / cell).floor().max(0.0) as usize).min(self.cols - 1);
            let last_row = (((max_z - z0) / cell).floor().max(0.0) as usize).min(self.rows - 1);
            if area.abs() >= 1e-6 {
                for row in first_row..=last_row {
                    for col in first_col..=last_col {
                        let (x, z) = (x0 + (col as f32 + 0.5) * cell, z0 + (row as f32 + 0.5) * cell);
                        let wb = ((b[0] - x) * (c[2] - z) - (c[0] - x) * (b[2] - z)) / area;
                        let wc = ((c[0] - x) * (a[2] - z) - (a[0] - x) * (c[2] - z)) / area;
                        let wa = 1.0 - wb - wc;
                        if wa >= 0.0
                            && wb >= 0.0
                            && wc >= 0.0
                            && let Some(i) = self.cell_of(x, z)
                        {
                            heights.push((i, wa * a[1] + wb * b[1] + wc * c[1]));
                        }
                    }
                }
            }
            if steep {
                heights.sort_unstable_by_key(|&(i, _)| i);
                for run in heights.chunk_by(|a, b| a.0 == b.0) {
                    mark(run[0].0, low_y, high_y);
                }
            } else {
                for &(i, y) in &heights {
                    mark(i, y, y);
                }
            }
            heights.clear();
        }
    }

    /// Each cell's ground ([`Ground::Own`]): the lowest of what's there where that's all low (bare) and joins
    /// the ground at `road`, else the nearest such cell's (spread out a cell at a time); the road's height
    /// everywhere where none does.
    fn own_ground(&self, triangles: &[[[f32; 3]; 3]], road: [f32; 3]) -> Vec<f32> {
        let count = self.cols * self.rows;
        let (mut low, mut high) = (vec![f32::INFINITY; count], vec![f32::NEG_INFINITY; count]);
        self.cover(triangles, &mut |i, y0, y1| {
            low[i] = low[i].min(y0);
            high[i] = high[i].max(y1);
        });
        // The ground: bare cells reached from where the road enters (within ROAD_REACH of it, as high as it
        // give or take GROUND_STEP), a cell at a time, never a step of more than GROUND_STEP between them (a
        // flat roof is bare too, but walls and its drop part it from the ground). With none near the road (a
        // pump island under its roof, no yard of its own), the road's height everywhere.
        let bare = |i: usize| low[i].is_finite() && high[i] - low[i] < LEAST_TOP;
        let mut level = vec![f32::NAN; count];
        let mut queue = std::collections::VecDeque::new();
        let near_road = |i: usize| {
            let (x, z) = (
                self.x0 + ((i % self.cols) as f32 + 0.5) * self.cell,
                self.z0 + ((i / self.cols) as f32 + 0.5) * self.cell,
            );
            (x - road[0]).hypot(z - road[2]) <= ROAD_REACH && (low[i] - road[1]).abs() <= GROUND_STEP
        };
        queue.extend((0..count).filter(|&i| bare(i) && near_road(i)));
        if queue.is_empty() {
            return vec![road[1]; count];
        }
        for &i in &queue {
            level[i] = low[i];
        }
        while let Some(i) = queue.pop_front() {
            let (row, col) = (i / self.cols, i % self.cols);
            for (r, c) in [(row.wrapping_sub(1), col), (row + 1, col), (row, col.wrapping_sub(1)), (row, col + 1)] {
                if r >= self.rows || c >= self.cols {
                    continue;
                }
                let j = r * self.cols + c;
                if level[j].is_nan() && bare(j) && (low[j] - low[i]).abs() <= GROUND_STEP {
                    level[j] = low[j];
                    queue.push_back(j);
                }
            }
        }
        // Everywhere else (under a building, a roof, in a courtyard), the nearest ground's.
        queue.extend((0..count).filter(|&i| !level[i].is_nan()));
        while let Some(i) = queue.pop_front() {
            let (row, col) = (i / self.cols, i % self.cols);
            for (r, c) in [(row.wrapping_sub(1), col), (row + 1, col), (row, col.wrapping_sub(1)), (row, col + 1)] {
                if r < self.rows && c < self.cols && level[r * self.cols + c].is_nan() {
                    level[r * self.cols + c] = level[i];
                    queue.push_back(r * self.cols + c);
                }
            }
        }
        level
    }
}

/// A model's ground under each place of it ([`Ground`]): the model's 0, or, for a prefab's, its own as [`trace`] finds it.
pub struct Levels {
    grid: Grid,
    levels: Option<Vec<f32>>,
}

impl Levels {
    pub fn new(model_box: &ModelBox, triangles: &[[[f32; 3]; 3]], ground: Ground) -> Self {
        let grid = Grid::over(model_box);
        let levels = match ground {
            Ground::Own(road) => Some(grid.own_ground(triangles, road)),
            Ground::Level => None,
        };
        Self { grid, levels }
    }

    /// The ground's height at (x, z), the model's own space: the nearest cell's, off its grid.
    pub fn at(&self, x: f32, z: f32) -> f32 {
        let Some(levels) = &self.levels else { return 0.0 };
        let Grid { cell, x0, z0, cols, rows, .. } = self.grid;
        let col = (((x - x0) / cell).floor().max(0.0) as usize).min(cols - 1);
        let row = (((z - z0) / cell).floor().max(0.0) as usize).min(rows - 1);
        levels[row * cols + col]
    }
}

/// The parts of a model from above (see the module's notes): its triangles on a grid, each cell which heights
/// the model is at over it (a column of [`HEIGHT_STEPS`], over `ground`); in each cell, what stands on the
/// ground (up to the first gap) and what's lifted above it; cells of like bottoms and tops traced as outlines.
pub fn trace(model_box: &ModelBox, triangles: &[[[f32; 3]; 3]], ground: Ground) -> Vec<ShapePart> {
    trace_levelled(model_box, triangles, ground).0
}

/// [`trace`], and the ground's levels it measured from ([`Levels`]: what stands on them is kept by them too, once
/// worked out, not twice: the review of 2026-11-11, M10).
pub fn trace_levelled(model_box: &ModelBox, triangles: &[[[f32; 3]; 3]], ground: Ground) -> (Vec<ShapePart>, Levels) {
    let grid = Grid::over(model_box);
    let Grid { cell, x0, z0, cols, rows, .. } = grid;
    let own = matches!(ground, Ground::Own(_));
    // Each cell's ground: the model's 0, or its own (on the same grid).
    let levelled = Levels::new(model_box, triangles, ground);
    let levels = &levelled.levels;
    let level = |i: usize| levels.as_ref().map_or(0.0, |levels| levels[i]);
    let reach = match levels {
        None => model_box.max[1],
        Some(levels) => levels.iter().map(|level| model_box.max[1] - level).fold(0.0, f32::max),
    };
    // Each cell's heights over its ground, in steps of `step` from the ground up (what's lower, in the first).
    let step = HEIGHT_STEP.max(reach / HEIGHT_STEPS as f32);
    let step_of = |y: f32| ((y / step).floor().max(0.0) as u32).min(HEIGHT_STEPS - 1);
    let (mut column, mut high) = (vec![0u64; cols * rows], vec![f32::NEG_INFINITY; cols * rows]);
    grid.cover(triangles, &mut |at, y0, y1| {
        let (y0, y1) = (y0 - level(at), y1 - level(at));
        column[at] |= (u64::MAX >> (63 - step_of(y1))) & (u64::MAX << step_of(y0));
        high[at] = high[at].max(y1);
    });
    let at = |row: usize, col: usize| row * cols + col;
    let height_of = |step_index: u32| step_index as f32 * step;
    let clear = step_of(CLEARANCE);
    // Each cell's geometry from the ground up (to the first gap of GAP or more), the top it reaches, and
    // whatever is above that: its lowest point, and the cell's top.
    let mut ground_top = vec![f32::NEG_INFINITY; cols * rows];
    let mut upper_bottom = vec![f32::INFINITY; cols * rows];
    for i in 0..cols * rows {
        let heights = column[i];
        if heights == 0 || high[i] < LEAST_TOP {
            continue;
        }
        let lowest = heights.trailing_zeros();
        if lowest > clear {
            upper_bottom[i] = height_of(lowest);
            continue;
        }
        // A gap ends it once it's GAP high and reaches CLEARANCE: under that, nothing stands (a building's
        // walls on a floor raised over its ground, a loading dock's), so what's over it stands on the ground.
        let (mut last, mut gap) = (lowest, 0.0);
        for index in lowest + 1..HEIGHT_STEPS {
            if heights & (1 << index) != 0 {
                (last, gap) = (index, 0.0);
            } else {
                gap += step;
                if gap >= GAP && height_of(index + 1) >= CLEARANCE {
                    break;
                }
            }
        }
        ground_top[i] = height_of(last + 1).min(high[i]);
        let above = if last + 1 < HEIGHT_STEPS { heights >> (last + 1) } else { 0 };
        if above != 0 {
            upper_bottom[i] = height_of(last + 1 + above.trailing_zeros());
        }
    }
    let solid: Vec<bool> = (0..cols * rows).map(|i| high[i] >= LEAST_TOP).collect();
    // What stands on the ground (walls, pillars, a tower's shaft, the pumps under a roof), a cell more all
    // round: gaps up to two cells (doors) closed.
    let standing: Vec<bool> = (0..cols * rows).map(|i| ground_top[i] >= LEAST_TOP).collect();
    let mut blocked = vec![false; cols * rows];
    for row in 0..rows {
        for col in 0..cols {
            if standing[at(row, col)] {
                for r in row.saturating_sub(1)..=(row + 1).min(rows - 1) {
                    for c in col.saturating_sub(1)..=(col + 1).min(cols - 1) {
                        blocked[at(r, c)] = true;
                    }
                }
            }
        }
    }
    // Open ground joined to the outside: from the grid's edge, through what isn't blocked.
    let mut open = vec![false; cols * rows];
    let mut queue: Vec<(usize, usize)> = Vec::new();
    for row in 0..rows {
        for col in 0..cols {
            if (row == 0 || col == 0 || row == rows - 1 || col == cols - 1) && !blocked[at(row, col)] {
                open[at(row, col)] = true;
                queue.push((row, col));
            }
        }
    }
    while let Some((row, col)) = queue.pop() {
        let next = [(row.wrapping_sub(1), col), (row + 1, col), (row, col.wrapping_sub(1)), (row, col + 1)];
        for (r, c) in next {
            if r < rows && c < cols && !open[at(r, c)] && !blocked[at(r, c)] {
                open[at(r, c)] = true;
                queue.push((r, c));
            }
        }
    }
    // A blocked cell with nothing standing in it (a roof round a pillar) beside open ground is open too: the
    // closing kept the flood out of it, not the ground. (Inside a house, the strip along its walls is beside
    // the house's middle, not open ground.)
    let beside_open: Vec<bool> = (0..cols * rows)
        .map(|i| {
            let (row, col) = (i / cols, i % cols);
            blocked[i]
                && !standing[i]
                && [(row.wrapping_sub(1), col), (row + 1, col), (row, col.wrapping_sub(1)), (row, col + 1)]
                    .iter()
                    .any(|&(r, c)| r < rows && c < cols && open[at(r, c)])
        })
        .collect();
    for (i, beside) in beside_open.into_iter().enumerate() {
        open[i] |= beside;
    }
    // Each cell's parts: from the ground, to the top of what stands there (all of it, for a cell with nothing
    // standing that open ground doesn't reach: a house's middle under its roof); lifted, from the lowest of
    // what's above that to the cell's top (a roof over pumps, a tower's pod, a canopy over open ground).
    let mut ground: Vec<Option<f32>> = vec![None; cols * rows];
    let mut lifted: Vec<Option<(f32, f32)>> = vec![None; cols * rows];
    for i in 0..cols * rows {
        if !solid[i] {
            continue;
        }
        if standing[i] {
            ground[i] = Some(ground_top[i]);
            if upper_bottom[i].is_finite() {
                lifted[i] = Some((upper_bottom[i], high[i]));
            }
        } else if open[i] && upper_bottom[i].is_finite() {
            lifted[i] = Some((upper_bottom[i], high[i]));
        } else {
            ground[i] = Some(high[i]);
        }
    }
    // A roof over open ground walled along at least WALLED_SHARE of its edge is a building with a door or a
    // side open (a garage's hall, its floor reached from the yard), not a roof over the open: from the ground.
    let mut seen = vec![false; cols * rows];
    let roof_only =
        |i: usize, ground: &[Option<f32>], lifted: &[Option<(f32, f32)>]| lifted[i].is_some() && ground[i].is_none();
    for start in 0..cols * rows {
        if seen[start] || !roof_only(start, &ground, &lifted) {
            continue;
        }
        let (mut roof, mut queue) = (vec![start], vec![start]);
        seen[start] = true;
        let (mut edges, mut walled) = (0usize, 0usize);
        while let Some(i) = queue.pop() {
            let (row, col) = (i / cols, i % cols);
            for (r, c) in [(row.wrapping_sub(1), col), (row + 1, col), (row, col.wrapping_sub(1)), (row, col + 1)] {
                let inside = r < rows && c < cols;
                if inside && roof_only(at(r, c), &ground, &lifted) {
                    if !seen[at(r, c)] {
                        seen[at(r, c)] = true;
                        roof.push(at(r, c));
                        queue.push(at(r, c));
                    }
                    continue;
                }
                edges += 1;
                walled += usize::from(inside && standing[at(r, c)]);
            }
        }
        if walled as f32 >= WALLED_SHARE * edges as f32 {
            for i in roof {
                (ground[i], lifted[i]) = (Some(high[i]), None);
            }
        }
    }
    // A prefab's fences, walls and hedges round its yard (narrower than twice THIN_REACH) left out where they
    // run, joined to a building or not: what's left of its cells once they're worn away by THIN_REACH all
    // round, grown back as far.
    if own {
        let reach = (THIN_REACH / cell).round() as usize;
        let kept = opened(&ground.iter().map(Option::is_some).collect::<Vec<_>>(), rows, cols, reach);
        let kept_lifted = opened(&lifted.iter().map(Option::is_some).collect::<Vec<_>>(), rows, cols, reach);
        for i in 0..cols * rows {
            if !kept[i] {
                ground[i] = None;
            }
            if !kept_lifted[i] {
                lifted[i] = None;
            }
        }
    }
    // Bands of like tops and like bottoms: a new one where the next is further up than a step from the band's top.
    let from = |band: &(f32, f32)| band.1;
    let bands = |values: Vec<f32>| -> Vec<(f32, f32)> {
        let mut values = values;
        values.sort_by(f32::total_cmp);
        values.dedup();
        let mut bands: Vec<(f32, f32)> = Vec::new();
        for value in values {
            match bands.last_mut() {
                Some(band) if value - from(band) <= (0.1 * from(band)).max(LEAST_TOP) => band.1 = value,
                _ => bands.push((value, value)),
            }
        }
        bands
    };
    let tops = bands(ground.iter().flatten().copied().chain(lifted.iter().flatten().map(|&(_, top)| top)).collect());
    let bottoms = bands(lifted.iter().flatten().map(|&(bottom, _)| bottom).collect());
    let band_of = |bands: &[(f32, f32)], value: f32| bands.iter().position(|band| value <= band.1).unwrap_or(0);
    // Each part's band, by cell: from the ground (`None` bottom band) or lifted.
    let mut keys: Vec<(Option<usize>, usize)> = Vec::new();
    let ground_key: Vec<Option<(Option<usize>, usize)>> =
        ground.iter().map(|top| top.map(|top| (None, band_of(&tops, top)))).collect();
    let lifted_key: Vec<Option<(Option<usize>, usize)>> = lifted
        .iter()
        .map(|part| part.map(|(bottom, top)| (Some(band_of(&bottoms, bottom)), band_of(&tops, top))))
        .collect();
    keys.extend(ground_key.iter().flatten().copied());
    keys.extend(lifted_key.iter().flatten().copied());
    keys.sort_unstable();
    keys.dedup();
    let corner = |(col, row): (u16, u16)| -> Point { [x0 + f32::from(col) * cell, z0 + f32::from(row) * cell] };
    let mut parts = Vec::new();
    for wanted in keys {
        let (bottom_band, top_band) = wanted;
        // Its bottom the band's lowest (a slab no thinner than the model), its top the band's highest.
        let bottom = bottom_band.map_or(0.0, |band| bottoms[band].0);
        let top = tops[top_band].1;
        let cells = if bottom_band.is_some() { &lifted_key } else { &ground_key };
        let shapes =
            region_outlines(rows as u16, cols as u16, |row, col| cells[at(row.into(), col.into())] == Some(wanted));
        for shape in shapes {
            let rings: Vec<Vec<Point>> = shape
                .iter()
                .map(|ring| simplify_ring(&ring.iter().map(|&c| corner(c)).collect::<Vec<_>>(), OUTLINE_TOLERANCE))
                .filter(|ring| ring.len() >= 3)
                .collect();
            let area = rings.first().map_or(0.0, |ring| ring_area(ring))
                - rings.iter().skip(1).map(|ring| ring_area(ring)).sum::<f32>();
            let length: f32 = rings.iter().map(|ring| ring_length(ring)).sum();
            let thin = own && 2.0 * area < LEAST_THICKNESS * length;
            if area >= LEAST_PART && !rings.is_empty() && !thin {
                parts.push((area, ShapePart { rings, bottom, top }));
            }
        }
    }
    parts.sort_by(|a, b| b.0.total_cmp(&a.0));
    parts.truncate(if own { MOST_PREFAB_PARTS } else { MOST_PARTS });
    let mut parts: Vec<ShapePart> = parts.into_iter().map(|(_, part)| part).collect();
    // A part left out leaves its hole in what's round it, and the map would show through a solid roof. A hole is open
    // ground, a courtyard, only if the model has nothing there, or a part that's kept.
    let covers = |part: &ShapePart, p: Point| contains(&part.rings[0], p) && !part.rings[1..].iter().any(|hole| contains(hole, p));
    for index in 0..parts.len() {
        let left_out = |hole: &Vec<Point>| {
            let ([x_low, z_low], [x_high, z_high]) = hole.iter().fold(([f32::MAX; 2], [f32::MIN; 2]), |(low, high), p| {
                ([low[0].min(p[0]), low[1].min(p[1])], [high[0].max(p[0]), high[1].max(p[1])])
            });
            let (mut all, mut lost) = (0, 0);
            for row in ((z_low - z0) / cell) as usize..=(((z_high - z0) / cell) as usize).min(rows - 1) {
                for col in ((x_low - x0) / cell) as usize..=(((x_high - x0) / cell) as usize).min(cols - 1) {
                    let p = [x0 + (col as f32 + 0.5) * cell, z0 + (row as f32 + 0.5) * cell];
                    if contains(hole, p) {
                        all += 1;
                        lost += usize::from(solid[at(row, col)] && !parts.iter().enumerate().any(|(other, part)| other != index && covers(part, p)));
                    }
                }
            }
            lost * 2 > all
        };
        let kept: Vec<Vec<Point>> = parts[index].rings[1..].iter().filter(|hole| !left_out(hole)).cloned().collect();
        parts[index].rings.truncate(1);
        parts[index].rings.extend(kept);
    }
    (parts, levelled)
}

/// The cells of `mask` (`rows` × `cols`) left once worn away by `reach` cells all round and grown back as far:
/// whatever is narrower than twice `reach` gone, the rest as it was.
fn opened(mask: &[bool], rows: usize, cols: usize, reach: usize) -> Vec<bool> {
    // How many cells of `cells` there are in each rectangle from the corner: each window's count at once.
    let sums = |cells: &[bool]| -> Vec<u32> {
        let mut sums = vec![0u32; (rows + 1) * (cols + 1)];
        for row in 0..rows {
            for col in 0..cols {
                sums[(row + 1) * (cols + 1) + col + 1] = u32::from(cells[row * cols + col])
                    + sums[row * (cols + 1) + col + 1]
                    + sums[(row + 1) * (cols + 1) + col]
                    - sums[row * (cols + 1) + col];
            }
        }
        sums
    };
    let window = |sums: &[u32], row: usize, col: usize| -> (u32, usize) {
        let (r0, c0) = (row.saturating_sub(reach), col.saturating_sub(reach));
        let (r1, c1) = ((row + reach + 1).min(rows), (col + reach + 1).min(cols));
        let count = sums[r1 * (cols + 1) + c1] + sums[r0 * (cols + 1) + c0]
            - sums[r0 * (cols + 1) + c1]
            - sums[r1 * (cols + 1) + c0];
        (count, (2 * reach + 1) * (2 * reach + 1))
    };
    let all = sums(mask);
    // Worn away: kept where its whole window is in the mask (the grid's edge counting as outside).
    let worn: Vec<bool> = (0..rows * cols)
        .map(|i| {
            let (count, size) = window(&all, i / cols, i % cols);
            count as usize == size
        })
        .collect();
    let left = sums(&worn);
    (0..rows * cols).map(|i| mask[i] && window(&left, i / cols, i % cols).0 > 0).collect()
}

/// A ring's length all round (m).
fn ring_length(ring: &[Point]) -> f32 {
    ring.iter().zip(ring.iter().cycle().skip(1)).map(|(a, b)| (b[0] - a[0]).hypot(b[1] - a[1])).sum()
}

/// A ring's area (m², either way round): in f64 from its first point ([`crate::sea::signed_area`]; in f32 from the
/// map's middle, a building's was off by thousands of m² 100 km out).
fn ring_area(ring: &[Point]) -> f32 {
    (crate::sea::signed_area(ring).abs() / 2.0) as f32
}

/// A building placed on the map, a part of it: its outline and holes (metres: x east, z south), how high it
/// reaches and where it starts above the ground, what kind, and its item's uid; how high in the game the ground
/// it stands on is, and whether it's a part of a bridge.
#[derive(Clone, Debug, PartialEq)]
pub struct Building {
    pub rings: Vec<Vec<Point>>,
    pub height: f32,
    pub base: f32,
    pub kind: BuildingKind,
    pub uid: u64,
    pub ground: f32,
    pub bridge: bool,
}

/// A building's shape placed at `node`, scaled by `scale`: its parts (its box's, if it has none); none if it's
/// too small to be a building.
pub fn place(shape: &ModelShape, node: &Node, scale: [f32; 3], kind: BuildingKind, uid: u64) -> Vec<Building> {
    let model = &shape.model_box;
    let (width, depth) =
        (((model.max[0] - model.min[0]) * scale[0]).abs(), ((model.max[2] - model.min[2]) * scale[2]).abs());
    let height = model.max[1] * scale[1].abs();
    if width.min(depth) < LEAST_SIDE || width * depth < LEAST_AREA || height < LEAST_HEIGHT {
        return Vec::new();
    }
    let [w, _, y, _] = node.rotation;
    let yaw = 2.0 * y.atan2(w);
    let (sin, cos) = yaw.sin_cos();
    // Scaled, then turned about the up axis as the game turns its nodes (geometry: its forward (0, -1) to
    // (-sin, -cos)).
    let at = |[x, z]: Point| -> Point {
        let (x, z) = (x * scale[0], z * scale[2]);
        [node.position[0] + x * cos + z * sin, node.position[2] - x * sin + z * cos]
    };
    let up = scale[1].abs();
    if shape.parts.is_empty() {
        let (x0, x1, z0, z1) = (model.min[0], model.max[0], model.min[2], model.max[2]);
        let base = match kind {
            BuildingKind::Canopy => (height - CANOPY_THICKNESS).max(0.0),
            _ => 0.0,
        };
        let ring = [[x0, z0], [x1, z0], [x1, z1], [x0, z1]].map(at).to_vec();
        return vec![Building { rings: vec![ring], height, base, kind, uid, ground: node.position[1], bridge: false }];
    }
    shape
        .parts
        .iter()
        .map(|part| Building {
            rings: part.rings.iter().map(|ring| ring.iter().copied().map(at).collect()).collect(),
            height: part.top * up,
            base: part.bottom * up,
            // A part lifted over open ground is a roof the truck can be under (a filling station's, a border
            // crossing's, a shop's over its door), whatever its model's folder (a filling station's shop is in
            // `gas_station`): left out while it's over the truck (owner 2026-10-22). On the ground, a building.
            kind: if part.bottom > 0.0 {
                BuildingKind::Canopy
            } else if kind == BuildingKind::Canopy {
                BuildingKind::Building
            } else {
                kind
            },
            uid,
            ground: node.position[1],
            bridge: false,
        })
        .collect()
}

/// A building placed on the map, all its drawing needs (and none of the map): its node (where, turned how),
/// its model, scale and item's uid; for a place's prefab, its variant and where a road enters it (its model a
/// prefab's token then).
#[derive(Clone, Copy, Debug)]
pub struct Placement {
    pub node: Node,
    pub model: u64,
    pub scale: [f32; 3],
    pub uid: u64,
    pub prefab: Option<PrefabLook>,
}

/// A place's prefab as placed: its model's variant (a token), and where a road enters it (its first node: x,
/// y, z in its own space), its ground found from there ([`Ground::Own`]).
#[derive(Clone, Copy, Debug)]
pub struct PrefabLook {
    pub variant: u64,
    pub road: [f32; 3],
}

/// The buildings of `map` within `bounds` (`[x0, z0, x1, z1]`) as [`Placement`]s, to draw once the map has
/// been let go (export.rs).
pub fn placements(map: &Map, models: &HashMap<u64, BuildingModel>, bounds: [f32; 4]) -> Vec<Placement> {
    map.models
        .iter()
        .filter(|placed| models.contains_key(&placed.model))
        .filter_map(|placed| {
            let node = *map.nodes.get(&placed.node)?;
            let [x, _, z] = node.position;
            (x >= bounds[0] && x <= bounds[2] && z >= bounds[1] && z <= bounds[3]).then_some(Placement {
                node,
                model: placed.model,
                scale: placed.scale,
                uid: placed.uid,
                prefab: None,
            })
        })
        .collect()
}

/// The places' prefabs of `map` within `bounds` as [`Placement`]s: those the map marks as places (a filling
/// station, a service, a garage, a dealer, a hotel, a rest area, a recruitment agency, a weigh station: by
/// their spawn points and rest triggers, as area.rs marks them; and companies' depots), shown on the map. Their
/// buildings are part of their prefab's model, not models of their own (owner 2026-10-22: they looked flat).
pub fn prefab_placements(map: &Map, prefabs: &mut Prefabs, bounds: [f32; 4]) -> Vec<Placement> {
    let companies: HashSet<u64> = map
        .items
        .iter()
        .filter_map(|item| match item.data {
            ItemData::Company { prefab, .. } => Some(prefab),
            _ => None,
        })
        .collect();
    let mut placements = Vec::new();
    for item in &map.items {
        let ItemData::Prefab { flags, model, variant, nodes, origin, .. } = &item.data else { continue };
        if flags.hidden || flags.secret {
            continue;
        }
        let Some(prefab) = prefabs.get(*model) else { continue };
        if !companies.contains(&item.uid) && !area::has_places(prefab) {
            continue;
        }
        let (Some(own), Some(at)) =
            (prefab.nodes.get(*origin as usize), nodes.first().and_then(|uid| map.nodes.get(uid)))
        else {
            continue;
        };
        let [x, _, z] = at.position;
        if x < bounds[0] || x > bounds[2] || z < bounds[1] || z > bounds[3] {
            continue;
        }
        let node = PrefabPlacement::new(own, at).as_node();
        let Some(first) = prefab.nodes.first() else { continue };
        let road = [first.position[0], first.height, first.position[1]];
        let prefab = Some(PrefabLook { variant: *variant, road });
        placements.push(Placement { node, model: *model, scale: [1.0; 3], uid: item.uid, prefab });
    }
    placements.sort_by_key(|placed| placed.model);
    placements
}

/// How far apart two walls' ends may be and still meet (m): a structure set's corner piece (`left90`) ends a
/// wall a metre or so from where the next starts.
const WALL_SNAP: f32 = 2.0;
/// A run of walls that doesn't close on itself is closed across the gap between its ends when that's no more
/// than this share of its length, and it has at least three walls: a hall walled all round but where it meets
/// another building (Nuremberg's, its west side open to its factory: 64 m of 710).
const WALL_GAP_SHARE: f32 = 0.25;
/// A run's walls carried on from its ends to the corner they leave out, each no further than this share of
/// its length (a rectangle's two missing sides are at most half its outline: Nuremberg's hall at (2229,
/// 6491), its east wall 120 m of 313 left to the building it meets).
const WALL_CORNER_SHARE: f32 = 0.5;

/// The buildings a map's structure sets make (`map.walls`: a building put together from walls repeated along
/// curves round it; `/asset/building/logistics/warehouse/structure_set_logistic_01`, 12 m high, has no roof of
/// its own: its roof's details are model items at its walls' top). Its walls, each along the game's curve
/// between its ends, are joined where they meet; a run that closes on itself (or nearly: [`WALL_GAP_SHARE`])
/// is a building's outline, raised to its tallest wall's height (its variation's parts' top). Runs that don't
/// close (a wall alone, a corner) are left out. Within `bounds` (`[x0, z0, x1, z1]`) by their first wall's start.
pub fn structure_buildings(
    map: &Map,
    sets: &HashMap<u64, StructureSet>,
    files: &GameFiles,
    bounds: [f32; 4],
) -> Vec<Building> {
    // Each set's walls' heights in each variation they're in, on the build's threads (one set at a time, before:
    // the review of 2026-11-11, minor 8).
    let unseen = UnseenMaterials::default();
    let mut pairs: Vec<(u64, u64)> = map
        .walls
        .iter()
        .filter(|wall| sets.contains_key(&wall.model))
        .map(|wall| (wall.model, wall.variation))
        .collect();
    pairs.sort_unstable();
    pairs.dedup();
    let height = |&(model, variation): &(u64, u64)| wall_height(&sets[&model], variation, files, &unseen);
    let found = crate::threads::in_order_telling(&pairs, 1, height, &mut |_| {});
    let heights: HashMap<(u64, u64), Option<f32>> = pairs.into_iter().zip(found).collect();
    // Each set's walls joined on their own: a mall's fronts run along a warehouse set's walls, their ends
    // meeting at the same places (one taken for the other, neither closed).
    let mut by_set: BTreeMap<u64, (Vec<Vec<Point>>, Vec<Wall>)> = BTreeMap::new();
    for wall in &map.walls {
        let (Some(start), Some(end)) = (map.nodes.get(&wall.start), map.nodes.get(&wall.end)) else { continue };
        let Some(height) = heights.get(&(wall.model, wall.variation)).copied().flatten() else { continue };
        let (lines, walls) = by_set.entry(wall.model).or_default();
        lines.push(road_curve(start, end));
        walls.push((wall.uid, height, [start.position[0], start.position[2]], start.position[1]));
    }
    // And each set's buildings, on the threads too, in the sets' order.
    let sets: Vec<&(Vec<Vec<Point>>, Vec<Wall>)> = by_set.values().collect();
    let each = |(lines, walls): &&(Vec<Vec<Point>>, Vec<Wall>)| set_buildings(lines, walls, bounds);
    crate::threads::in_order_telling(&sets, 1, each, &mut |_| {}).into_iter().flatten().collect()
}

/// A structure set's wall as its buildings need it: its item's uid, its height, where it starts, and how high
/// that is in the game.
type Wall = (u64, f32, Point, f32);

/// The buildings one structure set's walls make (`lines`, and each wall's [`Wall`]): see
/// [`structure_buildings`].
fn set_buildings(lines: &[Vec<Point>], walls: &[Wall], bounds: [f32; 4]) -> Vec<Building> {
    let mut buildings = Vec::new();
    for line in join_lines(lines, WALL_SNAP) {
        let Some(ring) = close_run(&line.points, line.pieces.len()) else { continue };
        let ring = simplify_ring(&ring, OUTLINE_TOLERANCE);
        let (uid, _, at, ground) = walls[line.pieces[0]];
        if ring.len() < 3
            || ring_area(&ring) < LEAST_AREA
            || at[0] < bounds[0]
            || at[0] > bounds[2]
            || at[1] < bounds[1]
            || at[1] > bounds[3]
        {
            continue;
        }
        let height = line.pieces.iter().map(|&piece| walls[piece].1).fold(0.0, f32::max);
        buildings.push(Building { rings: vec![ring], height, base: 0.0, kind: BuildingKind::Building, uid, ground, bridge: false });
    }
    buildings
}

/// A run of walls (its points, of `walls` walls) as a building's outline, if it closes: on itself (its ends
/// within [`WALL_SNAP`]); or, of three walls or more, with the corner its walls leave out (its last wall carried
/// on and its first carried back meet within [`WALL_CORNER_SHARE`] of its length of both ends: a hall whose
/// corner meets another building); or across a gap no longer than [`WALL_GAP_SHARE`] of it. `None` if it
/// doesn't.
pub fn close_run(points: &[Point], walls: usize) -> Option<Vec<Point>> {
    let (&first, &last) = (points.first()?, points.last()?);
    let gap = (last[0] - first[0]).hypot(last[1] - first[1]);
    if gap <= WALL_SNAP {
        return (points.len() > 3).then(|| points[..points.len() - 1].to_vec());
    }
    if walls < 3 {
        return None;
    }
    let length: f32 = points.windows(2).map(|step| (step[1][0] - step[0][0]).hypot(step[1][1] - step[0][1])).sum();
    let reach = WALL_CORNER_SHARE * length;
    // Each end's way out: the last wall's way on, the first's way back (from points at least a metre in).
    let way = |from: Point, towards: Point| -> Option<Point> {
        let (dx, dz) = (from[0] - towards[0], from[1] - towards[1]);
        let length = dx.hypot(dz);
        (length > 1e-3).then(|| [dx / length, dz / length])
    };
    let far = |end: Point| move |p: &&Point| (p[0] - end[0]).hypot(p[1] - end[1]) >= 1.0;
    let on = points.iter().rev().find(far(last)).and_then(|&p| way(last, p));
    let back = points.iter().find(far(first)).and_then(|&p| way(first, p));
    if let (Some(on), Some(back)) = (on, back) {
        // last + t·on = first + s·back.
        let cross = on[0] * back[1] - on[1] * back[0];
        if cross.abs() > 0.1 {
            let (dx, dz) = (first[0] - last[0], first[1] - last[1]);
            let t = (dx * back[1] - dz * back[0]) / cross;
            let s = (dx * on[1] - dz * on[0]) / cross;
            if (0.0..=reach).contains(&t) && (0.0..=reach).contains(&s) {
                let mut ring = points.to_vec();
                ring.push([last[0] + t * on[0], last[1] + t * on[1]]);
                return Some(ring);
            }
        }
    }
    (gap <= WALL_GAP_SHARE * length).then(|| points.to_vec())
}

/// How high a structure set's wall in `variation` stands over its curve (m): the top of its variation's parts'
/// pieces (those drawn: [`seen_pieces`]), or of its whole model where the variation names none of its parts;
/// `None` if its model doesn't read.
fn wall_height(set: &StructureSet, variation: u64, files: &GameFiles, unseen: &UnseenMaterials) -> Option<f32> {
    let pmg = files.read(&format!("{}.pmg", set.path.strip_suffix(".pmd")?)).ok()??;
    let mut shown = seen_pieces(files, &set.path, &pmg, 0, unseen)?;
    if let Some(parts) = set.variations.get(&variation) {
        let model = Pmg::read(&pmg)?;
        let mut in_variation = vec![false; shown.len()];
        for part in 0..model.part_count()? {
            let part = model.part(part)?;
            if part.name.is_some_and(|name| parts.contains(&name)) {
                let (first, end) = (part.pieces.start, part.pieces.end.min(in_variation.len()));
                if first < end {
                    in_variation[first..end].fill(true);
                }
            }
        }
        if in_variation.iter().any(|&piece| piece) {
            for (seen, in_variation) in shown.iter_mut().zip(in_variation) {
                *seen &= in_variation;
            }
        }
    }
    let triangles = shown_triangles(&pmg, Some(&shown))?;
    let top = triangles.iter().flatten().map(|corner| corner[1]).fold(f32::NEG_INFINITY, f32::max);
    (top >= LEAST_HEIGHT).then_some(top)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pmg::LOCATOR_SIZE;

    /// The current `.pmg` format's layout ([`crate::pmg`]), as the models made here are written: its first word, its
    /// box, its pieces' count and records, its parts' count and records, its locators.
    const PMG_MAGIC: u32 = crate::pmg::CURRENT;
    const BOX_AT: usize = 48;
    const PIECE_COUNT_AT: usize = 4;
    const PIECES_AT: usize = 84;
    const PIECE_SIZE: usize = 100;
    const PART_COUNT_AT: usize = 8;
    const PARTS_AT: usize = 76;
    const PART_SIZE: usize = 24;
    const LOCATORS_AT: usize = 80;

    /// `flats_close_01_be_lux.pmg`'s first 72 bytes (the base game's, 1.61): its box from (−8.0, −1.0, −2.0)
    /// to (10.7, 12.3, 4.7).
    fn flats_header() -> Vec<u8> {
        let words: [u32; 18] = [
            0x506d6715, 0x37, 0x0a, 0, 0, 1, 0, 0, 0x3e9482f0, 0x40b7db14, 0x3fe9d1e8, 0x414dd036, 0xc1002467,
            0xbf7e8349, 0xbffdf60d, 0x412adc61, 0x4144dda8, 0x4096c056,
        ];
        words.iter().flat_map(|word| word.to_le_bytes()).collect()
    }

    fn node(x: f32, z: f32, rotation: [f32; 4]) -> Node {
        Node { uid: 1, position: [x, 0.0, z], rotation, backward_item: 0, forward_item: 0, flags: 0 }
    }

    /// A `.pmg` of these triangles, one piece, its box theirs.
    fn pmg(triangles: &[[[f32; 3]; 3]]) -> Vec<u8> {
        let corners: Vec<[f32; 3]> = triangles.iter().flatten().copied().collect();
        let mut min = [f32::MAX; 3];
        let mut max = [f32::MIN; 3];
        for corner in &corners {
            for axis in 0..3 {
                min[axis] = min[axis].min(corner[axis]);
                max[axis] = max[axis].max(corner[axis]);
            }
        }
        let table = 112usize;
        let vertices_at = table + PIECE_SIZE;
        let indices_at = vertices_at + corners.len() * 12;
        let mut data = vec![0u8; indices_at + corners.len() * 2];
        fn put(data: &mut [u8], at: usize, value: u32) {
            data[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        put(&mut data, 0, PMG_MAGIC);
        put(&mut data, PIECE_COUNT_AT, 1);
        for axis in 0..3 {
            put(&mut data, BOX_AT + 4 * axis, min[axis].to_bits());
            put(&mut data, BOX_AT + 12 + 4 * axis, max[axis].to_bits());
        }
        put(&mut data, PIECES_AT, table as u32);
        put(&mut data, table, corners.len() as u32);
        put(&mut data, table + 4, corners.len() as u32);
        put(&mut data, table + 4 * 15, 12);
        put(&mut data, table + 4 * 16, vertices_at as u32);
        put(&mut data, table + 4 * 24, indices_at as u32);
        for (index, corner) in corners.iter().enumerate() {
            for (axis, value) in corner.iter().enumerate() {
                put(&mut data, vertices_at + index * 12 + axis * 4, value.to_bits());
            }
            data[indices_at + index * 2..indices_at + index * 2 + 2].copy_from_slice(&(index as u16).to_le_bytes());
        }
        data
    }

    /// A box's faces as triangles: its four sides, and its top and bottom if asked.
    fn box_faces(min: [f32; 3], max: [f32; 3], top: bool, bottom: bool) -> Vec<[[f32; 3]; 3]> {
        let [x0, y0, z0] = min;
        let [x1, y1, z1] = max;
        let quad = |a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3]| [[a, b, c], [a, c, d]];
        let mut faces = Vec::new();
        faces.extend(quad([x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0]));
        faces.extend(quad([x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]));
        faces.extend(quad([x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]));
        faces.extend(quad([x1, y0, z0], [x1, y0, z1], [x1, y1, z1], [x1, y1, z0]));
        if top {
            faces.extend(quad([x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1]));
        }
        if bottom {
            faces.extend(quad([x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]));
        }
        faces
    }

    fn area(part: &ShapePart) -> f32 {
        ring_area(&part.rings[0]) - part.rings.iter().skip(1).map(|ring| ring_area(ring)).sum::<f32>()
    }

    #[test]
    fn reads_each_parts_locators() {
        // Two parts, as the Scania PGR's interior has them: one locator, then two (its records after the
        // first part's), one of them turned a quarter about y.
        let (parts_at, locators_at) = (112usize, 160usize);
        let mut data = vec![0u8; locators_at + 3 * LOCATOR_SIZE];
        let put = |data: &mut Vec<u8>, at: usize, value: u32| data[at..at + 4].copy_from_slice(&value.to_le_bytes());
        put(&mut data, 0, PMG_MAGIC);
        put(&mut data, PART_COUNT_AT, 2);
        put(&mut data, PARTS_AT, parts_at as u32);
        put(&mut data, LOCATORS_AT, locators_at as u32);
        for (part, (count, first)) in [(1u32, 0u32), (2, 1)].into_iter().enumerate() {
            put(&mut data, parts_at + part * PART_SIZE + 16, count);
            put(&mut data, parts_at + part * PART_SIZE + 20, first);
        }
        let s = std::f32::consts::FRAC_1_SQRT_2;
        let turned = [s, 0.0, s, 0.0];
        let locators: [(&str, [f32; 3], [f32; 4]); 3] = [
            ("int_rearwall", [0.681, -0.025, 1.115], [1.0, 0.0, 0.0, 0.0]),
            ("doorpanel", [-0.157, -0.547, -0.288], [1.0, 0.0, 0.0, 0.0]),
            ("toyhang", [0.5, 0.2, -0.7], turned),
        ];
        for (index, (name, position, rotation)) in locators.iter().enumerate() {
            let at = locators_at + index * LOCATOR_SIZE;
            let token = crate::token::string_to_token(name).unwrap();
            put(&mut data, at, token as u32);
            put(&mut data, at + 4, (token >> 32) as u32);
            for (i, value) in position.iter().chain(&[1.0f32]).chain(rotation).enumerate() {
                put(&mut data, at + 8 + 4 * i, value.to_bits());
            }
        }
        let read = model_locators(&data).unwrap();
        let expected: Vec<Locator> = locators
            .iter()
            .map(|(name, position, rotation)| Locator {
                name: (*name).into(),
                position: *position,
                rotation: *rotation,
            })
            .collect();
        assert_eq!(read, expected);
        assert_eq!(model_locators(b"not a model"), None);
        // A record past the end: none.
        put(&mut data, parts_at + PART_SIZE + 16, 3);
        assert_eq!(model_locators(&data), None);
    }

    #[test]
    fn a_piece_or_triangle_that_doesnt_read_is_left_out_not_the_model() {
        // Two triangles in one piece, the second's last index past its vertices; and a piece whose stride is 0.
        let triangle = [[0.0, 0.0, 0.0], [1.0, 2.0, 0.0], [0.0, 3.0, 4.0]];
        let mut data = pmg(&[triangle, triangle]);
        let table = 112;
        let indices_at = u32::from_le_bytes(data[table + 96..table + 100].try_into().unwrap()) as usize;
        data[indices_at + 10..indices_at + 12].copy_from_slice(&99u16.to_le_bytes());
        assert_eq!(model_triangles(&data), Some(vec![triangle]), "the triangle that reads");
        // Its index count damaged to 4 billion: as far as the file holds them, never a walk of billions.
        data[table..table + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        let started = std::time::Instant::now();
        assert_eq!(model_triangles(&data), Some(vec![triangle]));
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        data[table + 60..table + 64].copy_from_slice(&0u32.to_le_bytes());
        assert_eq!(model_triangles(&data), Some(vec![]), "the piece left out, the model still one");
    }

    #[test]
    fn reads_a_models_box_and_triangles() {
        let model = model_box(&flats_header()).unwrap();
        assert!((model.min[0] + 8.009).abs() < 0.001 && (model.max[1] - 12.304).abs() < 0.001);
        assert!((model.max[2] - 4.711).abs() < 0.001);
        assert_eq!(model_box(b"not a model"), None);
        assert_eq!(model_box(&flats_header()[..60]), None);
        let triangle = [[0.0, 0.0, 0.0], [1.0, 2.0, 0.0], [0.0, 3.0, 4.0]];
        assert_eq!(model_triangles(&pmg(&[triangle])), Some(vec![triangle]));
        // Its pieces past the file's end (the header alone): none, the box kept.
        assert_eq!(model_triangles(&flats_header()), None);
        assert_eq!(model_shape(&flats_header()).unwrap().parts, vec![]);
    }

    #[test]
    fn a_house_stands_on_the_ground_its_outline_traced() {
        // Walls and a roof, 10 x 6 m, 8 m high, no floor: its middle under the roof is the house, not open.
        let shape = model_shape(&pmg(&box_faces([0.0, 0.0, 0.0], [10.0, 8.0, 6.0], true, false))).unwrap();
        assert_eq!(shape.parts.len(), 1, "{:?}", shape.parts);
        let part = &shape.parts[0];
        assert!(part.bottom == 0.0 && (part.top - 8.0).abs() < 0.01, "{part:?}");
        // Its outline the house's, give or take half a cell round it.
        assert!((60.0..=70.0).contains(&area(part)), "{}", area(part));
    }

    #[test]
    fn a_canopy_is_a_roof_over_open_ground_and_its_pillars_drop_out() {
        // A slab 20 x 8 m from 5.0 to 5.2 m up, on four pillars 0.4 m square.
        let mut faces = box_faces([0.0, 5.0, 0.0], [20.0, 5.2, 8.0], true, true);
        for (x, z) in [(1.0, 1.0), (18.6, 1.0), (1.0, 6.6), (18.6, 6.6)] {
            faces.extend(box_faces([x, 0.0, z], [x + 0.4, 5.0, z + 0.4], false, false));
        }
        let shape = model_shape(&pmg(&faces)).unwrap();
        let roof = shape.parts.iter().max_by(|a, b| area(a).total_cmp(&area(b))).unwrap();
        assert!((roof.bottom - 5.0).abs() < 0.01 && (roof.top - 5.2).abs() < 0.01, "{roof:?}");
        assert!(area(roof) > 140.0, "{}", area(roof));
        // Nothing from the ground but slivers, and those left out.
        assert!(shape.parts.iter().all(|part| part.bottom > 0.0), "{:?}", shape.parts);
        // Placed: the roof is the kind left out while the truck is under it, whatever its model's folder.
        let placed = place(&shape, &node(0.0, 0.0, [1.0, 0.0, 0.0, 0.0]), [1.0; 3], BuildingKind::Building, 7);
        assert!(placed.iter().all(|part| part.kind == BuildingKind::Canopy));
        // And a house filed as a filling station's (its shop) stands as a building.
        let house = model_shape(&pmg(&box_faces([0.0, 0.0, 0.0], [10.0, 8.0, 6.0], true, false))).unwrap();
        let placed = place(&house, &node(0.0, 0.0, [1.0, 0.0, 0.0, 0.0]), [1.0; 3], BuildingKind::Canopy, 7);
        assert!(placed.iter().all(|part| part.kind == BuildingKind::Building && part.base == 0.0));
    }

    #[test]
    fn a_buildings_top_is_as_high_as_it_gets_all_along() {
        // A wall 40 m long and 2 m thick, its top falling from 20 m to 4 m: a cable's fall, an arch's side.
        let corner = |x: f32, y: f32, z: f32| [x, y, z];
        let side = |z: f32| {
            let (a, b, c, d) = (corner(0.0, 0.0, z), corner(40.0, 0.0, z), corner(40.0, 4.0, z), corner(0.0, 20.0, z));
            [[a, b, c], [a, c, d]]
        };
        let (near, far) = (side(0.0), side(2.0));
        let top = [
            [corner(0.0, 20.0, 0.0), corner(40.0, 4.0, 0.0), corner(40.0, 4.0, 2.0)],
            [corner(0.0, 20.0, 0.0), corner(40.0, 4.0, 2.0), corner(0.0, 20.0, 2.0)],
        ];
        let triangles: Vec<[[f32; 3]; 3]> = near.into_iter().chain(far).chain(top).collect();
        let model_box = ModelBox { min: [0.0, 0.0, 0.0], max: [40.0, 20.0, 2.0] };
        // A building's stays as high as it gets, all along.
        let flat = trace(&model_box, &triangles, Ground::Level);
        assert!(flat.iter().all(|part| part.top > 17.0), "{flat:?}");
    }

    #[test]
    fn a_part_left_out_leaves_no_hole_in_what_s_round_it() {
        // A roof from 7 to 8 m, 30 m a side, on a column too small to be a part, and a courtyard open to the sky in a
        // block round it.
        let mut faces = box_faces([0.0, 7.0, 0.0], [30.0, 8.0, 30.0], true, true);
        faces.extend(box_faces([14.0, 0.0, 14.0], [16.0, 8.0, 16.0], true, false));
        let shape = model_shape(&pmg(&faces)).unwrap();
        let roof = shape.parts.iter().find(|part| part.bottom > 0.0).expect("the roof");
        assert_eq!(roof.rings.len(), 1, "{roof:?}");
        let mut faces = Vec::new();
        for (from, to) in [([0.0, 0.0], [40.0, 10.0]), ([0.0, 30.0], [40.0, 40.0]), ([0.0, 10.0], [10.0, 30.0]), ([30.0, 10.0], [40.0, 30.0])] {
            faces.extend(box_faces([from[0], 0.0, from[1]], [to[0], 8.0, to[1]], true, false));
        }
        let shape = model_shape(&pmg(&faces)).unwrap();
        assert_eq!(shape.parts.iter().map(|part| part.rings.len()).max(), Some(2), "{:?}", shape.parts);
    }

    #[test]
    fn a_tower_and_its_sphere_are_parts_of_their_own() {
        // A shaft 4 x 4 m up to 100 m, and round it from 80 to 90 m a 12 x 12 m pod (closed: top and bottom).
        let mut faces = box_faces([4.0, 0.0, 4.0], [8.0, 100.0, 8.0], true, false);
        faces.extend(box_faces([0.0, 80.0, 0.0], [12.0, 90.0, 12.0], true, true));
        let shape = model_shape(&pmg(&faces)).unwrap();
        let shaft = shape.parts.iter().find(|part| (part.top - 100.0).abs() < 0.01).expect("the shaft");
        assert_eq!(shaft.bottom, 0.0, "{:?}", shape.parts);
        let pod = shape.parts.iter().find(|part| (part.top - 90.0).abs() < 0.01).expect("the pod");
        // Its underside, to a height step (100 m in 64).
        assert!((pod.bottom - 80.0).abs() < 1.6, "{pod:?}");
        // One pod, not a rim round a flat middle.
        assert_eq!(shape.parts.iter().filter(|part| (part.top - 90.0).abs() < 0.01).count(), 1, "{:?}", shape.parts);
        assert!(area(pod) > 100.0 && area(shaft) < 40.0, "{} {}", area(pod), area(shaft));
    }

    /// The bounds of a building's outline.
    fn bounds(building: &Building) -> [f32; 4] {
        building.rings[0].iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |b, p| {
            [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])]
        })
    }

    #[test]
    fn stands_a_building_where_the_game_does() {
        // Placed in Berlin (sector data, 1.61): its node, its rotation, and the game's own bounds of it.
        let shape = ModelShape { model_box: model_box(&flats_header()).unwrap(), parts: Vec::new() };
        let cases = [
            ([-15982.09, 4004.8906], [0.9267056, 0.0, 0.3757882, 0.0], [-15988.992, 3997.7578, -15973.992, 4012.7422]),
            ([-15960.613, 4013.043], [0.38796425, 0.0, -0.9216745, 0.0], [-15968.617, 4005.1016, -15953.816, 4022.168]),
            (
                [-16979.719, 202.21094],
                [0.5787551, 0.0, -0.81550145, 0.0],
                [-16985.445, 193.60547, -16975.262, 210.57813],
            ),
        ];
        for (at, rotation, game) in cases {
            let placed = place(&shape, &node(at[0], at[1], rotation), [1.0; 3], BuildingKind::Building, 7);
            let ours = bounds(&placed[0]);
            let off = ours.iter().zip(game).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
            assert!(off < 3.0, "{ours:?} against the game's {game:?}");
            assert!((placed[0].height - 12.304).abs() < 0.001 && placed[0].base == 0.0);
        }
    }

    #[test]
    fn a_shape_is_placed_scaled_and_turned_and_a_prop_is_no_building() {
        let shape = model_shape(&pmg(&box_faces([0.0, 0.0, 0.0], [10.0, 8.0, 6.0], true, false))).unwrap();
        // Turned a quarter (yaw 90°: x along -z), twice as high.
        let turn = std::f32::consts::FRAC_PI_4;
        let placed = place(
            &shape,
            &node(100.0, 200.0, [turn.cos(), 0.0, turn.sin(), 0.0]),
            [1.0, 2.0, 1.0],
            BuildingKind::Building,
            7,
        );
        assert_eq!(placed.len(), 1);
        assert!((placed[0].height - 16.0).abs() < 0.01);
        let b = bounds(&placed[0]);
        // Its 10 m along x now along z, going north (−z) from the node; its 6 m along x.
        assert!((b[2] - b[0] - 6.5).abs() < 1.0 && (b[3] - b[1] - 10.5).abs() < 1.0, "{b:?}");
        // Scaled down to a charger's size: none.
        assert!(
            place(&shape, &node(0.0, 0.0, [1.0, 0.0, 0.0, 0.0]), [0.05, 1.0, 0.05], BuildingKind::Building, 7)
                .is_empty()
        );
        // A canopy whose triangles weren't read: its box's top as a slab.
        let model_box = ModelBox { min: [0.0; 3], max: [20.0, 5.0, 8.0] };
        let canopy = place(
            &ModelShape { model_box, parts: Vec::new() },
            &node(0.0, 0.0, [1.0, 0.0, 0.0, 0.0]),
            [1.0; 3],
            BuildingKind::Canopy,
            7,
        );
        assert!((canopy[0].base - (5.0 - CANOPY_THICKNESS)).abs() < 0.001);
    }

    #[test]
    fn knows_a_collision_mesh_and_a_shadow_from_what_is_drawn() {
        assert!(unseen_material("material : \"eut2.none\" {\n}"));
        assert!(unseen_material("effect : \"eut2.none.rfx\" {\n\tmapping : 1\n}"));
        assert!(unseen_material("effect : \"eut2.shadowonly.rfx\" {\n}"));
        assert!(!unseen_material(
            "effect : \"eut2.dif.spec.rfx\" {\n\tsource : \"/material/tiled/bricks_gs_01.tobj\"\n}"
        ));
        assert!(!unseen_material("effect : \"eut2.nonesuch.rfx\" {\n}"));
        assert!(!unseen_material(""));
    }

    #[test]
    fn a_variant_shows_only_its_parts_pieces() {
        // A `.pmg` of three pieces in two parts (the first piece, then the other two), and its `.pmd`: two
        // variants, `default` showing both parts, `winter` hiding the second (its `visible` attribute 0).
        // Its pieces' records (their contents unread here) after the parts', within the file as a real model's are.
        let mut pmg = vec![0u8; 160 + 3 * PIECE_SIZE];
        let put = |data: &mut Vec<u8>, at: usize, value: u32| data[at..at + 4].copy_from_slice(&value.to_le_bytes());
        put(&mut pmg, 0, PMG_MAGIC);
        put(&mut pmg, PIECE_COUNT_AT, 3);
        put(&mut pmg, PIECES_AT, 160);
        put(&mut pmg, PART_COUNT_AT, 2);
        put(&mut pmg, PARTS_AT, 100);
        for (part, (count, first)) in [(1, 0), (2, 1)].into_iter().enumerate() {
            put(&mut pmg, 100 + PART_SIZE * part + 8, count);
            put(&mut pmg, 100 + PART_SIZE * part + 12, first);
        }
        let token = |name: &str| crate::token::string_to_token(name).unwrap();
        let mut pmd = vec![0u8; 200];
        let put_long =
            |data: &mut Vec<u8>, at: usize, value: u64| data[at..at + 8].copy_from_slice(&value.to_le_bytes());
        // Version, materials, looks, pieces, variants, parts, attributes, a variant's values' bytes; then where
        // the variants' names, the parts' attribute ranges, the values and the attributes are.
        for (index, value) in [4, 0, 0, 0, 2, 2, 2, 8, 0, 0, 64, 80, 96, 112].into_iter().enumerate() {
            put(&mut pmd, 4 * index, value);
        }
        put_long(&mut pmd, 64, token("default"));
        put_long(&mut pmd, 72, token("winter"));
        // Each part its own attribute: the first part's is attribute 0, the second's attribute 1.
        for (at, value) in [(80, 0), (84, 1), (88, 1), (92, 2)] {
            put(&mut pmd, at, value);
        }
        // The values, by variant: both visible, then the second hidden.
        for (at, value) in [(96, 1), (100, 1), (104, 1), (108, 0)] {
            put(&mut pmd, at, value);
        }
        // The attributes: both `visible`, at offsets 0 and 4 of a variant's values.
        for (attribute, offset) in [(0, 0), (1, 4)] {
            put_long(&mut pmd, 112 + 16 * attribute, token("visible"));
            put(&mut pmd, 112 + 16 * attribute + 12, offset);
        }
        assert_eq!(visible_pieces(&pmd, &pmg, token("default")), Some(vec![true, true, true]));
        assert_eq!(visible_pieces(&pmd, &pmg, token("winter")), Some(vec![true, false, false]));
        // A variant it hasn't, or a descriptor that isn't one: none said (all drawn).
        assert_eq!(visible_pieces(&pmd, &pmg, token("summer")), None);
        assert_eq!(visible_pieces(b"not a model", &pmg, token("default")), None);
    }

    /// A box's faces (`box_faces`) with its bottom raised over a ground that slopes along x.
    fn slope(x: f32) -> f32 {
        3.0 + 0.05 * x
    }

    #[test]
    fn a_prefab_stands_its_building_on_its_own_ground_and_leaves_its_yard_and_fence_out() {
        // A yard 40 m square sloping from 3 m up to 5 m (its own ground, above the prefab's 0), a building on
        // it (its walls from a floor a metre over the ground, 9 m up), a 2 m fence round the yard, and a wall as
        // high as the building from the fence to it; the road in at its west edge, 3 m up.
        let ground = |x: f32, z: f32| [x, slope(x), z];
        let mut triangles = vec![
            [ground(0.0, 0.0), ground(40.0, 0.0), ground(40.0, 40.0)],
            [ground(0.0, 0.0), ground(40.0, 40.0), ground(0.0, 40.0)],
        ];
        let floor = slope(15.0) + 1.0;
        triangles.extend(box_faces([10.0, floor, 10.0], [20.0, floor + 8.0, 20.0], true, false));
        let fences = [
            ([2.0, 2.0], [38.0, 2.2]),
            ([2.0, 37.8], [38.0, 38.0]),
            ([2.0, 2.0], [2.2, 38.0]),
            ([37.8, 2.0], [38.0, 38.0]),
        ];
        for (min, max) in fences {
            let base = slope(min[0]);
            triangles.extend(box_faces([min[0], base, min[1]], [max[0], base + 2.0, max[1]], true, false));
        }
        triangles.extend(box_faces([2.2, slope(2.2), 15.0], [10.0, floor + 8.0, 15.2], true, false));
        let model_box = ModelBox { min: [0.0, 3.0, 0.0], max: [40.0, floor + 8.0, 40.0] };
        let parts = trace(&model_box, &triangles, Ground::Own([0.0, 3.0, 20.0]));
        assert_eq!(parts.len(), 1, "{parts:?}");
        let building = &parts[0];
        // From its ground (where its walls meet the slope, give or take its ground's rise across it), 9 m up.
        assert_eq!(building.bottom, 0.0);
        assert!((8.5..=10.0).contains(&building.top), "{building:?}");
        assert!((90.0..=130.0).contains(&area(building)), "{}", area(building));
        // The wall that runs into it isn't part of it.
        let west = building.rings[0].iter().map(|p| p[0]).fold(f32::MAX, f32::min);
        assert!(west > 9.0, "{building:?}");
    }

    #[test]
    fn a_pump_islands_roof_with_no_yard_of_its_own_is_over_the_road() {
        // A roof 16 x 14 m from 5.5 to 7 m up on four pillars, a kerbed island under it, and no ground of its
        // own: measured from the road's height.
        let mut triangles = box_faces([0.0, 5.5, 0.0], [16.0, 7.0, 14.0], true, true);
        triangles.extend(box_faces([2.0, 0.0, 6.0], [14.0, 0.2, 8.0], true, false));
        for (x, z) in [(1.0, 1.0), (14.6, 1.0), (1.0, 12.6), (14.6, 12.6)] {
            triangles.extend(box_faces([x, 0.0, z], [x + 0.4, 5.5, z + 0.4], false, false));
        }
        let model_box = ModelBox { min: [0.0, 0.0, 0.0], max: [16.0, 7.0, 14.0] };
        let parts = trace(&model_box, &triangles, Ground::Own([8.0, 0.0, -10.0]));
        let roof = parts.iter().max_by(|a, b| area(a).total_cmp(&area(b))).expect("the roof");
        assert!(roof.bottom >= 5.0 && (roof.top - 7.0).abs() < 0.6, "{roof:?}");
        assert!(area(roof) > 180.0, "{}", area(roof));
    }

    #[test]
    fn a_hall_open_on_one_side_is_a_building_not_a_roof() {
        // Walls on three sides of a 12 x 8 m hall, 6 m high, its roof over its floor; open to the south.
        let mut triangles = box_faces([0.0, 0.0, 0.0], [12.0, 6.0, 8.0], true, false);
        let open_side = [[0.0, 0.0, 8.0], [12.0, 0.0, 8.0], [12.0, 6.0, 8.0]];
        triangles.retain(|triangle| !triangle.iter().all(|corner| corner[2] == 8.0) || triangle == &open_side);
        triangles.retain(|triangle| triangle != &open_side);
        let model_box = ModelBox { min: [0.0; 3], max: [12.0, 6.0, 8.0] };
        let parts = trace(&model_box, &triangles, Ground::Level);
        let hall = parts.iter().max_by(|a, b| area(a).total_cmp(&area(b))).expect("the hall");
        assert_eq!(hall.bottom, 0.0, "{parts:?}");
        assert!(area(hall) > 90.0, "{}", area(hall));
        assert!(parts.iter().all(|part| part.bottom == 0.0), "{parts:?}");
    }

    #[test]
    fn whats_narrower_than_the_opening_goes_and_the_rest_stays() {
        // A 6 x 6 block and a line 1 cell wide off it, on a 10 x 12 grid.
        let (rows, cols) = (10, 12);
        let mut mask = vec![false; rows * cols];
        for row in 2..8 {
            for col in 2..8 {
                mask[row * cols + col] = true;
            }
        }
        for col in 8..12 {
            mask[4 * cols + col] = true;
        }
        let kept = opened(&mask, rows, cols, 1);
        assert!((2..8).all(|row| (2..8).all(|col| kept[row * cols + col])));
        assert!((9..12).all(|col| !kept[4 * cols + col]));
        assert_eq!(kept.iter().filter(|&&k| k).count(), 36);
    }

    #[test]
    fn walls_round_a_building_close_and_a_corner_left_out_is_put_back() {
        // Round a 60 x 30 m hall: closed on itself, a corner piece's metre short.
        let square = [[0.0, 0.0], [60.0, 0.0], [60.0, 30.0], [0.0, 30.0], [0.0, 1.0]];
        assert_eq!(close_run(&square, 4), Some(square[..4].to_vec()));
        // Three sides and a part of the fourth (its north-east corner where it meets another building): the
        // corner put back where its last wall carried on meets its first carried back.
        let three = [[60.0, 10.0], [60.0, 0.0], [0.0, 0.0], [0.0, 30.0], [40.0, 30.0]];
        let ring = close_run(&three, 3).expect("closed");
        assert_eq!(ring.last(), Some(&[60.0, 30.0]));
        // Two walls alone (a corner of facades): nothing says how far it reaches.
        assert_eq!(close_run(&[[0.0, 30.0], [0.0, 0.0], [60.0, 0.0]], 2), None);
        // A wall alone: none either.
        assert_eq!(close_run(&[[0.0, 0.0], [60.0, 0.0]], 1), None);
    }

    #[test]
    fn a_structure_sets_closed_walls_are_a_building_as_high_as_its_walls() {
        let walls = [
            vec![[0.0, 0.0], [60.0, 0.0]],
            vec![[60.0, 0.5], [60.0, 30.0]],
            vec![[60.0, 30.0], [0.0, 30.0]],
            vec![[0.0, 30.0], [0.0, 0.0]],
            // A wall by itself, nowhere near: no building.
            vec![[200.0, 0.0], [260.0, 0.0]],
        ];
        let heights = [
            (1, 12.0, [0.0, 0.0], 0.0),
            (2, 12.0, [60.0, 0.5], 0.0),
            (3, 8.0, [60.0, 30.0], 0.0),
            (4, 12.0, [0.0, 30.0], 0.0),
            (5, 12.0, [200.0, 0.0], 0.0),
        ];
        let buildings = set_buildings(&walls, &heights, [-1000.0, -1000.0, 1000.0, 1000.0]);
        assert_eq!(buildings.len(), 1, "{buildings:?}");
        let hall = &buildings[0];
        assert_eq!((hall.base, hall.height, hall.kind), (0.0, 12.0, BuildingKind::Building));
        assert!((ring_area(&hall.rings[0]) - 1800.0).abs() < 40.0, "{}", ring_area(&hall.rings[0]));
        // Outside the bounds asked for (by its first wall's start): none.
        assert!(set_buildings(&walls, &heights, [100.0, 100.0, 200.0, 200.0]).is_empty());
    }
}
