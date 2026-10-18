//! What WestPlay takes from the player's game files, watched while it runs: from the save the game last
//! wrote (`westplay_mapdata::save`), the garages they own; from that save's profile, the mods they play
//! with (a map WestPlay doesn't allow); from the game's config, its language. The game saves often
//! (autosaves as the player drives), so a garage bought shows as theirs on the map soon after. Read off
//! the host's own thread (decrypting and reading a save takes a moment), and only when a newer save appears.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, SystemTime};

use westplay_mapdata::locale;
use westplay_mapdata::save::{self, Garages};
use westplay_setup::profiles::{find_mod, is_hvt_map};
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
            let mut last: Option<(PathBuf, SystemTime)> = None;
            loop {
                let latest = documents.as_deref().and_then(|dir| save::latest(dir, userdata.as_deref()));
                let stamped = latest.and_then(|path| Some((std::fs::metadata(&path).ok()?.modified().ok()?, path)));
                if let Some((modified, path)) = stamped
                    && last.as_ref() != Some(&(path.clone(), modified))
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
                            };
                            if sender.send(player).is_err() {
                                return;
                            }
                        }
                        Err(error) => log!("save {}: {error}", path.display()),
                    }
                    last = Some((path, modified));
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
