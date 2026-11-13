//! Vector tiles: the map cut into squares per zoom level (Web Mercator, the Mapbox Vector Tile format), so
//! the Maps app loads only what's on screen, at the detail its zoom needs.
//!
//! Tiles are made top down, like geojson-vt: each tile's shapes are clipped from its parent's, so even the
//! sea's huge outlines are cut cheaply. A feature appears from its `minzoom` on, simplified to half a
//! pixel at each zoom; the last zoom's tiles are finer still, since the map zooms in past them.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
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
    /// Adds each tile's layers to the tile already there, if there is one (a tile is a list of layers: two
    /// written one after the other read as one), rather than writing it anew: the buildings, written in a
    /// pass of their own (export.rs).
    pub append: bool,
}

impl Default for TileOptions {
    fn default() -> Self {
        Self { min_zoom: 6, max_zoom: 14, extent: 4096, max_extent: 16384, buffer: 1.0 / 64.0, append: false }
    }
}

/// Writes tile (z, x, y)'s bytes in `dir`: anew, or after what's there (`append`); with `sea`, a tile of open sea
/// alone isn't written but listed in it, and one added to after (`append`) is written with its sea under it. Whether
/// a file was written.
fn put_tile(
    dir: &Path,
    (z, x, y): (u8, u32, u32),
    bytes: &[u8],
    TileOptions { append, buffer, .. }: TileOptions,
    sea: Option<&SeaTiles>,
) -> std::io::Result<bool> {
    if let Some(sea) = sea
        && !append
        && is_sea_only(bytes, buffer)
    {
        sea.leave(z, x, y, bytes);
        return Ok(false);
    }
    let folder = dir.join(z.to_string()).join(x.to_string());
    std::fs::create_dir_all(&folder)?;
    let path = folder.join(format!("{y}.pbf"));
    if let Some(under) = sea.filter(|_| append).and_then(|sea| sea.take(z, x, y)) {
        std::fs::write(path, [under.as_slice(), bytes].concat())?;
    } else if append {
        use std::io::Write;
        std::fs::OpenOptions::new().create(true).append(true).open(path)?.write_all(bytes)?;
    } else {
        std::fs::write(path, bytes)?;
    }
    Ok(true)
}
/// The layer the open sea is drawn in: a square over the whole map, the land over it (area.rs).
pub const SEA_LAYER: &str = "sea";
/// The list of the tiles left unwritten as open sea alone, beside the tiles ([`SeaTiles`]), and the folder of each
/// zoom's sea tile.
pub const SEA_INDEX: &str = "sea.json";
const SEA_FOLDER: &str = "sea";

/// The tiles that are open sea alone ([`is_sea_only`]), left unwritten: each zoom's, and the bytes of one of them (the
/// least of those met: the same whichever thread met which first), which the cab is given in their place
/// (ui/src/cabin/maps/seaTiles.ts). A map whose roads lie far apart is mostly sea between them, and its tiles cover
/// all of it: hundreds of thousands of files of the same few bytes (a player's map of 380 map mods, 1,906 x 1,389 km,
/// 2026-11-12; owner: "let's do it", the map looking the same).
#[derive(Default)]
pub struct SeaTiles {
    inner: Mutex<SeaInner>,
}

#[derive(Default)]
struct SeaInner {
    tiles: std::collections::BTreeSet<(u8, u32, u32)>,
    bytes: std::collections::BTreeMap<u8, Vec<u8>>,
}

impl SeaTiles {
    fn lock(&self) -> std::sync::MutexGuard<'_, SeaInner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Tile (z, x, y), `bytes`, is sea alone: left unwritten.
    fn leave(&self, z: u8, x: u32, y: u32, bytes: &[u8]) {
        let mut inner = self.lock();
        inner.tiles.insert((z, x, y));
        let kept = inner.bytes.entry(z).or_insert_with(|| bytes.to_vec());
        if bytes < kept.as_slice() {
            *kept = bytes.to_vec();
        }
    }

