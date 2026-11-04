//! The surfaces' tiles: a block of them worked out at once from the shapes reaching it, the deepest zoom's outlines
//! joined per kind and cut to the tile. Further-out zooms have only what's still shown from there (water in models), each
//! made from the one before.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use crate::area::METRES_PER_DEGREE;
use crate::tiles::{bytes_field, command, push_point, varint_field};
use crate::world::RoadClass;

use super::ground::{Painted, Quad};
use super::levels::{Face, FaceIndex};
use super::shapes::{self, Ring, Spot};
use super::{CLAIM_REACH, Kind, LANE_HALF, SURFACE_FAR_ZOOM, SURFACE_ZOOM, TRACK_HALF, quad_near, raised, ring_box};

/// Tile units across, as the page's map keeps them.
pub const EXTENT: u32 = 8192;
/// Outlines are joined on a grid this much finer.
const FINE: i32 = 8;
/// How far shapes reach past a tile's edge, metres at the deepest zoom.
pub const REACH: f64 = 20.0;
/// How far an outline may stray when thinned, tile units.
const THIN: f64 = 0.25;
/// From further out, how far, tile units.
const THIN_FAR: f64 = 0.5;
/// A rings' smallest area from further out, tile units squared: under this nobody sees it.
const SMALLEST_FAR: i64 = 4;
/// Ground this much higher than paving lies over it, m: the game lays lawns 15 cm over the paving it leaves under them.
const LAID_OVER: f64 = 0.02;
/// Paving is cut on a grid this fine, units a metre.
const CUT_GRID: f64 = 32.0;
/// The most rings an outline may have with its holes: MapLibre fills one with its 500 largest and no more (its
/// EARCUT_MAX_RINGS), and the rest come out filled, the pool in a square with them.
const MOST_RINGS: usize = 400;

/// A point in Web Mercator world units (0..1).
pub fn mercator(x: f64, z: f64) -> [f64; 2] {
    let lat = (-z / METRES_PER_DEGREE).to_radians();
    [
        (x / METRES_PER_DEGREE + 180.0) / 360.0,
        0.5 - (std::f64::consts::FRAC_PI_4 + lat / 2.0).tan().ln() / (2.0 * std::f64::consts::PI),
    ]
}

/// Where a point of [`mercator`]'s is on the ground: x, z.
pub fn ground_of(x: f64, y: f64) -> [f64; 2] {
    let lat = (std::f64::consts::PI * (1.0 - 2.0 * y)).sinh().atan().to_degrees();
    [(x * 360.0 - 180.0) * METRES_PER_DEGREE, -lat * METRES_PER_DEGREE]
}

/// The tile a point of the ground is in.
pub fn tile_of(x: f64, z: f64, zoom: u8) -> [u32; 2] {
    let [mx, my] = mercator(x, z);
    let scale = (1u64 << zoom) as f64;
    [(mx * scale).floor().max(0.0) as u32, (my * scale).floor().max(0.0) as u32]
}

/// The ground a tile covers, min x, min z, max x, max z, as [`tile_of`] shares ground out to tiles.
pub fn tile_box(x: u32, y: u32, zoom: u8) -> [f64; 4] {
    let span = 1u32 << (SURFACE_ZOOM - zoom);
    let (grid, side) = (TileGrid::new(x * span, y * span), f64::from(span) * f64::from(EXTENT) * f64::from(FINE));
    let ([x0, z0], [x1, z1]) = (grid.ground(0.0, 0.0), grid.ground(side, side));
    [x0, z0, x1, z1]
}

/// A shape on the ground: what it is, whether it's off the game's own map, and its outline's points (x, y, z). `raw`: an
/// outline drawn directly (a road's band, a quad), which may turn either way; otherwise it comes from a joined outline,
/// and a hole turns the other way from its outline.
pub struct Shape {
    pub kind: Kind,
    pub near: bool,
    pub raw: bool,
    pub ring: Vec<[f64; 3]>,
}

/// A stretch of lane with no surface of its prefab's own under it. What's painted paved or bare within reach of it is
/// its road; between two shown roads (`band`) it's road itself, whatever's under it (the arm of a crossing with only its
/// asphalt in the model, a drawbridge's moving deck, a rail crossing's tracks).
pub struct Claim {
    pub line: Vec<[f64; 3]>,
    pub class: RoadClass,
    /// Off the game's own map.
    pub near: bool,
    pub band: bool,
    pub tunnel: bool,
}

/// Everything reaching one block of tiles.
pub struct Block {
    tile: (u32, u32),
    zoom: u8,
    shapes: Vec<Shape>,
    /// Painted ground: quiet where it's paved, road where lanes run through it.
    pub quads: Vec<Quad>,
    /// Ground that may lie over the paving.
    pub over: Vec<Quad>,
    /// Lanes with no surface of their own.
    pub claims: Vec<Claim>,
    /// The faces of what may pass over what, with each one's level (none where nothing does).
    pub faces: Vec<Face>,
    pub levels: Vec<u8>,
    /// Which of its shapes and of its faces are of what rises: the roads drawn at their heights (raised.rs).
    pub raised_shapes: HashSet<u32>,
    pub raised_faces: HashSet<u32>,
    /// Lanes' points to check what's drawn under (an audit): what they are, and where.
    pub probes: Option<Vec<(u32, [f64; 2])>>,
}

/// What's in a tile: outlines by level, what they are, whether they show only close up and whether they're of what's drawn
/// at its height, each joined and cut.
type Key = (u8, Kind, bool, bool);

#[derive(Default)]
struct Layer {
    surfaces: BTreeMap<Key, Vec<Vec<Ring>>>,
    /// The true edges of raised roads, by level, whether they show only close up and whether they're of what's drawn at its
    /// height.
    edges: BTreeMap<(u8, bool, bool), Vec<Vec<Spot>>>,
}

