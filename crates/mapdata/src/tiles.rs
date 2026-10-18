//! Vector tiles: the map cut into squares per zoom level (Web Mercator, the Mapbox Vector Tile format), so
//! the Maps app loads only what's on screen, at the detail its zoom needs.
//!
//! Tiles are made top down, like geojson-vt: each tile's shapes are clipped from its parent's, so even the
//! sea's huge outlines are cut cheaply. A feature appears from its `minzoom` on, simplified to half a
//! pixel at each zoom; the last zoom's tiles are finer still, since the map zooms in past them.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A point in Web Mercator world units: 0..1 from west to east and from north to south.
pub type Mercator = [f64; 2];

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Str(String),
    Num(f64),
    Bool(bool),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Geometry {
    /// A place (drawn as an icon).
    Point(Mercator),
    Lines(Vec<Vec<Mercator>>),
    /// One polygon: its outline, then its holes.
    Polygon(Vec<Vec<Mercator>>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Feature {
    pub layer: &'static str,
    /// The first zoom it appears at, and the last (`u8::MAX`: every zoom from the first).
    pub minzoom: u8,
    pub maxzoom: u8,
    pub props: Vec<(&'static str, Value)>,
    pub geometry: Geometry,
}

#[derive(Clone, Copy, Debug)]
pub struct TileOptions {
    pub min_zoom: u8,
    pub max_zoom: u8,
    /// Tile units across, below the last zoom and at it (finer, as the map zooms in past it).
    pub extent: u32,
    pub max_extent: u32,
    /// How far shapes reach past a tile's edge, as a share of the tile (so lines and outlines don't show
    /// seams where tiles meet).
    pub buffer: f64,
}

impl Default for TileOptions {
    fn default() -> Self {
        Self { min_zoom: 6, max_zoom: 14, extent: 4096, max_extent: 16384, buffer: 1.0 / 64.0 }
    }
}

/// A feature and its bounding box (min x, min y, max x, max y).
struct Item {
    feature: Feature,
    bbox: [f64; 4],
}

/// Writes the tiles of `features` into `dir` as `{z}/{x}/{y}.pbf` (only tiles with something in them).
/// Returns how many it wrote. `progress` is told how far it is, 0 to 1: the first zooms a tenth, then the
/// threads' trees as they finish.
pub fn write_tiles(
    features: Vec<Feature>,
    options: TileOptions,
    dir: &Path,
    progress: &mut dyn FnMut(f32),
) -> std::io::Result<usize> {
    progress(0.0);
    let items: Vec<Item> =
        features.into_iter().filter_map(|feature| Some(Item { bbox: bbox(&feature.geometry)?, feature })).collect();
    let Some(all) = items.iter().map(|item| item.bbox).reduce(union) else { return Ok(0) };

    // The first zooms in turn (a whole game map fits in a handful of their tiles), down to the zoom where
    // the work splits into many trees, shared out between threads.
    let z = options.min_zoom;
    let scale = (1u64 << z) as f64;
    let range =
        |lo: f64, hi: f64| ((lo * scale).floor().max(0.0) as u32)..=((hi * scale).floor().min(scale - 1.0) as u32);
    let split = options.max_zoom.min(SPLIT_ZOOM).max(z);
    let mut tasks = Vec::new();
    let mut written = 0;
    for x in range(all[0], all[2]) {
        for y in range(all[1], all[3]) {
            let tile_items = clip_items(&items, z, tile_bounds(z, x, y, options.buffer));
            if !tile_items.is_empty() {
                written += descend(z, x, y, tile_items, split, options, dir, &mut tasks)?;
            }
        }
    }
    drop(items);
    progress(0.1);
    let total = tasks.len().max(1);
    let finished = AtomicUsize::new(0);
    let tasks = Mutex::new(tasks);
    let written = Mutex::new(written);
    let failure = Mutex::new(None);
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    // Threads still working: each counts itself out when it ends, however it ends (a panic too).
    let working = AtomicUsize::new(threads);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                let _leaving = Leaving(&working);
                while let Some((x, y, tile_items)) = tasks.lock().unwrap().pop() {
                    match make_tile(split, x, y, tile_items, options, dir) {
                        Ok(count) => *written.lock().unwrap() += count,
                        Err(error) => *failure.lock().unwrap() = Some(error),
                    }
                    finished.fetch_add(1, Ordering::Relaxed);
                }
            });
        }
        // Meanwhile, this thread tells how far the trees are, until every thread has ended.
        while working.load(Ordering::Acquire) > 0 {
            progress(0.1 + 0.9 * finished.load(Ordering::Relaxed) as f32 / total as f32);
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
    });
    progress(1.0);
    match failure.into_inner().unwrap() {
        Some(error) => Err(error),
        None => Ok(written.into_inner().unwrap()),
    }
}