    /// The sea under tile (z, x, y), if it was left unwritten: off the list, as the tile is written now (a pass after
    /// adds to it: the buildings, one over the water).
    fn take(&self, z: u8, x: u32, y: u32) -> Option<Vec<u8>> {
        let mut inner = self.lock();
        if !inner.tiles.remove(&(z, x, y)) {
            return None;
        }
        inner.bytes.get(&z).cloned()
    }

    /// How many tiles were left unwritten.
    pub fn len(&self) -> usize {
        self.lock().tiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Writes the list into `dir` ([`SEA_INDEX`]: by zoom, its sea tile's file and its rows' runs of tiles, each the
    /// row and its first and last column) and each zoom's sea tile; `false`, and nothing written, with none left.
    pub fn write(&self, dir: &Path) -> std::io::Result<bool> {
        let inner = self.lock();
        if inner.tiles.is_empty() {
            return Ok(false);
        }
        let mut runs: std::collections::BTreeMap<u8, Vec<[u32; 3]>> = std::collections::BTreeMap::new();
        // By zoom, then row, then column: each row's tiles side by side as one run.
        let mut by_row: Vec<(u8, u32, u32)> = inner.tiles.iter().map(|&(z, x, y)| (z, y, x)).collect();
        by_row.sort_unstable();
        for (z, y, x) in by_row {
            let zoom = runs.entry(z).or_default();
            match zoom.last_mut() {
                Some(run) if run[0] == y && run[2] + 1 == x => run[2] = x,
                _ => zoom.push([y, x, x]),
            }
        }
        std::fs::create_dir_all(dir.join(SEA_FOLDER))?;
        let mut zooms = serde_json::Map::new();
        for (z, rows) in runs {
            let file = format!("{SEA_FOLDER}/{z}.pbf");
            std::fs::write(dir.join(&file), &inner.bytes[&z])?;
            zooms.insert(z.to_string(), serde_json::json!({ "tile": file, "rows": rows }));
        }
        std::fs::write(dir.join(SEA_INDEX), serde_json::json!({ "zooms": zooms }).to_string())?;
        Ok(true)
    }
}

/// Whether a tile's bytes are open sea alone: one layer, the sea's, of one shape with nothing said of it, a square
/// round the whole tile and as far past it as its shapes reach (`buffer`, a share of the tile: the map's sea cut to a
/// tile where nothing else lies). Anything else isn't: a shore through it, an island's hole in it, a road, a field the
/// reader doesn't know; nor the sea ending at the tile's edge (the map's last edge, drawn softened there).
pub fn is_sea_only(tile: &[u8], buffer: f64) -> bool {
    sea_square(tile, buffer).unwrap_or(false)
}

/// A protocol buffer's field: its number, and a whole number or bytes.
enum Field<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
}

fn read_varint(at: &mut &[u8]) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let (&byte, rest) = at.split_first()?;
        *at = rest;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

/// The fields of a message, in order; `None` if it doesn't read (or has a field of a kind a tile's sea hasn't).
fn fields(mut at: &[u8]) -> Option<Vec<(u64, Field<'_>)>> {
    let mut out = Vec::new();
    while !at.is_empty() {
        let key = read_varint(&mut at)?;
        let field = match key & 7 {
            0 => Field::Varint(read_varint(&mut at)?),
            2 => {
                let length = usize::try_from(read_varint(&mut at)?).ok()?;
                let (bytes, rest) = (at.get(..length)?, at.get(length..)?);
                at = rest;
                Field::Bytes(bytes)
            }
            _ => return None,
        };
        out.push((key >> 3, field));
    }
    Some(out)
}

fn sea_square(tile: &[u8], buffer: f64) -> Option<bool> {
    let tile_fields = fields(tile)?;
    let [(3, Field::Bytes(layer))] = tile_fields[..] else { return Some(false) };
    let (mut name, mut features, mut extent) = (None, Vec::new(), 4096u64);
    for (number, field) in fields(layer)? {
        match (number, field) {
            (15, Field::Varint(_)) => {}
            (1, Field::Bytes(text)) => name = Some(text),
            (2, Field::Bytes(feature)) => features.push(feature),
            (5, Field::Varint(units)) => extent = units,
            // Keys or values: something said of a shape.
            _ => return Some(false),
        }
    }
    let [feature] = features[..] else { return Some(false) };
    if name != Some(SEA_LAYER.as_bytes()) {
        return Some(false);
    }
    let (mut kind, mut geometry) = (None, None);
    for (number, field) in fields(feature)? {
        match (number, field) {
            (3, Field::Varint(value)) => kind = Some(value),
            (4, Field::Bytes(commands)) => geometry = Some(commands),
            _ => return Some(false),
        }
    }
    let (Some(3), Some(mut commands)) = (kind, geometry) else { return Some(false) };
    let mut words = Vec::new();
    while !commands.is_empty() {
        words.push(read_varint(&mut commands)?);
    }
    // One ring: a move to its first point, lines to the rest, closed.
    let [move_to, first @ .., close] = &words[..] else { return Some(false) };
    let (Some(&line_to), true) = (first.get(2), *move_to == (1 << 3) | 1 && *close == (1 << 3) | 7) else {
        return Some(false);
    };
    let count = (line_to >> 3) as usize;
    if line_to & 7 != 2 || first.len() != 3 + 2 * count {
        return Some(false);
    }
    let zigzag = |word: u64| ((word >> 1) as i64) ^ -((word & 1) as i64);
    let mut point = [0i64; 2];
    let mut points = Vec::with_capacity(count + 1);
    for pair in [&first[..2], &first[3..]].concat().chunks(2) {
        point = [point[0] + zigzag(pair[0]), point[1] + zigzag(pair[1])];
        points.push(point);
    }
    let low = [points.iter().map(|p| p[0]).min()?, points.iter().map(|p| p[1]).min()?];
    let high = [points.iter().map(|p| p[0]).max()?, points.iter().map(|p| p[1]).max()?];
    let extent = i64::try_from(extent).ok()?;
    let past = (extent as f64 * buffer).round() as i64;
    // Round the whole tile and past it, each point a corner, each side along an edge (no corner skipped: not a bow
    // tie).
    let covers = low[0] <= -past && low[1] <= -past && high[0] >= extent + past && high[1] >= extent + past;
    let corners = points.iter().all(|p| (p[0] == low[0] || p[0] == high[0]) && (p[1] == low[1] || p[1] == high[1]));
    let mut distinct = points.clone();
    distinct.sort_unstable();
    distinct.dedup();
    let sides = (0..points.len()).all(|i| {
        let (a, b) = (points[i], points[(i + 1) % points.len()]);
        (a[0] == b[0]) != (a[1] == b[1]) || a == b
    });
    Some(covers && corners && distinct.len() == 4 && sides)
}

/// A feature and its bounding box (min x, min y, max x, max y).
/// A feature in a tile being made: what it is, shared by every tile it's in; its shape there, shared with the tile
/// above where it lies wholly inside that one and copied only where it's cut (every level of the first zooms copied
/// every feature whole, all of a map's features again at each: the review of 2026-11-11, M10); its box.
struct Item {
    about: Arc<About>,
    geometry: Arc<Geometry>,
    bbox: [f64; 4],
}

/// What a feature is, but its shape ([`Feature`]).
struct About {
    layer: &'static str,
    minzoom: u8,
    maxzoom: u8,
    props: Vec<(&'static str, Value)>,
}

impl Item {
    /// A feature as an item, if it has a shape.
    fn of(feature: Feature) -> Option<Self> {
        let Feature { layer, minzoom, maxzoom, props, geometry } = feature;
        let bbox = bbox(&geometry)?;
        Some(Self { about: Arc::new(About { layer, minzoom, maxzoom, props }), geometry: Arc::new(geometry), bbox })
    }
}

/// Writes the tiles of `features` into `dir` as `{z}/{x}/{y}.pbf` (only tiles with something in them; with `sea`, not
/// those of open sea alone: listed in it). Returns how many it wrote. `progress` is told how far it is, 0 to 1: the
/// first zooms a tenth, then the threads' trees as they finish.
pub fn write_tiles(
    features: Vec<Feature>,
    options: TileOptions,
    dir: &Path,
    sea: Option<&SeaTiles>,
    progress: &mut dyn FnMut(f32),
) -> std::io::Result<usize> {
    progress(0.0);
    let items: Vec<Item> = features.into_iter().filter_map(Item::of).collect();
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
                written += descend(z, x, y, tile_items, split, options, dir, sea, &mut tasks)?;
            }
        }
    }
    drop(items);
    progress(0.1);
    // The trees with the most in them first (taken from the end), so none of the longest is left to run alone at the
    // end (the review of 2026-11-11, minor 3).
    tasks.sort_by_key(|(_, _, items)| items.len());
    let total = tasks.len().max(1);
    let finished = AtomicUsize::new(0);
    let tasks = Mutex::new(tasks);
    let written = Mutex::new(written);
    let failure = Mutex::new(None);
    let threads = crate::threads::for_a_build();
    // Threads still working: each counts itself out when it ends, however it ends (a panic too).
    let working = AtomicUsize::new(threads);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                let _leaving = crate::threads::Leaving(&working);
                // Until one can't be written: then the first error is the answer, and the rest aren't made (the review
                // of 2026-11-11, minor 2: a full disk had every tile made before its error came back).
                while failure.lock().unwrap().is_none()
                    && let Some((x, y, tile_items)) = tasks.lock().unwrap().pop()
                {
                    match make_tile(split, x, y, tile_items, options, dir, sea) {
                        Ok(count) => *written.lock().unwrap() += count,
                        Err(error) => {
                            failure.lock().unwrap().get_or_insert(error);
                        }
                    }
                    finished.fetch_add(1, Ordering::Relaxed);
                }
            });
        }
        // Meanwhile, this thread tells how far the trees are, until every thread has ended.
        while crate::threads::any_working(&working) {
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
    sea: Option<&SeaTiles>,
    tasks: &mut Vec<(u32, u32, Vec<Item>)>,
) -> std::io::Result<usize> {
    if z == split {
        tasks.push((x, y, items));
        return Ok(0);
    }
    let mut written = 0;
    if let Some(bytes) = encode_tile(&items, z, x, y, options.extent, options) {
        written += usize::from(put_tile(dir, (z, x, y), &bytes, options, sea)?);
    }
    for (cx, cy) in [(2 * x, 2 * y), (2 * x + 1, 2 * y), (2 * x, 2 * y + 1), (2 * x + 1, 2 * y + 1)] {
        let child = clip_items(&items, z + 1, tile_bounds(z + 1, cx, cy, options.buffer));
        if !child.is_empty() {
            written += descend(z + 1, cx, cy, child, split, options, dir, sea, tasks)?;
        }
    }
    Ok(written)
}

