//! The cab's mark (`westplay_shared::CAB_MARK_WIDTH`), in the dashboard scripts of every truck the tablet goes in.
//!
//! The camera system takes the cab being drawn for the camera in it: the game draws a truck's screen scripts every
//! frame the camera is in the cab, wherever the player looks, and never from outside. A truck whose screen is
//! WestPlay's tells it by that screen; WestPlay's tablet can't, being drawn with the cab's models, left out as the
//! player looks away from it (owner 2026-10-25: the music muffled as the head turned in the interior; "either you
//! are in the interior, or you are not"). Every interior has a dashboard script (its cluster's display,
//! `dashboard_path`), drawn as the screens' are: the mark, a fully transparent image a unit square in its corner,
//! goes in each, and the plugin sees it drawn as it sees the screens.
//!
//! A script is the game's own or a mod's, and one path may be in several installed mods (a dashboard mod over the
//! game's, two trucks' mods bringing one script): which the game uses depends on the profile played. So a script
//! is marked only where every copy installed is the same, and the game, any mod, gets it back with the mark;
//! otherwise it's left as it is, and that truck's cab is told by its screen alone, as before.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use westplay_mapdata::archive::Archive;
use westplay_mapdata::files::{GameFiles, Mount};
use westplay_mapdata::sii;

use crate::tablet::{self, Spot};

/// The mark's texture's name, beside the screens' (`game_mod::SCREEN_DIR`).
pub(crate) const TEXTURE: &str = "cab_mark";
/// The dashboard scripts the mod has the mark in, a path a line: what [`current`] compares.
pub(crate) const LIST: &str = "material/ui/westplay/cab_marks.txt";

/// The dashboard scripts to have the mark in, and those left as they are.
#[derive(Debug, Default, PartialEq)]
pub struct Marks {
    /// Each script's path and its text with the mark.
    pub scripts: Vec<(String, Vec<u8>)>,
    /// A script left as it is: its path and why.
    pub left: Vec<(String, String)>,
}

impl Marks {
    /// The list kept in the mod ([`LIST`]).
    pub(crate) fn list(&self) -> String {
        self.scripts.iter().map(|(path, _)| format!("{path}\n")).collect()
    }
}

/// The marks for the trucks the tablet goes in: the game's (`game`, the installed game mounted) and `mod_trucks`
/// (the installed mods': `tablet::mod_trucks`), their dashboard scripts read from the game and `mods` (the installed
/// mods' files, archives or folders, active in a profile or not).
pub fn for_tablet(game: &GameFiles, mods: &[PathBuf], mod_trucks: &[Spot]) -> Marks {
    let own = tablet::trucks(game).unwrap_or_default();
    let trucks: BTreeSet<&str> = own.iter().chain(mod_trucks).map(|spot| spot.truck.as_str()).collect();
    let mods: Vec<GameFiles> = mods.iter().filter_map(|path| mounted(path)).collect();
    marks(game, &mods, &trucks)
}

/// A mod's files, mounted alone; `None` if it can't be opened.
fn mounted(path: &Path) -> Option<GameFiles> {
    let mount = if path.is_dir() { Mount::Folder(path.to_owned()) } else { Mount::Archive(Archive::open(path).ok()?) };
    Some(GameFiles::new(vec![mount]))
}

/// The marks for `trucks`' interiors' dashboard scripts, from `game` and `mods` (each mounted alone).
fn marks(game: &GameFiles, mods: &[GameFiles], trucks: &BTreeSet<&str>) -> Marks {
    let sources = || std::iter::once(game).chain(mods);
    let paths: BTreeSet<String> = sources().flat_map(|files| dashboards(files, trucks)).collect();
    let mut found = Marks::default();
    for path in paths {
        let copies: Vec<Vec<u8>> = sources().filter_map(|files| files.read(&path).ok().flatten()).collect();
        let Some(first) = copies.first() else {
            found.left.push((path, "not in the game or an installed mod".into()));
            continue;
        };
        if copies.iter().any(|copy| copy != first) {
            found.left.push((path, format!("{} copies installed, not all the same", copies.len())));
            continue;
        }
        let marked = String::from_utf8(first.clone())
            .map_err(|_| "not UTF-8 text".to_owned())
            .and_then(|text| crate::game_mod::patch_dashboard_script(&text, TEXTURE));
        match marked {
            Ok(text) => found.scripts.push((path, text.into_bytes())),
            Err(why) => found.left.push((path, why)),
        }
    }
    found
}