impl Layer {
    fn is_empty(&self) -> bool {
        self.surfaces.is_empty() && self.edges.is_empty()
    }
}

impl Block {
    pub fn new(tile: (u32, u32), zoom: u8) -> Self {
        Self {
            tile,
            zoom,
            shapes: Vec::new(),
            quads: Vec::new(),
            over: Vec::new(),
            claims: Vec::new(),
            faces: Vec::new(),
            levels: Vec::new(),
            raised_shapes: HashSet::new(),
            raised_faces: HashSet::new(),
            probes: None,
        }
    }

    pub fn add(&mut self, shape: Shape) {
        if shape.ring.len() >= 3 {
            self.shapes.push(shape);
        }
    }

    /// How many shapes it has so far.
    pub fn shape_count(&self) -> usize {
        self.shapes.len()
    }

    /// The paving with what lies over it taken out, added to its shapes: each quad's own, by where they are in the shapes.
    pub fn uncover(&mut self) -> Vec<std::ops::Range<usize>> {
        let over = QuadIndex::new(&self.over);
        let mut spans = Vec::new();
        for quad in &self.quads {
            if quad.paint == Painted::Paved {
                let from = self.shapes.len();
                self.shapes.extend(uncovered(quad, &self.over, &over));
                spans.push(from..self.shapes.len());
            }
        }
        spans
    }

    /// The road surfaces in `span` of its shapes, in pieces: an outline drawn directly on its own, others with their holes.
    pub fn road_surface(&self, span: std::ops::Range<usize>) -> Vec<Vec<Vec<[f64; 3]>>> {
        let mut pieces = Vec::new();
        let mut own: BTreeMap<Kind, Vec<Vec<[f64; 3]>>> = BTreeMap::new();
        for shape in self.shapes[span].iter().filter(|shape| matches!(shape.kind, Kind::Road(_))) {
            if shape.raw {
                pieces.push(vec![shape.ring.clone()]);
            } else {
                own.entry(shape.kind).or_default().push(shape.ring.clone());
            }
        }
        pieces.extend(own.into_values());
        pieces
    }

    /// The quiet ground of each of `spans` of its shapes, in pieces: an outline drawn directly on its own, others with
    /// their holes.
    pub fn quiet_pieces(&self, spans: &[std::ops::Range<usize>]) -> Vec<Vec<Vec<[f64; 3]>>> {
        let mut pieces = Vec::new();
        for span in spans {
            let mut own = Vec::new();
            for shape in self.shapes[span.clone()].iter().filter(|shape| shape.kind == Kind::Quiet && !shape.near) {
                if shape.raw {
                    pieces.push(vec![shape.ring.clone()]);
                } else {
                    own.push(shape.ring.clone());
                }
            }
            if !own.is_empty() {
                pieces.push(own);
            }
        }
        pieces
    }

    /// The cells with paving in them.
    pub fn paving(&self) -> Paving {
        let mut paving = Paving::default();
        for quad in self.quads.iter().filter(|quad| quad.paint == Painted::Paved) {
            let [x0, z0, x1, z1] = ring_box(&quad.corners);
            for cell in cells(x0, z0, x1, z1) {
                paving.cells.insert(cell);
            }
        }
        paving
    }

    /// Writes this block's tiles under `dir`: how many, and their bytes. `seen` is told what's drawn under each probe.
    pub fn write(self, dir: &Path, seen: &mut dyn FnMut(u32, [f64; 2], Option<Kind>)) -> std::io::Result<(usize, u64)> {
        let levels = SURFACE_ZOOM - self.zoom;
        let span = 1u32 << levels;
        let (base_x, base_y) = (self.tile.0 << levels, self.tile.1 << levels);
        let mut in_tile: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
        let tiles_of = |[x0, z0, x1, z1]: [f64; 4]| {
            let [tx0, ty0] = tile_of(x0 - REACH, z0 - REACH, SURFACE_ZOOM);
            let [tx1, ty1] = tile_of(x1 + REACH, z1 + REACH, SURFACE_ZOOM);
            (tx0.max(base_x)..=tx1.min(base_x + span - 1))
                .flat_map(move |x| (ty0.max(base_y)..=ty1.min(base_y + span - 1)).map(move |y| (x, y)))
        };
        for (index, shape) in self.shapes.iter().enumerate() {
            for tile in tiles_of(ring_box(&shape.ring)) {
                in_tile.entry(tile).or_default().push(index as u32);
            }
        }
        // Raised faces and lines, by tile.
        let mut faces_in: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
        for (index, face) in self.faces.iter().enumerate() {
            if self.levels[index] > 0 {
                for tile in tiles_of(ring_box(&face.t)) {
                    faces_in.entry(tile).or_default().push(index as u32);
                    in_tile.entry(tile).or_default();
                }
            }
        }
        let face_index = FaceIndex::new(&self.faces);
        let quad_index = QuadIndex::new(&self.quads);
        let mut probes_in: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
        for (index, (_, at)) in self.probes.iter().flatten().enumerate() {
            let [x, y] = tile_of(at[0], at[1], SURFACE_ZOOM);
            if (base_x..base_x + span).contains(&x) && (base_y..base_y + span).contains(&y) {
                probes_in.entry((x, y)).or_default().push(index as u32);
            }
        }
        let mut written = 0;
        let mut bytes = 0u64;
        let mut level: HashMap<(u32, u32), Layer> = HashMap::new();
        for ((x, y), list) in &in_tile {
            let upper = faces_in.get(&(*x, *y)).map_or(&[][..], Vec::as_slice);
            let layer = self.deepest(*x, *y, list, upper, &face_index, &quad_index);
            if let (Some(probes), Some(here)) = (&self.probes, probes_in.get(&(*x, *y))) {
                check_probes(&layer, &TileGrid::new(*x, *y), probes, here, seen);
            }
            if layer.is_empty() {
                continue;
            }
            let encoded = encode(&layer, f64::from(FINE), THIN * f64::from(FINE), 0, raised_file(*x, *y));
            bytes += put(dir, SURFACE_ZOOM, *x, *y, &encoded)?;
            written += 1;
            level.insert((*x, *y), layer);
        }
        // Further out: only what shows from there, each tile from the four inside it.
        for zoom in (SURFACE_FAR_ZOOM..SURFACE_ZOOM).rev() {
            let mut parents: HashMap<(u32, u32), Vec<Child>> = HashMap::new();
            for (tile, layer) in level {
                parents.entry((tile.0 / 2, tile.1 / 2)).or_default().push((tile, layer));
            }
            level = HashMap::new();
            for (tile, children) in parents {
                let layer = merged(tile, &children);
                if layer.is_empty() {
                    continue;
                }
                let encoded = encode(&layer, f64::from(FINE), THIN_FAR * f64::from(FINE), SMALLEST_FAR, 0);
                bytes += put(dir, zoom, tile.0, tile.1, &encoded)?;
                written += 1;
                level.insert(tile, layer);
            }
        }
        Ok((written, bytes))
    }

