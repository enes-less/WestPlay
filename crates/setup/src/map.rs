//! The player's map, built for the Maps app from their own game: the map a profile plays (`europe`, or a
//! standalone map mod's), from the game, its DLCs and the profile's map mods, into WestPlay's data folder. One
//! map for each combination (a vanilla profile and a ProMods one each have theirs), each built again only
//! when what it's built from changes: the game's version or DLCs, the map mods (or their files), the
//! language, or how WestPlay builds maps.
//!
//! What a map is built from is kept in it (`map.json`'s `source`): the mods by their file names (never their
//! names as shown: a licence file's can hold the player's key), sizes and times.
//!
//! A map is only ever built while the game is closed: building the whole map takes several gigabytes of
//! memory (its peak private bytes 5.19 GB for the base map with every DLC, 7.15 GB for ProMods 2.84: map_check,
//! 2026-11-11), which a PC running the game may not have to spare. A build under way ends when the game starts
//! (host/maps.rs).
//! Players run WestPlay first, for its first setup, then the game; a map to build again (the game updated, a
//! map mod changed) waits until the game is closed, the cab keeping the map it has meanwhile.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde_json::json;
use westplay_mapdata::archive::Archive;
use westplay_mapdata::export::{self, ExportError, Options, Progress, Summary};
use westplay_mapdata::files::{FileError, GameFiles, Mount};
use westplay_mapdata::world::MapError;

use crate::game::{self, Game};
use crate::profiles::{Mod, Profile};

/// How WestPlay builds maps: raise it when a map built before is no longer what it would build now. 2 (1.2):
/// map mods mounted in the game's order (they had been the wrong way round), ProMods' hidden archives,
/// nodes two map mods share, filling stations two fuel companies stand by. 3 (after 1.2.0): the road graph
/// for WestPlay's own routes (route-graph.bin beside the tiles). 4 (after 1.2.0): rest areas marked only by
/// their triggers, made of roads and models rather than a rest area's prefab. 5 (after 1.2.0): which country
/// each part of the map is in and their time zones (countries.json beside the tiles), for the cab's clock. 6
/// (after 1.2.0): the countries' squares without a road filled with the nearest's (the page only looks up). 7
/// (after 1.2.0): the road graph marks the game's invisible roads (border crossings' and toll plazas' lanes),
/// never a waypoint's place, and leaves out roads no vehicle may drive (`traffic_lane.no_vehicles` only). 8
/// (after 1.2.0): the buildings, raised (the tiles' `building` layer: buildings.rs). 9 (after 1.2.0): their
/// shapes traced from their models (outlines, heights, roofs over open ground), not their boxes. 10 (after
/// 1.2.0): the places' own buildings, from their prefabs' models (filling stations, garages, depots…). 11
/// (after 1.2.0): the buildings structure sets' walls close round, their roofs' details no longer on the ground.
/// 12 (after 1.2.0): a place's prefab the map gives no roads draws none (a filling station's forecourt lanes
/// were a white blob). 13 (after 1.2.0): it draws its way through again, as wide as the roads meeting it (12
/// left the roads into such stations stopping short), none where nothing joins it; `blke_40`'s roads on their lane.
/// 14 (after 1.2.0): the tunnels, the roads' `tunnel` (area.rs mark_tunnels). 15 (after 2.0.3): map mods read
/// that were left out (sectors of 895, environment areas before 901, emptied directories in protected
/// archives) and older mods' prefabs (from version 22, skipped before); only a map that left a mod out is
/// built again for it ([`LEFT_OUT_ONLY`]). 16 to 25 were builders since scrapped, so local maps may carry them, and
/// nothing was built by 26. 27 (after 2.1.4): the new drawing (surface/): roads and junctions as exact surfaces in tiles of
/// their own. 29 (after 2.1.4; nothing was built by 28): service areas' ground (the tiles' `service` layer), private roads
/// back in the road graph, only to reach or leave what's on them (a route never goes through one), and each place's node
/// chosen by what a route gets to, through a service area to the far end of the lane past the place (access.rs).
/// 31 (after 2.1.4; nothing was built by 30): road lines for what the game leaves off its map too, and no-entry signs at
/// those roads' entrances (no_entry.rs). 33 (after 2.1.4; nothing was built by 32): the roads at their real heights
/// (surface/raised.rs). 35 (after 2.1.4; nothing was built by 34): only intersections and bridges rise. 36: grass and
/// yards round a road aren't road, a road's height goes evenly along each piece, and junctions in tunnels are tunnels.
/// 37: the flat map's copy of a raised road goes by its file, for the cab to take off the map; a filling station along a
/// two-way road has only that road. 39 (nothing was built by 38): a bridge over open water that comes down to its ends
/// stands as high as it is over the water. 41 (nothing was built by 40): the buildings in their real shapes, their
/// models' own triangles (crates/mapdata meshes.rs), and ATS's landmarks (landmarks.rs). 42: ATS's sea held to the
/// game's own picture of its world and that picture drawn beyond the map (crates/mapdata world_picture.rs), not its
/// edges carried straight out. 43: ATS's map only as the player's game has it, its map DLCs' states (crates/mapdata
/// dlc.rs). 44 (2.2.1): ATS's map starts at San Francisco (it started in empty desert), and every map is prepared
/// again with the surfaces' lower memory (the maps alike, byte for byte): owner 2026-11-04, "everyone should get SF",
/// and the maps prepared again with the update. 45: models in the game's older formats (0x14, 0x13, 0x12) read as they
/// are (crates/mapdata pmg.rs: a map mod's older junctions and buildings drawn, the Oyuncuyusbis map's 702), and the same
/// map from one build to the next.
pub const BUILDER_VERSION: u32 = 45;
/// The builders whose change reached only maps that left a mod out (15: mods read that were left out before): a map that
/// left none out, built by a builder since which there were only these, is as today's, and is kept (a mod's old prefabs
/// it skipped are drawn once it's built again for another reason). Every other builder builds every map again. (Until
/// 2026-11-11 it was the oldest builder whose maps were today's but for those, to be moved up with every builder that
/// changed every map; left at 40, it kept maps built by 41 to 43 that left nothing out through 42, 43 and 44.)
const LEFT_OUT_ONLY: &[u32] = &[15];

