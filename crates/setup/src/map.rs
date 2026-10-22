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
//! memory (5.3 GB for the base map with every DLC), which a PC running the game may not have to spare.
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
/// 14 (after 1.2.0): the tunnels, the roads' `tunnel` (area.rs mark_tunnels).
pub const BUILDER_VERSION: u32 = 14;

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
    export::built_from(folder).as_ref() != Some(source)
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

/// Where a map keeps the mods left out of it (none: no file).
const LEFT_OUT_FILE: &str = "left-out.json";

/// The map mods left out of the map built in `folder`.
pub fn left_out(folder: &Path) -> Vec<LeftOut> {
    std::fs::read(folder.join(LEFT_OUT_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
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
    let options = Options {
        map: profile.map.clone(),
        language: language.map(str::to_owned),
        around: None,
        source: Some(source(game, profile, language)),
        stop,
    };
    let mut skipped: Vec<PathBuf> = Vec::new();
    let mut left_out: Vec<LeftOut> = Vec::new();
    loop {
        let built = mount_without(game, profile, &skipped)
            .map_err(BuildError::from)
            .and_then(|files| Ok(export::export(&files, &options, &out, progress)?));
        let error = match built {
            Ok(summary) => {
                let kept = out.join(LEFT_OUT_FILE);
                if left_out.is_empty() {
                    let _ = std::fs::remove_file(&kept);
                } else {
                    let json = serde_json::to_vec_pretty(&left_out).map_err(std::io::Error::from);
                    json.and_then(|json| std::fs::write(&kept, json)).map_err(ExportError::Io)?;
                }
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
fn stamp(path: &Path) -> (u64, u64) {
    let Ok(metadata) = std::fs::metadata(path) else { return (0, 0) };
    let modified = metadata.modified().ok().and_then(|time| time.duration_since(UNIX_EPOCH).ok());
    (metadata.len(), modified.map_or(0, |since| since.as_secs()))
}

/// FNV-1a, 64 bits: a short, stable name for a combination of mods.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{FoundBy, Version};
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
}