    /// A tile at the deepest zoom.
    fn deepest(&self, x: u32, y: u32, list: &[u32], upper: &[u32], index: &FaceIndex, quads: &QuadIndex) -> Layer {
        let to_tile = TileGrid::new(x, y);
        let mut groups: BTreeMap<Key, Vec<Ring>> = BTreeMap::new();
        // What rises, of what's drawn at its height: its road, and the ground that goes with it.
        let at_height = |kind: Kind| matches!(kind, Kind::Road(_) | Kind::Quiet | Kind::Rail);
        for &index in list {
            let shape = &self.shapes[index as usize];
            let ring: Ring = shape.ring.iter().map(|p| to_tile.spot(p[0], p[2])).collect();
            let ring = if shape.raw { shapes::outward(ring) } else { ring };
            let raised = at_height(shape.kind) && self.raised_shapes.contains(&index);
            groups.entry((0, shape.kind, shape.near, raised)).or_default().push(ring);
        }
        for &face in upper {
            let raised = self.raised_faces.contains(&face);
            let (level, face) = (self.levels[face as usize], &self.faces[face as usize]);
            let ring: Ring = face.t.iter().map(|p| to_tile.spot(p[0], p[2])).collect();
            if shapes::twice_area(&ring) != 0 {
                groups.entry((level, face.kind, face.near, raised && at_height(face.kind))).or_default().push(shapes::outward(ring));
            }
        }
        self.claimed(&to_tile, quads, &mut groups);
        // The quiet ground goes on under the roads, so nothing shows between them.
        for risen in [false, true] {
            let under: Vec<Ring> = groups
                .iter()
                .filter(|((level, kind, _, raised), _)| *level == 0 && matches!(kind, Kind::Road(_)) && *raised == risen)
                .flat_map(|(_, rings)| rings.iter().cloned())
                .collect();
            if !under.is_empty() {
                groups.entry((0, Kind::Quiet, false, risen)).or_default().extend(under);
            }
        }
        let reach = (f64::from(EXTENT) / 64.0) as i32 * FINE;
        let side = EXTENT as i32 * FINE;
        let clip = vec![shapes::rectangle(-reach, -reach, side + reach, side + reach)];
        let mut layer = Layer::default();
        for (key, rings) in groups {
            let parts = cut_up(shapes::intersect_shapes(&rings, &clip));
            if !parts.is_empty() {
                layer.surfaces.insert(key, parts);
            }
        }
        for (key, parts) in &layer.surfaces {
            if key.0 > 0 && matches!(key.1, Kind::Road(_)) {
                let edges = true_edges(&to_tile, parts, index, side, reach);
                layer.edges.entry((key.0, key.2, key.3)).or_default().extend(edges);
            }
        }
        layer
    }

    /// The painted ground lanes with no surface of their own run through: road, round them.
    fn claimed(&self, to_tile: &TileGrid, index: &QuadIndex, groups: &mut BTreeMap<Key, Vec<Ring>>) {
        let scale = to_tile.units_per_metre;
        let (x0, z0, x1, z1) = to_tile.world_box(CLAIM_REACH + REACH);
        let mut by_class: BTreeMap<(RoadClass, bool, bool), Vec<&Claim>> = BTreeMap::new();
        for claim in &self.claims {
            if claim.line.iter().any(|p| p[0] >= x0 && p[0] <= x1 && p[2] >= z0 && p[2] <= z1) {
                by_class.entry((claim.class, claim.near, claim.tunnel)).or_default().push(claim);
            }
        }
        let on_tile = |line: &[[f64; 3]]| -> Vec<[f64; 2]> {
            line.iter()
                .map(|p| {
                    let s = to_tile.floating(p[0], p[2]);
                    [s[0] / scale, s[1] / scale]
                })
                .collect()
        };
        for ((class, near, tunnel), claims) in by_class {
            let kind = if tunnel { Kind::Tunnel(class) } else { Kind::of(class) };
            let bands: Vec<Vec<[f64; 2]>> = claims.iter().filter(|claim| claim.band).map(|claim| on_tile(&claim.line)).collect();
            let half = if class == RoadClass::Rail { TRACK_HALF } else { LANE_HALF };
            let band = shapes::around_lines(&bands, half, scale);
            if !band.is_empty() {
                groups.entry((0, kind, near, false)).or_default().extend(band);
            }
            if class == RoadClass::Rail {
                continue;
            }
            let quads = index.under(&self.quads, claims.iter().flat_map(|claim| claim.line.iter()), CLAIM_REACH);
            if quads.is_empty() {
                continue;
            }
            let ground: Vec<Ring> = quads
                .iter()
                .map(|&quad| shapes::outward(self.quads[quad as usize].corners.iter().map(|c| to_tile.spot(c[0], c[2])).collect()))
                .filter(|ring: &Ring| shapes::twice_area(ring) != 0)
                .collect();
            let lines: Vec<Vec<[f64; 2]>> = claims.iter().map(|claim| on_tile(&claim.line)).collect();
            let around = shapes::around_lines(&lines, CLAIM_REACH, scale);
            let road = shapes::intersect(&shapes::union(&ground), &around);
            if !road.is_empty() {
                groups.entry((0, kind, near, false)).or_default().extend(road);
            }
        }
    }
}

