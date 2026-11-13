//! The map for the Maps app, exported from the mounted game (its DLCs and map mods too): vector tiles of one
//! map (`europe`, or a standalone map mod's), its place names and where the player can be routed, into a
//! folder the app serves the cab's map from.
//!
//! Writes `<out>/tiles-<time>/{z}/{x}/{y}.pbf`, the road signs' sheet and `places.json` beside them, and
//! `<out>/map.json`: the tiles' folder, zooms and bounds, where the map starts, its place names, and what it
//! was built from (`source`, the caller's: what decides whether it's built again). Each export's tiles go in a
//! folder of their own, so a map running from the last export keeps its tiles until it next loads.

use std::collections::HashSet;
use std::fmt;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;

use crate::area::{self, Bounds, Counts};
use crate::buildings;
use crate::countries;
use crate::dlc;
use crate::files::{FileError, GameFiles};
use crate::landmarks;
use crate::locale::Strings;
use crate::meshes;
use crate::no_entry;
use crate::routing;
use crate::sea::SeaOptions;
use crate::sector::ItemData;
use crate::signs;
use crate::surface;
use crate::tiles::{self, TileOptions};
use crate::token::{string_to_token, token_to_string};
use crate::world::{self, Map, MapError, Prefabs};
use crate::world_picture::{self, WorldPicture};

/// Where a map starts when there's no truck to follow (a browser, the companion's screen): a city of the base game's,
/// so every player's map has it. ETS2's Calais, where WestPlay's map began; ATS's San Francisco (owner 2026-11-04:
/// the middle of the map, which the states a player owns decide, can be empty desert). A map without it starts at
/// its middle.
fn start_city(map: &str) -> &'static str {
    match map {
        "usa" => "san_francisc",
        _ => "calais",
    }
}
/// Cells of the grid the open sea is found on for a whole map, metres: fine enough for the coast, coarse
/// enough for a whole map in memory (tens of millions of cells).
const WHOLE_MAP_SEA_CELL: f32 = 20.0;
/// The folder of the surfaces' tiles, in the tiles' own.
const SURFACE_FOLDER: &str = "surface";
/// How long an export's tiles are kept after the next export, seconds: a day.
const KEEP_TILES_SECONDS: u64 = 24 * 60 * 60;

/// What to export.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// The map: `europe` (the base game's, with its DLCs and the mods that change it) or a standalone map's.
    pub map: String,
    /// The language place names come in (`tr_tr`), or their own.
    pub language: Option<String>,
    /// Only the area around a city (its id, and a radius in metres), for development.
    pub around: Option<(String, f32)>,
    /// What the map is built from (the caller's), kept in `map.json` to tell whether it's built again.
    pub source: Option<serde_json::Value>,
    /// The time zone the SDK's game time is in (minutes; the game's: `None`, ETS2's), for the cab's clock
    /// (`countries.json`'s `base`). Not part of what it's built from: a map built before has none, and is ETS2's.
    pub game_zone: Option<i32>,
    /// The sea found by what's nearest each empty place (`SeaOptions::nearest`: ATS's map, land inland out to
    /// its edges), not by the regions reaching the edge (ETS2's).
    pub sea_by_nearest: bool,
    /// The game's list of its map DLCs' guards (dlc.rs `read_guards`, from its executable: ATS's), for drawing
    /// only what the player's game has; `None`: the map drawn whole (ETS2's, whose guards aren't known).
    pub dlc_guards: Option<Vec<String>>,
    /// Set to stop the export (another thread may): it ends at its next step with [`ExportError::Stopped`],
    /// letting go of all it holds. The map it was writing isn't touched: its manifest is written last.
    pub stop: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    /// What the caller left out of it (map mods whose files couldn't be read), kept in `map.json` as `leftOut`:
    /// written with the map, at once (kept in a file of its own after the manifest, a failure writing it failed a
    /// built map, and a build ended between the two left a map whose mods left out were forgotten: the review of
    /// 2026-11-11, minor 26).
    pub left_out: Option<serde_json::Value>,
    /// Every piece of painted water drawn once all the land is in, as a map whose water came late is drawn again
    /// ([`Counts::water_late`]), not each as soon as the last land over it is: more memory, the same map. For the
    /// test that holds the two to the same bytes (the review of 2026-11-11, tests).
    pub water_held: bool,
}

/// How far an export has got.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Progress {
    pub stage: Stage,
    /// Of the whole export, 0 to 1.
    pub done: f32,
    /// How big the map is, told once, as drawing starts (what the drawing's memory grows with).
    pub size: Option<MapSize>,
}

/// How big a map is to draw: its items, how far its roads reach (min x, min z, max x, max z, metres), and
/// the sea's grid over that (cells, and a cell's size in metres).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapSize {
    pub items: usize,
    pub extent: [f32; 4],
    pub sea_cells: u64,
    pub sea_cell: f32,
}

