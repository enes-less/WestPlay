//! The whole map of a game: its sectors' items and nodes, and the definitions they refer to (road looks,
//! prefab models, cities).

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;

use crate::data::{self, DataOwner, RoadTerrain};
use crate::files::{FileError, GameFiles};
use crate::prefab::{self, Prefab};
use crate::sector::{self, Item, ItemData, Node, SectorError};
use crate::sii;
use crate::token::string_to_token;

/// How the map draws a road.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RoadClass {
    /// Paths, service and dirt roads (no regular traffic).
    Minor,
    Local,
    /// Expressways: fast roads that aren't motorways.
    Trunk,
    Motorway,
    Rail,
}

impl RoadClass {
    pub fn name(self) -> &'static str {
        match self {
            Self::Minor => "minor",
            Self::Local => "local",
            Self::Trunk => "trunk",
            Self::Motorway => "motorway",
            Self::Rail => "rail",
        }
    }

    /// The width of a typical road of the class, metres: the page sizes each class on screen for it (its
    /// `TYPICAL_WIDTH`), and roads drawn far out, where lanes don't show, are given it.
    pub fn typical_width(self) -> f32 {
        match self {
            Self::Motorway => 16.0,
            Self::Trunk => 12.0,
            Self::Local => 8.0,
            Self::Minor => 6.0,
            Self::Rail => 4.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RoadLook {
    pub class: RoadClass,
    /// Lanes in both directions together.
    pub lanes: u32,
    /// Gap between the two directions (a central reservation), metres.
    pub offset: f32,
}

/// Width of one lane on the map, metres.
pub const LANE_WIDTH: f32 = 4.0;

impl RoadLook {
    /// The paved width the map draws, metres (both directions, without the gap between them).
    pub fn width(&self) -> f32 {
        self.lanes.max(1) as f32 * LANE_WIDTH
    }
}

/// A city's definition (`city_data`).
#[derive(Clone, Debug)]
pub struct City {
    /// Its name in its own language and script (the game's `city_name`).
    pub name: String,
    /// How many people live there (0 for the few places the game gives none: ports, border crossings).
    pub population: u32,
    /// Its name in the player's language, as a reference into the game's text (`@@varna@@`,
    /// [`crate::locale`]).
    pub localized: Option<String>,
}

#[derive(Default)]
pub struct Definitions {
    pub road_looks: HashMap<u64, RoadLook>,
    /// Prefab model token → its descriptor's path.
    pub prefab_paths: HashMap<u64, String>,
    /// City token → the city as the game defines it.
    pub cities: HashMap<u64, City>,
    /// Company token (`company.permanent.*`) → its name as the game shows it (a `@@key@@` reference is
    /// looked up in the game's text, [`crate::locale`]).
    pub companies: HashMap<u64, String>,
    /// Ferry port or train terminal token (`ferry.*`) → its name as the game shows it (as `companies`).
    pub ferries: HashMap<u64, String>,
    /// Terrain materials that are water (sea, lakes, rivers): terrain painted with them is water.
    pub water_materials: HashSet<u64>,
    /// Terrain profile token → its shape across a terrain side.
    pub terrain_profiles: HashMap<u64, TerrainProfile>,
}

/// The shape of the ground across a terrain side: rows of the given widths, from the inner edge out, and
/// the ground's height at each row's inner edge (the last of each repeats outwards).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TerrainProfile {
    pub steps: Vec<f32>,
    pub heights: Vec<f32>,
}

/// Reads the definitions the map needs. Each kind lives in a base file (`/def/world/road_look.sii`)
/// and its extensions from DLCs and mods (`/def/world/road_look.<id>.sii`).
pub fn load_definitions(files: &GameFiles) -> Result<Definitions, FileError> {
    let mut definitions = Definitions::default();
    for unit in sii::read_family(files, "/def/world", "road_look")? {
        if let Some(token) = token_after(&unit.name, "road.") {
            definitions.road_looks.insert(token, road_look(&unit));
        }
    }
    for unit in sii::read_family(files, "/def/world", "prefab")? {
        if let (Some(token), Some(path)) = (token_after(&unit.name, "prefab."), unit.get("prefab_desc")) {
            definitions.prefab_paths.insert(token, path.to_owned());
        }
    }
    for unit in sii::read_family(files, "/def/world", "terrain_material")? {
        if let (Some(token), Some(path)) = (token_after(&unit.name, "terrain_mat."), unit.get("path"))
            && is_water_material(path)
        {
            definitions.water_materials.insert(token);
        }
    }
    for unit in sii::read_family(files, "/def/world", "terrain_profile")? {
        if let Some(token) = token_after(&unit.name, "terrain.") {
            let numbers = |key| unit.all(key).filter_map(|value| value.parse().ok()).collect();
            definitions
                .terrain_profiles
                .insert(token, TerrainProfile { steps: numbers("step"), heights: numbers("height") });
        }
    }
    for unit in sii::read_family(files, "/def", "company")? {
        if let (Some(token), Some(name)) = (token_after(&unit.name, "company.permanent."), unit.get("name")) {
            definitions.companies.insert(token, name.to_owned());
        }
    }
    for unit in sii::read_family(files, "/def", "ferry")? {
        let name = unit.get("ferry_name_localized").or(unit.get("ferry_name"));
        if let (Some(token), Some(name)) = (token_after(&unit.name, "ferry."), name) {
            definitions.ferries.insert(token, name.to_owned());
        }
    }
    for unit in sii::read_family(files, "/def", "city")? {
        if let (Some(token), Some(name)) = (token_after(&unit.name, "city."), unit.get("city_name")) {
            let population = unit.get("population").and_then(|value| value.parse().ok()).unwrap_or(0);
            let localized = unit.get("city_name_localized").map(str::to_owned);
            definitions.cities.insert(token, City { name: name.to_owned(), population, localized });
        }
    }
    Ok(definitions)
}

/// Whether a terrain material (its `.mat` file) is water: the game's water materials are all named
/// `water…` (`/umatlib/legacy/terrain/water/water_ocean.mat`, `…/water/transparent/water_calm_01_stream_a.mat`).
fn is_water_material(path: &str) -> bool {
    path.rsplit('/').next().is_some_and(|file| file.starts_with("water"))
}

fn token_after(name: &str, prefix: &str) -> Option<u64> {
    string_to_token(name.strip_prefix(prefix)?)
}

fn road_look(unit: &sii::Unit) -> RoadLook {
    let lanes: Vec<&str> = unit.all("lanes_left").chain(unit.all("lanes_right")).collect();
    let class = lanes.iter().map(|lane| lane_class(lane)).max();
    match class {
        Some(class) => RoadLook {
            class,
            lanes: lanes.iter().filter(|lane| lane_class(lane) == class).count() as u32,
            offset: unit.number("road_offset").unwrap_or(0.0),
        },
        // Older looks give their size instead of lanes: one lane per 4.5 m on each side.
        None => {
            let size = unit.number("road_size_left").unwrap_or(4.5) + unit.number("road_size_right").unwrap_or(4.5);
            RoadLook {
                class: RoadClass::Local,
                lanes: (size / 4.5).round().max(1.0) as u32,
                offset: unit.number("road_offset").unwrap_or(0.0),
            }
        }
    }
}

/// What a lane (`traffic_lane.road.motorway.full_in`) says about its road.
fn lane_class(lane: &str) -> RoadClass {
    let kind = lane.strip_prefix("traffic_lane.").unwrap_or(lane);
    if kind.starts_with("road.motorway") {
        RoadClass::Motorway
    } else if kind.starts_with("road.expressway") {
        RoadClass::Trunk
    } else if kind.starts_with("road.local.dust") || kind.starts_with("road.local.narrow") {
        RoadClass::Minor
    } else if kind.starts_with("road.") {
        RoadClass::Local
    } else if kind.starts_with("rail.train") {
        RoadClass::Rail
    } else {
        // Trams, footpaths, slow roads.
        RoadClass::Minor
    }
}

/// Every sector of one map (`europe`): items and nodes of its `.base` and `.aux` files, and the roads'
/// terrain from its `.data` files.
pub struct Map {
    pub items: Vec<Item>,
    pub nodes: HashMap<u64, Node>,
    pub road_terrain: HashMap<u64, RoadTerrain>,
}

/// Reads all of a map's sectors, as mounted (a DLC's sector replaces the base game's of the same name,
/// carrying both's items).
/// Why a map couldn't be read.
#[derive(Debug)]
pub enum MapError {
    File(FileError),
    Sector { path: String, error: SectorError },
}

impl fmt::Display for MapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(error) => write!(f, "{error}"),
            Self::Sector { path, error } => write!(f, "{path}: {error}"),
        }
    }
}