/// `parts` (each an outline with its holes) cut in two, again and again, until none has over [`MOST_RINGS`] rings. The two
/// sides overlap a tile unit, so no seam shows between them.
fn cut_up(parts: Vec<Vec<Ring>>) -> Vec<Vec<Ring>> {
    let mut done = Vec::with_capacity(parts.len());
    let mut left = parts;
    while let Some(shape) = left.pop() {
        if shape.len() <= MOST_RINGS {
            done.push(shape);
            continue;
        }
        let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        for p in &shape[0] {
            (x0, y0, x1, y1) = (x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y));
        }
        let (mid_x, mid_y) = ((x0 + x1) / 2, (y0 + y1) / 2);
        let halves = if x1 - x0 >= y1 - y0 {
            [shapes::rectangle(x0, y0, mid_x + FINE, y1), shapes::rectangle(mid_x - FINE, y0, x1, y1)]
        } else {
            [shapes::rectangle(x0, y0, x1, mid_y + FINE), shapes::rectangle(x0, mid_y - FINE, x1, y1)]
        };
        for half in halves {
            left.extend(shapes::intersect_shapes(&shape, &[half]));
        }
    }
    done
}

const QUAD_CELL: f64 = 16.0;

/// The cells of [`QUAD_CELL`] a box reaches.
fn cells(x0: f64, z0: f64, x1: f64, z1: f64) -> impl Iterator<Item = (i32, i32)> {
    let (first_z, last_z) = ((z0 / QUAD_CELL).floor() as i32, (z1 / QUAD_CELL).floor() as i32);
    ((x0 / QUAD_CELL).floor() as i32..=(x1 / QUAD_CELL).floor() as i32).flat_map(move |x| (first_z..=last_z).map(move |z| (x, z)))
}

/// Where a block has paving.
#[derive(Default)]
pub struct Paving {
    cells: HashSet<(i32, i32)>,
}

impl Paving {
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Whether a box (min x, min z, max x, max z) reaches any.
    pub fn reaches(&self, [x0, z0, x1, z1]: [f64; 4]) -> bool {
        cells(x0, z0, x1, z1).any(|cell| self.cells.contains(&cell))
    }
}

/// How high a quad's ground is at a point, on the plane of the half it's in.
fn height_at(corners: &[[f64; 3]; 4], x: f64, z: f64) -> f64 {
    // A point of a triangle: how high, and whether it's inside.
    let on = |a: [f64; 3], b: [f64; 3], c: [f64; 3]| -> Option<(f64, bool)> {
        let (ux, uz, vx, vz) = (b[0] - a[0], b[2] - a[2], c[0] - a[0], c[2] - a[2]);
        let det = ux * vz - uz * vx;
        if det.abs() < 1e-9 {
            return None;
        }
        let (dx, dz) = (x - a[0], z - a[2]);
        let (s, t) = ((dx * vz - dz * vx) / det, (ux * dz - uz * dx) / det);
        Some((a[1] + s * (b[1] - a[1]) + t * (c[1] - a[1]), s >= 0.0 && t >= 0.0 && s + t <= 1.0))
    };
    let [a, b, c, d] = *corners;
    match (on(a, b, c), on(a, c, d)) {
        (Some((height, true)), _) | (_, Some((height, _))) | (Some((height, false)), None) => height,
        (None, None) => (a[1] + b[1] + c[1] + d[1]) / 4.0,
    }
}

/// A quad of paving as quiet ground, less what lies over it.
fn uncovered(quad: &Quad, over: &[Quad], index: &QuadIndex) -> Vec<Shape> {
    let whole = ring_box(&quad.corners);
    let [x0, z0, x1, z1] = whole;
    let above: Vec<&Quad> = index
        .in_box(whole)
        .into_iter()
        .map(|other| &over[other as usize])
        .filter(|other| {
            let [ox0, oz0, ox1, oz1] = ring_box(&other.corners);
            let (x, z) = ((x0.max(ox0) + x1.min(ox1)) / 2.0, (z0.max(oz0) + z1.min(oz1)) / 2.0);
            height_at(&other.corners, x, z) - height_at(&quad.corners, x, z) > LAID_OVER
        })
        .collect();
    if above.is_empty() {
        return vec![Shape { kind: Kind::Quiet, near: false, raw: true, ring: quad.corners.to_vec() }];
    }
    let spot = |p: &[f64; 3]| Spot::new(((p[0] - x0) * CUT_GRID).round() as i32, ((p[2] - z0) * CUT_GRID).round() as i32);
    let ring = |corners: &[[f64; 3]; 4]| -> Ring {
        let mut ring: Ring = corners.iter().map(spot).collect();
        ring.dedup();
        shapes::outward(ring)
    };
    let cut: Vec<Ring> = above.iter().map(|other| ring(&other.corners)).filter(|ring| shapes::twice_area(ring) != 0).collect();
    shapes::difference(&[ring(&quad.corners)], &cut)
        .into_iter()
        .map(|left| Shape {
            kind: Kind::Quiet,
            near: false,
            raw: false,
            ring: left.iter().map(|p| [x0 + f64::from(p.x) / CUT_GRID, 0.0, z0 + f64::from(p.y) / CUT_GRID]).collect(),
        })
        .collect()
}

/// Painted ground by the cells it reaches.
struct QuadIndex {
    cells: HashMap<(i32, i32), Vec<u32>>,
}

