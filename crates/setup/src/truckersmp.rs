//! TruckersMP's map, for a map of its own in the cab (PLAN.md: Navigation plan, B; owner 2026-10-21).
//!
//! TruckersMP mounts packs of its own over the game, into the game's mod pool (game.log: `[fs] device
//! <TruckersMP>\installation/data/ets2/mods/<pack>.mp mounted to mod pool.`, in their order), and none of the
//! profile's mods. A few of its packs carry map sectors, each replacing the base map's sector of the same
//! name (its HQ, event sites: read 2026-10-21), and which of them it mounts follows its events. So on
//! TruckersMP every profile plays one map: the game and its DLCs with TruckersMP's packs over them. It's
//! built as a stand-in profile ([`profile`]) whose mods are those packs, as a profile's map mods are.
//!
//! Which packs, in which order: the last TruckersMP session's, as its game.log says ([`mounted`]), kept
//! ([`remember`]); until there's been one, every pack TruckersMP's launcher has put in its installation
//! ([`packs`]). Only TruckersMP's installation folder and the game's log are looked at: TruckersMP's own
//! files (its client, its data and caches) are left alone.

use std::path::{Path, PathBuf};

use crate::places;
use crate::profiles::{self, Profile};

/// TruckersMP's installation (`%APPDATA%\TruckersMP\installation`), if it's installed.
pub fn installation() -> Option<PathBuf> {
    installation_in(&places::roaming()?)
}

fn installation_in(roaming: &Path) -> Option<PathBuf> {
    let dir = roaming.join("TruckersMP").join("installation");
    dir.join("data").is_dir().then_some(dir)
}

/// The packs TruckersMP has for ETS2 (its own and those it shares with ATS), sorted: the shared ones first,
/// each folder's by name.
pub fn packs(installation: &Path) -> Vec<PathBuf> {
    let mut all = Vec::new();
    for folder in ["shared", "ets2"] {
        let mut found: Vec<PathBuf> = std::fs::read_dir(installation.join("data").join(folder).join("mods"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("mp")))
            .collect();
        found.sort();
        all.extend(found);
    }
    all
}

/// The packs a game session mounted from TruckersMP's installation, in order, from its game.log; `None` if the
/// session wasn't TruckersMP's.
pub fn mounted(log: &str) -> Option<Vec<PathBuf>> {
    let packs: Vec<PathBuf> = log
        .lines()
        .filter_map(|line| {
            let device = line.split_once("[fs] device ")?.1.strip_suffix(" mounted to mod pool.")?;
            let path = device.replace('/', "\\");
            path.to_lowercase().contains("\\truckersmp\\installation\\").then(|| PathBuf::from(path))
        })
        .collect();
    (!packs.is_empty()).then_some(packs)
}

/// The packs the last TruckersMP session mounted, as WestPlay kept them (`record`: a file of WestPlay's),
/// those still there; `None` before any.
pub fn remembered(record: &Path) -> Option<Vec<PathBuf>> {
    let text = std::fs::read_to_string(record).ok()?;
    let packs: Vec<PathBuf> =
        serde_json::from_str::<Vec<PathBuf>>(&text).ok()?.into_iter().filter(|p| p.is_file()).collect();
    (!packs.is_empty()).then_some(packs)
}

/// Keeps the packs a TruckersMP session mounted, for its map until the next session; whether that changed them.
pub fn remember(record: &Path, packs: &[PathBuf]) -> bool {
    let text = serde_json::to_string_pretty(packs).unwrap_or_default();
    if std::fs::read_to_string(record).ok().as_deref() == Some(text.as_str()) {
        return false;
    }
    std::fs::write(record, text).is_ok()
}