impl std::error::Error for MapError {}

impl From<FileError> for MapError {
    fn from(error: FileError) -> Self {
        Self::File(error)
    }
}

pub fn load_map(files: &GameFiles, name: &str) -> Result<Map, MapError> {
    read_map(files, name, false)
}

/// The map as the export draws it: [`load_map`]'s, but only its items of the kinds the map draws (roads,
/// prefabs, terrain, map areas, cities…), the nodes they use, and the roads' terrain. A map's other items
/// (models, signs, vegetation: most of them, 4.7 of the base game's 5.5 million) are dropped as each sector
/// is read, and the nodes only they used once every sector is (a road's node may be in the next one). Nothing
/// the export reads is left out: it steps over the others, and looks nodes up only from its items.
pub fn load_drawn_map(files: &GameFiles, name: &str) -> Result<Map, MapError> {
    let mut map = read_map(files, name, true)?;
    let used = used_nodes(&map.items);
    map.nodes.retain(|uid, _| used.contains(uid));
    map.nodes.shrink_to_fit();
    map.items.shrink_to_fit();
    Ok(map)
}

/// Every node an item names: its ends, its points, the node it stands at.
fn used_nodes(items: &[Item]) -> HashSet<u64> {
    let mut used = HashSet::new();
    for item in items {
        match &item.data {
            ItemData::Road { start_node, end_node, .. } | ItemData::Terrain { start_node, end_node, .. } => {
                used.insert(*start_node);
                used.insert(*end_node);
            }
            ItemData::Prefab { nodes, .. } | ItemData::MapArea { nodes, .. } => used.extend(nodes.iter().copied()),
            ItemData::City { node, .. }
            | ItemData::Company { node, .. }
            | ItemData::Ferry { node, .. }
            | ItemData::MapOverlay { node, .. }
            | ItemData::BezierPatch { node, .. } => {
                used.insert(*node);
            }
            ItemData::Other => {}
        }
    }
    used
}