impl QuadIndex {
    fn new(quads: &[Quad]) -> Self {
        let mut index: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for (at, quad) in quads.iter().enumerate() {
            let [x0, z0, x1, z1] = ring_box(&quad.corners);
            for cell in cells(x0, z0, x1, z1) {
                index.entry(cell).or_default().push(at as u32);
            }
        }
        Self { cells: index }
    }

    /// The quads in the cells a box reaches.
    fn in_box(&self, [x0, z0, x1, z1]: [f64; 4]) -> Vec<u32> {
        let mut found: Vec<u32> = cells(x0, z0, x1, z1).flat_map(|cell| self.cells.get(&cell).into_iter().flatten().copied()).collect();
        found.sort_unstable();
        found.dedup();
        found
    }

    /// The quads within `reach` of `points`, near enough their height.
    fn under<'a>(&self, quads: &[Quad], points: impl Iterator<Item = &'a [f64; 3]>, reach: f64) -> Vec<u32> {
        let mut found: Vec<u32> = Vec::new();
        for p in points {
            let (x0, x1) = (((p[0] - reach) / QUAD_CELL).floor() as i32, ((p[0] + reach) / QUAD_CELL).floor() as i32);
            let (z0, z1) = (((p[2] - reach) / QUAD_CELL).floor() as i32, ((p[2] + reach) / QUAD_CELL).floor() as i32);
            for x in x0..=x1 {
                for z in z0..=z1 {
                    for &quad in self.cells.get(&(x, z)).map_or(&[][..], Vec::as_slice) {
                        if quad_near(&quads[quad as usize], p, reach) {
                            found.push(quad);
                        }
                    }
                }
            }
        }
        found.sort_unstable();
        found.dedup();
        found
    }
}

/// From the ground to a tile's fine grid.
struct TileGrid {
    origin: [f64; 2],
    scale: f64,
    units_per_metre: f64,
}

impl TileGrid {
    fn new(x: u32, y: u32) -> Self {
        let tiles = (1u64 << SURFACE_ZOOM) as f64;
        let scale = tiles * f64::from(EXTENT) * f64::from(FINE);
        // A tile's width on the ground, metres.
        let units_per_metre = scale / (360.0 * METRES_PER_DEGREE);
        Self { origin: [f64::from(x), f64::from(y)], scale: f64::from(EXTENT) * f64::from(FINE), units_per_metre }
    }

    fn floating(&self, x: f64, z: f64) -> [f64; 2] {
        let tiles = (1u64 << SURFACE_ZOOM) as f64;
        let [mx, my] = mercator(x, z);
        [(mx * tiles - self.origin[0]) * self.scale, (my * tiles - self.origin[1]) * self.scale]
    }

    fn spot(&self, x: f64, z: f64) -> Spot {
        let [fx, fy] = self.floating(x, z);
        Spot::new(fx.round() as i32, fy.round() as i32)
    }

    /// The ground at a point of the fine grid: x, z.
    fn ground(&self, fx: f64, fy: f64) -> [f64; 2] {
        let tiles = (1u64 << SURFACE_ZOOM) as f64;
        let (mx, my) = ((fx / self.scale + self.origin[0]) / tiles, (fy / self.scale + self.origin[1]) / tiles);
        let lat = (2.0 * ((0.5 - my) * 2.0 * std::f64::consts::PI).exp().atan() - std::f64::consts::FRAC_PI_2).to_degrees();
        [(mx * 360.0 - 180.0) * METRES_PER_DEGREE, -lat * METRES_PER_DEGREE]
    }

    /// The ground this tile covers and `margin` metres round it: min x, min z, max x, max z.
    fn world_box(&self, margin: f64) -> (f64, f64, f64, f64) {
        let tiles = (1u64 << SURFACE_ZOOM) as f64;
        let size = 360.0 * METRES_PER_DEGREE / tiles;
        let x0 = self.origin[0] * size - 180.0 * METRES_PER_DEGREE;
        // The world's middle row is z 0; near the equator Mercator's rows are the plate's.
        let z0 = self.origin[1] * size - 180.0 * METRES_PER_DEGREE;
        (x0 - margin, z0 - margin, x0 + size + margin, z0 + size + margin)
    }
}

/// How many fine units high the rows are that a tile's outlines are looked up in.
const PROBE_ROW: i32 = 512;
/// How far beside a seam an audit's point looks, fine units: about 4 cm.
const PROBE_BESIDE: f64 = 2.0;

/// What's drawn at each probe: road if any is, else the first of the rest.
fn check_probes(layer: &Layer, to_tile: &TileGrid, probes: &[(u32, [f64; 2])], here: &[u32], seen: &mut dyn FnMut(u32, [f64; 2], Option<Kind>)) {
    // Each kind's sides by row.
    let mut rows: BTreeMap<Kind, HashMap<i32, Vec<(Spot, Spot)>>> = BTreeMap::new();
    for ((_, kind, ..), parts) in &layer.surfaces {
        let kind_rows = rows.entry(*kind).or_default();
        for ring in parts.iter().flatten() {
            for i in 0..ring.len() {
                let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
                for row in a.y.min(b.y).div_euclid(PROBE_ROW)..=a.y.max(b.y).div_euclid(PROBE_ROW) {
                    kind_rows.entry(row).or_default().push((a, b));
                }
            }
        }
    }
    let covers = |sides: &HashMap<i32, Vec<(Spot, Spot)>>, x: f64, y: f64| -> bool {
        let mut winding = 0;
        for (a, b) in sides.get(&(y as i32).div_euclid(PROBE_ROW)).map_or(&[][..], Vec::as_slice) {
            let (ay, by) = (f64::from(a.y), f64::from(b.y));
            if (ay <= y) != (by <= y) {
                let cross = f64::from(a.x) + (y - ay) / (by - ay) * f64::from(b.x - a.x);
                if cross > x {
                    winding += if by > ay { 1 } else { -1 };
                }
            }
        }
        winding != 0
    };
    // A lane's end lies exactly on the seam between a road and its junction: either side of it counts.
    let beside = [[0.0, 0.0], [PROBE_BESIDE, 0.0], [-PROBE_BESIDE, 0.0], [0.0, PROBE_BESIDE], [0.0, -PROBE_BESIDE]];
    for &index in here {
        let (what, at) = probes[index as usize];
        let [x, y] = to_tile.floating(at[0], at[1]);
        let mut found = None;
        for [dx, dy] in beside {
            for (kind, sides) in &rows {
                if covers(sides, x + dx, y + dy) && (found.is_none() || matches!(kind, Kind::Road(_) | Kind::Tunnel(_) | Kind::Rail)) {
                    found = Some(*kind);
                }
            }
            if found.is_some() {
                break;
            }
        }
        seen(what, at, found);
    }
}