/// Where each step of a build starts, as a share of the whole, by the time each took: the base map (335 s) and ProMods
/// 2.84 (446 s), normal effort, 2026-11-12 (`WESTPLAY_MAP_PHASES`, phases.rs). The bar gave the tiles a quarter and
/// told nothing through the surfaces' sorting and the buildings' models, a fifth of the time, and a ninth to the block
/// pass, nearly half of it (the review of 2026-11-11, M3; players' "stuck at 92%"). Measured again when a step's work
/// changes much. The companion's bar has [`share::READING`], [`share::TILES`] and [`share::SAVING`] as its stages' ends (ui/src/companion/room/room.ts
/// `MAP_STAGES`).
pub mod share {
    /// The definitions and water materials, from 0; then the sectors read.
    pub const SECTORS: f32 = 0.005;
    /// The sectors' end, and [`super::Stage::Reading`]'s: then the landmarks, countries, DLCs, strings, and the route
    /// network.
    pub const READING: f32 = 0.03;
    /// The surfaces (surface/mod.rs: the junctions sorted, the block pass).
    pub const SURFACES: f32 = 0.035;
    /// The road signs' sheet and the world's picture.
    pub const SIGNS: f32 = 0.761;
    /// The area (area.rs: the items, the water, the sea, the land, the places).
    pub const AREA: f32 = 0.762;
    /// The raised roads over water, the buildings' placements, labels, places, no-entry signs.
    pub const TAIL: f32 = 0.875;
    /// The tiles ([`super::Stage::Tiles`] from here).
    pub const TILES: f32 = 0.881;
    /// The buildings in their real shapes (meshes.rs).
    pub const MESHES: f32 = 0.902;
    /// The walled buildings fitted and the buildings' tiles.
    pub const BUILDING_TILES: f32 = 0.994;
    /// The sheets, places, countries, route graph and manifest written, older tile sets let go (the host's "saving").
    pub const SAVING: f32 = 0.996;
}

/// Where a build is after its area was drawn a second time ([`Counts::water_late`]): the rest of the bar from
/// [`share::TAIL`] shared over what's done again and what's after, by the time each takes (never back down: the host
/// passes nothing lower on). `again`: the share of the reading done again (0 to 1), of the area drawn again, or `None`
/// for a step after them, at `done` (its place on the bar had there been none again).
fn after_again(again: Again, done: f32) -> f32 {
    let (read, area, rest) = (share::READING - share::SECTORS, share::TAIL - share::AREA, 1.0 - share::TAIL);
    let all = read + area + rest;
    let from = share::TAIL;
    let at = match again {
        Again::Reading(read_done) => read * read_done,
        Again::Area(area_done) => read + area * area_done,
        Again::After => read + area + (done - share::TAIL).max(0.0),
    };
    from + (1.0 - from) * at / all
}

#[derive(Clone, Copy)]
enum Again {
    Reading(f32),
    Area(f32),
    After,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// The map's sectors and definitions.
    Reading,
    /// Roads, junctions, water and land drawn as shapes.
    Drawing,
    /// The shapes cut into tiles and written.
    Tiles,
    Done,
}

/// What an export made.
#[derive(Debug)]
pub struct Summary {
    pub items: usize,
    pub counts: Counts,
    pub tiles: usize,
    /// The tiles of open sea alone, not written (tiles.rs `SeaTiles`).
    pub sea_tiles: usize,
    pub places: usize,
    /// The tiles' folder, in `out`.
    pub folder: String,
    /// Road signs whose image couldn't be read (left out).
    pub missing_signs: Vec<String>,
    /// Why the countries' time zones couldn't be read, if they couldn't (the map built without them: the cab's
    /// clock shows the game's time).
    pub time_zones_unread: Option<String>,
    /// The buildings raised (buildings.rs).
    pub buildings: usize,
    /// How well the game's picture of its world fitted the map (world_picture.rs: the share of its road nodes off
    /// the picture's sea), and whether it was used (it fits [`world_picture::FITS`]); `None`: not looked at (a
    /// map whose world lies in the sea, ETS2's, or no placement known).
    pub world: Option<(f32, bool)>,
    /// What was left out as the player's game hasn't it (dlc.rs: a guard's DLC, a state it hasn't); `None`: the
    /// map drawn whole ([`Options::dlc_guards`] not given).
    pub not_owned: Option<dlc::Dropped>,
    /// The definitions and prefabs' descriptors that couldn't be read or understood, so what they define isn't on
    /// the map ([`GameFiles::note_unread`]).
    pub unread: crate::files::Unread,
}

#[derive(Debug)]
pub enum ExportError {
    Files(FileError),
    Map(MapError),
    /// The map has no roads (no sectors of it are mounted).
    Empty(String),
    /// The city asked for isn't on the map.
    UnknownCity(String),
    Io(std::io::Error),
    /// Stopped by the caller ([`Options::stop`]).
    Stopped,
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Files(error) => write!(f, "{error}"),
            Self::Map(error) => write!(f, "{error}"),
            Self::Empty(map) => write!(f, "the map {map} has no roads"),
            Self::UnknownCity(city) => write!(f, "{city} isn't on the map"),
            Self::Io(error) => write!(f, "{error}"),
            Self::Stopped => write!(f, "stopped"),
        }
    }
}

impl std::error::Error for ExportError {}

impl From<FileError> for ExportError {
    fn from(error: FileError) -> Self {
        Self::Files(error)
    }
}

impl From<MapError> for ExportError {
    fn from(error: MapError) -> Self {
        Self::Map(error)
    }
}

impl From<std::io::Error> for ExportError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// The game's picture of its world's sea, beside the tiles.
const WORLD_FILE: &str = "world.json";

