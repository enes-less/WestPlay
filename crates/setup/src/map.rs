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

use crate::game::{self, Game};
use crate::profiles::{Mod, Profile};

/// How WestPlay builds maps: raise it when a map built before is no longer what it would build now.
pub const BUILDER_VERSION: u32 = 1;

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

/// The game mounted with the profile's map mods over it, in its load order: the profile lists its mods top
/// first, as the game's Mod Manager shows them, the top one overriding those below, so they mount bottom
/// first.
pub fn mount(game: &Game, profile: &Profile) -> Result<GameFiles, FileError> {
    let mut files = GameFiles::install(&game.dir)?;
    for m in profile.map_mods().collect::<Vec<_>>().into_iter().rev() {
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

/// Builds the map `profile` plays into its folder in `maps`, telling `progress`; not while the game runs.
/// Returns the folder.
pub fn build(
    game: &Game,
    profile: &Profile,
    language: Option<&str>,
    maps: &Path,
    progress: &mut dyn FnMut(Progress),
) -> Result<(PathBuf, Summary), BuildError> {
    if game::running() {
        return Err(BuildError::GameRunning);
    }
    let out = folder(maps, profile);
    let files = mount(game, profile)?;
    let options = Options {
        map: profile.map.clone(),
        language: language.map(str::to_owned),
        around: None,
        source: Some(source(game, profile, language)),
    };
    let summary = export::export(&files, &options, &out, progress)?;
    Ok((out, summary))
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
            map: MapContent { sectors: sectors.iter().map(|s| (*s).to_owned()).collect(), definitions },
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