/// Counts a thread out of `working` when it ends.
struct Leaving<'a>(&'a AtomicUsize);

impl Drop for Leaving<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Release);
    }
}

/// The zoom from which tiles are made in parallel, one tree per tile (about 40 km across).
const SPLIT_ZOOM: u8 = 10;

/// A tile above the split zoom: writes it, and goes on down to its descendants there, which it leaves in
/// `tasks` for the threads.
#[allow(clippy::too_many_arguments)]
fn descend(
    z: u8,
    x: u32,
    y: u32,
    items: Vec<Item>,
    split: u8,
    options: TileOptions,
    dir: &Path,
    tasks: &mut Vec<(u32, u32, Vec<Item>)>,
) -> std::io::Result<usize> {
    if z == split {
        tasks.push((x, y, items));
        return Ok(0);
    }
    let mut written = 0;
    if let Some(bytes) = encode_tile(&items, z, x, y, options.extent, options) {
        let folder = dir.join(z.to_string()).join(x.to_string());
        std::fs::create_dir_all(&folder)?;
        std::fs::write(folder.join(format!("{y}.pbf")), bytes)?;
        written += 1;
    }
    for (cx, cy) in [(2 * x, 2 * y), (2 * x + 1, 2 * y), (2 * x, 2 * y + 1), (2 * x + 1, 2 * y + 1)] {
        let child = clip_items(&items, z + 1, tile_bounds(z + 1, cx, cy, options.buffer));
        if !child.is_empty() {
            written += descend(z + 1, cx, cy, child, split, options, dir, tasks)?;
        }
    }
    Ok(written)
}

/// Writes tile (z, x, y) from `items` (already clipped to it), then its children's.
fn make_tile(z: u8, x: u32, y: u32, items: Vec<Item>, options: TileOptions, dir: &Path) -> std::io::Result<usize> {
    let mut written = 0;
    let last = z == options.max_zoom;
    let extent = if last { options.max_extent } else { options.extent };
    let bytes = encode_tile(&items, z, x, y, extent, options);
    if let Some(bytes) = bytes {
        let folder = dir.join(z.to_string()).join(x.to_string());
        std::fs::create_dir_all(&folder)?;
        std::fs::write(folder.join(format!("{y}.pbf")), bytes)?;
        written += 1;
    }
    if !last {
        for (cx, cy) in [(2 * x, 2 * y), (2 * x + 1, 2 * y), (2 * x, 2 * y + 1), (2 * x + 1, 2 * y + 1)] {
            let child = clip_items(&items, z + 1, tile_bounds(z + 1, cx, cy, options.buffer));
            if !child.is_empty() {
                written += make_tile(z + 1, cx, cy, child, options, dir)?;
            }
        }
    }
    Ok(written)
}

/// A tile's square in world units, grown by `buffer` (a share of the tile) on each side.
fn tile_bounds(z: u8, x: u32, y: u32, buffer: f64) -> [f64; 4] {
    let size = 1.0 / (1u64 << z) as f64;
    let pad = size * buffer;
    [x as f64 * size - pad, y as f64 * size - pad, (x + 1) as f64 * size + pad, (y + 1) as f64 * size + pad]
}

fn union(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]
}

fn bbox(geometry: &Geometry) -> Option<[f64; 4]> {
    let parts = match geometry {
        Geometry::Point(p) => return Some([p[0], p[1], p[0], p[1]]),
        Geometry::Lines(lines) => lines,
        Geometry::Polygon(rings) => rings,
    };
    parts.iter().flatten().map(|p| [p[0], p[1], p[0], p[1]]).reduce(union)
}

