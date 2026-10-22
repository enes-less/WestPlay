//! The whole map of a game: its sectors' items and nodes, and the definitions they refer to (road looks,
//! prefab models, cities).

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};

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
    /// A road the game doesn't show (its look's `invisible: true`, as `road.blke_40`, "invis road 1 one
    /// way"): traffic and the GPS may use it, but it's no road piece the player sees.
    pub invisible: bool,
    /// No vehicle may drive it: every lane of it `traffic_lane.no_vehicles` (`road.template48`, "it road 1
    /// minim"; `road.blkw1mdc`, "blkw road 1 minim dirt center"). Drawn, never routed along (owner
    /// 2026-10-22: WestPlay's routes went where the game's GPS never goes).
    pub no_vehicles: bool,
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
    /// Prefab model token → its model's descriptor's path (`.pmd`; its `.pmg` beside it): the places'
    /// buildings are part of their prefab's model (buildings.rs).
    pub prefab_models: HashMap<u64, String>,
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
    /// Model token (`model.*`) → the building it is, for the models that are buildings ([`building_kind`]).
    pub buildings: HashMap<u64, BuildingModel>,
    /// Curve model token (`curve.*`) → the structure set it is: a building's walls, repeated along curves
    /// round it (`/model2/building/shop/structure_set_*`, `/asset/building/logistics/warehouse/structure_set_*`).
    pub structure_sets: HashMap<u64, StructureSet>,
    /// Railing tokens (`railing.*`) the game defines as a tunnel's (`tunnel: true`: its walls and roof along the
    /// road): a road with one is a tunnel.
    pub tunnel_railings: HashSet<u64>,
    /// Curve model tokens (`curve.*`) that are a tunnel's ([`tunnel_curve`]): the tube the newer maps build their
    /// tunnels of (Iberia, the Balkans, Greece, the Nordic countries), laid along the road through it.
    pub tunnel_curves: HashSet<u64>,
}

/// A structure set (a curve model whose path names one): its model's file, and each variation's parts (the
/// variation's token → its parts' names as tokens: `win | center9 : 1.0` → `center9`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StructureSet {
    pub path: String,
    pub variations: HashMap<u64, Vec<u64>>,
}

/// Whether a curve model is a structure set's walls (its file `structure_set_*`: a warehouse's, a mall's, a
/// depot's), which make a building where they close round it (buildings.rs). Not its roofs or its roof's
/// details (`structure_set_roofs_01`: roofs on posts, over the walls or the open; `…_roof_details_…`), what's
/// underground (`structure_set_underground_*`), or props (`props/street/structure_set_arch_01_gr`).
pub fn wall_set(path: &str) -> bool {
    let mut folders: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    let Some(file) = folders.pop() else { return false };
    file.starts_with("structure_set")
        && !file.contains("roof")
        && !file.contains("underground")
        && !folders.iter().any(|folder| NOT_BUILDING_FOLDERS.contains(folder))
}

/// Whether a curve model is a tunnel's, by its file: in a `tunnel` folder or named a tunnel's
/// (`/model2/tunnel/ibe_hw2_one_way_round_tunnel.pmd`, `/model2/landmark/veliko_tunnel_bg_blke.pmd`,
/// `/asset/rock/concrete/rock_limestone_01_concrete_tunnel_02.pmd`); the lights and props the game lays along
/// a tunnel's inside too (`tunnel/tunnel_light_01_poc`, `bridge/balt_tunnel_lights_01_rus`).
pub fn tunnel_curve(path: &str) -> bool {
    path.contains("tunnel")
}

/// A curve model's variations as its definition gives them: `name | part : weight | part : weight…` each (a
/// first line with a count alone is no variation).
fn variations(unit: &sii::Unit) -> HashMap<u64, Vec<u64>> {
    unit.all("variation")
        .filter_map(|line| {
            let mut fields = line.trim_matches('"').split('|').map(str::trim);
            let name = crate::token::string_to_token(fields.next()?)?;
            let parts: Vec<u64> =
                fields.filter_map(|part| crate::token::string_to_token(part.split(':').next()?.trim())).collect();
            (!parts.is_empty()).then_some((name, parts))
        })
        .collect()
}

/// A model that's a building: its file (`/model2/building/flats/….pmd`) and what kind.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildingModel {
    pub path: String,
    pub kind: BuildingKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildingKind {
    Building,
    /// A city's own landmark (`/model2/landmark/<city>/…`).
    Landmark,
    /// A border crossing's or a filling station's roof over the lanes (owner 2026-10-22: shown, but not while
    /// the truck is under it).
    Canopy,
}

