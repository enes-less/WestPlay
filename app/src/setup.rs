//! The companion's setup, for its onboarding: finding the game (or the player showing where it is), checking
//! it (its version, DLCs, the profiles' map mods, the truck screens WestPlay can draw on), and installing
//! WestPlay into it (the plugin, and the game mod built from the player's own game). The map is then built
//! by the host (`westplay_host::maps`), for the game set up here. What setup did is kept in `setup.json`
//! (`westplay_setup::record`): the game, and whether the onboarding was finished.
//!
//! Errors the onboarding knows come back as keys it puts in the player's language (`notTheGame`,
//! `gameGone`, `noPlugin`, `noDocuments`); others (the system's own) as they are.
//!
//! Nothing here is kept or sent anywhere: profile and mod names are shown to the player, never written down
//! (and a Road to Heavy mod's never even shown: its licence file's name holds the player's key).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Manager, State, WebviewWindow};
use westplay_host::maps::SetUpGame;
use westplay_mapdata::files::GameFiles;
use westplay_setup::game::{self, FoundBy, Game};
use westplay_setup::plugin::{self, InstallError};
use westplay_setup::record::Record;
use westplay_setup::trucks::{self, Fit};
use westplay_setup::{dlc, game_mod, pick, places, profiles};

/// What setup did, and the game it set up, shared with the host (for the map).
pub struct SetupStore(Arc<Inner>);

struct Inner {
    /// WestPlay's config folder, where the record is kept.
    dir: PathBuf,
    record: Mutex<Record>,
    game: Arc<SetUpGame>,
    /// WestPlay's plugin, to install (`None`: this build of WestPlay has none).
    plugin: Option<PathBuf>,
}

impl SetupStore {
    /// The record kept in `dir`, and the game it names for the host.
    pub fn load(dir: &Path, app: &AppHandle) -> Self {
        let record = Record::load(dir);
        let game = Arc::new(SetUpGame::new(record.game.clone()));
        Self(Arc::new(Inner { dir: dir.to_path_buf(), record: Mutex::new(record), game, plugin: plugin_file(app) }))
    }

    /// The game set up, for the host.
    pub fn game(&self) -> Arc<SetUpGame> {
        self.0.game.clone()
    }
}

impl Inner {
    fn record(&self) -> Record {
        self.record.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn update(&self, change: impl FnOnce(&mut Record)) -> Result<(), String> {
        let mut record = self.record.lock().unwrap_or_else(|e| e.into_inner());
        change(&mut record);
        record.save(&self.dir).map_err(|e| format!("Couldn't save WestPlay's setup: {e}"))?;
        self.game.set(record.game.clone());
        Ok(())
    }
}

/// WestPlay's plugin: shipped beside the app (its resources, or the app's own folder in an unbundled release
/// build); while developing, the release build of it (the one the dev loop builds and installs).
fn plugin_file(app: &AppHandle) -> Option<PathBuf> {
    let candidates: Vec<PathBuf> = if cfg!(debug_assertions) {
        vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/release").join(plugin::FILE_NAME)]
    } else {
        let resources = app.path().resource_dir().ok().map(|dir| dir.join(plugin::FILE_NAME));
        let beside = std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.join(plugin::FILE_NAME)));
        resources.into_iter().chain(beside).collect()
    };
    candidates.into_iter().find(|path| path.is_file())
}

/// An install of the game, as the companion shows it.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameInfo {
    dir: PathBuf,
    /// `steam`, `installed` (another store's, or SCS's own installer) or `chosen`.
    found_by: &'static str,
    version: Option<String>,
    /// WestPlay has been tried with this version.
    tested: bool,
    dlcs: usize,
}