/// The road nodes of a map, where they lie on the ground plane: what the game's picture of its world is checked
/// against.
fn road_points(map: &world::Map) -> impl Iterator<Item = [f32; 2]> + '_ {
    map.items
        .iter()
        .filter_map(|item| match &item.data {
            ItemData::Road { start_node, end_node, .. } => Some([*start_node, *end_node]),
            _ => None,
        })
        .flatten()
        .filter_map(|uid| map.nodes.get(&uid))
        .map(|node| [node.position[0], node.position[2]])
}

/// The game's picture of its world's sea as GeoJSON for the cab (`kind` `sea`), degrees as the tiles have them,
/// traced to its pixel's size (the picture being no finer): drawn over the cab's ground.
fn world_json(world: &WorldPicture) -> serde_json::Value {
    let ring = |points: &[[f32; 2]]| -> Vec<[f64; 2]> {
        let mut ring: Vec<[f64; 2]> = points
            .iter()
            .map(|p| {
                let [lng, lat] = area::to_lng_lat(*p);
                [(lng * 1e5).round() / 1e5, (lat * 1e5).round() / 1e5]
            })
            .collect();
        if let Some(&first) = ring.first() {
            ring.push(first);
        }
        ring
    };
    let sea: Vec<Vec<Vec<[f64; 2]>>> = world
        .sea_shapes(world.placement.scale)
        .iter()
        .map(|shape| std::iter::once(ring(&shape.outline)).chain(shape.holes.iter().map(|hole| ring(hole))).collect())
        .collect();
    let feature = json!({ "type": "Feature", "properties": { "kind": "sea" }, "geometry": { "type": "MultiPolygon", "coordinates": sea } });
    json!({ "type": "FeatureCollection", "features": [feature] })
}

/// Where the game's picture of its world lies, as the cab's map has it: `[west, south, east, north]`, degrees.
fn world_bounds(world: &WorldPicture) -> [f64; 4] {
    let [x0, z0, x1, z1] = world.bounds();
    let [west, north] = area::to_lng_lat([x0, z0]);
    let [east, south] = area::to_lng_lat([x1, z1]);
    [west, south, east, north]
}

/// Where the picture's land is cut off (world_picture.rs `cut`), as the cab's map has it: `[west, south, east,
/// north]`, degrees, each `null` where it isn't: the cab's camera stops there.
fn world_cut(world: &WorldPicture) -> [Option<f64>; 4] {
    let [min_x, min_z, max_x, max_z] = world.cut();
    let lng = |x: f32| area::to_lng_lat([x, 0.0])[0];
    let lat = |z: f32| area::to_lng_lat([0.0, z])[1];
    [min_x.map(lng), max_z.map(lat), max_x.map(lng), min_z.map(lat)]
}

/// The map `map`'s scale between cities (its world this much smaller than the distances its GPS gives), from its
/// `/map/<map>.mbd`: ETS2's europe 19, ATS's usa 20 (2026-10-28: a float at 0x3c of the 69-byte file, the cities'
/// 3 after it). `None` if the file isn't there or doesn't hold one that makes sense.
pub fn world_scale(files: &GameFiles, map: &str) -> Option<f32> {
    scale_in(&files.read(&format!("map/{map}.mbd")).ok()??)
}

fn scale_in(mbd: &[u8]) -> Option<f32> {
    let scale = f32::from_le_bytes(mbd.get(0x3c..0x40)?.try_into().ok()?);
    (1.0..=100.0).contains(&scale).then_some(scale)
}