/// Folders a building's model is in, anywhere in its path: the game's own (`model2/building/flats`,
/// `model2/mid_city_buildings`, `model2/village_far`, `model2/warehouse`) and map mods' (ProMods:
/// `model/flint/building/belgium`, `model/general/house`, `model/east/big_house`, `model/houses/uk`,
/// `model/village/village_cz`).
const BUILDING_FOLDERS: &[&str] = &[
    "building",
    "buildings",
    "mid_city_buildings",
    "house",
    "houses",
    "big_house",
    "apartement",
    "apartment",
    "flats",
    "warehouse",
];
/// Folders that aren't buildings wherever they are (a building folder above them or not).
const NOT_BUILDING_FOLDERS: &[&str] =
    &["props", "vehicle", "sign", "road", "bridge", "rock", "panorama", "billboard", "bus_stops", "tunnels", "gates"];

/// What kind of building a model is, by the folders of its file (a model's `category` is only its DLC):
/// houses, flats, shops, offices, industry, warehouses, villages near and far, landmarks; and the roofs of
/// border crossings and filling stations. Not props, backdrops (`panorama`), rocks, gates, bus stops,
/// vehicles. Every folder of the path counts, not only the first: map mods file their own a level deeper,
/// under an author's (ProMods' `model/flint/building/…`, `model/flint/landmark/…`). A folder that only names
/// an author or a place (`model2/ted/az`, `model/germany/frankfurt`) tells nothing: left out. A structure
/// set's piece placed as a model of its own (`structure_set_roof_details_01_at`, on a hall's roof) is drawn
/// with its set's building ([`wall_set`]), not on its own.
pub fn building_kind(path: &str) -> Option<BuildingKind> {
    let path = path.trim_start_matches('/');
    let mut folders: Vec<&str> = path.split('/').collect();
    let file = folders.pop()?;
    if !matches!(folders.first(), Some(&"model") | Some(&"model2")) || file.starts_with("structure_set") {
        return None;
    }
    let any = |names: &[&str]| folders.iter().any(|folder| names.contains(folder));
    if any(NOT_BUILDING_FOLDERS) {
        None
    } else if any(&["landmark"]) {
        Some(BuildingKind::Landmark)
    } else if any(&["border_crossing", "gas_station"]) {
        Some(BuildingKind::Canopy)
    } else if any(BUILDING_FOLDERS) || folders.iter().any(|folder| folder.starts_with("village")) {
        Some(BuildingKind::Building)
    } else {
        None
    }
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
        if let (Some(token), Some(path)) = (token_after(&unit.name, "prefab."), unit.get("model_desc")) {
            definitions.prefab_models.insert(token, path.trim_matches('"').to_owned());
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
    for unit in sii::read_family(files, "/def/world", "model")? {
        if let (Some(token), Some(path)) = (token_after(&unit.name, "model."), unit.get("model_desc")) {
            let path = path.trim_matches('"');
            if let Some(kind) = building_kind(path) {
                definitions.buildings.insert(token, BuildingModel { path: path.to_owned(), kind });
            }
        }
    }
    for unit in sii::read_family(files, "/def/world", "curve_model")? {
        if let (Some(token), Some(path)) = (token_after(&unit.name, "curve."), unit.get("model_desc")) {
            let path = path.trim_matches('"');
            if wall_set(path) {
                let set = StructureSet { path: path.to_owned(), variations: variations(&unit) };
                definitions.structure_sets.insert(token, set);
            }
            if tunnel_curve(path) {
                definitions.tunnel_curves.insert(token);
            }
        }
    }
    for unit in sii::read_family(files, "/def/world", "railing")? {
        if let Some(token) = token_after(&unit.name, "railing.")
            && unit.get("tunnel") == Some("true")
        {
            definitions.tunnel_railings.insert(token);
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
    let invisible = unit.get("invisible") == Some("true");
    let no_vehicles = !lanes.is_empty() && lanes.iter().all(|lane| *lane == "traffic_lane.no_vehicles");
    match class {
        Some(class) => RoadLook {
            class,
            lanes: lanes.iter().filter(|lane| lane_class(lane) == class).count() as u32,
            offset: unit.number("road_offset").unwrap_or(0.0),
            invisible,
            no_vehicles,
        },
        // Older looks give their size instead of lanes: one lane per 4.5 m on each side.
        None => {
            let size = unit.number("road_size_left").unwrap_or(4.5) + unit.number("road_size_right").unwrap_or(4.5);
            RoadLook {
                class: RoadClass::Local,
                lanes: (size / 4.5).round().max(1.0) as u32,
                offset: unit.number("road_offset").unwrap_or(0.0),
                invisible,
                no_vehicles,
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
    /// The drawn map's buildings ([`load_drawn_map_telling`]): kept apart from `items` and in a few bytes each
    /// (an item is the size of its largest kind's, and ProMods' map has some 300,000 buildings).
    pub models: Vec<PlacedModel>,
    /// The drawn map's structure sets' walls (buildings.rs makes buildings of them), kept apart as `models`.
    pub walls: Vec<PlacedWall>,
}

/// A structure set's wall, as a curve item says: its item's uid, its curve model and variation, its ends.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedWall {
    pub uid: u64,
    pub model: u64,
    pub variation: u64,
    pub start: u64,
    pub end: u64,
}

/// A building placed on the map, as a model item says: its item's uid, its model, the node it stands at,
/// its scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedModel {
    pub uid: u64,
    pub model: u64,
    pub node: u64,
    pub scale: [f32; 3],
}

/// What a map's reading does with an item: keeps it, keeps it as a building ([`PlacedModel`]) or a structure
/// set's wall ([`PlacedWall`]), or drops it.
enum Keep {
    Item,
    Building,
    Wall,
    Drop,
}

/// Reads all of a map's sectors, as mounted (a DLC's sector replaces the base game's of the same name,
/// carrying both's items).
/// Why a map couldn't be read.
#[derive(Debug)]
pub enum MapError {
    File(FileError),
    /// A sector file that couldn't be understood, and the archive (or folder) it was read from.
    Sector {
        path: String,
        from: Option<PathBuf>,
        error: SectorError,
    },
}

impl MapError {
    fn sector(files: &GameFiles, path: &str, error: SectorError) -> Self {
        Self::Sector { path: path.to_owned(), from: files.source(path).map(Path::to_path_buf), error }
    }
}

impl fmt::Display for MapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(error) => write!(f, "{error}"),
            Self::Sector { path, from: Some(from), error } => write!(f, "{path} in {}: {error}", from.display()),
            Self::Sector { path, from: None, error } => write!(f, "{path}: {error}"),
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
    read_map(files, name, None, &mut |_| {})
}

/// The map as the export draws it: [`load_map`]'s, but only its items of the kinds the map draws (roads,
/// prefabs, terrain, map areas, cities…), the nodes they use, and the roads' terrain. A map's other items
/// (models, signs, vegetation: most of them, 4.7 of the base game's 5.5 million) are dropped as each sector
/// is read, and the nodes only they used once every sector is (a road's node may be in the next one). Nothing
/// the export reads is left out: it steps over the others, and looks nodes up only from its items.
pub fn load_drawn_map(files: &GameFiles, name: &str) -> Result<Map, MapError> {
    load_drawn_map_telling(files, name, &Definitions::default(), &mut |_| {})
}

/// [`load_drawn_map`] with the buildings in it too (the model items whose model `definitions` names as a
/// building: of a whole map's 1.2 million models, a quarter of a million; and the structure sets' walls),
/// telling how far it has read (0 to 1, by the map's sector files) as it goes.
pub fn load_drawn_map_telling(
    files: &GameFiles,
    name: &str,
    definitions: &Definitions,
    progress: &mut dyn FnMut(f32),
) -> Result<Map, MapError> {
    let mut map = read_map(files, name, Some(definitions), progress)?;
    map.items.shrink_to_fit();
    Ok(map)
}

/// Every node an item names: its ends, its points, the node it stands at.
fn used_nodes(items: &[Item]) -> HashSet<u64> {
    let mut used = HashSet::new();
    for item in items {
        match &item.data {
            ItemData::Road { start_node, end_node, .. }
            | ItemData::Terrain { start_node, end_node, .. }
            | ItemData::Curve { start_node, end_node, .. } => {
                used.insert(*start_node);
                used.insert(*end_node);
            }
            ItemData::Prefab { nodes, .. } | ItemData::MapArea { nodes, .. } | ItemData::RestTrigger { nodes } => {
                used.extend(nodes.iter().copied())
            }
            ItemData::City { node, .. }
            | ItemData::Company { node, .. }
            | ItemData::Ferry { node, .. }
            | ItemData::MapOverlay { node, .. }
            | ItemData::BezierPatch { node, .. }
            | ItemData::Model { node, .. } => {
                used.insert(*node);
            }
            ItemData::Other => {}
        }
    }
    used
}

/// A map's sectors read; with `drawn`, only the items the map draws kept (of the models, the buildings it
/// names; of the curves, the structure sets' walls), and the nodes they use ([`load_drawn_map`]).
fn read_map(
    files: &GameFiles,
    name: &str,
    drawn: Option<&Definitions>,
    progress: &mut dyn FnMut(f32),
) -> Result<Map, MapError> {
    let dir = format!("/map/{name}");
    let (mut items, mut nodes, mut road_terrain) = (Vec::new(), ReadNodes::default(), HashMap::new());
    let (mut models, mut walls) = (Vec::new(), Vec::new());
    let sectors: Vec<String> = files.list(&dir)?.into_iter().filter(|file| file.ends_with(".base")).collect();
    let sector_count = sectors.len();
    let mut told = 0;
    for (index, file) in sectors.into_iter().enumerate() {
        // Each hundredth of the sectors read, told.
        let hundredths = index * 100 / sector_count.max(1);
        if hundredths > told {
            told = hundredths;
            progress(index as f32 / sector_count as f32);
        }
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
            let terrain = data::parse(&bytes, &owners).map_err(|error| MapError::sector(files, &data_path, error))?;
            road_terrain.extend(terrain);
        }
        let aux = read_sector(files, &base_path.replace(".base", ".aux"))?;
        let parts: Vec<sector::Sector> = [Some(base), aux].into_iter().flatten().collect();
        let mut kept = Kept { items: &mut items, models: &mut models, walls: &mut walls };
        add_sector(&mut kept, &mut nodes, &file, parts, |item| match (drawn, &item.data) {
            (None, _) => Keep::Item,
            (Some(_), ItemData::Other) => Keep::Drop,
            (Some(drawn), ItemData::Model { model, .. }) if drawn.buildings.contains_key(model) => Keep::Building,
            (Some(_), ItemData::Model { .. }) => Keep::Drop,
            (Some(drawn), ItemData::Curve { model, .. }) if drawn.structure_sets.contains_key(model) => Keep::Wall,
            (Some(drawn), ItemData::Curve { model, .. }) if drawn.tunnel_curves.contains(model) => Keep::Item,
            (Some(_), ItemData::Curve { .. }) => Keep::Drop,
            (Some(_), _) => Keep::Item,
        });
    }
    let nodes = if drawn.is_some() {
        let mut used = used_nodes(&items);
        used.extend(models.iter().map(|model| model.node));
        used.extend(walls.iter().flat_map(|wall| [wall.start, wall.end]));
        nodes.into_map(|uid| used.contains(&uid))
    } else {
        nodes.into_map(|_| true)
    };
    models.shrink_to_fit();
    walls.shrink_to_fit();
    Ok(Map { items, nodes, road_terrain, models, walls })
}

/// What a map's reading keeps: its items, and apart from them its buildings and its structure sets' walls.
struct Kept<'a> {
    items: &'a mut Vec<Item>,
    models: &'a mut Vec<PlacedModel>,
    walls: &'a mut Vec<PlacedWall>,
}

/// The nodes as the sectors are read: each sector's, as they come (no table over them all: the base game's
/// are 7.5 million, and a hash map of them a gigabyte, though the drawing uses a fifth of them), and where
/// each uid's latest is (as a map of them, the latest of a uid read twice is the one kept).
#[derive(Default)]
struct ReadNodes {
    sectors: Vec<Vec<Node>>,
    latest: HashMap<u64, (u32, u32)>,
}

impl ReadNodes {
    fn get(&self, uid: u64) -> Option<&Node> {
        self.latest.get(&uid).map(|&(sector, at)| &self.sectors[sector as usize][at as usize])
    }

    /// A sector's nodes, after those before.
    fn add(&mut self, nodes: Vec<Node>) {
        let sector = self.sectors.len() as u32;
        for (at, node) in nodes.iter().enumerate() {
            self.latest.insert(node.uid, (sector, at as u32));
        }
        self.sectors.push(nodes);
    }

    /// Each uid's latest node, of those `keep` says.
    fn into_map(self, keep: impl Fn(u64) -> bool) -> HashMap<u64, Node> {
        self.latest
            .into_iter()
            .filter(|&(uid, _)| keep(uid))
            .map(|(uid, (sector, at))| (uid, self.sectors[sector as usize][at as usize]))
            .collect()
    }
}

/// Adds a sector's files (its `.base`, then its `.aux`: their items, then their nodes) to `map`, keeping the
/// items `keep` says (the buildings it says into `models`). A node an earlier sector already put somewhere else, further than a sector away, is
/// given a uid of its own here, and this sector's items name that: a map mod may reuse a uid another map has
/// (ProMods' Middle East one of the base game's, 116 km off), and in the game each sector's roads run to its
/// own node, not across the map (one taken for the other drew two roads 116 km long, their terrain with them,
/// and the drawing ran out of memory).
fn add_sector(
    kept: &mut Kept,
    nodes: &mut ReadNodes,
    file: &str,
    parts: Vec<sector::Sector>,
    keep: impl Fn(&Item) -> Keep,
) {
    let mut renamed: HashMap<u64, u64> = HashMap::new();
    for node in parts.iter().flat_map(|part| &part.nodes) {
        let elsewhere = nodes.get(node.uid).is_some_and(|earlier| {
            (earlier.position[0] - node.position[0]).hypot(earlier.position[2] - node.position[2]) > SECTOR_SIZE
        });
        if elsewhere && !renamed.contains_key(&node.uid) {
            let own = own_uid(node.uid, file, |uid| nodes.get(uid).is_some() || renamed.values().any(|&u| u == uid));
            renamed.insert(node.uid, own);
        }
    }
    for part in parts {
        for mut item in part.items {
            let keeping = keep(&item);
            if matches!(keeping, Keep::Drop) {
                continue;
            }
            if !renamed.is_empty() {
                rename_nodes(&mut item.data, &renamed);
            }
            match (keeping, item.data) {
                (Keep::Building, ItemData::Model { model, node, scale }) => {
                    kept.models.push(PlacedModel { uid: item.uid, model, node, scale })
                }
                (Keep::Wall, ItemData::Curve { model, variation, start_node, end_node }) => {
                    kept.walls.push(PlacedWall { uid: item.uid, model, variation, start: start_node, end: end_node })
                }
                (_, data) => kept.items.push(Item { data, ..item }),
            }
        }
        let mut own = part.nodes;
        for node in &mut own {
            if let Some(&renamed) = renamed.get(&node.uid) {
                node.uid = renamed;
            }
        }
        nodes.add(own);
    }
}

/// A uid of its own for a node a sector (`file`) shares with another far off: the same every time, and none
/// taken (`taken`).
fn own_uid(uid: u64, file: &str, taken: impl Fn(u64) -> bool) -> u64 {
    // FNV-1a of the sector's name, then splitmix64 steps until one's free.
    let mut candidate = file
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3))
        ^ uid;
    loop {
        candidate = candidate.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = candidate;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^= z >> 31;
        if z != 0 && !taken(z) {
            return z;
        }
    }
}

