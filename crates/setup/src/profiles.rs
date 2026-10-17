//! The player's profiles (in the game's Documents folder, and Steam Cloud's), and for each the map it plays
//! and the mods it plays with, in its load order: where each one's files are (a folder, a zip, or the game's
//! own `.scs` archive), and what each brings to the map. Read-only.
//!
//! What makes a mod a map mod, from what's in it rather than what it says: sectors of a map (`map/<name>/
//! sec*.base`) change the roads, of its own map (`hvtmap`, a standalone map a profile plays instead of the
//! base game's) or of the base game's (`europe`: ProMods, road connections, fixes); and definitions the map
//! is drawn from (cities, countries, companies, roads' looks, prefabs) change its names and places. What
//! the mod declares (its manifest's categories: `map`, `truck`…) is kept to show, as some declare none.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use westplay_mapdata::archive::Archive;
use westplay_mapdata::save;
use westplay_mapdata::sii::Unit;

use crate::game::{Game, STEAM_APP};

/// A profile of the player's.
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    /// As the game shows it.
    pub name: String,
    pub dir: PathBuf,
    /// Kept by Steam Cloud.
    pub cloud: bool,
    /// When it was last saved (`None`: never).
    pub last_saved: Option<SystemTime>,
    /// The map it plays (its `map_path`): `europe`, the base game's, or a standalone map mod's.
    pub map: String,
    /// The mods it plays with, in its load order (as the profile lists them).
    pub mods: Vec<Mod>,
}

/// A mod a profile plays with.
#[derive(Clone, Debug, PartialEq)]
pub struct Mod {
    /// How the profile names it: the file's name, or `mod_workshop_package.<id>`.
    pub package: String,
    /// As the game shows it.
    pub name: String,
    /// Its files: an archive or a folder to mount (`None`: not found).
    pub files: Option<PathBuf>,
    /// What it says it is (its manifest's `category[]`: `map`, `truck`, `trailer`…), if it says.
    pub categories: Vec<String>,
    /// What it brings to the map.
    pub map: MapContent,
    /// Why it couldn't be read, if it couldn't.
    pub problem: Option<String>,
}

/// What a mod brings to the map.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapContent {
    /// The maps it has sectors of: its own (`hvtmap`), or the base game's (`europe`).
    pub sectors: Vec<String>,
    /// It has definitions the map is drawn from (cities, countries, companies, roads' looks, prefabs).
    pub definitions: bool,
}

impl Mod {
    /// Whether it matters to the map `map` (a profile's): it has sectors of that map, or definitions any map
    /// is drawn from.
    pub fn affects_map(&self, map: &str) -> bool {
        self.map.sectors.iter().any(|sectors| sectors == map) || self.map.definitions
    }

    /// Whether players would call it a map mod: it has a map's sectors, or says it's one.
    pub fn is_map_mod(&self) -> bool {
        !self.map.sectors.is_empty() || self.categories.iter().any(|category| category == "map")
    }

    /// Whether it's HVT's map, which WestPlay doesn't allow ([`is_hvt_map`]).
    pub fn is_hvt_map(&self) -> bool {
        is_hvt_map(&format!("{}|{}", self.package, self.name))
    }
}

/// Whether a mod (as a profile lists it, `<package>|<name>`) is HVT's map, Road to Heavy (in parts,
/// `EA_HVT_RTH_V1.3_Part1|Road to Heavy (v1.3) - Part 1`, and its add-ons and licence files), which WestPlay
/// doesn't allow. Their other mods (buses) don't change the map.
pub fn is_hvt_map(listed: &str) -> bool {
    let listed = listed.to_lowercase();
    listed.contains("road to heavy") || listed.contains("hvt_rth")
}

impl Profile {
    /// The profile a new player makes: the base game's map, no mods. Its map is the one built before the
    /// player has a profile (a first run), ready for the one they make.
    pub fn new_player() -> Self {
        Self {
            name: String::new(),
            dir: PathBuf::new(),
            cloud: false,
            last_saved: None,
            map: "europe".into(),
            mods: vec![],
        }
    }

    /// Whether it plays HVT's map, which WestPlay doesn't allow.
    pub fn plays_hvt_map(&self) -> bool {
        self.mods.iter().any(Mod::is_hvt_map)
    }

    /// The mods that matter to the map the profile plays, in load order: what its map is built from (with the
    /// game and its DLCs), and what rebuilds it when it changes.
    pub fn map_mods(&self) -> impl Iterator<Item = &Mod> {
        self.mods.iter().filter(|m| m.affects_map(&self.map))
    }
}