/// Exports `options.map` from `files` into `out`, telling `progress` along the way.
pub fn export(
    files: &GameFiles,
    options: &Options,
    out: &Path,
    progress: &mut dyn FnMut(Progress),
) -> Result<Summary, ExportError> {
    // The map's size goes with the next word after it's known.
    let size = std::cell::Cell::new(None);
    let mut tell = |stage, done| progress(Progress { stage, done, size: size.take() });
    tell(Stage::Reading, 0.0);
    crate::phases::mark("definitions");
    let mut definitions = world::load_definitions(files)?;
    crate::phases::mark("water materials");
    surface::find_water(files, &mut definitions);
    let stop = options.stop.clone().unwrap_or_default();
    let stopped = || stop.load(std::sync::atomic::Ordering::Relaxed);
    // Reading, to 0.15 of the whole, by the map's sector files.
    crate::phases::mark("sectors");
    tell(Stage::Reading, share::SECTORS);
    let mut map = world::load_drawn_map_telling(files, &options.map, &definitions, &mut |done| {
        tell(Stage::Reading, share::SECTORS + (share::READING - share::SECTORS) * done)
    })?;
    if stopped() {
        return Err(ExportError::Stopped);
    }
    // A game with no landmark folder of its own: its landmarks, now its placements are known (landmarks.rs).
    crate::phases::mark("landmarks, countries, DLCs, strings");
    landmarks::pick(&mut map, &options.map, &mut definitions, files);
    // Which country each part of the map is in, and their time zones: the cab's clock (countries.rs). Countries
    // that can't be read leave the zones out (the clock then the game's time): never the map for the clock.
    let mut countries = countries::grid(map.nodes.values());
    let time_zones_unread = match countries::time_zones(files) {
        Ok((zones, areas)) => {
            (countries.zones, countries.areas) = (zones, areas);
            countries.base = options.game_zone;
            None
        }
        Err(error) => Some(error.to_string()),
    };
    // Only what the player's game has: the guards' DLCs, the states it has (dlc.rs; owner 2026-11-04: "If I don't
    // own, it should not draw everything").
    let owned = match &options.dlc_guards {
        Some(guards) => Some(dlc::Owned::read(files, guards)?),
        None => None,
    };
    let not_owned = owned.as_ref().map(|owned| dlc::keep_owned(&mut map, owned, &countries));
    let strings = match &options.language {
        Some(language) => Strings::load(files, language)?,
        None => Strings::default(),
    };
    let items = map.items.len();

    let (bounds, cell) = match &options.around {
        Some((city, radius)) => {
            let centre = city_centre(&map, city).ok_or_else(|| ExportError::UnknownCity(city.clone()))?;
            (Bounds::around(centre, *radius), SeaOptions::default().cell)
        }
        // The whole map: as far as its roads reach. (No margin: empty ground all round it would join every
        // gap at the map's edges into one region with the sea.)
        None => (road_extent(&map).ok_or_else(|| ExportError::Empty(options.map.clone()))?, WHOLE_MAP_SEA_CELL),
    };
    let [x0, z0, x1, z1] = bounds.0;
    let sea_cells = ((x1 - x0) / cell).ceil().max(0.0) as u64 * ((z1 - z0) / cell).ceil().max(0.0) as u64;
    size.set(Some(MapSize { items, extent: bounds.0, sea_cells, sea_cell: cell }));
    tell(Stage::Drawing, share::READING);
    // Told the map's size, the caller may stop it before the drawing makes its grid.
    if stopped() {
        return Err(ExportError::Stopped);
    }
    let mut prefabs = Prefabs::new(files, &definitions);
    // The road network for WestPlay's own route planner (routing.rs), written beside the tiles.
    // Drawing, from 0.15 to 0.6 of the whole: the route network to 0.2, the surfaces to 0.45, the rest from there.
    crate::phases::mark("route network");
    let graph = routing::build_telling(&map, &definitions, &mut prefabs, files, &mut |done| {
        tell(Stage::Drawing, share::READING + (share::SURFACES - share::READING) * done)
    })?
    .with_scale(world_scale(files, &options.map).unwrap_or(routing::DEFAULT_SCALE));
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs());
    let folder = format!("tiles-{seconds}");
    let tiles_dir = out.join(&folder);
    // Marked unfinished while it's written ([`UNFINISHED`]), the mark taken off once it's whole; let go if this build
    // doesn't get that far, and by the next build if it was ended by force (a build stopped, failed or ended left
    // gigabytes behind in a folder of its own, never let go: the review of 2026-11-11, M5).
    remove_unfinished(out, previous_tiles(out).as_deref());
    std::fs::create_dir_all(&tiles_dir)?;
    std::fs::write(tiles_dir.join(UNFINISHED), b"")?;
    let unfinished = Unfinished(Some(tiles_dir.clone()));
    let surfaces_dir = tiles_dir.join(SURFACE_FOLDER);
    std::fs::create_dir_all(&surfaces_dir)?;
    crate::phases::mark("surfaces");
    let surfaces = surface::write(files, &definitions, &map, bounds.0, &surfaces_dir, &stop, None, &mut |done| {
        tell(Stage::Drawing, share::SURFACES + (share::SIGNS - share::SURFACES) * done)
    })?;
    if stopped() {
        return Err(ExportError::Stopped);
    }
    // The road number signs, their images from the game in one sheet.
    crate::phases::mark("signs, world picture");
    let (sprite, missing_signs) = signs::sprite(files, &signs::road_signs(&map));
    // The game's picture of its world, for a map whose world doesn't lie in the sea: its sea held to it, its sea and
    // the land beyond its edges drawn beyond the area by the cab (world_picture.rs), if it fits this map's roads.
    let picture = if options.sea_by_nearest { WorldPicture::read(files, &options.map)? } else { None };
    let world_fit = picture.as_ref().map(|picture| picture.fits(road_points(&map)));
    let world = picture.filter(|_| world_fit.is_some_and(|fit| fit >= world_picture::FITS)).map(std::sync::Arc::new);
    let sea = SeaOptions { cell, nearest: options.sea_by_nearest, world: world.clone(), ..SeaOptions::default() };
    crate::phases::mark("area");
    let mut export = area::export(
        &mut map,
        &definitions,
        &surfaces,
        &graph,
        &strings,
        &mut prefabs,
        &sprite.signs,
        bounds,
        sea.clone(),
        !options.water_held,
        &stop,
        &mut |done| tell(Stage::Drawing, share::AREA + (share::TAIL - share::AREA) * done),
    )
    .ok_or(ExportError::Stopped)?;
    // The rest of the bar, after the area: shared over what's left, had the area to be drawn again.
    let mut again = false;
    if export.counts.water_late {
        // Land came for a piece of painted water already drawn (its reach misjudged): the map read again (the
        // drawing let go of its terrain) and drawn with every piece's land held to the end, as always comes out
        // right.
        // The first map let go before it's read again (both held, the build took two maps' memory), the reading and
        // the drawing again told as they go, and stoppable (the review of 2026-11-11, M4).
        drop(export);
        drop(map);
        again = true;
        crate::phases::mark("area again (water late)");
        map = world::load_drawn_map_telling(files, &options.map, &definitions, &mut |done| {
            tell(Stage::Drawing, after_again(Again::Reading(done), 0.0))
        })?;
        if stopped() {
            return Err(ExportError::Stopped);
        }
        // As the first time: only the landmarks picked then (a reading keeps every candidate), what the game hasn't
        // left out.
        landmarks::keep_picked(&mut map, &definitions);
        if let Some(owned) = &owned {
            dlc::keep_owned(&mut map, owned, &countries);
        }
        export = area::export(
            &mut map,
            &definitions,
            &surfaces,
            &graph,
            &strings,
            &mut prefabs,
            &sprite.signs,
            bounds,
            sea,
            false,
            &stop,
            &mut |done| tell(Stage::Drawing, after_again(Again::Area(done), 0.0)),
        )
        .ok_or(ExportError::Stopped)?;
        export.counts.water_late = true;
    }
    // Each step's place on the bar from here: as measured, or, the area drawn again, shared over what's left.
    let place = |done: f32| if again { after_again(Again::After, done) } else { done };
    tell(Stage::Drawing, place(share::TAIL));
    crate::phases::mark("raised over water");
    let sea = &export.sea;
    let marked = surface::raised::mark_water(&surfaces_dir, &|x, z| sea.water_at(x, z), &stop, &mut |done| {
        tell(Stage::Drawing, place(share::TAIL + (share::TILES - share::TAIL) * 0.3 * done))
    })?;
    if !marked {
        return Err(ExportError::Stopped);
    }
    // The sea's grid needed no more (it was kept to the tiles: the review of 2026-11-11, minor 7).
    drop(export.sea);
    // The buildings, raised on the map (buildings.rs; owner 2026-10-22): where each stands, kept for a pass of
    // their own once the other tiles are written and let go.
    tell(Stage::Drawing, place(share::TAIL + (share::TILES - share::TAIL) * 0.3));
    crate::phases::mark("building placements");
    let mut placements = buildings::placements(&map, &definitions.buildings, bounds.0);
    // And the places' own buildings, part of their prefabs (owner 2026-10-22: they looked flat).
    placements.extend(buildings::prefab_placements(&map, &mut prefabs, bounds.0));
    // And the buildings put together from structure sets' walls (owner 2026-10-22).
    tell(Stage::Drawing, place(share::TAIL + (share::TILES - share::TAIL) * 0.45));
    crate::phases::mark("structure buildings");
    let walled = buildings::structure_buildings(&map, &definitions.structure_sets, files, bounds.0);
    crate::phases::mark("labels, places");
    let centre = [(x0 + x1) / 2.0, (z0 + z1) / 2.0];
    let start = if options.around.is_some() { centre } else { city_centre(&map, start_city(&options.map)).unwrap_or(centre) };
    let labels: Vec<_> = export
        .labels
        .iter()
        .map(|label| {
            let mut entry = json!({ "name": label.name, "kind": label.kind, "at": area::to_lng_lat(label.at), "population": label.population });
            if let Some(localized) = &label.localized {
                entry["localized"] = json!(localized);
            }
            entry
        })
        .collect();
    // Where the player can be routed, each with the map node the game's GPS takes it as (hex: the uids pass
    // 2^53), and its city named in the player's language.
    let city_name = |city: u64| {
        let city = definitions.cities.get(&city)?;
        let localized = city.localized.as_deref().and_then(|text| strings.resolve(text));
        Some(localized.unwrap_or(&city.name).to_owned())
    };
    let places: Vec<_> = export
        .destinations
        .iter()
        .map(|place| {
            let mut entry =
                json!({ "kind": place.kind, "at": area::to_lng_lat(place.at), "node": format!("{:x}", place.node) });
            if let Some(also) = place.also {
                entry["also"] = json!(format!("{also:x}"));
            }
            if let Some(name) = &place.name {
                entry["name"] = json!(name);
            }
            if let Some(localized) = &place.localized {
                entry["localized"] = json!(localized);
            }
            if let Some(city) = place.city.and_then(city_name) {
                entry["city"] = json!(city);
            }
            // The city's id too (as the save names it), for the player's own garages and headquarters.
            if let Some(id) = place.city.and_then(token_to_string) {
                entry["cityId"] = json!(id);
            }
            entry
        })
        .collect();
    // No-entry signs at the hidden roads' entrances, now the places are known.
    let reached: HashSet<u64> = export.destinations.iter().flat_map(|place| [Some(place.node), place.also]).flatten().collect();
    tell(Stage::Drawing, place(share::TAIL + (share::TILES - share::TAIL) * 0.8));
    crate::phases::mark("no-entry signs");
    let signs = no_entry::signs(&map, &definitions, &mut prefabs, &reached);
    export.features.extend(signs.iter().map(|&at| area::no_entry_feature(at)));
    let counts = export.counts;
    drop(map);
    tell(Stage::Tiles, place(share::TILES));

    let previous = previous_tiles(out);
    let tile_options = TileOptions::default();
    // The tiles of open sea alone, left unwritten (the cab is given the sea there: tiles.rs `SeaTiles`).
    let sea_tiles = tiles::SeaTiles::default();
    // The tiles, then the buildings' meshes ([`share`]).
    crate::phases::mark("tiles");
    let tiles = tiles::write_tiles(export.features, tile_options, &tiles_dir, Some(&sea_tiles), &mut |done| {
        tell(Stage::Tiles, place(share::TILES + (share::MESHES - share::TILES) * done));
    })?;
    // Then the buildings in their real shapes, in files of their own (meshes.rs); what of them stays an outline (a roof
    // over lanes, a structure set's hall) in the tiles' building layer.
    let mut raised = Vec::new();
    let mut add = |mut building: buildings::Building| {
        surfaces.decks.fit(&mut building);
        if building.height > building.base {
            raised.push(area::building_feature(&building));
        }
    };
    crate::phases::mark("meshes");
    let shaped = meshes::write(
        &placements,
        &definitions.buildings,
        &definitions.prefab_models,
        files,
        &surfaces.decks,
        &tiles_dir,
        &mut add,
        &mut |done| tell(Stage::Tiles, place(share::MESHES + (share::BUILDING_TILES - share::MESHES) * done)),
    )?;
    drop(placements);
    crate::phases::mark("walled buildings fitted");
    walled.into_iter().for_each(add);
    let building_count = raised.len() + shaped.placings;
    let append = TileOptions { append: true, ..tile_options };
    crate::phases::mark("building tiles");
    tiles::write_tiles(raised, append, &tiles_dir, Some(&sea_tiles), &mut |done| {
        tell(Stage::Tiles, place(share::BUILDING_TILES + (share::SAVING - share::BUILDING_TILES) * done))
    })?;
    let sea_listed = sea_tiles.write(&tiles_dir)?;
    // Beside the tiles: the sign sheet, as MapLibre fetches it for any screen (`signs` or `signs@2x`; the
    // index gives each image's pixel ratio).
    crate::phases::mark("sheets, places, countries, route graph, manifest");
    let index = serde_json::to_vec(&sprite.index).map_err(std::io::Error::other)?;
    for name in ["signs", "signs@2x"] {
        std::fs::write(tiles_dir.join(format!("{name}.png")), &sprite.png)?;
        std::fs::write(tiles_dir.join(format!("{name}.json")), &index)?;
    }
    tell(Stage::Tiles, place(share::SAVING));

    let [west, north] = area::to_lng_lat([x0, z0]);
    let [east, south] = area::to_lng_lat([x1, z1]);
    let manifest = json!({
        "map": options.map,
        // `null`: not read (the cab takes ETS2's).
        "scale": world_scale(files, &options.map),
        "minzoom": tile_options.min_zoom,
        "maxzoom": tile_options.max_zoom,
        "bounds": [west, south, east, north],
        "centre": area::to_lng_lat(start),
        "tiles": folder,
        // The tiles of open sea alone, not written: their list, and the sea tile the cab is given for each.
        "seaTiles": sea_listed.then(|| format!("{folder}/{}", tiles::SEA_INDEX)),
        "surfaces": format!("{folder}/{SURFACE_FOLDER}"),
        // The buildings in their real shapes (meshes.rs).
        "meshes": format!("{folder}/{}", meshes::FOLDER),
        "signs": "signs",
        "language": options.language,
        "labels": labels,
        "places": format!("{folder}/places.json"),
        "countries": format!("{folder}/countries.json"),
        // The game's picture of its world (world_picture.rs): its file (the sea in it), where it lies and where its
        // land is cut off; `null` without one: the cab draws the sea all round the map then.
        "world": world.as_ref().map(|world| json!({ "file": format!("{folder}/{WORLD_FILE}"), "bounds": world_bounds(world), "cut": world_cut(world) })),
        "source": options.source,
        "leftOut": options.left_out,
    });
    std::fs::write(tiles_dir.join("places.json"), serde_json::to_vec(&places).map_err(std::io::Error::other)?)?;
    if let Some(world) = &world {
        std::fs::write(tiles_dir.join(WORLD_FILE), world_json(world).to_string())?;
    }
    std::fs::write(tiles_dir.join("countries.json"), countries.to_json().to_string())?;
    let mut written = std::io::BufWriter::new(std::fs::File::create(tiles_dir.join(routing::FILE))?);
    graph.write(&mut written)?;
    std::io::Write::flush(&mut written)?;
    drop(graph);
    unfinished.done()?;
    // The manifest last, written whole: a map loading meanwhile finds the previous one.
    let partial = out.join("map.json.partial");
    std::fs::write(&partial, serde_json::to_vec(&manifest).map_err(std::io::Error::other)?)?;
    std::fs::rename(&partial, out.join("map.json"))?;
    // Older tile sets go, but not the previous one or any from the last day: a map still running from one
    // (exports made one after another while it runs) keeps its tiles.
    crate::phases::mark("old tile sets");
    for entry in std::fs::read_dir(out).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let tile_set = name == "tiles" || name.starts_with("tiles-");
        let made = name.strip_prefix("tiles-").and_then(|made| made.parse::<u64>().ok()).unwrap_or(0);
        let recent = seconds.saturating_sub(made) < KEEP_TILES_SECONDS;
        if tile_set && name != folder && Some(&name) != previous.as_ref() && !recent {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
    crate::phases::mark("done");
    tell(Stage::Done, 1.0);
    Ok(Summary {
        items,
        counts,
        tiles,
        sea_tiles: sea_tiles.len(),
        places: places.len(),
        folder,
        missing_signs,
        time_zones_unread,
        buildings: building_count,
        world: world_fit.map(|fit| (fit, world.is_some())),
        not_owned,
        unread: files.unread(),
    })
}