/// An item's nodes renamed (`renamed`: old uid → new).
fn rename_nodes(data: &mut ItemData, renamed: &HashMap<u64, u64>) {
    let rename = |uid: &mut u64| {
        if let Some(&own) = renamed.get(uid) {
            *uid = own;
        }
    };
    match data {
        ItemData::Road { start_node, end_node, .. }
        | ItemData::Terrain { start_node, end_node, .. }
        | ItemData::Curve { start_node, end_node, .. } => {
            rename(start_node);
            rename(end_node);
        }
        ItemData::Prefab { nodes, .. } | ItemData::MapArea { nodes, .. } | ItemData::RestTrigger { nodes } => {
            nodes.iter_mut().for_each(rename)
        }
        ItemData::City { node, .. }
        | ItemData::Company { node, .. }
        | ItemData::Ferry { node, .. }
        | ItemData::MapOverlay { node, .. }
        | ItemData::BezierPatch { node, .. }
        | ItemData::Model { node, .. } => rename(node),
        ItemData::Other => {}
    }
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
    let (mut items, mut nodes) = (Vec::new(), ReadNodes::default());
    for &(x, z) in sectors {
        let file = format!("sec{x:+05}{z:+05}.base");
        let base = format!("/map/{name}/{file}");
        let mut parts = Vec::new();
        for path in [base.clone(), base.replace(".base", ".aux")] {
            parts.extend(read_sector(files, &path)?);
        }
        let mut kept = Kept { items: &mut items, models: &mut Vec::new(), walls: &mut Vec::new() };
        add_sector(&mut kept, &mut nodes, &file, parts, |_| Keep::Item);
    }
    let nodes = nodes.into_map(|_| true);
    Ok(Map { items, nodes, road_terrain: HashMap::new(), models: Vec::new(), walls: Vec::new() })
}