/// The folder of the map `profile` plays, in `maps` (WestPlay's maps folder): one per map and its map mods.
pub fn folder(maps: &Path, profile: &Profile) -> PathBuf {
    let mods: Vec<&str> = profile.map_mods().map(|m| m.package.as_str()).collect();
    maps.join(format!("{}-{:016x}", profile.map, fnv1a(mods.join("\n").as_bytes())))
}

/// What the map `profile` plays would be built from now.
pub fn source(game: &Game, profile: &Profile, language: Option<&str>) -> serde_json::Value {
    let mods: Vec<_> = profile
        .map_mods()
        .map(|m| {
            let (size, modified) = m.files.as_deref().map(stamp).unwrap_or((0, 0));
            json!({ "package": m.package, "size": size, "modified": modified })
        })
        .collect();
    json!({
        "builder": BUILDER_VERSION,
        "game": game.version.map(|version| version.to_string()),
        "dlcs": game.dlcs,
        "map": profile.map,
        "mods": mods,
        "language": language,
    })
}

/// Whether the map in `folder` isn't (yet) what `source` would build.
pub fn needs_building(folder: &Path, source: &serde_json::Value) -> bool {
    let Some(built) = export::built_from(folder) else { return true };
    if &built == source {
        return false;
    }
    // Built by a builder since which only maps that left a mod out differ, and it left none: as today's.
    let builder = built["builder"].as_u64().unwrap_or(0);
    if same_but_left_out(builder, BUILDER_VERSION, LEFT_OUT_ONLY) && left_out(folder).is_empty() {
        let mut as_now = built;
        as_now["builder"] = json!(BUILDER_VERSION);
        return &as_now != source;
    }
    true
}

/// Whether a map built by `builder` is as one built by `today` but for the mods it left out: older, and every builder
/// since one of `only` ([`LEFT_OUT_ONLY`]).
fn same_but_left_out(builder: u64, today: u32, only: &[u32]) -> bool {
    builder < u64::from(today)
        && (builder + 1..=u64::from(today)).all(|since| only.iter().any(|&o| u64::from(o) == since))
}