/// A map's sectors read; with `drawn`, only the items the map draws kept ([`load_drawn_map`]).
fn read_map(files: &GameFiles, name: &str, drawn: bool) -> Result<Map, MapError> {
    let dir = format!("/map/{name}");
    let mut map = Map { items: Vec::new(), nodes: HashMap::new(), road_terrain: HashMap::new() };
    for file in files.list(&dir)?.into_iter().filter(|file| file.ends_with(".base")) {
        let base_path = format!("{dir}/{file}");
        let Some(base) = read_sector(files, &base_path)? else { continue };
        // Which .data entries are roads' and which prefabs': their layouts differ.
        let owners: HashMap<u64, DataOwner> = base
            .items
            .iter()
            .filter_map(|item| match item.data {
                ItemData::Road { .. } => Some((item.uid, DataOwner::Road)),
                ItemData::Prefab { .. } => Some((item.uid, DataOwner::Prefab)),
                _ => None,
            })
            .collect();
        let data_path = base_path.replace(".base", ".data");
        if let Some(bytes) = files.read(&data_path)? {
            let terrain = data::parse(&bytes, &owners).map_err(|error| MapError::Sector { path: data_path, error })?;
            map.road_terrain.extend(terrain);
        }
        let aux = read_sector(files, &base_path.replace(".base", ".aux"))?;
        for sector in [Some(base), aux].into_iter().flatten() {
            if drawn {
                map.items.extend(sector.items.into_iter().filter(|item| !matches!(item.data, ItemData::Other)));
            } else {
                map.items.extend(sector.items);
            }
            map.nodes.extend(sector.nodes.into_iter().map(|node| (node.uid, node)));
        }
    }
    Ok(map)
}

/// The size of a map sector (a square of the ground, each in its own files), metres.
pub const SECTOR_SIZE: f32 = 4000.0;