/// A sector file, or `None` if the sector doesn't have it.
fn read_sector(files: &GameFiles, path: &str) -> Result<Option<sector::Sector>, MapError> {
    let Some(data) = files.read(path)? else { return Ok(None) };
    sector::parse(&data).map(Some).map_err(|error| MapError::sector(files, path, error))
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

    /// The definitions the prefabs are read with.
    pub fn definitions(&self) -> &'a Definitions {
        self.definitions
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

    /// [`add_sector`] keeping the items in `items` and the buildings in `models` (the walls let go).
    fn add(
        items: &mut Vec<Item>,
        models: &mut Vec<PlacedModel>,
        nodes: &mut ReadNodes,
        file: &str,
        parts: Vec<sector::Sector>,
        keep: impl Fn(&Item) -> Keep,
    ) {
        add_sector(&mut Kept { items, models, walls: &mut Vec::new() }, nodes, file, parts, keep);
    }

    #[test]
    fn a_node_two_sectors_place_far_apart_is_each_sectors_own() {
        let node = |uid: u64, x: f32| Node {
            uid,
            position: [x, 0.0, 0.0],
            rotation: [1.0, 0.0, 0.0, 0.0],
            backward_item: 0,
            forward_item: 0,
            flags: 0,
        };
        let port = |uid: u64, node: u64| Item {
            uid,
            item_type: 0,
            bounds: [0.0; 4],
            data: ItemData::Ferry { train: false, port: 1, node },
        };
        let sector = |items, nodes| sector::Sector { items, nodes };
        let (mut items, mut nodes) = (Vec::new(), ReadNodes::default());
        // The base game's sector: node 7 at x 0, node 8 at x 10.
        let near = vec![sector(vec![port(1, 7)], vec![node(7, 0.0), node(8, 10.0)])];
        add(&mut items, &mut Vec::new(), &mut nodes, "sec+0000+0000.base", near, |_| Keep::Item);
        // A mod's, 116 km off, its node 7 its own; and node 8 a few metres from where the other put it.
        let far = vec![sector(vec![port(2, 7), port(3, 8)], vec![node(7, 116_000.0), node(8, 13.0)])];
        add(&mut items, &mut Vec::new(), &mut nodes, "sec+0029+0000.base", far, |_| Keep::Item);
        let map = Map {
            items,
            nodes: nodes.into_map(|_| true),
            road_terrain: HashMap::new(),
            models: Vec::new(),
            walls: Vec::new(),
        };
        let node_of = |item: &Item| match item.data {
            ItemData::Ferry { node, .. } => node,
            _ => unreachable!(),
        };
        let at = |uid: u64| map.nodes[&uid].position[0];
        // The base game's port at its own node, the mod's at its own, 116 km off.
        assert_eq!((node_of(&map.items[0]), at(7)), (7, 0.0));
        let own = node_of(&map.items[1]);
        assert_ne!(own, 7);
        assert_eq!(at(own), 116_000.0);
        // A few metres apart: left as it was (the later one taken).
        assert_eq!((node_of(&map.items[2]), at(8)), (8, 13.0));
        // The same every time.
        assert_eq!(own_uid(7, "sec+0029+0000.base", |_| false), own);

        // A building in the mod's sector, kept apart as one (and a tree dropped): at its own node too.
        let model = |uid: u64, model: u64, node: u64| Item {
            uid,
            item_type: sector::item_type::MODEL,
            bounds: [0.0; 4],
            data: ItemData::Model { model, node, scale: [1.0, 1.0, -1.0] },
        };
        let (mut items, mut models, mut nodes) = (Vec::new(), Vec::new(), ReadNodes::default());
        let near = vec![sector(vec![], vec![node(7, 0.0)])];
        add(&mut items, &mut models, &mut nodes, "sec+0000+0000.base", near, |_| Keep::Item);
        let far = vec![sector(vec![model(4, 0xB0, 7), model(5, 0x77, 7)], vec![node(7, 116_000.0)])];
        let keep = |item: &Item| match item.data {
            ItemData::Model { model: 0xB0, .. } => Keep::Building,
            _ => Keep::Drop,
        };
        add(&mut items, &mut models, &mut nodes, "sec+0029+0000.base", far, keep);
        assert!(items.is_empty());
        assert_eq!(models, [PlacedModel { uid: 4, model: 0xB0, node: own, scale: [1.0, 1.0, -1.0] }]);
    }

    fn unit(attributes: &[(&str, &str)]) -> sii::Unit {
        sii::Unit {
            class: "road_look".into(),
            name: "road.x".into(),
            attributes: attributes.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        }
    }

    #[test]
    fn knows_buildings_by_their_models_folder() {
        use BuildingKind::*;
        let kind = building_kind;
        assert_eq!(kind("/model2/building/flats/flats_lux/flats_close_01_be_lux.pmd"), Some(Building));
        assert_eq!(kind("/model/building/mid_city_buildings/x.pmd"), Some(Building));
        assert_eq!(kind("/model2/village_far/house_14_far_it.pmd"), Some(Building));
        assert_eq!(kind("/model2/landmark/taranto/castle.pmd"), Some(Landmark));
        assert_eq!(kind("/model2/border_crossing/border_ipsala_gate_04_gr.pmd"), Some(Canopy));
        assert_eq!(kind("/model2/gas_station/car_charger_01_poc.pmd"), Some(Canopy));
        // A map mod's, a level deeper under its author's folder (ProMods).
        assert_eq!(kind("/model/flint/building/belgium/block_of_flats_be_01.pmd"), Some(Building));
        assert_eq!(kind("/model/flint/landmark/oostende/oostende_water_tower_01.pmd"), Some(Landmark));
        assert_eq!(kind("/model/general/house/h.pmd"), Some(Building));
        assert_eq!(kind("/model/east/big_house/h.pmd"), Some(Building));
        // A structure set's piece on its own: drawn with its set.
        assert_eq!(kind("/model2/building/shop/structure_set_roof_details_01_at.pmd"), None);
        assert!(wall_set("/asset/building/logistics/warehouse/structure_set_logistic_01.pmd"));
        assert!(wall_set("/model2/building/shop/structure_set_mall_01_at.pmd"));
        assert!(wall_set("/prefab2/depot/paper_mill/structure_set_paper_mill_01_poc.pmd"));
        for not_walls in [
            "/model2/building/structure_set_roofs_01.pmd",
            "/asset/building/shop/structure_set_roof_details_02_at.pmd",
            "/model2/building/logistic/structure_set_underground_01_gr.pmd",
            "/model2/props/street/structure_set_arch_01_gr.pmd",
            "/model2/props/road/curb_blke.pmd",
        ] {
            assert!(!wall_set(not_walls), "{not_walls}");
        }
        assert_eq!(kind("/model/village/village_cz/h.pmd"), Some(Building));
        assert_eq!(kind("/model/flint/props/mragowo_board_flint.pmd"), None);
        // An author's or a place's folder alone: unknown, left out.
        assert_eq!(kind("/model2/ted/az/x.pmd"), None);
        assert_eq!(kind("/model/germany/frankfurt/x.pmd"), None);
        // Not buildings: bus stops, props, backdrops, rocks, gates.
        for path in [
            "/model2/building/bus_stops/stop_01.pmd",
            "/model2/props/street/bench_02_gr.pmd",
            "/model/panorama/uk_village/a.pmd",
            "/model2/rock/r.pmd",
            "/model2/gates/gate_village_06_ro_blke.pmd",
            "/prefab2/x.pmd",
        ] {
            assert_eq!(kind(path), None, "{path}");
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
        assert!(!local.invisible);
        // The game's invisible roads (`road.blke_40`, "invis road 1 one way").
        let invisible = road_look(&unit(&[("lanes_right", "traffic_lane.slow_road"), ("invisible", "true")]));
        assert!(invisible.invisible && !invisible.no_vehicles);
        // A road no vehicle may drive (`road.template48`), and one with a lane that may be driven.
        let minim = road_look(&unit(&[
            ("lanes_left", "traffic_lane.no_vehicles"),
            ("lanes_right", "traffic_lane.no_vehicles"),
        ]));
        assert!(minim.no_vehicles);
        assert!(!motorway.no_vehicles && !local.no_vehicles);
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