/// Writes tile (z, x, y) from `items` (already clipped to it), then its children's.
fn make_tile(
    z: u8,
    x: u32,
    y: u32,
    items: Vec<Item>,
    options: TileOptions,
    dir: &Path,
    sea: Option<&SeaTiles>,
) -> std::io::Result<usize> {
    let mut written = 0;
    let last = z == options.max_zoom;
    let extent = if last { options.max_extent } else { options.extent };
    let bytes = encode_tile(&items, z, x, y, extent, options);
    if let Some(bytes) = bytes {
        written += usize::from(put_tile(dir, (z, x, y), &bytes, options, sea)?);
    }
    if !last {
        for (cx, cy) in [(2 * x, 2 * y), (2 * x + 1, 2 * y), (2 * x, 2 * y + 1), (2 * x + 1, 2 * y + 1)] {
            let child = clip_items(&items, z + 1, tile_bounds(z + 1, cx, cy, options.buffer));
            if !child.is_empty() {
                written += make_tile(z + 1, cx, cy, child, options, dir, sea)?;
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
            item.about.maxzoom >= z
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
            let geometry = if inside { item.geometry.clone() } else { Arc::new(clip(&item.geometry, bounds)?) };
            let bbox = bbox(&geometry)?;
            Some(Item { about: item.about.clone(), geometry, bbox })
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
    /// Where each key and value is in its list (a list was searched from its start for each: thousands of a dense
    /// tile's heights, the review of 2026-11-11, minor 4).
    key_at: HashMap<&'static str, u32>,
    value_at: HashMap<ValueKey, u32>,
}

impl LayerBuilder {
    fn key(&mut self, key: &'static str) -> u32 {
        *self.key_at.entry(key).or_insert_with(|| {
            self.keys.push(key);
            (self.keys.len() - 1) as u32
        })
    }

    fn value(&mut self, value: &Value) -> u32 {
        let next = self.values.len() as u32;
        let at = match ValueKey::of(value) {
            Some(key) => *self.value_at.entry(key).or_insert(next),
            None => next,
        };
        if at == next {
            self.values.push(value.clone());
        }
        at
    }
}

/// A value as the index of a tile's values finds it: as values are equal (a number by its bits, but 0 and -0 the same);
/// a number that isn't one (NaN) equals nothing, so it's never found.
#[derive(Clone, PartialEq, Eq, Hash)]
enum ValueKey {
    Str(String),
    Num(u64),
    Bool(bool),
}

impl ValueKey {
    fn of(value: &Value) -> Option<Self> {
        Some(match value {
            Value::Str(text) => Self::Str(text.clone()),
            Value::Num(number) if number.is_nan() => return None,
            Value::Num(number) => Self::Num(if *number == 0.0 { 0f64.to_bits() } else { number.to_bits() }),
            Value::Bool(yes) => Self::Bool(*yes),
        })
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
    for item in items.iter().filter(|item| (item.about.minzoom..=item.about.maxzoom).contains(&z)) {
        let Some(geometry) = encode_geometry(&item.geometry, &to_tile, tolerance) else { continue };
        let layer = match layers.iter().position(|(name, _)| *name == item.about.layer) {
            Some(index) => &mut layers[index].1,
            None => {
                layers.push((item.about.layer, LayerBuilder::default()));
                &mut layers.last_mut().unwrap().1
            }
        };
        let mut tags = Vec::new();
        for (key, value) in &item.about.props {
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
        let kind = match *item.geometry {
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
                // A ring with no points (the review of 2026-11-11, minor 1: it panicked, closing it on its first).
                let Some(first) = ring.first() else {
                    if index == 0 {
                        return None;
                    }
                    continue;
                };
                let mut points: Vec<[f64; 2]> = ring.iter().map(to_tile).collect();
                points.push(to_tile(first));
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

pub(crate) fn command(id: u32, count: u32) -> u32 {
    (id & 7) | (count << 3)
}

pub(crate) fn push_point(commands: &mut Vec<u32>, cursor: &mut [i64; 2], p: [i64; 2]) {
    let zigzag = |n: i64| ((n << 1) ^ (n >> 63)) as u32;
    commands.push(zigzag(p[0] - cursor[0]));
    commands.push(zigzag(p[1] - cursor[1]));
    *cursor = p;
}

pub(crate) fn varint(bytes: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        bytes.push((value as u8) | 0x80);
        value >>= 7;
    }
    bytes.push(value as u8);
}

pub(crate) fn varint_field(bytes: &mut Vec<u8>, field: u32, value: u64) {
    varint(bytes, (field as u64) << 3);
    varint(bytes, value);
}

pub(crate) fn bytes_field(bytes: &mut Vec<u8>, field: u32, value: &[u8]) {
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

    /// A polygon with an empty ring: an empty hole is passed over, an empty outline is nothing; no panic.
    #[test]
    fn an_empty_ring_is_passed_over() {
        let to_tile = |p: &Mercator| [p[0] * 4096.0, p[1] * 4096.0];
        let square = vec![[0.1, 0.1], [0.9, 0.1], [0.9, 0.9], [0.1, 0.9]];
        let with_hole = Geometry::Polygon(vec![square.clone(), Vec::new()]);
        let alone = Geometry::Polygon(vec![square]);
        assert_eq!(encode_geometry(&with_hole, &to_tile, 0.5), encode_geometry(&alone, &to_tile, 0.5));
        assert_eq!(encode_geometry(&Geometry::Polygon(vec![Vec::new()]), &to_tile, 0.5), None);
    }

    /// A tile's keys and values are each listed once, in the order first met, as equal values are (0 and -0 one
    /// value; a NaN never equal, so listed each time).
    #[test]
    fn keys_and_values_are_listed_once_as_equal_values_are() {
        let mut layer = LayerBuilder::default();
        assert_eq!((layer.key("kind"), layer.key("height"), layer.key("kind")), (0, 1, 0));
        let values = [
            Value::Str("road".into()),
            Value::Num(0.0),
            Value::Num(-0.0),
            Value::Str("road".into()),
            Value::Num(f64::NAN),
            Value::Num(f64::NAN),
            Value::Bool(true),
            Value::Num(12.5),
            Value::Num(12.5),
        ];
        let at: Vec<u32> = values.iter().map(|value| layer.value(value)).collect();
        assert_eq!(at, [0, 1, 1, 0, 2, 3, 4, 5, 5]);
        assert_eq!(layer.values.len(), 6);
    }

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
    fn adds_a_layer_to_the_tiles_already_written() {
        let dir = std::env::temp_dir().join(format!("westplay-tiles-append-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let feature = |layer, at: f64| Feature {
            layer,
            minzoom: 4,
            maxzoom: u8::MAX,
            props: Vec::new(),
            geometry: Geometry::Polygon(vec![vec![
                [at, at],
                [at + 0.001, at],
                [at + 0.001, at + 0.001],
                [at, at + 0.001],
            ]]),
        };
        let options = TileOptions { min_zoom: 4, max_zoom: 4, ..TileOptions::default() };
        let other = TileOptions { min_zoom: 4, max_zoom: 4, append: true, ..TileOptions::default() };
        // Roads in one tile; then buildings in it and in another.
        write_tiles(vec![feature("road", 0.5)], options, &dir, None, &mut |_| {}).unwrap();
        let tile = |x: u32, y: u32| dir.join("4").join(x.to_string()).join(format!("{y}.pbf"));
        let roads = std::fs::read(tile(8, 8)).unwrap();
        write_tiles(vec![feature("building", 0.5), feature("building", 0.1)], other, &dir, None, &mut |_| {}).unwrap();
        let both = std::fs::read(tile(8, 8)).unwrap();
        let alone = std::fs::read(tile(1, 1)).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        // The tile already there: its roads as they were, the buildings' layer after them.
        assert!(both.len() > roads.len() && both[..roads.len()] == roads[..]);
        let added = encode_tile(
            &[Item::of(feature("building", 0.5)).unwrap()],
            4,
            8,
            8,
            other.max_extent,
            other,
        )
        .unwrap();
        assert_eq!(both[roads.len()..], added[..]);
        // A tile with buildings alone: written anew.
        assert!(!alone.is_empty());
    }

    /// How far the tiles' shapes reach past them, as written.
    const BUFFER: f64 = 1.0 / 64.0;

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len()).step_by(2).map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap()).collect()
    }

    /// A square of `layer` from `low` to `high` (world units) over every zoom from 1; with `hole`, an island in it.
    fn square(layer: &'static str, low: f64, high: f64, hole: Option<[f64; 2]>) -> Feature {
        let ring = |a: f64, b: f64| vec![[a, a], [b, a], [b, b], [a, b]];
        let mut rings = vec![ring(low, high)];
        rings.extend(hole.map(|[a, b]| ring(a, b)));
        Feature { layer, minzoom: 1, maxzoom: u8::MAX, props: Vec::new(), geometry: Geometry::Polygon(rings) }
    }

    #[test]
    fn a_tile_of_open_sea_alone_is_told_from_any_other() {
        // The base map's own (2026-11-10): open sea at zoom 14, its square started at two corners; at zoom 12.
        for alone in [
            "1a2578020a0373656112181803221409808402ff031a00808802ff87020000ff87020f28808001",
            "1a2578020a0373656112181803221409ff038084021a00ff870280880200008088020f28808001",
            "1a1f78020a0373656112131803220f0980417f1a008042ff410000ff410f288020",
        ] {
            assert!(is_sea_only(&hex(alone), BUFFER), "{alone}");
        }
        // A shore through it (the base map's: the sea over part of the tile).
        let shore = "1a2278020a0373656112151803221109ef03ff031a008088020f0000ff87020f28808001";
        assert!(!is_sea_only(&hex(shore), BUFFER));
        // The same square in another layer; cut short; nothing.
        let land = "1a2578020a036c616e12181803221409808402ff031a00808802ff87020000ff87020f28808001";
        assert!(!is_sea_only(&hex(land), BUFFER));
        assert!(!is_sea_only(&hex("1a2578020a0373656112181803221409808402ff031a00808802ff87"), BUFFER));
        assert!(!is_sea_only(&[], BUFFER));
        // As the writer makes them: the sea alone over a tile; with an island in it; with a road over it.
        let options = TileOptions::default();
        let tile = |features: Vec<Feature>| {
            let items: Vec<Item> = features.into_iter().filter_map(Item::of).collect();
            let clipped = clip_items(&items, 4, tile_bounds(4, 8, 8, options.buffer));
            encode_tile(&clipped, 4, 8, 8, options.extent, options).unwrap()
        };
        assert!(is_sea_only(&tile(vec![square(SEA_LAYER, 0.25, 0.75, None)]), BUFFER));
        assert!(!is_sea_only(&tile(vec![square(SEA_LAYER, 0.25, 0.75, Some([0.51, 0.52]))]), BUFFER));
        let road = square("road", 0.51, 0.52, None);
        assert!(!is_sea_only(&tile(vec![square(SEA_LAYER, 0.25, 0.75, None), road]), BUFFER));
        // The sea ending at the tile's edge (the map's last), not past it: drawn softened there, so written.
        assert!(!is_sea_only(&tile(vec![square(SEA_LAYER, 0.5, 0.75, None)]), BUFFER));
    }

    #[test]
    fn tiles_of_open_sea_alone_are_listed_not_written_and_given_back_under_what_comes_after() {
        let dir = crate::test_dir::TempDir::new();
        let options = TileOptions { min_zoom: 1, max_zoom: 4, ..TileOptions::default() };
        // The sea over about a quarter of the world (its shore through the tiles at its edge), and an island in it.
        let island = square("land", 0.4, 0.41, None);
        let sea = SeaTiles::default();
        let written =
            write_tiles(vec![square(SEA_LAYER, 0.26, 0.74, None), island], options, &dir.path, Some(&sea), &mut |_| {})
                .unwrap();
        let file = |z: u8, x: u32, y: u32| dir.path.join(z.to_string()).join(x.to_string()).join(format!("{y}.pbf"));
        let listed: Vec<(u8, u32, u32)> = sea.lock().tiles.iter().copied().collect();
        // At zoom 4 a tile is a sixteenth of the world: the sea's, but the island's and the shore's.
        assert!(listed.contains(&(4, 10, 10)) && !listed.contains(&(4, 6, 6)));
        assert!(listed.iter().all(|&(z, x, y)| !file(z, x, y).exists()), "listed, not written");
        assert!(file(4, 6, 6).exists() && file(4, 4, 4).exists(), "the island's and the shore's are written");
        assert_eq!(written, count_tiles(&dir.path).values().sum::<usize>());
        // Each zoom's sea tile: the least of those met, open sea alone.
        let kept = sea.lock().bytes.clone();
        assert!(kept.values().all(|bytes| is_sea_only(bytes, BUFFER)));
        // Buildings after, one of them over the open sea: its tile written, the sea under it, off the list.
        let append = TileOptions { append: true, ..options };
        write_tiles(vec![square("building", 0.6, 0.601, None)], append, &dir.path, Some(&sea), &mut |_| {}).unwrap();
        let over = std::fs::read(file(4, 9, 9)).unwrap();
        assert_eq!(over[..kept[&4].len()], kept[&4][..]);
        assert!(!sea.lock().tiles.contains(&(4, 9, 9)) && sea.lock().tiles.contains(&(4, 10, 10)));
        // The list: by zoom, its sea tile and its rows' runs.
        assert!(sea.write(&dir.path).unwrap());
        let index: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.path.join(SEA_INDEX)).unwrap()).unwrap();
        let zoom = &index["zooms"]["4"];
        assert_eq!(std::fs::read(dir.path.join(zoom["tile"].as_str().unwrap())).unwrap(), kept[&4]);
        let mut again = std::collections::BTreeSet::new();
        for (z, zoom) in index["zooms"].as_object().unwrap() {
            for run in zoom["rows"].as_array().unwrap() {
                let [y, first, last] = [0, 1, 2].map(|i| run[i].as_u64().unwrap() as u32);
                again.extend((first..=last).map(|x| (z.parse::<u8>().unwrap(), x, y)));
            }
        }
        assert_eq!(again, sea.lock().tiles.clone());
        // Nothing listed: nothing written.
        assert!(!SeaTiles::default().write(&dir.path.join("none")).unwrap());
    }

    #[test]
    fn each_zooms_sea_tile_is_the_least_met_whatever_the_order() {
        let (one, other) = (SeaTiles::default(), SeaTiles::default());
        one.leave(14, 1, 1, b"b");
        one.leave(14, 2, 1, b"a");
        other.leave(14, 2, 1, b"a");
        other.leave(14, 1, 1, b"b");
        assert_eq!(one.lock().bytes, other.lock().bytes);
        assert_eq!(one.take(14, 1, 1), Some(b"a".to_vec()));
        assert_eq!(one.take(14, 1, 1), None);
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
        let written = write_tiles(vec![road(2, u8::MAX)], options, &dir, None, &mut |_| {}).unwrap();
        let counts = count_tiles(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!counts.contains_key(&1), "nothing shows at zoom 1");
        assert!(counts[&2] >= 1 && counts[&4] >= 1);
        assert_eq!(written, counts.values().sum::<usize>());

        // Shown up to zoom 3 only: no tiles past it.
        write_tiles(vec![road(2, 3)], options, &dir, None, &mut |_| {}).unwrap();
        let counts = count_tiles(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(counts[&3] >= 1 && !counts.contains_key(&4));

        // Shorter than a pixel at zoom 2 (a 512-pixel tile is a quarter of the world across), but not at 3.
        let short = Feature { geometry: Geometry::Lines(vec![vec![[0.5, 0.5], [0.5003, 0.5]]]), ..road(2, u8::MAX) };
        write_tiles(vec![short], options, &dir, None, &mut |_| {}).unwrap();
        let counts = count_tiles(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!counts.contains_key(&2) && counts[&3] >= 1);
    }
}