/// The game mounted with the profile's map mods over it, in its load order: the profile lists its mods from
/// the bottom of the game's Mod Manager up (checked 2026-10-19 against a profile's Mod Manager), the top one
/// overriding those below, so they mount as listed, each over those before.
pub fn mount(game: &Game, profile: &Profile) -> Result<GameFiles, FileError> {
    mount_without(game, profile, &[])
}

/// [`mount`], leaving out the mods whose files are `left_out`.
fn mount_without(game: &Game, profile: &Profile, left_out: &[PathBuf]) -> Result<GameFiles, FileError> {
    let mut files = GameFiles::install(&game.dir)?;
    for m in profile.map_mods() {
        if m.files.as_ref().is_some_and(|path| left_out.contains(path)) {
            continue;
        }
        if let Some(mount) = open(m)? {
            files.mount(mount);
        }
    }
    Ok(files)
}

fn open(m: &Mod) -> Result<Option<Mount>, FileError> {
    let Some(path) = m.files.as_deref() else { return Ok(None) };
    if path.is_dir() {
        return Ok(Some(Mount::Folder(path.to_path_buf())));
    }
    Archive::open(path).map(|archive| Some(Mount::Archive(archive))).map_err(|error| FileError {
        path: String::new(),
        mount: path.to_path_buf(),
        error,
    })
}

/// Why a map wasn't built.
#[derive(Debug)]
pub enum BuildError {
    /// The game is running: maps are built while it's closed.
    GameRunning,
    Export(ExportError),
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GameRunning => write!(f, "the game is running: the map is built once it's closed"),
            Self::Export(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for BuildError {}

impl From<ExportError> for BuildError {
    fn from(error: ExportError) -> Self {
        Self::Export(error)
    }
}

impl From<FileError> for BuildError {
    fn from(error: FileError) -> Self {
        Self::Export(ExportError::Files(error))
    }
}

/// A map built: its folder, what it holds, and the map mods left out of it.
pub struct Built {
    pub folder: PathBuf,
    pub summary: Summary,
    pub left_out: Vec<LeftOut>,
}

/// A map mod left out of a profile's map: its files couldn't be read (a damaged archive, or one protected in a
/// way WestPlay can't read), so the map was built from the rest.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LeftOut {
    /// As the Mod Manager names it.
    pub name: String,
    /// What couldn't be read, and why.
    pub error: String,
}

/// Where a map built before its manifest held them kept the mods left out of it (none: no file).
const LEFT_OUT_FILE: &str = "left-out.json";

/// The map mods left out of the map built in `folder`: its manifest's `leftOut`, or (a map built before that) its
/// `left-out.json`.
pub fn left_out(folder: &Path) -> Vec<LeftOut> {
    let read = |path: PathBuf| {
        std::fs::read(path).ok().and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
    };
    if let Some(left) = read(folder.join("map.json")).and_then(|manifest| manifest.get("leftOut").cloned()) {
        return serde_json::from_value(left).unwrap_or_default();
    }
    read(folder.join(LEFT_OUT_FILE)).and_then(|left| serde_json::from_value(left).ok()).unwrap_or_default()
}

/// Builds the map `profile` plays into its folder in `maps`, telling `progress`; not while the game runs.
/// A map mod whose files can't be read is left out and the map built from the rest (and from the start
/// again: `progress` goes back), rather than none built: the game itself reads what it can.
pub fn build(
    game: &Game,
    profile: &Profile,
    language: Option<&str>,
    maps: &Path,
    stop: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    progress: &mut dyn FnMut(Progress),
) -> Result<Built, BuildError> {
    if game::running() {
        return Err(BuildError::GameRunning);
    }
    let out = folder(maps, profile);
    let mut options = Options {
        map: profile.map.clone(),
        language: language.map(str::to_owned),
        around: None,
        source: Some(source(game, profile, language)),
        stop,
        game_zone: Some(game.kind.sdk_time_zone()),
        sea_by_nearest: game.kind == game::Kind::Ats,
        // ATS's map holds every state, each guarded by its DLC: drawn only as the player's game has it (its guards'
        // list read from its executable; unread, the map drawn whole and the host says so).
        dlc_guards: (game.kind == game::Kind::Ats)
            .then(|| std::fs::read(game.dir.join(game.kind.executable())).ok())
            .flatten()
            .and_then(|exe| westplay_mapdata::dlc::read_guards(&exe)),
        left_out: None,
        water_held: false,
    };
    let mut skipped: Vec<PathBuf> = Vec::new();
    let mut left_out: Vec<LeftOut> = Vec::new();
    loop {
        // What's left out so far, written into the map's manifest with it.
        options.left_out = Some(serde_json::to_value(&left_out).map_err(|error| ExportError::Io(error.into()))?);
        let built = mount_without(game, profile, &skipped)
            .map_err(BuildError::from)
            .and_then(|files| Ok(export::export(&files, &options, &out, progress)?));
        let error = match built {
            Ok(summary) => {
                // The file an older build kept them in, no longer read (the manifest's are): let go.
                let _ = std::fs::remove_file(out.join(LEFT_OUT_FILE));
                return Ok(Built { folder: out, summary, left_out });
            }
            Err(error) => error,
        };
        let culprit = unread_mod(&error, profile).filter(|m| m.files.as_ref().is_some_and(|f| !skipped.contains(f)));
        let Some(m) = culprit else { return Err(error) };
        left_out.push(LeftOut { name: m.name.clone(), error: error.to_string() });
        skipped.extend(m.files.clone());
    }
}

/// The profile's map mod whose files a build failed to read, if that's why it failed (never the game's own:
/// a game that can't be read has no map without it).
fn unread_mod<'a>(error: &BuildError, profile: &'a Profile) -> Option<&'a Mod> {
    let from = match error {
        BuildError::Export(ExportError::Files(error) | ExportError::Map(MapError::File(error))) => &error.mount,
        BuildError::Export(ExportError::Map(MapError::Sector { from: Some(from), .. })) => from,
        _ => return None,
    };
    profile.map_mods().find(|m| m.files.as_deref() == Some(from.as_path()))
}