/// A raised road's edges that really are edges: not where it goes on at its own height, on to the road it's raised from.
fn true_edges(to_tile: &TileGrid, parts: &[Vec<Ring>], index: &FaceIndex, side: i32, reach: i32) -> Vec<Vec<Spot>> {
    let metre = to_tile.units_per_metre;
    let mut lines = Vec::new();
    for ring in parts.iter().flatten() {
        let mut line: Vec<Spot> = Vec::new();
        // Whole rings go round: from a point that isn't an edge if there is one, so a line isn't broken at its start.
        for i in 0..ring.len() {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            let (ax, ay, bx, by) = (f64::from(a.x), f64::from(a.y), f64::from(b.x), f64::from(b.y));
            let long = (bx - ax).hypot(by - ay);
            let on_frame = [a, b].iter().all(|p| p.x <= -reach || p.y <= -reach || p.x >= side + reach || p.y >= side + reach);
            let mut edge = false;
            if long > 0.0 && !on_frame {
                let (mx, my) = ((ax + bx) / 2.0, (ay + by) / 2.0);
                // Outlines turn with their inside on the left of the way they go.
                let (nx, ny) = ((by - ay) / long, -(bx - ax) / long);
                let inside = to_tile.ground(mx - nx * 0.3 * metre, my - ny * 0.3 * metre);
                let outside = to_tile.ground(mx + nx * 0.6 * metre, my + ny * 0.6 * metre);
                let highs: Vec<f64> = index.heights(inside[0], inside[1]).collect();
                edge = match highs.iter().copied().reduce(f64::max) {
                    Some(high) => !index.heights(outside[0], outside[1]).any(|h| (h - high).abs() < 1.5),
                    None => false,
                };
            }
            if edge {
                if line.is_empty() {
                    line.push(a);
                }
                line.push(b);
            } else if line.len() >= 2 {
                lines.push(std::mem::take(&mut line));
            } else {
                line.clear();
            }
        }
        if line.len() >= 2 {
            lines.push(line);
        }
    }
    lines
}

/// A tile inside another.
type Child = ((u32, u32), Layer);

/// A tile's layer from the four inside it: what's left from further out, on its own grid.
fn merged(tile: (u32, u32), children: &[Child]) -> Layer {
    let side = EXTENT as i32 * FINE;
    let mut groups: BTreeMap<Key, Vec<Ring>> = BTreeMap::new();
    for (child, layer) in children {
        let (dx, dy) = (((child.0 - tile.0 * 2) as i32) * side, ((child.1 - tile.1 * 2) as i32) * side);
        let halved = |p: &Spot| Spot::new((p.x + dx) / 2, (p.y + dy) / 2);
        for ((level, kind, near, _), parts) in &layer.surfaces {
            if *near || *kind != Kind::Water {
                continue;
            }
            let rings = groups.entry((*level, *kind, false, false)).or_default();
            for ring in parts.iter().flatten() {
                rings.push(ring.iter().map(halved).collect());
            }
        }
    }
    let mut out = Layer::default();
    for (key, rings) in groups {
        let parts = shapes::shapes(&rings);
        if !parts.is_empty() {
            out.surfaces.insert(key, parts);
        }
    }
    out
}

fn put(dir: &Path, zoom: u8, x: u32, y: u32, bytes: &[u8]) -> std::io::Result<u64> {
    let folder = dir.join(zoom.to_string()).join(x.to_string());
    std::fs::create_dir_all(&folder)?;
    std::fs::write(folder.join(format!("{y}.pbf")), bytes)?;
    Ok(bytes.len() as u64)
}

/// Adds the service areas' ground to the tiles under `dir`, the layer `service` after what's in each (a tile is its layers
/// one after another): worked out from the paving of all the blocks round it, it comes once they're written. `rings`: outlines
/// with their holes, which turn the other way. How many tiles, and their bytes.
pub fn add_service(dir: &Path, rings: &[&Vec<[f64; 3]>]) -> std::io::Result<(usize, u64)> {
    let mut in_tile: BTreeMap<(u32, u32), Vec<u32>> = BTreeMap::new();
    for (index, ring) in rings.iter().enumerate() {
        let [x0, z0, x1, z1] = ring_box(ring);
        let [tx0, ty0] = tile_of(x0 - REACH, z0 - REACH, SURFACE_ZOOM);
        let [tx1, ty1] = tile_of(x1 + REACH, z1 + REACH, SURFACE_ZOOM);
        for x in tx0..=tx1 {
            for y in ty0..=ty1 {
                in_tile.entry((x, y)).or_default().push(index as u32);
            }
        }
    }
    let reach = (f64::from(EXTENT) / 64.0) as i32 * FINE;
    let side = EXTENT as i32 * FINE;
    let clip = vec![shapes::rectangle(-reach, -reach, side + reach, side + reach)];
    let round = |p: &Spot| [(f64::from(p.x) / f64::from(FINE)).round() as i64, (f64::from(p.y) / f64::from(FINE)).round() as i64];
    let mut written = 0;
    let mut bytes = 0u64;
    for ((x, y), list) in in_tile {
        let to_tile = TileGrid::new(x, y);
        let here: Vec<Ring> = list.iter().map(|&ring| rings[ring as usize].iter().map(|p| to_tile.spot(p[0], p[2])).collect()).collect();
        let parts = cut_up(shapes::intersect_shapes(&here, &clip));
        let (commands, _) = polygon_commands(&parts, &round, THIN * f64::from(FINE), 0);
        if commands.is_empty() {
            continue;
        }
        let mut writer = Writer::new("service", &[]);
        writer.feature(None, &[], 3, &commands);
        let mut tile = Vec::new();
        writer.finish(&mut tile);
        let folder = dir.join(SURFACE_ZOOM.to_string()).join(x.to_string());
        std::fs::create_dir_all(&folder)?;
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(folder.join(format!("{y}.pbf")))?;
        std::io::Write::write_all(&mut file, &tile)?;
        written += 1;
        bytes += tile.len() as u64;
    }
    Ok((written, bytes))
}

