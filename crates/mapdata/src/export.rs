//! The map for the Maps app, exported from the mounted game (its DLCs and map mods too): vector tiles of one
//! map (`europe`, or a standalone map mod's), its place names and where the player can be routed, into a
//! folder the app serves the cab's map from.
//!
//! Writes `<out>/tiles-<time>/{z}/{x}/{y}.pbf`, the road signs' sheet and `places.json` beside them, and
//! `<out>/map.json`: the tiles' folder, zooms and bounds, where the map starts, its place names, and what it
//! was built from (`source`, the caller's: what decides whether it's built again). Each export's tiles go in a
//! folder of their own, so a map running from the last export keeps its tiles until it next loads.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;

use crate::area::{self, Bounds, Counts};
use crate::files::{FileError, GameFiles};
use crate::locale::Strings;
use crate::sea::SeaOptions;
use crate::sector::ItemData;
use crate::signs;
use crate::tiles::{self, TileOptions};
use crate::token::{string_to_token, token_to_string};
use crate::world::{self, Map, MapError, Prefabs};

/// Where a map starts when there's no truck to follow (a browser): Calais, where WestPlay's map began, on a
/// map that has it.
const START_CITY: &str = "calais";
/// Cells of the grid the open sea is found on for a whole map, metres: fine enough for the coast, coarse
/// enough for a whole map in memory (tens of millions of cells).
const WHOLE_MAP_SEA_CELL: f32 = 20.0;
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
}

/// How far an export has got.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Progress {
    pub stage: Stage,
    /// Of the whole export, 0 to 1.
    pub done: f32,
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
    /// Junctions whose drawn road splits at a point inside them, by model, and one place each (an audit).
    pub centre_forks: Vec<(String, usize, [f32; 2])>,
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
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Files(error) => write!(f, "{error}"),
            Self::Map(error) => write!(f, "{error}"),
            Self::Empty(map) => write!(f, "the map {map} has no roads"),
            Self::UnknownCity(city) => write!(f, "{city} isn't on the map"),
            Self::Io(error) => write!(f, "{error}"),
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

/// Exports `options.map` from `files` into `out`, telling `progress` along the way.
pub fn export(
    files: &GameFiles,
    options: &Options,
    out: &Path,
    progress: &mut dyn FnMut(Progress),
) -> Result<Summary, ExportError> {
    let mut tell = |stage, done| progress(Progress { stage, done });
    tell(Stage::Reading, 0.0);
    let definitions = world::load_definitions(files)?;
    let map = world::load_drawn_map(files, &options.map)?;
    let strings = match &options.language {
        Some(language) => Strings::load(files, language)?,
        None => Strings::default(),
    };
    let items = map.items.len();
    tell(Stage::Drawing, 0.15);

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
    let mut prefabs = Prefabs::new(files, &definitions);
    // The road number signs, their images from the game in one sheet.
    let (sprite, missing_signs) = signs::sprite(files, &signs::road_signs(&map));
    let export = area::export(
        &map,
        &definitions,
        &strings,
        &mut prefabs,
        &sprite.signs,
        bounds,
        SeaOptions { cell, ..SeaOptions::default() },
        // Drawing, from 0.15 to 0.6 of the whole.
        &mut |done| tell(Stage::Drawing, 0.15 + 0.45 * done),
    );
    let mut forks: BTreeMap<String, (usize, [f32; 2])> = BTreeMap::new();
    for (model, at) in &export.centre_forks {
        forks.entry(model.clone()).or_insert((0, *at)).0 += 1;
    }
    let mut centre_forks: Vec<_> = forks.into_iter().map(|(model, (count, at))| (model, count, at)).collect();
    centre_forks.sort_by_key(|(_, count, _)| std::cmp::Reverse(*count));

    let centre = [(x0 + x1) / 2.0, (z0 + z1) / 2.0];
    let start = if options.around.is_some() { centre } else { city_centre(&map, START_CITY).unwrap_or(centre) };
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
    let counts = export.counts;
    drop(map);
    drop(export.sea);
    tell(Stage::Tiles, 0.6);

    let seconds = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs());
    let folder = format!("tiles-{seconds}");
    let tiles_dir = out.join(&folder);
    let previous = previous_tiles(out);
    let tile_options = TileOptions::default();
    std::fs::create_dir_all(&tiles_dir)?;
    // The tiles, from 0.6 to 0.95.
    let tiles = tiles::write_tiles(export.features, tile_options, &tiles_dir, &mut |done| {
        tell(Stage::Tiles, 0.6 + 0.35 * done);
    })?;
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
        "minzoom": tile_options.min_zoom,
        "maxzoom": tile_options.max_zoom,
        "bounds": [west, south, east, north],
        "centre": area::to_lng_lat(start),
        "tiles": folder,
        "signs": "signs",
        "language": options.language,
        "labels": labels,
        "places": format!("{folder}/places.json"),
        "source": options.source,
    });
    std::fs::write(tiles_dir.join("places.json"), serde_json::to_vec(&places).map_err(std::io::Error::other)?)?;
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
    Ok(Summary { items, counts, tiles, places: places.len(), folder, missing_signs, centre_forks })
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
