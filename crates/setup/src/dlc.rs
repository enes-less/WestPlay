//! A game's DLCs, as the game itself knows them: each DLC archive (`dlc_*.scs`) carries its manifest
//! (`<archive>.manifest.sii`, a `dlc_package` unit) with its name (`display_name: "DLC - Going East!"`), its
//! Steam app (a DLC can come in more than one archive), and what it brings (`features[]: map`). Counting
//! archives would count paint jobs and flags packs, several to a DLC, as DLCs.
//!
//! A map expansion is a DLC that says `map` and brings cities of its own, more than one (`def/city/`): the
//! trailer packs say `map` too (Krone's, Feldbinder's: each brings its factory's town, Werlte, Winsen), and
//! Steam calls them trailer packs. As Steam's pages have them (2026-10-18): Going East!, Scandinavia, Vive la
//! France!, Italia, Beyond the Baltic Sea (which says `cargo` too), Road to the Black Sea, Iberia, West
//! Balkans, Greece and Nordic Horizons are map expansions, bringing 15 to 41 cities each.

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
    /// A map expansion (see the module's notes).
    pub map: bool,
}

/// The DLCs in `game`, in archive order. An archive without a readable manifest still counts, by its name.
pub fn read(game: &Game) -> Vec<Dlc> {
    game.dlcs
        .iter()
        .map(|archive| {
            let opened = Archive::open(&game.dir.join(format!("{archive}.scs"))).ok();
            let manifest = opened
                .as_ref()
                .and_then(|opened| opened.read(&format!("{archive}.manifest.sii")).ok().flatten())
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
            let cities = opened
                .as_ref()
                .and_then(|opened| opened.list("def/city").ok().flatten())
                .map_or(0, |listing| listing.files.len());
            from_manifest(archive, manifest.as_deref(), cities)
        })
        .collect()
}

/// A DLC from its archive's manifest, and the cities the archive brings (`def/city/`'s files).
pub fn from_manifest(archive: &str, manifest: Option<&str>, cities: usize) -> Dlc {
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
        map: cities > 1 && package.is_some_and(|unit| unit.all("features").any(|feature| feature == "map")),
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

    /// This PC's game's DLCs, as the onboarding takes them (a map expansion or not), printed.
    #[test]
    #[ignore = "reads this PC's game"]
    fn lists_the_dlcs_here() {
        let game = crate::places::steam()
            .into_iter()
            .flat_map(|steam| crate::places::steam_libraries(&steam))
            .map(|library| library.join("steamapps").join("common").join("Euro Truck Simulator 2"))
            .find_map(|dir| Game::recognise(&dir))
            .expect("the game");
        for dlc in read(&game) {
            println!("{:28} {:8} map: {:5}  {}", dlc.archive, dlc.steam_app.as_deref().unwrap_or("-"), dlc.map, dlc.name);
        }
    }

    /// The trailer packs say `map` as the map expansions do (they bring their factory's town), and Steam has
    /// them as trailer packs: one city isn't a map expansion; Beyond the Baltic Sea, `map` and `cargo`, is.
    #[test]
    fn a_trailer_pack_with_its_factorys_town_is_no_map_expansion() {
        const KRONE: &str = "SiiNunit
{
dlc_package : _nameless.dlc_krone {
	display_name: \"DLC - Krone Trailer Pack\"
	steam_app_id: 933610
	features[]: map
	features[]: cargo
}
}
";
        const BALTIC: &str = "SiiNunit
{
dlc_package : _nameless.dlc_balt {
	display_name: \"DLC - Beyond the Baltic Sea\"
	steam_app_id: 925580
	features[]: map
	features[]: cargo
}
}
";
        assert!(!from_manifest("dlc_krone", Some(KRONE), 1).map);
        assert!(from_manifest("dlc_balt", Some(BALTIC), 35).map);
    }

    #[test]
    fn reads_a_dlc_from_its_manifest() {
        let east = from_manifest("dlc_east", Some(EAST), 16);
        assert_eq!(east.name, "Going East!");
        assert_eq!(east.steam_app.as_deref(), Some("227310"));
        assert!(east.map);
        let flags = from_manifest("dlc_flags_tr", Some(FLAGS), 0);
        assert_eq!(flags.name, "Turkish Paint Jobs");
        assert!(!flags.map);
        // No manifest: its archive's name.
        assert_eq!(from_manifest("dlc_odd", None, 0).name, "dlc_odd");
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
        let east = from_manifest("dlc_east", Some(EAST), 16);
        assert_eq!(count(&[metallics("dlc_metallics"), metallics("dlc_metallics2"), east]), 2);
    }
}