/// TruckersMP's map, as a profile: the base game's map with `packs` over it, in their order (each read as a
/// mod is: what it brings to the map), `dir` naming it (TruckersMP's installation).
pub fn profile(dir: &Path, packs: &[PathBuf]) -> Profile {
    Profile {
        name: "TruckersMP".into(),
        dir: dir.to_path_buf(),
        cloud: false,
        last_saved: None,
        map: "europe".into(),
        mods: packs
            .iter()
            .map(|pack| {
                let package = pack.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
                profiles::mod_from_files(&package, &package, pack)
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // As the owner's TruckersMP session logged its mounts (2026-10-21), with other lines between.
    const LOG: &str = r"00:00:02.318 : [fs] device C:/Users/someone/Documents/Euro Truck Simulator 2 mounted to user pool.
00:00:02.327 : [zipfs] data1.mp: Created, 198 entries
00:00:02.327 : [fs] device C:\Users\someone\AppData\Roaming\TruckersMP\installation/data/ets2/mods/data1.mp mounted to mod pool.
00:00:02.335 : [fs] device C:\Users\someone\AppData\Roaming\TruckersMP\installation/data/shared/mods/data2.mp mounted to mod pool.
00:00:03.363 : [fs] device C:\Users\someone\AppData\Roaming\TruckersMP\installation/data/ets2/mods/hq_final.mp mounted to mod pool.
00:00:03.513 : [ufs] mod pool: 25 packs, 15514 files, 2540 dirs
00:00:12.784 : [fs] device D:/Games/Euro Truck Simulator 2/dlc_east.scs mounted to scs pool.
00:00:29.354 : [fs] device C:/Users/someone/Documents/WestPlay/westplay.scs mounted to mod pool.
";

    #[test]
    fn reads_the_packs_a_truckersmp_session_mounted_in_order() {
        let base = r"C:\Users\someone\AppData\Roaming\TruckersMP\installation\data";
        assert_eq!(
            mounted(LOG),
            Some(vec![
                PathBuf::from(format!(r"{base}\ets2\mods\data1.mp")),
                PathBuf::from(format!(r"{base}\shared\mods\data2.mp")),
                PathBuf::from(format!(r"{base}\ets2\mods\hq_final.mp")),
            ])
        );
    }

    #[test]
    fn a_session_without_truckersmp_mounted_none() {
        let single_player: String =
            LOG.lines().filter(|line| !line.contains("TruckersMP")).collect::<Vec<_>>().join("\n");
        assert_eq!(mounted(&single_player), None);
    }

    #[test]
    fn finds_the_installation_and_its_packs() {
        let roaming = std::env::temp_dir().join(format!("westplay-tmp-{}", std::process::id()));
        let data = roaming.join("TruckersMP").join("installation").join("data");
        for (folder, file) in
            [("ets2", "hq_final.mp"), ("ets2", "data1.mp"), ("shared", "data2.mp"), ("ets2", "notes.txt")]
        {
            std::fs::create_dir_all(data.join(folder).join("mods")).unwrap();
            std::fs::write(data.join(folder).join("mods").join(file), b"").unwrap();
        }
        let installation = installation_in(&roaming).unwrap();
        let names: Vec<String> =
            packs(&installation).iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["data2.mp", "data1.mp", "hq_final.mp"]);
        // Remembered as a session mounted them, and forgotten if one is gone.
        let record = roaming.join("truckersmp.json");
        let session =
            vec![data.join("ets2").join("mods").join("hq_final.mp"), data.join("shared").join("mods").join("data2.mp")];
        assert!(remember(&record, &session));
        assert!(!remember(&record, &session));
        assert_eq!(remembered(&record), Some(session.clone()));
        std::fs::remove_file(&session[0]).unwrap();
        assert_eq!(remembered(&record), Some(vec![session[1].clone()]));
        std::fs::remove_dir_all(&roaming).unwrap();
        assert_eq!(installation_in(&roaming), None);
    }

    #[test]
    fn makes_truckersmp_a_profile_of_the_base_map_with_its_packs_in_order() {
        let packs = [PathBuf::from(r"C:\nowhere\data2.mp"), PathBuf::from(r"C:\nowhere\hq_final.mp")];
        let profile = profile(Path::new(r"C:\nowhere"), &packs);
        assert_eq!(profile.map, "europe");
        assert_eq!(profile.mods.iter().map(|m| m.package.as_str()).collect::<Vec<_>>(), ["data2", "hq_final"]);
        // A pack that can't be read says so (and brings nothing to the map).
        assert!(profile.mods.iter().all(|m| m.problem.is_some()));
    }
}