fn name_of(kind: Kind) -> &'static str {
    match kind {
        Kind::Quiet => "quiet",
        Kind::Building => "building",
        Kind::Water => "water",
        Kind::Rail => "rail",
        Kind::Road(_) => "road",
        Kind::Tunnel(_) => "tunnel",
    }
}

/// A property's value in a tile layer.
#[derive(Clone, PartialEq)]
enum Value {
    Text(String),
    True,
    Number(f64),
}

/// A tile layer's keys, values and features as they're written.
struct Writer {
    keys: &'static [&'static str],
    values: Vec<Value>,
    out: Vec<u8>,
}

impl Writer {
    fn new(name: &str, keys: &'static [&'static str]) -> Self {
        let mut out = Vec::new();
        varint_field(&mut out, 15, 2);
        bytes_field(&mut out, 1, name.as_bytes());
        Self { keys, values: Vec::new(), out }
    }

    fn value(&mut self, value: Value) -> u32 {
        match self.values.iter().position(|known| *known == value) {
            Some(index) => index as u32,
            None => {
                self.values.push(value);
                self.values.len() as u32 - 1
            }
        }
    }

    /// A feature of `kind` (2 lines, 3 polygons), with `id` if it has one.
    fn feature(&mut self, id: Option<u64>, tags: &[u32], kind: u64, commands: &[u32]) {
        let packed = |list: &[u32]| {
            let mut bytes = Vec::new();
            for value in list {
                crate::tiles::varint(&mut bytes, u64::from(*value));
            }
            bytes
        };
        let mut feature = Vec::new();
        if let Some(id) = id {
            varint_field(&mut feature, 1, id);
        }
        bytes_field(&mut feature, 2, &packed(tags));
        varint_field(&mut feature, 3, kind);
        bytes_field(&mut feature, 4, &packed(commands));
        bytes_field(&mut self.out, 2, &feature);
    }

    /// The layer's bytes, as a tile's field.
    fn finish(mut self, tile: &mut Vec<u8>) {
        for key in self.keys {
            bytes_field(&mut self.out, 3, key.as_bytes());
        }
        for value in &self.values {
            let mut encoded = Vec::new();
            match value {
                Value::Text(text) => bytes_field(&mut encoded, 1, text.as_bytes()),
                Value::True => varint_field(&mut encoded, 7, 1),
                Value::Number(number) => {
                    crate::tiles::varint(&mut encoded, (3 << 3) | 1);
                    encoded.extend(number.to_le_bytes());
                }
            }
            bytes_field(&mut self.out, 4, &encoded);
        }
        varint_field(&mut self.out, 5, u64::from(EXTENT));
        bytes_field(tile, 3, &self.out);
    }
}

/// A feature's polygons as a tile's commands, each ring thinned by `thin` and rounded by `round`, the smallest left out; and
/// how many rings. A reader tells outlines from holes by the way they turn alone, so a ring that thinning and rounding have
/// turned round goes: a sliver of a hole taken for an outline takes every hole after it from the outline they're in, and
/// a lawn and the pool on it come out filled.
fn polygon_commands(parts: &[Vec<Ring>], round: &impl Fn(&Spot) -> [i64; 2], thin: f64, smallest: i64) -> (Vec<u32>, usize) {
    let mut commands: Vec<u32> = Vec::new();
    let mut cursor = [0i64; 2];
    let mut rings = 0;
    // The way this feature's outlines turn.
    let mut way = 0;
    for shape in parts {
        for (index, ring) in shape.iter().enumerate() {
            let Some(thinned) = shapes::thin_ring(ring, thin) else {
                if index == 0 {
                    break;
                }
                continue;
            };
            let mut points: Vec<[i64; 2]> = Vec::with_capacity(thinned.len());
            for p in &thinned {
                let q = round(p);
                if points.last() != Some(&q) {
                    points.push(q);
                }
            }
            if points.len() > 1 && points.first() == points.last() {
                points.pop();
            }
            let area: i64 = (0..points.len())
                .map(|i| {
                    let (a, b) = (points[i], points[(i + 1) % points.len()]);
                    a[0] * b[1] - b[0] * a[1]
                })
                .sum();
            if way == 0 && index == 0 {
                way = area.signum();
            }
            let turns_right = area.signum() == if index == 0 { way } else { -way };
            if points.len() < 3 || area.abs() < 2 * smallest.max(1) || !turns_right {
                if index == 0 {
                    break;
                }
                continue;
            }
            commands.push(command(1, 1));
            push_point(&mut commands, &mut cursor, points[0]);
            commands.push(command(2, points.len() as u32 - 1));
            for &p in &points[1..] {
                push_point(&mut commands, &mut cursor, p);
            }
            commands.push(command(7, 1));
            rings += 1;
        }
    }
    (commands, rings)
}