/// What an export in `out` was built from (its manifest's `source`), if there's one.
pub fn built_from(out: &Path) -> Option<serde_json::Value> {
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(out.join("map.json")).ok()?).ok()?;
    manifest.get("source").filter(|source| !source.is_null()).cloned()
}

/// The file in a tile folder while it's being written. Never renaming the folder: a folder of tens of thousands of
/// files is held by whatever has one of them open (a virus scanner reading one just written: "used by another process",
/// players' maps' folders, 2026-10-22), one small file seldom.
const UNFINISHED: &str = ".unfinished";

/// A tile folder being written ([`UNFINISHED`]): let go when dropped, unless it's [`Unfinished::done`].
struct Unfinished(Option<std::path::PathBuf>);

impl Unfinished {
    /// The folder whole: its mark taken off (tried a few times, a moment apart, if something holds it), and kept.
    fn done(mut self) -> std::io::Result<()> {
        let dir = self.0.take().expect("a folder being written");
        let mark = dir.join(UNFINISHED);
        let mut tries = 0;
        loop {
            match std::fs::remove_file(&mark) {
                Ok(()) => return Ok(()),
                Err(_) if tries < 10 => {
                    tries += 1;
                    std::thread::sleep(std::time::Duration::from_millis(200));
                }
                Err(error) => {
                    // Kept as the build's: an error, the folder let go with it, as any failed build's.
                    self.0 = Some(dir);
                    return Err(error);
                }
            }
        }
    }
}