/// The sector a point (x, z) of the ground is in.
pub fn sector_of(point: [f32; 2]) -> (i32, i32) {
    ((point[0] / SECTOR_SIZE).floor() as i32, (point[1] / SECTOR_SIZE).floor() as i32)
}

/// Some sectors of the map `name` (`europe`), their items and nodes only (not their terrain): what a part
/// of the map needs, read in a moment rather than the whole map's seconds.
pub fn load_sectors(files: &GameFiles, name: &str, sectors: &BTreeSet<(i32, i32)>) -> Result<Map, MapError> {
    let mut map = Map { items: Vec::new(), nodes: HashMap::new(), road_terrain: HashMap::new() };
    for &(x, z) in sectors {
        let base = format!("/map/{name}/sec{x:+05}{z:+05}.base");
        for path in [base.clone(), base.replace(".base", ".aux")] {
            if let Some(sector) = read_sector(files, &path)? {
                map.items.extend(sector.items);
                map.nodes.extend(sector.nodes.into_iter().map(|node| (node.uid, node)));
            }
        }
    }
    Ok(map)
}

/// A sector file, or `None` if the sector doesn't have it.
fn read_sector(files: &GameFiles, path: &str) -> Result<Option<sector::Sector>, MapError> {
    let Some(data) = files.read(path)? else { return Ok(None) };
    sector::parse(&data).map(Some).map_err(|error| MapError::Sector { path: path.to_owned(), error })
}

/// Prefab descriptors, read once each.
pub struct Prefabs<'a> {
    files: &'a GameFiles,
    definitions: &'a Definitions,
    cache: HashMap<u64, Option<Prefab>>,
}

impl<'a> Prefabs<'a> {
    pub fn new(files: &'a GameFiles, definitions: &'a Definitions) -> Self {
        Self { files, definitions, cache: HashMap::new() }
    }

    /// The prefab of a model token, or `None` if it's unknown or unreadable.
    pub fn get(&mut self, model: u64) -> Option<&Prefab> {
        let (files, definitions) = (self.files, self.definitions);
        self.cache
            .entry(model)
            .or_insert_with(|| {
                let path = definitions.prefab_paths.get(&model)?;
                prefab::parse(&files.read(path).ok()??).ok()
            })
            .as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(attributes: &[(&str, &str)]) -> sii::Unit {
        sii::Unit {
            class: "road_look".into(),
            name: "road.x".into(),
            attributes: attributes.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        }
    }

    #[test]
    fn classes_roads_by_their_busiest_lanes() {
        let motorway = road_look(&unit(&[
            ("lanes_left", "traffic_lane.road.motorway"),
            ("lanes_left", "traffic_lane.road.motorway.full_out"),
            ("lanes_right", "traffic_lane.road.motorway"),
            ("lanes_right", "traffic_lane.no_vehicles"),
            ("road_offset", "10.5"),
        ]));
        assert_eq!((motorway.class, motorway.lanes, motorway.offset), (RoadClass::Motorway, 3, 10.5));
        assert_eq!(motorway.width(), 3.0 * LANE_WIDTH);

        let local = road_look(&unit(&[("lanes_left", "traffic_lane.road.local.high_density")]));
        assert_eq!(local.class, RoadClass::Local);
        let rail = road_look(&unit(&[("lanes_left", "traffic_lane.rail.train.cargo.wire")]));
        assert_eq!(rail.class, RoadClass::Rail);
    }

    #[test]
    fn knows_water_materials_by_their_file() {
        assert!(is_water_material("/umatlib/legacy/terrain/water/water_ocean.mat"));
        assert!(is_water_material("/umatlib/ground/water/water_calm_r_x_c.mat"));
        assert!(!is_water_material("/umatlib/legacy/terrain/balkan/mud_water_grass_mid_blke_height.mat"));
        assert!(!is_water_material("/umatlib/ground/grass/grass_01.mat"));
    }

    #[test]
    fn sizes_older_looks_by_their_width() {
        let look = road_look(&unit(&[("road_size_left", "9.0"), ("road_size_right", "9.0")]));
        assert_eq!((look.class, look.lanes), (RoadClass::Local, 4));
    }
}