/// The map a profile plays, from its `map_path` (`/map/hvtmap.mbd` → `hvtmap`); `europe` without one.
pub fn map_of(map_path: Option<&str>) -> String {
    map_path
        .and_then(|path| path.trim().strip_prefix("/map/")?.strip_suffix(".mbd").map(str::to_owned))
        .filter(|name| !name.is_empty() && !name.contains('/'))
        .unwrap_or_else(|| "europe".to_owned())
}

/// Every profile, from the game's Documents folder (`game_documents`) and Steam Cloud's (under Steam's
/// `userdata`), the most recently saved first; each one's mods found for `game` (its Workshop folder, its
/// version) and the Documents folder's `mod`.
pub fn find(game_documents: &Path, steam_userdata: Option<&Path>, game: Option<&Game>) -> Vec<Profile> {
    let mut folders = vec![(game_documents.join("profiles"), false), (game_documents.join("steam_profiles"), true)];
    if let Some(userdata) = steam_userdata {
        for user in std::fs::read_dir(userdata).into_iter().flatten().flatten() {
            folders.push((user.path().join(STEAM_APP).join("remote").join("profiles"), true));
        }
    }
    let mod_dir = game_documents.join("mod");
    let mut profiles: Vec<Profile> = folders
        .iter()
        .flat_map(|(folder, cloud)| {
            std::fs::read_dir(folder).into_iter().flatten().flatten().map(move |entry| (entry.path(), *cloud))
        })
        .filter_map(|(dir, cloud)| read_profile(&dir, cloud, game, &mod_dir))
        .collect();
    profiles.sort_by_key(|profile| std::cmp::Reverse(profile.last_saved));
    profiles
}

fn read_profile(dir: &Path, cloud: bool, game: Option<&Game>, mod_dir: &Path) -> Option<Profile> {
    let units = read_units(&dir.join("profile.sii"))?;
    let user = units.iter().find(|unit| unit.class == "user_profile");
    let name = user
        .and_then(|unit| unit.get("profile_name"))
        .map(str::to_owned)
        .or_else(|| name_from_folder(dir.file_name()?.to_str()?))?;
    let mods = save::active_mods(&units).iter().map(|listed| find_mod(listed, game, mod_dir)).collect();
    let map = map_of(user.and_then(|unit| unit.get("map_path")));
    Some(Profile { name, dir: dir.to_path_buf(), cloud, last_saved: last_saved(dir), map, mods })
}

fn read_units(path: &Path) -> Option<Vec<Unit>> {
    save::read(&std::fs::read(path).ok()?).ok()
}

/// A profile's name from its folder's: the name's UTF-8 bytes in hexadecimal (`485654202D2053617665`).
pub fn name_from_folder(folder: &str) -> Option<String> {
    if folder.is_empty() || !folder.len().is_multiple_of(2) {
        return None;
    }
    let bytes: Option<Vec<u8>> =
        (0..folder.len()).step_by(2).map(|i| u8::from_str_radix(folder.get(i..i + 2)?, 16).ok()).collect();
    String::from_utf8(bytes?).ok()
}

/// When any of a profile's saves was last written.
fn last_saved(dir: &Path) -> Option<SystemTime> {
    std::fs::read_dir(dir.join("save"))
        .ok()?
        .flatten()
        .filter_map(|slot| std::fs::metadata(slot.path().join("game.sii")).ok()?.modified().ok())
        .max()
}

/// A mod as a profile lists it (`<package>|<name as shown>`): its files, in the Documents folder's `mod` or
/// the game's Workshop folder, and the maps it carries.
pub fn find_mod(listed: &str, game: Option<&Game>, mod_dir: &Path) -> Mod {
    let (package, name) = listed.split_once('|').unwrap_or((listed, listed));
    let mut found = Mod {
        package: package.to_owned(),
        name: name.to_owned(),
        files: None,
        categories: Vec::new(),
        map: MapContent::default(),
        problem: None,
    };
    let files = match workshop_id(package) {
        Some(id) => {
            let folder = game.and_then(Game::workshop).map(|workshop| workshop.join(id.to_string()));
            folder.and_then(|folder| workshop_files(&folder, game.and_then(|game| game.version)))
        }
        None => ["scs", "zip"]
            .iter()
            .map(|extension| mod_dir.join(format!("{package}.{extension}")))
            .chain([mod_dir.join(package)])
            .find(|path| path.exists()),
    };
    let Some(files) = files else {
        found.problem = Some("not found".into());
        return found;
    };
    match ModFiles::open(&files) {
        Ok(opened) => {
            found.categories = opened.categories();
            found.map = opened.map_content();
        }
        Err(problem) => found.problem = Some(problem),
    }
    found.files = Some(files);
    found
}

