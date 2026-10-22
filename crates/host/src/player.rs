//! What WestPlay takes from the player's game files, watched while it runs: from the newest save of the
//! profile the game has selected (or the save it last wrote: `westplay_mapdata::save`), the garages they own; from that save's profile, the mods they play
//! with (a map WestPlay doesn't allow) and its time zones option (the profile's `config.cfg`); from the
//! game's config, its language. The game saves often (autosaves as the player drives), so a garage bought
//! shows as theirs on the map soon after. Read off the host's own thread (decrypting and reading a save
//! takes a moment), and only when a newer save appears or the profile's options are saved.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, SystemTime};

use westplay_mapdata::locale;
use westplay_mapdata::save::{self, Garages};
use westplay_setup::profiles::{self, find_mod, is_hvt_map};
use westplay_setup::{game, places};

/// How often to look for a newer save.
const CHECK_EVERY: Duration = Duration::from_secs(10);

/// What each newer save says.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Player {
    pub garages: Garages,
    /// The player's profile plays on a map by HVT (Heavy Vehicles Turkey), which WestPlay doesn't allow.
    pub hvt_map: bool,
    /// The game's language (`tr_tr`…), from its config.
    pub language: Option<String>,
    /// The profile has the game's time zones on (Options › Gameplay › Time zones: "Only time" or "Full
    /// info"): the game's clock shows the time of the country the truck is in.
    pub time_zones: bool,
}

pub struct PlayerWatch {
    pub updates: Receiver<Player>,
}

impl PlayerWatch {
    pub fn start() -> Self {
        let (sender, updates) = mpsc::channel();
        let _ = std::thread::Builder::new().name("westplay-player".into()).spawn(move || {
            let documents = places::game_documents();
            // For the mods a profile plays: its Workshop folder and version, if the game is found.
            let game = game::find().into_iter().next();
            // Steam Cloud keeps the game's profiles under Steam's `userdata`.
            let userdata = places::steam().map(|dir| dir.join("userdata")).filter(|dir| dir.is_dir());
            let mut last: Option<(PathBuf, SystemTime, Option<SystemTime>)> = None;
            loop {
                // The newest save of the profile the game has selected (its log), else the newest of all.
                let latest = documents.as_deref().and_then(|dir| {
                    let selected = profiles::selected(dir);
                    let of_selected = selected.as_deref().and_then(|name| {
                        save::latest_of(dir, userdata.as_deref(), |folder| {
                            profiles::name_from_folder(folder).as_deref() == Some(name)
                        })
                    });
                    of_selected.or_else(|| save::latest(dir, userdata.as_deref()))
                });
                let stamped = latest.and_then(|path| Some((std::fs::metadata(&path).ok()?.modified().ok()?, path)));
                // The profile's options (its time zones) are saved apart from its saves.
                let options = stamped.as_ref().and_then(|(_, path)| profile_config(path));
                let options_written = options.as_deref().and_then(|file| std::fs::metadata(file).ok()?.modified().ok());
                if let Some((modified, path)) = stamped
                    && last.as_ref() != Some(&(path.clone(), modified, options_written))
                {
                    match read_units(&path) {
                        Ok(units) => {
                            let mods = save::profile_of(&path)
                                .and_then(|profile| read_units(&profile).ok())
                                .map(|units| save::active_mods(&units))
                                .unwrap_or_default();
                            // HVT's maps by their name, or (any name) by what's in them and who made them.
                            let mod_dir = documents.as_deref().map(|dir| dir.join("mod")).unwrap_or_default();
                            let hvt_map = mods.iter().any(|listed| {
                                is_hvt_map(listed) || find_mod(listed, game.as_ref(), &mod_dir).is_hvt_map()
                            });
                            let player = Player {
                                garages: save::garages(&units),
                                hvt_map,
                                language: documents.as_deref().and_then(game_language),
                                time_zones: options
                                    .as_deref()
                                    .and_then(|file| std::fs::read_to_string(file).ok())
                                    .is_some_and(|config| time_zones(&config)),
                            };
                            if sender.send(player).is_err() {
                                return;
                            }
                        }
                        Err(error) => log!("save {}: {error}", path.display()),
                    }
                    last = Some((path, modified, options_written));
                }
                std::thread::sleep(CHECK_EVERY);
            }
        });
        Self { updates }
    }
}

fn read_units(path: &Path) -> Result<Vec<westplay_mapdata::sii::Unit>, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    save::read(&data).map_err(|e| e.to_string())
}

/// The game's language, from its config (`uset g_lang_init "tr_tr"`).
fn game_language(game_documents: &Path) -> Option<String> {
    locale::game_language(&std::fs::read_to_string(game_documents.join("config.cfg")).ok()?)
}

/// The options file of a save's profile (`profiles/<profile>/config.cfg`).
fn profile_config(save: &Path) -> Option<PathBuf> {
    Some(save::profile_of(save)?.parent()?.join("config.cfg"))
}

/// Whether a profile's options (its `config.cfg`) have the game's time zones on: `uset g_time_zones "1"`
/// ("Only time") or `"2"` ("Full info"); `"0"`, or none written, is off.
fn time_zones(config: &str) -> bool {
    config.lines().any(|line| {
        let mut words = line.split_whitespace();
        words.next() == Some("uset")
            && words.next() == Some("g_time_zones")
            && words.next().is_some_and(|value| !matches!(value.trim_matches('"'), "" | "0"))
    })
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    #[test]
    fn reads_whether_the_profile_has_time_zones_on() {
        assert!(!super::time_zones("uset g_hud_speed_limit \"1\"\nuset g_time_zones \"0\"\n"));
        assert!(super::time_zones("uset g_time_zones \"1\"\r\n"));
        assert!(super::time_zones("uset g_heavy_cargo_popup \"1\"\nuset g_time_zones \"2\"\n"));
        assert!(!super::time_zones("uset g_lang_init \"tr_tr\"\n"));
    }

    #[test]
    fn finds_the_options_of_a_saves_profile() {
        assert_eq!(
            super::profile_config(Path::new("profiles/57/save/autosave/game.sii")),
            Some(PathBuf::from("profiles/57/config.cfg"))
        );
    }
}