/// A file's (or folder's) size and when it was last changed (seconds since 1970), to tell a mod updated.
///
/// A folder's: all its files' sizes together, and the newest time of them and its folders. (The folder's own was
/// taken: its size 0 on Windows, its time changed only by what's in it directly, so a file changed deeper in a
/// folder mod never built its map again: the review of 2026-11-11, minor 27.)
fn stamp(path: &Path) -> (u64, u64) {
    let seconds = |metadata: &std::fs::Metadata| {
        let since = metadata.modified().ok().and_then(|time| time.duration_since(UNIX_EPOCH).ok());
        since.map_or(0, |since| since.as_secs())
    };
    let Ok(metadata) = std::fs::metadata(path) else { return (0, 0) };
    if !metadata.is_dir() {
        return (metadata.len(), seconds(&metadata));
    }
    let (mut size, mut newest) = (0u64, seconds(&metadata));
    let mut folders = vec![path.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).into_iter().flatten().flatten() {
            let Ok(metadata) = entry.metadata() else { continue };
            newest = newest.max(seconds(&metadata));
            if metadata.is_dir() {
                folders.push(entry.path());
            } else {
                size += metadata.len();
            }
        }
    }
    (size, newest)
}

/// FNV-1a, 64 bits: a short, stable name for a combination of mods.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{FoundBy, Kind, Version};
    use crate::profiles::MapContent;

    fn a_mod(package: &str, sectors: &[&str], definitions: bool) -> Mod {
        Mod {
            package: package.into(),
            name: package.into(),
            files: None,
            categories: vec![],
            author: None,
            map: MapContent { sectors: sectors.iter().map(|s| (*s).to_owned()).collect(), definitions, hidden: false },
            problem: None,
        }
    }

    fn profile(map: &str, mods: Vec<Mod>) -> Profile {
        Profile { name: "P".into(), dir: PathBuf::new(), cloud: false, last_saved: None, map: map.into(), mods }
    }

    fn game() -> Game {
        Game {
            kind: Kind::Ets2,
            dir: PathBuf::new(),
            found_by: FoundBy::Steam,
            version: Some(Version([1, 61, 1, 9])),
            dlcs: vec!["dlc_east".into()],
        }
    }

    #[test]
    fn mounts_the_mod_managers_top_over_those_below() {
        // Two mods with the same file; the profile lists them from the Mod Manager's bottom up, as Eren's lists
        // ProMods' Definition Package last and the Mod Manager shows it at the top.
        let root = std::env::temp_dir().join(format!("westplay-mount-{}", std::process::id()));
        let with_city = |name: &str| {
            let dir = root.join(name);
            std::fs::create_dir_all(dir.join("def")).unwrap();
            std::fs::write(dir.join("def").join("city.sii"), name).unwrap();
            dir
        };
        let as_mod = |name: &str| Mod { files: Some(with_city(name)), ..a_mod(name, &[], true) };
        let (below, top) = (as_mod("below"), as_mod("top"));
        let game = Game { dir: root.join("game"), ..game() };
        std::fs::create_dir_all(&game.dir).unwrap();
        let files = mount(&game, &profile("europe", vec![below.clone(), top.clone()])).unwrap();
        assert_eq!(files.read("/def/city.sii").unwrap().unwrap(), b"top");
        // The other way round in the Mod Manager: the other one.
        let files = mount(&game, &profile("europe", vec![top, below])).unwrap();
        assert_eq!(files.read("/def/city.sii").unwrap().unwrap(), b"below");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A folder mod's stamp changes when a file deep in it does (its size, then its time).
    #[test]
    fn a_folder_mods_stamp_changes_with_a_file_deep_in_it() {
        let dir = std::env::temp_dir().join(format!("westplay-stamp-{}", std::process::id()));
        let deep = dir.join("map").join("europe");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("sec+0000+0000.base"), b"12345").unwrap();
        std::fs::write(dir.join("manifest.sii"), b"ab").unwrap();
        let first = stamp(&dir);
        assert_eq!(first.0, 7, "every file's size");
        std::fs::write(deep.join("sec+0000+0000.base"), b"1234567").unwrap();
        assert_eq!(stamp(&dir).0, 9);
        // The same size, later: its time.
        let file = std::fs::File::options().write(true).open(deep.join("sec+0000+0000.base")).unwrap();
        file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(120)).unwrap();
        drop(file);
        assert!(stamp(&dir).1 > first.1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_mods_left_out_are_the_manifests_or_an_older_maps_file() {
        let dir = std::env::temp_dir().join(format!("westplay-left-out-read-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let left = vec![LeftOut { name: "broken".into(), error: "a damaged archive".into() }];
        let write = |name: &str, value: serde_json::Value| std::fs::write(dir.join(name), value.to_string()).unwrap();
        // A map built since: its manifest's.
        write("map.json", json!({ "source": null, "leftOut": left }));
        assert_eq!(left_out(&dir), left);
        // Its manifest's, whatever an older build's file says (it isn't read any more).
        write(LEFT_OUT_FILE, json!([{ "name": "other", "error": "e" }]));
        assert_eq!(left_out(&dir), left);
        write("map.json", json!({ "source": null, "leftOut": [] }));
        assert!(left_out(&dir).is_empty());
        // A map built before: its file's.
        write("map.json", json!({ "source": null }));
        assert_eq!(left_out(&dir)[0].name, "other");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_mod_that_cant_be_read_is_named_and_left_out() {
        // A good mod, and a "mod" whose archive is damaged.
        let root = std::env::temp_dir().join(format!("westplay-left-out-{}", std::process::id()));
        let good = root.join("good");
        std::fs::create_dir_all(good.join("def")).unwrap();
        std::fs::write(good.join("def").join("city.sii"), b"good").unwrap();
        let broken = root.join("broken.scs");
        std::fs::write(&broken, b"SCS#not really an archive").unwrap();
        let game = Game { dir: root.join("game"), ..game() };
        std::fs::create_dir_all(&game.dir).unwrap();
        let mods = vec![
            Mod { files: Some(good), ..a_mod("good", &[], true) },
            Mod { files: Some(broken.clone()), ..a_mod("broken", &[], true) },
        ];
        let profile = profile("europe", mods);

        // Mounted as it is, the damaged archive fails, and the failure names it.
        let error = BuildError::from(mount(&game, &profile).err().unwrap());
        assert_eq!(unread_mod(&error, &profile).map(|m| m.name.as_str()), Some("broken"));
        // Without it, the rest mounts and reads.
        let files = mount_without(&game, &profile, &[broken]).unwrap();
        assert_eq!(files.read("/def/city.sii").unwrap().unwrap(), b"good");
        // A failure that isn't a mod's file names none: nothing to leave out.
        assert!(unread_mod(&BuildError::GameRunning, &profile).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn has_a_map_for_each_combination_of_map_mods() {
        let maps = Path::new("maps");
        let truck = a_mod("truck", &[], false);
        let promods = a_mod("promods", &["europe"], true);
        let vanilla = profile("europe", vec![truck.clone()]);
        let with_promods = profile("europe", vec![truck.clone(), promods.clone()]);
        // Truck mods don't make another map.
        assert_eq!(folder(maps, &vanilla), folder(maps, &profile("europe", vec![])));
        assert_ne!(folder(maps, &vanilla), folder(maps, &with_promods));
        assert!(folder(maps, &with_promods).file_name().unwrap().to_str().unwrap().starts_with("europe-"));
        // ProMods' models and assets, their folders hidden: they may hold its junctions' models (they do), so
        // they're mounted for the map and make another map.
        let mut models = a_mod("promods-models", &[], false);
        models.map.hidden = true;
        let with_models = profile("europe", vec![truck.clone(), models, promods.clone()]);
        assert_eq!(
            with_models.map_mods().map(|m| m.package.as_str()).collect::<Vec<_>>(),
            ["promods-models", "promods"]
        );
        assert_ne!(folder(maps, &with_models), folder(maps, &with_promods));
        // A standalone map's own.
        let standalone = profile("hvtmap", vec![a_mod("rth", &["hvtmap"], true)]);
        assert!(folder(maps, &standalone).file_name().unwrap().to_str().unwrap().starts_with("hvtmap-"));
    }

    #[test]
    fn is_built_again_when_what_it_is_built_from_changes() {
        let profile = profile("europe", vec![a_mod("promods", &["europe"], true), a_mod("truck", &[], false)]);
        let source = source(&game(), &profile, Some("en_us"));
        assert_eq!(source["mods"].as_array().unwrap().len(), 1);
        assert_eq!(source["game"], "1.61.1.9");
        // A folder with no map, or another's: built.
        let dir = std::env::temp_dir().join(format!("westplay-map-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(needs_building(&dir, &source));
        std::fs::write(dir.join("map.json"), json!({ "source": source }).to_string()).unwrap();
        assert!(!needs_building(&dir, &source));
        let mut updated = source.clone();
        updated["game"] = json!("1.62.0.1");
        assert!(needs_building(&dir, &updated));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_map_an_earlier_builder_made_is_built_again_unless_only_maps_that_left_a_mod_out_changed_since() {
        // As at 15, the builder that read the mods left out before: a map by 14 is today's but for those.
        assert!(same_but_left_out(14, 15, &[15]));
        // By 13 (14 changed every map), by 14 when 16 is today's, or by today's own: not.
        assert!(!same_but_left_out(13, 15, &[15]));
        assert!(!same_but_left_out(14, 16, &[15]));
        assert!(!same_but_left_out(15, 15, &[15]));
        // Today: every map by an earlier builder is built again, those by 41 to 43 that left nothing out too (they were
        // kept through 42, 43 and 44 till 2026-11-11).
        for builder in [14, 40, 41, 42, 43, u64::from(BUILDER_VERSION) - 1] {
            assert!(!same_but_left_out(builder, BUILDER_VERSION, LEFT_OUT_ONLY), "built by {builder}");
        }

        let profile = profile("europe", vec![a_mod("sud", &["europe"], true)]);
        let source = source(&game(), &profile, Some("en_us"));
        let dir = std::env::temp_dir().join(format!("westplay-map-left-out-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let built_by = |builder: u32| {
            let mut built = source.clone();
            built["builder"] = json!(builder);
            std::fs::write(dir.join("map.json"), json!({ "source": built }).to_string()).unwrap();
        };
        built_by(BUILDER_VERSION);
        assert!(!needs_building(&dir, &source));
        for builder in [41, 42, 43, BUILDER_VERSION - 1] {
            built_by(builder);
            assert!(needs_building(&dir, &source), "built by {builder}");
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