impl Drop for Unfinished {
    fn drop(&mut self) {
        if let Some(dir) = &self.0 {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

/// Lets go of the tile folders in `out` a build never finished ([`UNFINISHED`]: one ended by force can't let go of its
/// own), but the one the manifest names, `named` (never marked: but never let go on a mark's word alone).
fn remove_unfinished(out: &Path, named: Option<&str>) {
    for entry in std::fs::read_dir(out).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("tiles-") && Some(name.as_str()) != named && entry.path().join(UNFINISHED).exists() {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// The tile folder the last export's manifest names (`tiles` before manifests named one).
fn previous_tiles(out: &Path) -> Option<String> {
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(out.join("map.json")).ok()?).ok()?;
    Some(manifest.get("tiles").and_then(|tiles| tiles.as_str()).unwrap_or("tiles").to_owned())
}

/// The middle of a city's area on the map, by its id (`calais`), if it's there.
fn city_centre(map: &Map, city: &str) -> Option<[f32; 2]> {
    let token = string_to_token(city)?;
    map.items
        .iter()
        .filter_map(|item| match &item.data {
            ItemData::City { city, width, height, .. } if *city == token => {
                Some((width * height, [item.bounds[0] + width / 2.0, item.bounds[1] + height / 2.0]))
            }
            _ => None,
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, centre)| centre)
}

/// How far the map's roads and junctions reach (`None`: it has none).
fn road_extent(map: &Map) -> Option<Bounds> {
    let mut extent = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    let mut any = false;
    for item in &map.items {
        if matches!(item.data, ItemData::Road { .. } | ItemData::Prefab { .. }) {
            any = true;
            extent = [
                extent[0].min(item.bounds[0]),
                extent[1].min(item.bounds[1]),
                extent[2].max(item.bounds[2]),
                extent[3].max(item.bounds[3]),
            ];
        }
    }
    any.then_some(Bounds(extent))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unfinished_tile_folder_is_let_go_and_a_finished_one_kept() {
        let out = crate::test_dir::TempDir::new();
        let folder = |name: &str, marked: bool| {
            let dir = out.path.join(name);
            std::fs::create_dir_all(dir.join("14")).unwrap();
            std::fs::write(dir.join("14").join("1.pbf"), b"tile").unwrap();
            if marked {
                std::fs::write(dir.join(UNFINISHED), b"").unwrap();
            }
            dir
        };
        // A build ended by force left one; the manifest names another (never let go, marked or not); a third is whole.
        let (left, named, whole) = (folder("tiles-1", true), folder("tiles-2", true), folder("tiles-3", false));
        remove_unfinished(&out.path, Some("tiles-2"));
        assert!(!left.exists() && named.exists() && whole.exists());
        // A build that fails lets go of its own; one that's done keeps it, its mark off.
        let failed = folder("tiles-4", true);
        drop(Unfinished(Some(failed.clone())));
        assert!(!failed.exists());
        let done = folder("tiles-5", true);
        Unfinished(Some(done.clone())).done().unwrap();
        assert!(done.join("14").join("1.pbf").exists() && !done.join(UNFINISHED).exists());
    }

    #[test]
    fn after_the_area_drawn_again_the_bar_goes_on_up_to_the_end() {
        // Each step after the area, then the reading and the area again: always up, from the area's end to the whole.
        let steps = [
            after_again(Again::Reading(0.0), 0.0),
            after_again(Again::Reading(1.0), 0.0),
            after_again(Again::Area(0.5), 0.0),
            after_again(Again::Area(1.0), 0.0),
            after_again(Again::After, share::TAIL),
            after_again(Again::After, share::TILES),
            after_again(Again::After, share::SAVING),
            after_again(Again::After, 1.0),
        ];
        assert!(steps.windows(2).all(|pair| pair[0] <= pair[1]), "{steps:?}");
        assert_eq!((steps[0], steps[7]), (share::TAIL, 1.0));
        // The shares, in order.
        let order = [
            share::SECTORS,
            share::READING,
            share::SURFACES,
            share::SIGNS,
            share::AREA,
            share::TAIL,
            share::TILES,
            share::MESHES,
            share::BUILDING_TILES,
            share::SAVING,
        ];
        assert!(order.windows(2).all(|pair| pair[0] < pair[1]) && order[9] < 1.0);
    }

    #[test]
    fn reads_the_maps_scale_from_its_mbd() {
        // The ends of ETS2's `europe.mbd` and ATS's `usa.mbd` (1.61): the scale, the cities' 3, a last byte.
        let mbd = |scale: [u8; 4]| {
            let mut file = vec![0u8; 0x38];
            file.extend([0xde, 0xc0, 0xc0, 0xb2]);
            file.extend(scale);
            file.extend([0x00, 0x00, 0x40, 0x40, 0x01]);
            file
        };
        assert_eq!(scale_in(&mbd([0x00, 0x00, 0x98, 0x41])), Some(19.0));
        assert_eq!(scale_in(&mbd([0x00, 0x00, 0xa0, 0x41])), Some(20.0));
        // Too short, or no scale there.
        assert_eq!(scale_in(&[0u8; 0x30]), None);
        assert_eq!(scale_in(&mbd([0, 0, 0, 0])), None);
    }

    /// A real map's area built end to end three ways, the same bytes each (`same_map`), its progress only ever up, to
    /// the end: as players' builds are; with every thread and blocks at once while there's room (the companion's
    /// Prepare Maps Faster: the work shared out otherwise); with every piece of painted water held to the end (as a map
    /// whose water came late is drawn again). `WESTPLAY_AREA=<install folder>=<map>=<city>`: an ETS2 map's city by the sea,
    /// its painted water and its sea (`europe=calais`). The review of 2026-11-11, tests: C3, M4, M8.
    #[test]
    #[ignore = "builds a real map's area: WESTPLAY_AREA"]
    fn a_citys_area_built_end_to_end_is_the_same_however_its_work_is_shared() {
        let _turn = crate::threads::tests::ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        let area = std::env::var("WESTPLAY_AREA").expect("WESTPLAY_AREA: <install folder>=<map>=<city>");
        // From the end: the folder's path may hold anything but its last two.
        let [city, map, dir] = area.rsplitn(3, '=').collect::<Vec<_>>()[..] else { panic!("<folder>=<map>=<city>") };
        let files = GameFiles::open(Path::new(dir)).expect("the game's archives");
        let options = Options { map: map.into(), around: Some((city.into(), 6000.0)), ..Options::default() };
        let build = |options: &Options, out: &Path| {
            let mut told = Vec::new();
            let summary = export(&files, options, out, &mut |progress: Progress| told.push(progress.done))
                .unwrap_or_else(|error| panic!("{error}"));
            (summary, told)
        };
        let check = |first: &Path, second: &Path, how: &str| {
            let (differ, same) = crate::same_map::differences(first, second).expect("both built");
            assert!(differ.is_empty(), "{how}: {differ:?}");
            assert!(same > 0, "{how}: no tiles");
        };
        let (normal, fast, held) =
            (crate::test_dir::TempDir::new(), crate::test_dir::TempDir::new(), crate::test_dir::TempDir::new());
        let (summary, told) = build(&options, &normal.path);
        println!("{summary:?}");
        assert!(summary.tiles > 0 && summary.places > 0);
        assert!(told.windows(2).all(|pair| pair[0] <= pair[1]), "progress went back: {told:?}");
        assert_eq!(told.last(), Some(&1.0));
        {
            let _full = crate::threads::Effort::full(std::sync::Arc::new(|| true));
            build(&options, &fast.path);
        }
        check(&normal.path, &fast.path, "every thread, blocks at once");
        let (summary, _) = build(&Options { water_held: true, ..options.clone() }, &held.path);
        assert!(!summary.counts.water_late, "held to the end, no water comes late");
        check(&normal.path, &held.path, "water held to the end");
    }

    /// Each game's map's scale, from the installs named by `WESTPLAY_GAMES` (their folders, `;` between, each
    /// with its map: `<folder>=europe`).
    #[test]
    #[ignore = "needs the games: WESTPLAY_GAMES"]
    fn reads_each_games_scale() {
        let games = std::env::var("WESTPLAY_GAMES").expect("WESTPLAY_GAMES");
        for game in games.split(';').filter(|game| !game.is_empty()) {
            let (dir, map) = game.split_once('=').expect("<folder>=<map>");
            let files = GameFiles::open(Path::new(dir)).unwrap();
            println!("{map}: {:?}", world_scale(&files, map));
        }
    }
}