/// The items of a tile at zoom `z` within `bounds`, cut to them (none past their last zoom: they're gone
/// from the tile's descendants too).
fn clip_items(items: &[Item], z: u8, bounds: [f64; 4]) -> Vec<Item> {
    items
        .iter()
        .filter(|item| {
            item.feature.maxzoom >= z
                && item.bbox[0] <= bounds[2]
                && item.bbox[2] >= bounds[0]
                && item.bbox[1] <= bounds[3]
                && item.bbox[3] >= bounds[1]
        })
        .filter_map(|item| {
            // Wholly inside: no clipping needed.
            let inside = item.bbox[0] >= bounds[0]
                && item.bbox[2] <= bounds[2]
                && item.bbox[1] >= bounds[1]
                && item.bbox[3] <= bounds[3];
            let geometry = if inside { item.feature.geometry.clone() } else { clip(&item.feature.geometry, bounds)? };
            let bbox = bbox(&geometry)?;
            Some(Item { feature: Feature { geometry, ..item.feature.clone() }, bbox })
        })
        .collect()
}

/// Cuts a geometry to a rectangle: lines into the pieces inside it, polygon rings along its edges.
fn clip(geometry: &Geometry, bounds: [f64; 4]) -> Option<Geometry> {
    match geometry {
        Geometry::Point(p) => (bounds[0] <= p[0] && p[0] <= bounds[2] && bounds[1] <= p[1] && p[1] <= bounds[3])
            .then_some(geometry.clone()),
        Geometry::Lines(lines) => {
            let pieces: Vec<Vec<Mercator>> = lines.iter().flat_map(|line| clip_line(line, bounds)).collect();
            (!pieces.is_empty()).then_some(Geometry::Lines(pieces))
        }
        Geometry::Polygon(rings) => {
            let outline = clip_ring(&rings[0], bounds);
            if outline.len() < 3 {
                return None;
            }
            let mut clipped = vec![outline];
            clipped.extend(rings[1..].iter().map(|hole| clip_ring(hole, bounds)).filter(|hole| hole.len() >= 3));
            Some(Geometry::Polygon(clipped))
        }
    }
}

/// The pieces of a line inside a rectangle (Liang-Barsky on each segment).
fn clip_line(line: &[Mercator], [x0, y0, x1, y1]: [f64; 4]) -> Vec<Vec<Mercator>> {
    let mut pieces = Vec::new();
    let mut current: Vec<Mercator> = Vec::new();
    // The last segment reached its end inside: the next one, starting there, carries on the same piece.
    let mut open = false;
    for pair in line.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let (mut t0, mut t1) = (0.0f64, 1.0f64);
        let mut visible = true;
        for (p, q) in [(-dx, a[0] - x0), (dx, x1 - a[0]), (-dy, a[1] - y0), (dy, y1 - a[1])] {
            if p == 0.0 {
                if q < 0.0 {
                    visible = false;
                }
            } else {
                let t = q / p;
                if p < 0.0 {
                    t0 = t0.max(t);
                } else {
                    t1 = t1.min(t);
                }
            }
        }
        if !visible || t0 > t1 {
            if current.len() >= 2 {
                pieces.push(std::mem::take(&mut current));
            }
            current.clear();
            open = false;
            continue;
        }
        // The segment's own ends where it isn't cut (no rounding drift).
        let start = if t0 <= 0.0 { a } else { [a[0] + t0 * dx, a[1] + t0 * dy] };
        let end = if t1 >= 1.0 { b } else { [a[0] + t1 * dx, a[1] + t1 * dy] };
        if !(open && t0 <= 0.0) {
            if current.len() >= 2 {
                pieces.push(std::mem::take(&mut current));
            }
            current = vec![start];
        }
        current.push(end);
        open = t1 >= 1.0;
        if !open {
            pieces.push(std::mem::take(&mut current));
        }
    }
    if current.len() >= 2 {
        pieces.push(current);
    }
    pieces
}

/// A ring cut to a rectangle (Sutherland-Hodgman, edge by edge).
fn clip_ring(ring: &[Mercator], [x0, y0, x1, y1]: [f64; 4]) -> Vec<Mercator> {
    let mut points = ring.to_vec();
    let edges: [(usize, f64, bool); 4] = [(0, x0, true), (0, x1, false), (1, y0, true), (1, y1, false)];
    for (axis, limit, keep_above) in edges {
        if points.is_empty() {
            break;
        }
        let inside = |p: &Mercator| if keep_above { p[axis] >= limit } else { p[axis] <= limit };
        let mut next = Vec::with_capacity(points.len());
        for i in 0..points.len() {
            let (a, b) = (points[i], points[(i + 1) % points.len()]);
            let (a_in, b_in) = (inside(&a), inside(&b));
            if a_in {
                next.push(a);
            }
            if a_in != b_in {
                let t = (limit - a[axis]) / (b[axis] - a[axis]);
                next.push([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]);
            }
        }
        points = next;
    }
    points
}