/// The dashboard scripts (game paths, no leading slash) `trucks`' interiors in `files` name, their includes
/// followed. Definitions that aren't text (encrypted) name none.
fn dashboards(files: &GameFiles, trucks: &BTreeSet<&str>) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    for truck in trucks {
        let dir = format!("def/vehicle/truck/{truck}/interior");
        for name in files.list(&dir).unwrap_or_default().iter().filter(|name| name.ends_with(".sii")) {
            for unit in sii::read_units(files, &format!("{dir}/{name}")).unwrap_or_default() {
                if let Some(script) = unit.get("dashboard_path") {
                    paths.insert(script.trim_start_matches('/').to_owned());
                }
            }
        }
    }
    paths
}

/// Whether the mod built at `built` (WestPlay's `.scs`) has the marks as `marks`: the same scripts, the same.
pub fn current(marks: &Marks, built: &Path) -> bool {
    let Ok(archive) = Archive::open(built) else { return false };
    let ours = GameFiles::new(vec![Mount::Archive(archive)]);
    let read = |path: &str| ours.read(path).ok().flatten();
    read(LIST).as_deref() == Some(marks.list().as_bytes())
        && marks.scripts.iter().all(|(path, bytes)| read(path).as_ref() == Some(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of game files, written for a test, mounted.
    struct Files(PathBuf);

    impl Files {
        fn new(name: &str, files: &[(&str, &str)]) -> Self {
            let root = std::env::temp_dir().join(format!("westplay-cab-mark-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            for (path, text) in files {
                let path = root.join(path);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, text).unwrap();
            }
            Self(root)
        }

        fn mounted(&self) -> GameFiles {
            GameFiles::new(vec![Mount::Folder(self.0.clone())])
        }
    }

    impl Drop for Files {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A dashboard script as the game's are laid out (`scania_rcab_2009.sii`, shortened).
    const DASHBOARD: &str = "SiiNunit\n{\nui::window : _nameless.root {\n window_handler: null\n my_children: 1\n my_children[0]: _nameless._.bckg\n coords_l: 0\n coords_r: 1152\n coords_t: 864\n coords_b: 0\n id: 0\n layer: 0\n my_parent: null\n}\n\nui::text : _nameless._.bckg {\n text: \"<img src=/material/ui/white.mat>\"\n coords_l: 0\n coords_r: 1152\n coords_t: 864\n coords_b: 0\n layer: -1\n my_parent: _nameless.root\n}\n\n}\n";

    /// An interior's definition naming its dashboard in an include, as the game's do.
    const INTERIOR: &str = "SiiNunit\n{\naccessory_interior_data: day.x.interior\n{\n@include \"dash.sui\"\n}\n}\n";

    #[test]
    fn marks_each_dashboard_the_trucks_interiors_name_through_their_includes() {
        let game = Files::new(
            "game",
            &[
                ("def/vehicle/truck/scania.r/interior/day.sii", INTERIOR),
                ("def/vehicle/truck/scania.r/interior/dash.sui", "\tdashboard_path: \"/ui/dashboard/scania.sii\"\n"),
                ("ui/dashboard/scania.sii", DASHBOARD),
                // Another truck's, not one the tablet goes in.
                ("def/vehicle/truck/bus/interior/day.sii", INTERIOR),
                ("def/vehicle/truck/bus/interior/dash.sui", "dashboard_path: \"/ui/dashboard/bus.sii\"\n"),
                ("ui/dashboard/bus.sii", DASHBOARD),
            ],
        );
        let found = marks(&game.mounted(), &[], &BTreeSet::from(["scania.r"]));
        assert_eq!(found.left, []);
        let [(path, text)] = &found.scripts[..] else { panic!("{found:?}") };
        assert_eq!(path, "ui/dashboard/scania.sii");
        let text = String::from_utf8(text.clone()).unwrap();
        assert!(
            text.contains(
                "my_children: 2\n my_children[0]: _nameless._.bckg\n my_children[1]: _nameless.westplay.cab\n"
            )
        );
        assert!(text.contains("<img src=/material/ui/westplay/cab_mark.mat xscale=stretch yscale=stretch>"));
    }

    #[test]
    fn the_mod_built_has_the_mark_and_says_whether_its_marks_are_current() {
        let game = Files::new(
            "built",
            &[
                ("def/vehicle/truck/scania.r/interior/day.sii", INTERIOR),
                ("def/vehicle/truck/scania.r/interior/dash.sui", "dashboard_path: \"/ui/dashboard/scania.sii\"\n"),
                ("ui/dashboard/scania.sii", DASHBOARD),
            ],
        );
        let found = marks(&game.mounted(), &[], &BTreeSet::from(["scania.r"]));
        let (bytes, built) = crate::game_mod::build(&game.mounted(), &[], &GameFiles::new(Vec::new()), &found).unwrap();
        assert_eq!(built.cab_marks, 1);
        let at = game.0.join("westplay.scs");
        std::fs::write(&at, bytes).unwrap();
        let archive = Archive::open(&at).unwrap();
        let read = |path: &str| archive.read(path).unwrap();
        assert_eq!(read(LIST), Some(b"ui/dashboard/scania.sii\n".to_vec()));
        assert_eq!(read("ui/dashboard/scania.sii"), Some(found.scripts[0].1.clone()));
        // The mark's texture: fully transparent, at its size.
        let dds = read("material/ui/westplay/cab_mark.dds").unwrap();
        assert_eq!(dds.len(), 148 + westplay_shared::CAB_MARK_WIDTH as usize * 4);
        assert!(dds[148..].iter().all(|&byte| byte == 0));
        drop(archive);
        assert!(current(&found, &at));
        // A script changed (a mod updated), or one more or fewer: not current.
        let mut changed = Marks { scripts: found.scripts.clone(), left: Vec::new() };
        changed.scripts[0].1.push(b'\n');
        assert!(!current(&changed, &at));
        assert!(!current(&Marks::default(), &at));
    }

    #[test]
    fn leaves_a_script_whose_installed_copies_differ() {
        let game = Files::new("game-differ", &[("ui/dashboard/scania.sii", DASHBOARD)]);
        let truck = Files::new(
            "truck",
            &[
                ("def/vehicle/truck/pgr/interior/day.sii", INTERIOR),
                ("def/vehicle/truck/pgr/interior/dash.sui", "dashboard_path: \"/ui/dashboard/scania.sii\"\n"),
            ],
        );
        let trucks = BTreeSet::from(["pgr"]);
        // The truck's mod naming the game's script: marked.
        let found = marks(&game.mounted(), &[truck.mounted()], &trucks);
        assert_eq!(found.scripts.len(), 1);
        // A dashboard mod changing it: left as it is.
        let other = DASHBOARD.replace("white.mat", "black.mat");
        let dashboard_mod = Files::new("dashboard", &[("ui/dashboard/scania.sii", &other)]);
        let found = marks(&game.mounted(), &[truck.mounted(), dashboard_mod.mounted()], &trucks);
        assert_eq!(found.scripts, []);
        assert_eq!(
            found.left,
            [("ui/dashboard/scania.sii".to_owned(), "2 copies installed, not all the same".to_owned())]
        );
    }
}