/// The mods installed for the game, whether a profile has them active or not, each by what tells it: its
/// file's (or folder's) name in the Documents folder's `mod` (`Jaspers_DAF_2021_v1.2`) with its manifest's
/// name, and each Workshop item's manifest name (its files for `game`'s version).
pub fn installed(game_documents: &Path, game: Option<&Game>) -> Vec<String> {
    let named = |files: &Path, file_name: Option<String>| {
        let shown = ModFiles::open(files).ok().and_then(|opened| opened.display_name());
        let parts: Vec<String> = file_name.into_iter().chain(shown).collect();
        (!parts.is_empty()).then(|| parts.join(" "))
    };
    let in_mod_folder = std::fs::read_dir(game_documents.join("mod")).into_iter().flatten().flatten().filter_map(|entry| {
        let path = entry.path();
        let archive = path.extension().is_some_and(|extension| extension == "scs" || extension == "zip");
        if !archive && !path.is_dir() {
            return None;
        }
        let stem = path.file_stem().and_then(|stem| stem.to_str()).map(str::to_owned);
        named(&path, stem)
    });
    let workshop = game.and_then(Game::workshop);
    let in_workshop = workshop
        .iter()
        .flat_map(|folder| std::fs::read_dir(folder).into_iter().flatten().flatten())
        .filter_map(|item| {
            let files = workshop_files(&item.path(), game.and_then(|game| game.version))?;
            named(&files, None)
        });
    in_mod_folder.chain(in_workshop).collect()
}

/// A Workshop mod's item id, from its package name (`mod_workshop_package.00000000ABCDEF01`).
pub fn workshop_id(package: &str) -> Option<u64> {
    u64::from_str_radix(package.strip_prefix("mod_workshop_package.")?, 16).ok()
}

/// What of a Workshop item to mount for the game's `version`: the package its `versions.sii` names for that
/// version, else one for any version (`universal`, `latest`…), as a folder or an archive (`sound.scs`,
/// `latest.zip`); an item without the file is its own folder.
fn workshop_files(item: &Path, version: Option<crate::game::Version>) -> Option<PathBuf> {
    let Some(units) = read_units(&item.join("versions.sii")) else {
        return item.is_dir().then(|| item.to_path_buf());
    };
    let packages = packages_by_version(&units);
    let (package, _) = version
        .and_then(|version| packages.iter().find(|(_, versions)| versions.iter().any(|v| version.matches(v))))
        .or_else(|| packages.iter().find(|(_, versions)| versions.is_empty()))?;
    [item.join(package), item.join(format!("{package}.scs")), item.join(format!("{package}.zip"))]
        .into_iter()
        .find(|path| path.exists())
}

/// A Workshop item's packages (`versions.sii`): each one's folder and the game versions it's for (none: any,
/// its `universal` one).
pub fn packages_by_version(units: &[Unit]) -> Vec<(String, Vec<String>)> {
    units
        .iter()
        .filter(|unit| unit.class == "package_version_info")
        .filter_map(|unit| {
            let versions = unit.all("compatible_versions").map(str::to_owned).collect();
            Some((unit.get("package_name")?.to_owned(), versions))
        })
        .collect()
}

/// A mod's files, opened to look into: the game's own archive or a zip, or a folder.
enum ModFiles {
    Archive(Archive),
    Folder(PathBuf),
}

/// Definitions the map is drawn from: in `def/` (`city.sii`, `city.<mod>.sii`…) and `def/world/`.
const MAP_DEFINITIONS: [&str; 3] = ["city", "country", "company"];
const WORLD_DEFINITIONS: [&str; 4] = ["road_look", "prefab", "terrain_material", "terrain_profile"];

impl ModFiles {
    fn open(files: &Path) -> Result<Self, String> {
        if files.is_dir() {
            return Ok(Self::Folder(files.to_path_buf()));
        }
        Archive::open(files).map(Self::Archive).map_err(|error| error.to_string())
    }

    fn read(&self, path: &str) -> Option<Vec<u8>> {
        match self {
            Self::Archive(archive) => archive.read(path).ok()?,
            Self::Folder(root) => std::fs::read(root.join(path)).ok(),
        }
    }