/// Drops points within `tolerance` of the line through their neighbours (Douglas-Peucker), keeping ends.
fn simplify(line: &[[f64; 2]], tolerance: f64) -> Vec<[f64; 2]> {
    if line.len() < 3 {
        return line.to_vec();
    }
    let mut keep = vec![false; line.len()];
    keep[0] = true;
    keep[line.len() - 1] = true;
    let mut stack = vec![(0, line.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        let (mut worst, mut index) = (0.0, 0);
        for i in a + 1..b {
            let d = segment_distance(line[i], line[a], line[b]);
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

fn segment_distance(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length2 = dx * dx + dy * dy;
    let t = if length2 > 0.0 { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length2).clamp(0.0, 1.0) } else { 0.0 };
    ((p[0] - a[0] - t * dx).powi(2) + (p[1] - a[1] - t * dy).powi(2)).sqrt()
}

/// Twice a ring's signed area, in tile coordinates (y down: positive turns clockwise on screen).
fn signed_area(ring: &[[i64; 2]]) -> i64 {
    (0..ring.len())
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum()
}

/// One layer of a tile being built: its features, and the keys and values they share.
#[derive(Default)]
struct LayerBuilder {
    features: Vec<Vec<u8>>,
    keys: Vec<&'static str>,
    values: Vec<Value>,
}

impl LayerBuilder {
    fn key(&mut self, key: &'static str) -> u32 {
        index_of(&mut self.keys, key)
    }

    fn value(&mut self, value: &Value) -> u32 {
        index_of(&mut self.values, value.clone())
    }
}

fn index_of<T: PartialEq>(list: &mut Vec<T>, item: T) -> u32 {
    match list.iter().position(|existing| *existing == item) {
        Some(index) => index as u32,
        None => {
            list.push(item);
            (list.len() - 1) as u32
        }
    }
}

/// The tile's bytes, or `None` if nothing in it shows at this zoom.
fn encode_tile(items: &[Item], z: u8, x: u32, y: u32, extent: u32, options: TileOptions) -> Option<Vec<u8>> {
    let scale = (1u64 << z) as f64;
    let units = extent as f64;
    let to_tile = |p: &Mercator| [(p[0] * scale - x as f64) * units, (p[1] * scale - y as f64) * units];
    // Half a pixel of a 512-pixel tile, at the zoom the tile is shown at most (further for the last zoom).
    let pixels = if z == options.max_zoom { 512.0 * (options.max_extent / options.extent) as f64 } else { 512.0 };
    let tolerance = units / pixels * 0.5;

    let mut layers: Vec<(&'static str, LayerBuilder)> = Vec::new();
    for item in items.iter().filter(|item| (item.feature.minzoom..=item.feature.maxzoom).contains(&z)) {
        let Some(geometry) = encode_geometry(&item.feature.geometry, &to_tile, tolerance) else { continue };
        let layer = match layers.iter().position(|(name, _)| *name == item.feature.layer) {
            Some(index) => &mut layers[index].1,
            None => {
                layers.push((item.feature.layer, LayerBuilder::default()));
                &mut layers.last_mut().unwrap().1
            }
        };
        let mut tags = Vec::new();
        for (key, value) in &item.feature.props {
            tags.push(layer.key(key));
            tags.push(layer.value(value));
        }
        let mut feature = Vec::new();
        let packed = |values: &[u32]| {
            let mut bytes = Vec::new();
            for value in values {
                varint(&mut bytes, *value as u64);
            }
            bytes
        };
        if !tags.is_empty() {
            bytes_field(&mut feature, 2, &packed(&tags));
        }
        let kind = match item.feature.geometry {
            Geometry::Point(_) => 1,
            Geometry::Lines(_) => 2,
            Geometry::Polygon(_) => 3,
        };
        varint_field(&mut feature, 3, kind);
        bytes_field(&mut feature, 4, &packed(&geometry));
        layer.features.push(feature);
    }
    if layers.is_empty() {
        return None;
    }
    let mut tile = Vec::new();
    for (name, layer) in layers {
        let mut bytes = Vec::new();
        varint_field(&mut bytes, 15, 2); // version
        bytes_field(&mut bytes, 1, name.as_bytes());
        for feature in &layer.features {
            bytes_field(&mut bytes, 2, feature);
        }
        for key in &layer.keys {
            bytes_field(&mut bytes, 3, key.as_bytes());
        }
        for value in &layer.values {
            let mut encoded = Vec::new();
            match value {
                Value::Str(text) => bytes_field(&mut encoded, 1, text.as_bytes()),
                Value::Num(number) => {
                    varint(&mut encoded, (3 << 3) | 1);
                    encoded.extend(number.to_le_bytes());
                }
                Value::Bool(flag) => varint_field(&mut encoded, 7, *flag as u64),
            }
            bytes_field(&mut bytes, 4, &encoded);
        }
        varint_field(&mut bytes, 5, extent as u64);
        bytes_field(&mut tile, 3, &bytes);
    }
    Some(tile)
}

/// A geometry's drawing commands in tile units, or `None` if nothing of it is left at this size.
fn encode_geometry(geometry: &Geometry, to_tile: &impl Fn(&Mercator) -> [f64; 2], tolerance: f64) -> Option<Vec<u32>> {
    let quantize = |points: &[[f64; 2]]| -> Vec<[i64; 2]> {
        let mut out: Vec<[i64; 2]> = Vec::with_capacity(points.len());
        for p in points {
            let q = [p[0].round() as i64, p[1].round() as i64];
            if out.last() != Some(&q) {
                out.push(q);
            }
        }
        out
    };
    let mut commands = Vec::new();
    let mut cursor = [0i64; 2];
    let mut draw = |commands: &mut Vec<u32>, points: &[[i64; 2]], close: bool| {
        commands.push(command(1, 1));
        push_point(commands, &mut cursor, points[0]);
        commands.push(command(2, points.len() as u32 - 1));
        for &p in &points[1..] {
            push_point(commands, &mut cursor, p);
        }
        if close {
            commands.push(command(7, 1));
        }
    };
    match geometry {
        Geometry::Point(p) => {
            let [x, y] = to_tile(p);
            commands.push(command(1, 1));
            push_point(&mut commands, &mut cursor, [x.round() as i64, y.round() as i64]);
        }
        Geometry::Lines(lines) => {
            for line in lines {
                let points: Vec<[f64; 2]> = line.iter().map(to_tile).collect();
                let points = quantize(&simplify(&points, tolerance));
                // Shorter than a pixel (twice the tolerance): nothing anyone would see at this zoom.
                let length: f64 =
                    points.windows(2).map(|w| ((w[1][0] - w[0][0]) as f64).hypot((w[1][1] - w[0][1]) as f64)).sum();
                if points.len() >= 2 && length >= tolerance * 2.0 {
                    draw(&mut commands, &points, false);
                }
            }
        }
        Geometry::Polygon(rings) => {
            for (index, ring) in rings.iter().enumerate() {
                let mut points: Vec<[f64; 2]> = ring.iter().map(to_tile).collect();
                points.push(points[0]);
                let mut points = quantize(&simplify(&points, tolerance));
                if points.len() > 1 && points.first() == points.last() {
                    points.pop();
                }
                if points.len() < 3 {
                    if index == 0 {
                        return None;
                    }
                    continue;
                }
                // Outlines turn clockwise on screen (positive area), holes the other way.
                let area = signed_area(&points);
                if area == 0 {
                    if index == 0 {
                        return None;
                    }
                    continue;
                }
                if (area > 0) != (index == 0) {
                    points.reverse();
                }
                draw(&mut commands, &points, true);
            }
        }
    }
    (!commands.is_empty()).then_some(commands)
}

fn command(id: u32, count: u32) -> u32 {
    (id & 7) | (count << 3)
}

fn push_point(commands: &mut Vec<u32>, cursor: &mut [i64; 2], p: [i64; 2]) {
    let zigzag = |n: i64| ((n << 1) ^ (n >> 63)) as u32;
    commands.push(zigzag(p[0] - cursor[0]));
    commands.push(zigzag(p[1] - cursor[1]));
    *cursor = p;
}

fn varint(bytes: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        bytes.push((value as u8) | 0x80);
        value >>= 7;
    }
    bytes.push(value as u8);
}

fn varint_field(bytes: &mut Vec<u8>, field: u32, value: u64) {
    varint(bytes, (field as u64) << 3);
    varint(bytes, value);
}

fn bytes_field(bytes: &mut Vec<u8>, field: u32, value: &[u8]) {
    varint(bytes, ((field as u64) << 3) | 2);
    varint(bytes, value.len() as u64);
    bytes.extend(value);
}

/// Tiles written so far per zoom, for a summary.
pub fn count_tiles(dir: &Path) -> HashMap<u8, usize> {
    let mut counts = HashMap::new();
    for z in 0..=24u8 {
        let Ok(columns) = std::fs::read_dir(dir.join(z.to_string())) else { continue };
        let tiles = columns
            .flatten()
            .filter_map(|column| std::fs::read_dir(column.path()).ok())
            .map(|tiles| tiles.count())
            .sum();
        counts.insert(z, tiles);
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clips_lines_into_the_pieces_inside() {
        let line = vec![[-1.0, 0.5], [0.5, 0.5], [0.5, 2.0], [0.7, 0.2], [0.8, 0.2]];
        let pieces = clip_line(&line, [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(pieces.len(), 2);
        assert_eq!(pieces[0], vec![[0.0, 0.5], [0.5, 0.5], [0.5, 1.0]]);
        assert_eq!(pieces[1].last(), Some(&[0.8, 0.2]));
    }

    #[test]
    fn clips_rings_along_the_edges() {
        let ring = vec![[-1.0, -1.0], [2.0, -1.0], [2.0, 2.0], [-1.0, 2.0]];
        let clipped = clip_ring(&ring, [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(clipped.len(), 4);
        assert!(clipped.iter().all(|p| (0.0..=1.0).contains(&p[0]) && (0.0..=1.0).contains(&p[1])));
    }

    #[test]
    fn encodes_commands_with_zigzag_deltas_and_outlines_clockwise() {
        // A square drawn anticlockwise on screen (y down) comes out clockwise.
        let to_tile = |p: &Mercator| [p[0], p[1]];
        let square = Geometry::Polygon(vec![vec![[0.0, 0.0], [0.0, 10.0], [10.0, 10.0], [10.0, 0.0]]]);
        let commands = encode_geometry(&square, &to_tile, 0.0).unwrap();
        // Reversed: it starts from (10, 0), then runs down the right side first. zigzag(10) = 20.
        assert_eq!(commands[0], command(1, 1));
        assert_eq!(&commands[1..3], &[20, 0]); // MoveTo(10, 0)
        assert_eq!(commands[3], command(2, 3));
        assert_eq!(&commands[4..6], &[0, 20]); // LineTo(10, 10)
        assert_eq!(*commands.last().unwrap(), command(7, 1));
    }

    #[test]
    fn encodes_a_place_as_one_point_and_clips_it_by_position() {
        let to_tile = |p: &Mercator| [p[0], p[1]];
        let place = Geometry::Point([3.4, 7.6]);
        // MoveTo(3, 8): zigzag 6 and 16.
        assert_eq!(encode_geometry(&place, &to_tile, 0.5).unwrap(), [command(1, 1), 6, 16]);
        assert_eq!(clip(&place, [0.0, 0.0, 5.0, 10.0]), Some(place.clone()));
        assert_eq!(clip(&place, [4.0, 0.0, 5.0, 10.0]), None);
    }

    #[test]
    fn writes_only_tiles_with_something_shown() {
        let dir = std::env::temp_dir().join(format!("westplay-tiles-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // A short road near the middle of the world, shown from zoom 2.
        let road = |minzoom, maxzoom| Feature {
            layer: "road",
            minzoom,
            maxzoom,
            props: vec![("class", Value::Str("local".into())), ("width", Value::Num(8.0))],
            geometry: Geometry::Lines(vec![vec![[0.5, 0.5], [0.51, 0.51]]]),
        };
        let options = TileOptions { min_zoom: 1, max_zoom: 4, ..TileOptions::default() };
        let written = write_tiles(vec![road(2, u8::MAX)], options, &dir, &mut |_| {}).unwrap();
        let counts = count_tiles(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!counts.contains_key(&1), "nothing shows at zoom 1");
        assert!(counts[&2] >= 1 && counts[&4] >= 1);
        assert_eq!(written, counts.values().sum::<usize>());

        // Shown up to zoom 3 only: no tiles past it.
        write_tiles(vec![road(2, 3)], options, &dir, &mut |_| {}).unwrap();
        let counts = count_tiles(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(counts[&3] >= 1 && !counts.contains_key(&4));

        // Shorter than a pixel at zoom 2 (a 512-pixel tile is a quarter of the world across), but not at 3.
        let short = Feature { geometry: Geometry::Lines(vec![vec![[0.5, 0.5], [0.5003, 0.5]]]), ..road(2, u8::MAX) };
        write_tiles(vec![short], options, &dir, &mut |_| {}).unwrap();
        let counts = count_tiles(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!counts.contains_key(&2) && counts[&3] >= 1);
    }
}
