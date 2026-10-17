//! A game's DLCs, as the game itself knows them: each DLC archive (`dlc_*.scs`) carries its manifest
//! (`<archive>.manifest.sii`, a `dlc_package` unit) with its name (`display_name: "DLC - Going East!"`), its
//! Steam app (a DLC can come in more than one archive), and what it brings (`features[]: map`, a map
//! expansion). Counting archives would count paint jobs and flags packs, several to a DLC, as DLCs.

use westplay_mapdata::archive::Archive;
use westplay_mapdata::sii;

use crate::game::Game;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dlc {
    /// The archive's name: `dlc_east`.
    pub archive: String,
    /// As the game shows it, without its "DLC - " prefix: `Going East!`.
    pub name: String,
    /// Its Steam app, telling one DLC's archives apart from another's.
    pub steam_app: Option<String>,
    /// A map expansion.
    pub map: bool,
}

/// The DLCs in `game`, in archive order. An archive without a readable manifest still counts, by its name.
pub fn read(game: &Game) -> Vec<Dlc> {
    game.dlcs
        .iter()
        .map(|archive| {
            let path = game.dir.join(format!("{archive}.scs"));
            let manifest = Archive::open(&path)
                .ok()
                .and_then(|opened| opened.read(&format!("{archive}.manifest.sii")).ok().flatten())
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
            from_manifest(archive, manifest.as_deref())
        })
        .collect()
}

pub fn from_manifest(archive: &str, manifest: Option<&str>) -> Dlc {
    let units = manifest.map(sii::parse).unwrap_or_default();
    let package = units.iter().find(|unit| unit.class == "dlc_package");
    let name = package
        .and_then(|unit| unit.get("display_name"))
        .map(|name| name.trim_matches('"').trim_start_matches("DLC - ").trim().to_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| archive.to_owned());
    Dlc {
        archive: archive.to_owned(),
        name,
        steam_app: package.and_then(|unit| unit.get("steam_app_id")).map(str::to_owned),
        map: package.is_some_and(|unit| unit.all("features").any(|feature| feature == "map")),
    }
}

/// How many DLCs those archives are: one per Steam app (or per archive, for one without).
pub fn count(dlcs: &[Dlc]) -> usize {
    let mut apps: Vec<&str> = dlcs.iter().map(|dlc| dlc.steam_app.as_deref().unwrap_or(&dlc.archive)).collect();
    apps.sort_unstable();
    apps.dedup();
    apps.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    const EAST: &str = "SiiNunit\n{\ndlc_package : _nameless.dlc_east {\n\tid: eut2_east\n\tdisplay_name: \"DLC - Going East!\"\n\tsteam_app_id: 227310\n\tfeatures[]: map\n}\n}\n";
    const FLAGS: &str = "SiiNunit\n{\ndlc_package : _nameless.dlc_flags_tr {\n\tdisplay_name: \"DLC - Turkish Paint Jobs\"\n\tsteam_app_id: 388474\n}\n}\n";

    #[test]
    fn reads_a_dlc_from_its_manifest() {
        let east = from_manifest("dlc_east", Some(EAST));
        assert_eq!(east.name, "Going East!");
        assert_eq!(east.steam_app.as_deref(), Some("227310"));
        assert!(east.map);
        let flags = from_manifest("dlc_flags_tr", Some(FLAGS));
        assert_eq!(flags.name, "Turkish Paint Jobs");
        assert!(!flags.map);
        // No manifest: its archive's name.
        assert_eq!(from_manifest("dlc_odd", None).name, "dlc_odd");
    }

    /// The real thing, on this PC: `cargo test -p westplay-setup -- --ignored --nocapture this_pcs_dlcs`
    #[test]
    #[ignore = "looks at the PC's installs"]
    fn reads_this_pcs_dlcs() {
        for game in crate::game::find() {
            let dlcs = read(&game);
            let maps: Vec<&str> = dlcs.iter().filter(|dlc| dlc.map).map(|dlc| dlc.name.as_str()).collect();
            println!("{}: {} archives, {} DLCs; maps: {maps:?}", game.dir.display(), dlcs.len(), count(&dlcs));
        }
    }

    #[test]
    fn counts_a_dlcs_archives_once() {
        let metallics = |archive: &str| Dlc {
            archive: archive.into(),
            name: "Metallic Paint Jobs".into(),
            steam_app: Some("1".into()),
            map: false,
        };
        let east = from_manifest("dlc_east", Some(EAST));
        assert_eq!(count(&[metallics("dlc_metallics"), metallics("dlc_metallics2"), east]), 2);
    }
}