    /// A folder's subfolders and files (none if it isn't there, or its listing is hidden).
    fn list(&self, path: &str) -> (Vec<String>, Vec<String>) {
        match self {
            Self::Archive(archive) => {
                archive.list(path).ok().flatten().map(|listing| (listing.dirs, listing.files)).unwrap_or_default()
            }
            Self::Folder(root) => {
                let (mut dirs, mut files) = (Vec::new(), Vec::new());
                for entry in std::fs::read_dir(root.join(path)).into_iter().flatten().flatten() {
                    let Some(name) = entry.file_name().to_str().map(str::to_owned) else { continue };
                    if entry.path().is_dir() { dirs.push(name) } else { files.push(name) }
                }
                dirs.sort();
                files.sort();
                (dirs, files)
            }
        }
    }

    /// Its name, as its manifest gives it (`display_name`).
    fn display_name(&self) -> Option<String> {
        let units = save::read(&self.read("manifest.sii")?).ok()?;
        units.iter().find(|unit| unit.class == "mod_package")?.get("display_name").map(str::to_owned)
    }

    /// What its manifest says it is.
    fn categories(&self) -> Vec<String> {
        let Some(units) = self.read("manifest.sii").and_then(|data| save::read(&data).ok()) else { return Vec::new() };
        units
            .iter()
            .filter(|unit| unit.class == "mod_package")
            .flat_map(|unit| unit.all("category"))
            .map(str::to_owned)
            .collect()
    }