impl From<&Game> for GameInfo {
    fn from(game: &Game) -> Self {
        Self {
            dir: game.dir.clone(),
            found_by: match game.found_by {
                FoundBy::Steam => "steam",
                FoundBy::Installed => "installed",
                FoundBy::Chosen => "chosen",
            },
            version: game.version.map(|version| version.to_string()),
            tested: game.tested(),
            dlcs: game.dlcs.len(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupState {
    /// The onboarding was gone through: the companion opens on its own pages.
    finished: bool,
    /// The game WestPlay is set up in, if it is (and it's still there).
    game: Option<GameInfo>,
    /// WestPlay was updated since it put its plugin and mod into that game: the companion puts the new ones
    /// in (`install_into_game`) once the game is closed. Never in a development build (its plugin is the
    /// development loop's to install).
    updated: bool,
    /// Every install of the game found on the PC.
    found: Vec<GameInfo>,
    /// The game is running (setup installs into it and builds its map while it's closed).
    game_running: bool,
    /// The onboarding's language, a game language code (`tr_tr`), if it's known: see `onboarding_language`.
    /// Otherwise it follows Windows'.
    language: Option<String>,
}

#[tauri::command]
pub async fn setup_state(store: State<'_, SetupStore>) -> Result<SetupState, String> {
    let inner = store.0.clone();
    blocking(move || {
        let record = inner.record();
        let found = game::find();
        let game = record.game.as_deref().and_then(Game::recognise);
        Ok(SetupState {
            finished: record.finished,
            updated: game.is_some() && record.finished && is_update(record.version.as_deref()),
            game: game.as_ref().map(GameInfo::from),
            found: found.iter().map(GameInfo::from).collect(),
            game_running: game::running(),
            language: onboarding_language(),
        })
    })
    .await
}

/// The onboarding speaks the game's language, as WestPlay does in the cab (its words, and the map's place
/// names): the one the game was last set to (its `config.cfg`), or one set for trying it out
/// (`WESTPLAY_LANGUAGE`). A player who never started the game has neither: the onboarding then follows
/// Windows'.
fn onboarding_language() -> Option<String> {
    let set = std::env::var("WESTPLAY_LANGUAGE").ok().filter(|code| !code.is_empty());
    set.or_else(|| {
        let config = std::fs::read_to_string(places::game_documents()?.join("config.cfg")).ok()?;
        westplay_mapdata::locale::game_language(&config)
    })
}

#[tauri::command]
pub fn game_running() -> bool {
    game::running()
}

/// Asks the player where the game is (Windows' folder picker, titled `title`, in their language): the game
/// there, `None` if they cancelled.
#[tauri::command]
pub async fn choose_game_folder(window: WebviewWindow, title: String) -> Result<Option<GameInfo>, String> {
    let owner = window.hwnd().map(|hwnd| hwnd.0 as isize).unwrap_or(0);
    blocking(move || {
        let Some(dir) = pick::folder(owner, &title) else { return Ok(None) };
        Game::recognise(&dir).map(|game| Some(GameInfo::from(&game))).ok_or_else(|| "notTheGame".to_owned())
    })
    .await
}

/// What a check of the game found.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameCheck {
    game: GameInfo,
    /// How many DLCs the game has (`westplay_setup::dlc`: one per Steam app, paint jobs and flags packs
    /// included).
    dlcs: usize,
    /// Those that change the map (map expansions, and trailer packs with their companies), by name.
    map_dlcs: Vec<String>,
    /// The player's profiles, the most recently saved first.
    profiles: Vec<ProfileCheck>,
    /// The trucks WestPlay is made for (`westplay_setup::trucks`), and whether the player can have it in
    /// each: `ready`, `notInGame`, or `notYet` (in the game, but WestPlay doesn't draw on its screen yet).
    trucks: Vec<TruckCheck>,
    /// Why WestPlay's mod can't be built from this game, if it can't (a game version it doesn't know yet).
    mod_problem: Option<String>,
    /// WestPlay's plugin in the game: `missing`, `current`, or `different` (another version of it).
    plugin: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TruckCheck {
    /// Its id (`scania-smart-dash`), for the names with words in them, in the player's language.
    id: &'static str,
    name: &'static str,
    fit: &'static str,
    /// A mod's truck (missing: no profile plays with it), not the game's.
    from_mod: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileCheck {
    name: String,
    /// The map it plays: `europe`, or a standalone map mod's.
    map: String,
    /// The mods that change its map, as the game names them.
    map_mods: Vec<String>,
    /// Map mods WestPlay couldn't read (a format it doesn't know): their roads are left out of its map.
    unreadable: usize,
    /// It plays HVT's map, which WestPlay doesn't allow (no map for it; nothing of it named).
    hvt_map: bool,
    /// WestPlay's mod is in its mod list (it's enabled in the game's Mod Manager).
    westplay_mod: bool,
}

#[tauri::command]
pub async fn check_game(dir: PathBuf, store: State<'_, SetupStore>) -> Result<GameCheck, String> {
    let ours = store.0.plugin.clone();
    blocking(move || {
        let game = Game::recognise(&dir).ok_or("gameGone")?;
        let profiles = match places::game_documents() {
            Some(documents) => {
                let userdata = places::steam().map(|steam| steam.join("userdata"));
                profiles::find(&documents, userdata.as_deref(), Some(&game))
            }
            None => Vec::new(),
        };
        let all_profiles = profiles;
        let profiles = all_profiles
            .iter()
            .map(|profile| {
                let hvt_map = profile.plays_hvt_map();
                let map_mods: Vec<_> = profile.map_mods().filter(|m| !hvt_map && !m.is_hvt_map()).collect();
                ProfileCheck {
                    name: profile.name.clone(),
                    map: profile.map.clone(),
                    unreadable: map_mods.iter().filter(|m| m.problem.is_some()).count(),
                    map_mods: map_mods.iter().map(|m| m.name.clone()).collect(),
                    hvt_map,
                    westplay_mod: has_westplay_mod(profile),
                }
            })
            .collect();
        let files = GameFiles::install(&game.dir).map_err(|e| e.to_string())?;
        let (patched, mod_problem) = match game_mod::build(&files) {
            Ok((_, built)) => (built.patched, None),
            Err(problem) => (Vec::new(), Some(problem.to_string())),
        };
        let trucks = trucks::fit(&files, &patched, &all_profiles)
            .into_iter()
            .map(|(truck, fit)| TruckCheck {
                id: truck.id,
                name: truck.name,
                from_mod: matches!(truck.source, trucks::Source::Mod(_)),
                fit: match fit {
                    Fit::Ready => "ready",
                    Fit::NotInGame => "notInGame",
                    Fit::NotYet => "notYet",
                },
            })
            .collect();
        let dlcs = dlc::read(&game);
        let plugin = match ours.as_deref().map(|ours| plugin::state(&game, ours)) {
            Some(Ok(plugin::State::Current)) => "current",
            Some(Ok(plugin::State::Different)) => "different",
            _ if game.plugins_dir().join(plugin::FILE_NAME).is_file() => "different",
            _ => "missing",
        };
        Ok(GameCheck {
            dlcs: dlc::count(&dlcs),
            map_dlcs: dlcs.iter().filter(|dlc| dlc.map).map(|dlc| dlc.name.clone()).collect(),
            game: GameInfo::from(&game),
            profiles,
            trucks,
            mod_problem,
            plugin,
        })
    })
    .await
}

/// Whether WestPlay's mod is in a profile's mod list (`westplay|WestPlay screens`).
fn has_westplay_mod(profile: &profiles::Profile) -> bool {
    let ours = game_mod::FILE_NAME.trim_end_matches(".scs");
    profile.mods.iter().any(|m| m.package.eq_ignore_ascii_case(ours))
}

fn build_mod(game: &Game) -> Result<(Vec<u8>, game_mod::Built), String> {
    let files = GameFiles::install(&game.dir).map_err(|e| e.to_string())?;
    game_mod::build(&files).map_err(|e| e.to_string())
}

/// How installing the plugin went.
#[derive(Debug, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum PluginOutcome {
    Installed,
    /// The game is running: close it first.
    InUse,
    /// The game's folder needs administrator rights: ask again with `administrator`, and Windows asks the
    /// player.
    NeedsAdministrator,
    /// The player said no to Windows.
    Declined,
    Failed {
        error: String,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallReport {
    plugin: PluginOutcome,
    /// Why the mod couldn't be built or written, if it couldn't.
    mod_error: Option<String>,
    /// Everything's in: WestPlay is set up in this game (its map is built next).
    done: bool,
}

/// Installs WestPlay into the game in `dir`: its mod, built from this game, into the game's `mod` folder in
/// Documents, and its plugin into the game's plugins folder (with `administrator`, through Windows' prompt,
/// for that one copy). Once both are in, WestPlay is set up in this game.
#[tauri::command]
pub async fn install_into_game(
    dir: PathBuf,
    administrator: bool,
    store: State<'_, SetupStore>,
) -> Result<InstallReport, String> {
    let inner = store.0.clone();
    blocking(move || {
        let game = Game::recognise(&dir).ok_or("gameGone")?;
        let mod_error = install_mod(&game).err();
        let plugin = match inner.plugin.as_deref() {
            None => PluginOutcome::Failed { error: "noPlugin".into() },
            Some(ours) => install_plugin(&game, ours, administrator),
        };
        let done = mod_error.is_none() && matches!(plugin, PluginOutcome::Installed);
        if done {
            inner.update(|record| {
                record.game = Some(game.dir.clone());
                record.version = Some(VERSION.into());
            })?;
        }
        Ok(InstallReport { plugin, mod_error, done })
    })
    .await
}

fn install_mod(game: &Game) -> Result<(), String> {
    let (bytes, _) = build_mod(game)?;
    let mod_dir = places::game_documents().ok_or("noDocuments")?.join("mod");
    game_mod::install(&bytes, &mod_dir).map(|_| ()).map_err(|e| format!("Couldn't write the mod: {e}"))
}

fn install_plugin(game: &Game, ours: &Path, administrator: bool) -> PluginOutcome {
    if matches!(plugin::state(game, ours), Ok(plugin::State::Current)) {
        return PluginOutcome::Installed;
    }
    let installed = if administrator {
        plugin::install_as_administrator(game, ours)
    } else {
        plugin::install(game, ours).map(|_| ())
    };
    match installed {
        Ok(()) => PluginOutcome::Installed,
        Err(InstallError::InUse) => PluginOutcome::InUse,
        Err(InstallError::NeedsAdministrator) => PluginOutcome::NeedsAdministrator,
        Err(InstallError::Declined) => PluginOutcome::Declined,
        Err(InstallError::Io(error)) => PluginOutcome::Failed { error: error.to_string() },
    }
}

/// This WestPlay's version.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Whether this WestPlay is another version than the one that put its plugin and mod into the game
/// (`installed_by`; none: one from before versions were kept). Never in a development build.
fn is_update(installed_by: Option<&str>) -> bool {
    !cfg!(debug_assertions) && installed_by != Some(VERSION)
}

/// The onboarding was gone through to its end.
#[tauri::command]
pub fn finish_setup(store: State<SetupStore>) -> Result<(), String> {
    store.0.update(|record| record.finished = true)
}

/// Runs `work` off the async runtime's threads (it reads files, the registry, the game's archives).
async fn blocking<T: Send + 'static>(work: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work).await.map_err(|e| e.to_string())?
}

/// What taking WestPlay out of the game did. Mirrored by the companion's `UninstallReport`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallReport {
    plugin: PluginOutcome,
    mod_error: Option<String>,
    /// Profiles WestPlay was taken out of, and those it couldn't be (left as they were).
    profiles_changed: usize,
    profiles_failed: usize,
    /// WestPlay is out of the game (its plugin and mod gone): its own data can go next ([`leave_pc`]).
    done: bool,
}

/// Takes WestPlay out of the game it's set up in, the game closed: out of each profile's active mods (each
/// checked, written whole), its screens mod, then its plugin (with `administrator`, through Windows' prompt,
/// for that one file). Its own data stays until [`leave_pc`].
#[tauri::command]
pub async fn uninstall_from_game(administrator: bool, store: State<'_, SetupStore>) -> Result<UninstallReport, String> {
    let inner = store.0.clone();
    blocking(move || {
        if game::running() {
            return Err("gameRunning".into());
        }
        let documents = places::game_documents().ok_or("noDocuments")?;
        let userdata = places::steam().map(|steam| steam.join("userdata"));
        let profiles = westplay_setup::uninstall::remove_from_profiles(&documents, userdata.as_deref());
        let mod_error = westplay_setup::uninstall::remove_mod(&documents).err().map(|e| e.to_string());
        let plugin = match inner.record().game.as_deref().and_then(Game::recognise) {
            // No game set up (or it's gone): no plugin of WestPlay's anywhere to take out.
            None => PluginOutcome::Installed,
            Some(game) => {
                let removed =
                    if administrator { plugin::uninstall_as_administrator(&game) } else { plugin::uninstall(&game) };
                match removed {
                    Ok(()) => PluginOutcome::Installed,
                    Err(InstallError::InUse) => PluginOutcome::InUse,
                    Err(InstallError::NeedsAdministrator) => PluginOutcome::NeedsAdministrator,
                    Err(InstallError::Declined) => PluginOutcome::Declined,
                    Err(InstallError::Io(error)) => PluginOutcome::Failed { error: error.to_string() },
                }
            }
        };
        let done = mod_error.is_none() && matches!(plugin, PluginOutcome::Installed);
        Ok(UninstallReport {
            plugin,
            mod_error,
            profiles_changed: profiles.changed,
            profiles_failed: profiles.failed.len(),
            done,
        })
    })
    .await
}

/// WestPlay leaves the PC, once it's out of the game: its logs (Documents\WestPlay) and its setup go now; its
/// settings and maps too, unless `keep`; the rest of its data (the screens' browsers, with every sign-in,
/// which are in use until it ends) as soon as it has ended, by a step that waits for it; then, if it was
/// installed, its uninstaller, which takes the app itself away. Then it quits.
#[tauri::command]
pub fn leave_pc(keep: bool, app: AppHandle, store: State<SetupStore>) -> Result<(), String> {
    let config = store.0.dir.clone();
    let data = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
    if let Some(documents) = places::documents() {
        let _ = std::fs::remove_dir_all(documents.join("WestPlay"));
    }
    if keep {
        // A reinstall starts over (the onboarding) with the player's settings.
        let _ = std::fs::remove_file(config.join(westplay_setup::record::FILE_NAME));
    } else {
        let _ = std::fs::remove_dir_all(&config);
    }
    let mut folders: Vec<PathBuf> = if keep {
        std::fs::read_dir(&data)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            // The maps, and the player's own wallpapers and icon sets (their settings name them), stay.
            .filter(|path| {
                path.file_name().is_none_or(|name| name != "maps" && name != westplay_host::customize::FOLDER)
            })
            .collect()
    } else {
        vec![data.clone()]
    };
    folders.retain(|path| path.exists());
    let uninstaller = std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.join("uninstall.exe")));
    after_exit(&folders, uninstaller.filter(|path| path.is_file()).as_deref())?;
    app.exit(0);
    Ok(())
}

/// Runs a hidden step that waits a few seconds (WestPlay has ended by then), removes `paths` (folders or
/// files), and starts `uninstaller` quietly, if there is one.
fn after_exit(paths: &[PathBuf], uninstaller: Option<&Path>) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    let mut line = String::from("/d /c ping -n 4 127.0.0.1 >nul");
    for path in paths {
        let command = if path.is_dir() { "rmdir /s /q" } else { "del /f /q" };
        line.push_str(&format!(" & {command} \"{}\"", path.display()));
    }
    if let Some(uninstaller) = uninstaller {
        line.push_str(&format!(" & start \"\" \"{}\" /S", uninstaller.display()));
    }
    std::process::Command::new("cmd.exe")
        .raw_arg(line)
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Couldn't remove WestPlay's data: {e}"))
}