/// The raised file (raised.rs) with what's drawn at its height of the tile (x, y), as the features of that go by it: its
/// number among the files, from 1 (a feature's id of 0 is none; the cab's `flatCopyId`).
fn raised_file(x: u32, y: u32) -> u64 {
    let shift = SURFACE_ZOOM - raised::FILE_ZOOM;
    (u64::from(x >> shift) << raised::FILE_ZOOM) + u64::from(y >> shift) + 1
}

/// A tile's bytes: the surfaces as the layer `surface`, a feature for each kind, its polygons thinned by `thin` (in fine
/// units), rounded by `divide`, the smallest (tile units squared) left out; raised roads' edges as `surface_edge`. What's
/// drawn at its height goes by `file` ([`raised_file`]).
fn encode(layer: &Layer, divide: f64, thin: f64, smallest: i64, file: u64) -> Vec<u8> {
    let round = |p: &Spot| [(f64::from(p.x) / divide).round() as i64, (f64::from(p.y) / divide).round() as i64];
    let mut tile = Vec::new();
    let mut writer = Writer::new("surface", &["kind", "class", "near", "level"]);
    for ((level, kind, near, raised), parts) in &layer.surfaces {
        let (commands, _) = polygon_commands(parts, &round, thin, smallest);
        if commands.is_empty() {
            continue;
        }
        let mut tags = vec![0u32, writer.value(Value::Text(name_of(*kind).to_owned()))];
        if let Kind::Road(class) | Kind::Tunnel(class) = kind {
            tags.extend([1, writer.value(Value::Text(class.name().to_owned()))]);
        }
        if *near {
            tags.extend([2, writer.value(Value::True)]);
        }
        if *level > 0 {
            tags.extend([3, writer.value(Value::Number(f64::from(*level)))]);
        }
        writer.feature(raised.then_some(file), &tags, 3, &commands);
    }
    writer.finish(&mut tile);
    let line_commands = |line: &[Spot]| -> Option<Vec<u32>> {
        let mut points: Vec<[i64; 2]> = Vec::with_capacity(line.len());
        for p in line {
            let q = round(p);
            if points.last() != Some(&q) {
                points.push(q);
            }
        }
        (points.len() >= 2).then(|| {
            let mut commands = Vec::with_capacity(points.len() * 2 + 2);
            let mut cursor = [0i64; 2];
            commands.push(command(1, 1));
            push_point(&mut commands, &mut cursor, points[0]);
            commands.push(command(2, points.len() as u32 - 1));
            for &p in &points[1..] {
                push_point(&mut commands, &mut cursor, p);
            }
            commands
        })
    };
    if !layer.edges.is_empty() {
        let mut writer = Writer::new("surface_edge", &["level", "near"]);
        for ((level, near, raised), lines) in &layer.edges {
            let mut tags = vec![0, writer.value(Value::Number(f64::from(*level)))];
            if *near {
                tags.extend([1, writer.value(Value::True)]);
            }
            for commands in lines.iter().filter_map(|line| line_commands(line)) {
                writer.feature(raised.then_some(file), &tags, 2, &commands);
            }
        }
        writer.finish(&mut tile);
    }
    tile
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turned(ring: Ring) -> Ring {
        ring.into_iter().rev().collect()
    }

    #[test]
    fn a_road_that_rises_goes_by_its_file_and_what_passes_under_it_does_not() {
        let (x, y) = (16385, 16386);
        let [x0, z0, x1, z1] = tile_box(x, y, SURFACE_ZOOM);
        let band = |from: f64, to: f64| {
            let (a, b) = (z0 + (z1 - z0) * from, z0 + (z1 - z0) * to);
            vec![[x0, 0.0, a], [x1, 0.0, a], [x1, 0.0, b], [x0, 0.0, b]]
        };
        let road = Kind::Road(RoadClass::Local);
        let mut block = Block::new((x, y), SURFACE_ZOOM);
        block.add(Shape { kind: road, near: false, raw: true, ring: band(0.2, 0.3) });
        block.add(Shape { kind: road, near: false, raw: true, ring: band(0.6, 0.7) });
        block.raised_shapes.insert(0);
        let layer = block.deepest(x, y, &[0, 1], &[], &FaceIndex::new(&block.faces), &QuadIndex::new(&block.quads));
        // Each road, and the quiet ground under each: under one that goes, that goes too.
        for key in [(0, road, false, true), (0, road, false, false), (0, Kind::Quiet, false, true), (0, Kind::Quiet, false, false)] {
            assert!(layer.surfaces.contains_key(&key), "{:?}", layer.surfaces.keys().collect::<Vec<_>>());
        }
        // The file's number, as the cab works it out: its tile at the files' zoom.
        assert_eq!(raised_file(x, y), (8192 << 14) + 8193 + 1);
    }

    #[test]
    fn a_hole_rounding_turned_round_goes_and_those_after_it_stay_holes() {
        let outline = shapes::rectangle(0, 0, 8000, 8000);
        let hole = |x: i32| turned(shapes::rectangle(x, 100, x + 200, 300));
        // Turned the outline's way, as a sliver can be once rounded.
        let sliver = shapes::rectangle(1000, 100, 1200, 300);
        let parts = vec![vec![outline, hole(100), sliver, hole(2000)]];
        let (_, rings) = polygon_commands(&parts, &|p: &Spot| [i64::from(p.x), i64::from(p.y)], 0.0, 0);
        assert_eq!(rings, 3);
    }

    #[test]
    fn an_outline_with_too_many_holes_for_the_map_is_cut_up() {
        let mut shape = vec![shapes::rectangle(0, 0, 100_000, 1000)];
        for i in 0..1000 {
            let x = 10 + i * 100;
            shape.push(turned(shapes::rectangle(x, 10, x + 50, 60)));
        }
        let parts = cut_up(vec![shape]);
        assert!(parts.len() > 1);
        assert!(parts.iter().all(|shape| shape.len() <= MOST_RINGS));
        assert_eq!(parts.iter().map(|shape| shape.len() - 1).sum::<usize>(), 1000);
    }
}