    fn map_content(&self) -> MapContent {
        let (maps, _) = self.list("map");
        let sectors = maps
            .into_iter()
            .filter(|map| self.list(&format!("map/{map}")).1.iter().any(|f| f.ends_with(".base")))
            .collect();
        let defines = |files: &[String], kinds: &[&str]| {
            files
                .iter()
                .any(|file| kinds.iter().any(|kind| file.starts_with(&format!("{kind}.")) && file.ends_with(".sii")))
        };
        let definitions =
            defines(&self.list("def").1, &MAP_DEFINITIONS) || defines(&self.list("def/world").1, &WORLD_DEFINITIONS);
        MapContent { sectors, definitions }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mods installed on this PC for its game (the Documents folder's `mod` and the Workshop), printed.
    #[test]
    #[ignore = "reads this PC's game folders"]
    fn lists_the_mods_installed_here() {
        let documents = crate::places::game_documents().expect("the game's Documents folder");
        let game = crate::places::steam()
            .into_iter()
            .flat_map(|steam| crate::places::steam_libraries(&steam))
            .map(|library| library.join("steamapps").join("common").join("Euro Truck Simulator 2"))
            .find_map(|dir| Game::recognise(&dir));
        for named in installed(&documents, game.as_ref()) {
            println!("{named}");
        }
    }

    #[test]
    fn knows_hvts_map_from_their_other_mods() {
        assert!(is_hvt_map("EA_HVT_RTH_V1.3_Part1|Road to Heavy (v1.3) - Part 1"));
        assert!(is_hvt_map("EA_HVT_RTH_V1.3_Realistic_Lights_Addon|Realistic Lights (Addon) for Road to Heavy"));
        assert!(is_hvt_map("mod_workshop_package.00000000ABCDEF01|Road To Heavy v1.4"));
        // Their buses don't change the map; nor do other mods.
        assert!(!is_hvt_map("HVT_Gen3_BASE|HVT - MAN Lion's Coach 3rd Gen (BASE)"));
        assert!(!is_hvt_map("mod_workshop_package.00000000ADCFB054|tree_improved_4k"));
    }

    #[test]
    fn names_a_profile_from_its_folder() {
        assert_eq!(name_from_folder("485654202D2053617665").as_deref(), Some("HVT - Save"));
        assert_eq!(name_from_folder("C384"), Some("Ä".into()));
        assert_eq!(name_from_folder("4"), None);
        assert_eq!(name_from_folder("zz"), None);
    }

    #[test]
    fn reads_a_workshop_items_id() {
        assert_eq!(workshop_id("mod_workshop_package.00000000449D50B1"), Some(1_151_160_497));
        assert_eq!(workshop_id("Jaspers_DAF_2021_v1.2"), None);
    }

    #[test]
    fn picks_a_workshop_items_package_for_the_game_version() {
        let units = westplay_mapdata::sii::parse(
            "SiiNunit\n{\npackage_version_info : .144.content {\n package_name: \"144_content\"\n compatible_versions[]: \"1.44.*\"\n}\npackage_version_info : .universal {\n package_name: \"universal\"\n}\n}",
        );
        let packages = packages_by_version(&units);
        assert_eq!(packages, vec![("144_content".into(), vec!["1.44.*".into()]), ("universal".into(), vec![])]);
    }

    #[test]
    fn knows_the_map_a_profile_plays() {
        assert_eq!(map_of(Some("/map/hvtmap.mbd")), "hvtmap");
        assert_eq!(map_of(Some("/map/europe.mbd")), "europe");
        assert_eq!(map_of(None), "europe");
        assert_eq!(map_of(Some("/somewhere/else.txt")), "europe");
    }

    /// A mod folder holding `files` (path, contents).
    fn mod_folder(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("westplay-setup-{name}-{}", std::process::id()));
        for (path, contents) in files {
            let path = dir.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }
        dir
    }

    #[test]
    fn tells_map_mods_from_others_by_what_is_in_them() {
        let manifest = |category: &str| {
            format!("SiiNunit\n{{\nmod_package : .package_name {{\n category[]: \"{category}\"\n}}\n}}\n")
        };
        // A standalone map: its own map's sectors, and its cities.
        let standalone =
            mod_folder("standalone", &[("map/mymap/sec+0000+0000.base", "x"), ("def/city.mymap.sii", "x")]);
        // The base map changed (as ProMods or a road connection), saying nothing about itself.
        let extension = mod_folder("extension", &[("map/europe/sec-0001+0002.base", "x")]);
        // Map definitions only (a map's other part).
        let definitions = mod_folder("definitions", &[("def/world/prefab.mymap.sii", "x")]);
        // A truck, saying so.
        let truck_manifest = manifest("truck");
        let truck = mod_folder("truck", &[("manifest.sii", &truck_manifest), ("vehicle/truck/x.sii", "x")]);
        let found = |dir: &Path| find_mod(dir.file_name().unwrap().to_str().unwrap(), None, dir.parent().unwrap());

        let standalone_mod = found(&standalone);
        assert_eq!(standalone_mod.map, MapContent { sectors: vec!["mymap".into()], definitions: true });
        assert!(standalone_mod.is_map_mod() && standalone_mod.affects_map("mymap"));
        let extension_mod = found(&extension);
        assert!(
            extension_mod.is_map_mod() && extension_mod.affects_map("europe") && !extension_mod.affects_map("mymap")
        );
        let definitions_mod = found(&definitions);
        assert!(!definitions_mod.is_map_mod() && definitions_mod.affects_map("europe"));
        let truck_mod = found(&truck);
        assert_eq!(truck_mod.categories, ["truck"]);
        assert!(!truck_mod.is_map_mod() && !truck_mod.affects_map("europe"));
        for dir in [standalone, extension, definitions, truck] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn a_mod_that_isnt_there_says_so() {
        let found = find_mod("missing_mod|A missing mod", None, Path::new("Z:/no/such/folder"));
        assert_eq!((found.package.as_str(), found.name.as_str()), ("missing_mod", "A missing mod"));
        assert_eq!(found.problem.as_deref(), Some("not found"));
    }

    /// The real thing, on this PC: every profile and its mods. `cargo test -p westplay-setup -- --ignored --nocapture`
    #[test]
    #[ignore = "looks at the PC's profiles"]
    fn finds_the_profiles_on_this_pc() {
        let game = crate::game::find().into_iter().next();
        let documents = crate::places::game_documents().unwrap();
        let userdata = crate::places::steam().map(|steam| steam.join("userdata"));
        for profile in find(&documents, userdata.as_deref(), game.as_ref()) {
            println!("{} (plays {}, cloud {}, {} mods)", profile.name, profile.map, profile.cloud, profile.mods.len());
            for m in profile.mods.iter().filter(|m| m.is_map_mod() || m.affects_map(&profile.map)) {
                let role = if m.affects_map(&profile.map) { "matters to its map" } else { "a map mod, not of its map" };
                println!(
                    "  {} ({role}; sectors of {:?}, definitions {}, says {:?})",
                    m.name, m.map.sectors, m.map.definitions, m.categories
                );
            }
            for m in profile.mods.iter().filter(|m| m.problem.is_some()) {
                println!("  {} [{}]: {}", m.name, m.package, m.problem.as_deref().unwrap_or(""));
            }
        }
    }
}
