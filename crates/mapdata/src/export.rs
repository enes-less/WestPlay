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
    let mut definitions = world::load_definitions(files)?;
    surface::find_water(files, &mut definitions);
    let stop = options.stop.clone().unwrap_or_default();
    let stopped = || stop.load(std::sync::atomic::Ordering::Relaxed);
    // Reading, to 0.15 of the whole, by the map's sector files.
    let mut map = world::load_drawn_map_telling(files, &options.map, &definitions, &mut |done| {
        tell(Stage::Reading, 0.15 * done)
    })?;
    if stopped() {
        return Err(ExportError::Stopped);
    }
    // A game with no landmark folder of its own: its landmarks, now its placements are known (landmarks.rs).
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
    tell(Stage::Drawing, 0.15);
    // Told the map's size, the caller may stop it before the drawing makes its grid.
    if stopped() {
        return Err(ExportError::Stopped);
    }
    let mut prefabs = Prefabs::new(files, &definitions);
    // The road network for WestPlay's own route planner (routing.rs), written beside the tiles.
    // Drawing, from 0.15 to 0.6 of the whole: the route network to 0.2, the surfaces to 0.45, the rest from there.
    let graph = routing::build_telling(&map, &definitions, &mut prefabs, files, &mut |done| {
        tell(Stage::Drawing, 0.15 + 0.05 * done)
    })?;
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs());
    let folder = format!("tiles-{seconds}");
    let tiles_dir = out.join(&folder);
    let surfaces_dir = tiles_dir.join(SURFACE_FOLDER);
    std::fs::create_dir_all(&surfaces_dir)?;
    let surfaces = surface::write(files, &definitions, &map, bounds.0, &surfaces_dir, &stop, None, &mut |done| {
        tell(Stage::Drawing, 0.2 + 0.25 * done)
    })?;
    if stopped() {
        return Err(ExportError::Stopped);
    }
    // The road number signs, their images from the game in one sheet.
    let (sprite, missing_signs) = signs::sprite(files, &signs::road_signs(&map));
    // The game's picture of its world, for a map whose world doesn't lie in the sea: its sea held to it, its sea and
    // the land beyond its edges drawn beyond the area by the cab (world_picture.rs), if it fits this map's roads.
    let picture = if options.sea_by_nearest { WorldPicture::read(files, &options.map)? } else { None };
    let world_fit = picture.as_ref().map(|picture| picture.fits(road_points(&map)));
    let world = picture.filter(|_| world_fit.is_some_and(|fit| fit >= world_picture::FITS)).map(std::sync::Arc::new);
    let sea = SeaOptions { cell, nearest: options.sea_by_nearest, world: world.clone(), ..SeaOptions::default() };
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
        true,
        &stop,
        &mut |done| tell(Stage::Drawing, 0.45 + 0.15 * done),
    )
    .ok_or(ExportError::Stopped)?;
    if export.counts.water_late {
        // Land came for a piece of painted water already drawn (its reach misjudged): the map read again (the
        // drawing let go of its terrain) and drawn with every piece's land held to the end, as always comes out
        // right.
        drop(export);
        map = world::load_drawn_map_telling(files, &options.map, &definitions, &mut |_| {})?;
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
            &mut |done| tell(Stage::Drawing, 0.45 + 0.15 * done),
        )
        .ok_or(ExportError::Stopped)?;
        export.counts.water_late = true;
    }
    surface::raised::mark_water(&surfaces_dir, &|x, z| export.sea.water_at(x, z))?;
    // The buildings, raised on the map (buildings.rs; owner 2026-10-22): where each stands, kept for a pass of
    // their own once the other tiles are written and let go.
    let mut placements = buildings::placements(&map, &definitions.buildings, bounds.0);
    // And the places' own buildings, part of their prefabs (owner 2026-10-22: they looked flat).
    placements.extend(buildings::prefab_placements(&map, &mut prefabs, bounds.0));
    // And the buildings put together from structure sets' walls (owner 2026-10-22).
    let walled = buildings::structure_buildings(&map, &definitions.structure_sets, files, bounds.0);
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
    let signs = no_entry::signs(&map, &definitions, &mut prefabs, &reached);
    export.features.extend(signs.iter().map(|&at| area::no_entry_feature(at)));
    let counts = export.counts;
    drop(map);
    drop(export.sea);
    tell(Stage::Tiles, 0.6);

    let previous = previous_tiles(out);
    let tile_options = TileOptions::default();
    std::fs::create_dir_all(&tiles_dir)?;
    // The tiles, from 0.6 to 0.85.
    let tiles = tiles::write_tiles(export.features, tile_options, &tiles_dir, &mut |done| {
        tell(Stage::Tiles, 0.6 + 0.25 * done);
    })?;
    // Then the buildings in their real shapes, in files of their own (meshes.rs), to 0.92; what of them stays an
    // outline (a roof over lanes, a structure set's hall) in the tiles' building layer, to 0.95.
    let mut raised = Vec::new();
    let mut add = |mut building: buildings::Building| {
        surfaces.decks.fit(&mut building);
        if building.height > building.base {
            raised.push(area::building_feature(&building));
        }
    };
    let shaped = meshes::write(
        &placements,
        &definitions.buildings,
        &definitions.prefab_models,
        files,
        &surfaces.decks,
        &tiles_dir,
        &mut add,
        &mut |done| tell(Stage::Tiles, 0.85 + 0.07 * done),
    )?;
    drop(placements);
    walled.into_iter().for_each(add);
    let building_count = raised.len() + shaped.placings;
    let append = TileOptions { append: true, ..tile_options };
    tiles::write_tiles(raised, append, &tiles_dir, &mut |done| tell(Stage::Tiles, 0.92 + 0.03 * done))?;
    // Beside the tiles: the sign sheet, as MapLibre fetches it for any screen (`signs` or `signs@2x`; the
    // index gives each image's pixel ratio).
    let index = serde_json::to_vec(&sprite.index).map_err(std::io::Error::other)?;
    for name in ["signs", "signs@2x"] {
        std::fs::write(tiles_dir.join(format!("{name}.png")), &sprite.png)?;
        std::fs::write(tiles_dir.join(format!("{name}.json")), &index)?;
    }
    tell(Stage::Tiles, 0.95);

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
    // The manifest last, written whole: a map loading meanwhile finds the previous one.
    let partial = out.join("map.json.partial");
    std::fs::write(&partial, serde_json::to_vec(&manifest).map_err(std::io::Error::other)?)?;
    std::fs::rename(&partial, out.join("map.json"))?;
    // Older tile sets go, but not the previous one or any from the last day: a map still running from one
    // (exports made one after another while it runs) keeps its tiles.
    for entry in std::fs::read_dir(out).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let tile_set = name == "tiles" || name.starts_with("tiles-");
        let made = name.strip_prefix("tiles-").and_then(|made| made.parse::<u64>().ok()).unwrap_or(0);
        let recent = seconds.saturating_sub(made) < KEEP_TILES_SECONDS;
        if tile_set && name != folder && Some(&name) != previous.as_ref() && !recent {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
    tell(Stage::Done, 1.0);
    Ok(Summary {
        items,
        counts,
        tiles,
        places: places.len(),
        folder,
        missing_signs,
        time_zones_unread,
        buildings: building_count,
        world: world_fit.map(|fit| (fit, world.is_some())),
        not_owned,
    })
}

/// What an export in `out` was built from (its manifest's `source`), if there's one.
pub fn built_from(out: &Path) -> Option<serde_json::Value> {
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(out.join("map.json")).ok()?).ok()?;
    manifest.get("source").filter(|source| !source.is_null()).cloned()
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
