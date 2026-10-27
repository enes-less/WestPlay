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
use std::time::{Duration, SystemTime};

use serde::Serialize;
use tauri::{AppHandle, Manager, State, WebviewWindow};
use westplay_host::log;
use westplay_host::maps::SetUpGame;
use westplay_mapdata::files::GameFiles;
use westplay_setup::game::{self, FoundBy, Game};
use westplay_setup::plugin::{self, InstallError};
use westplay_setup::record::Record;
use westplay_setup::trucks::{self, Fit};
use westplay_setup::{cab_mark, dlc, game_mod, pick, places, profiles, tablet};

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

/// The player asks for every profile's map to be prepared again (the Map page): the host builds each anew in
/// turn, failed ones included.
#[tauri::command]
pub fn prepare_maps_again(store: State<'_, SetupStore>) {
    store.0.game.prepare_maps_again();
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
    /// Other CarPlay apps installed in the game, which take the trucks' screens (`ymnOne`): to be removed.
    other_apps: Vec<&'static str>,
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
    /// Its folder: the host's state of its map names it (`westplay_host::maps::ProfileMap`).
    dir: String,
    /// The map it plays: `europe`, or a standalone map mod's.
    map: String,
    /// The mods that change its map, as the game names them.
    map_mods: Vec<String>,
    /// Map mods WestPlay couldn't read (a format it doesn't know): their roads are left out of its map.
    unreadable: usize,
    /// It plays HVT's map, which WestPlay doesn't allow (no map for it; nothing of it named).
    hvt_map: bool,
}

#[tauri::command]
pub async fn check_game(dir: PathBuf, store: State<'_, SetupStore>) -> Result<GameCheck, String> {
    let ours = store.0.plugin.clone();
    blocking(move || {
        let game = Game::recognise(&dir).ok_or("gameGone")?;
        let (profiles, installed) = match places::game_documents() {
            Some(documents) => {
                let userdata = places::steam().map(|steam| steam.join("userdata"));
                let found = profiles::find(&documents, userdata.as_deref(), Some(&game));
                (found, profiles::installed(&documents, Some(&game)))
            }
            None => (Vec::new(), Vec::new()),
        };
        let all_profiles = profiles;
        let profiles = all_profiles
            .iter()
            .map(|profile| {
                let hvt_map = profile.plays_hvt_map();
                let map_mods: Vec<_> = profile.map_mods().filter(|m| !hvt_map && !m.is_hvt_map()).collect();
                ProfileCheck {
                    name: profile.name.clone(),
                    dir: profile.dir.to_string_lossy().into_owned(),
                    map: profile.map.clone(),
                    unreadable: map_mods.iter().filter(|m| m.problem.is_some()).count(),
                    map_mods: map_mods.iter().map(|m| m.name.clone()).collect(),
                    hvt_map,
                }
            })
            .collect();
        let files = GameFiles::install(&game.dir).map_err(|e| e.to_string())?;
        let screen_mods = game_mod::mod_screen_mods(&installed_mod_files(&game));
        let (patched, mod_problem) = match game_mod::build(&files, &[], &screen_mods, &cab_mark::Marks::default()) {
            Ok((_, built)) => (built.patched, None),
            Err(problem) => (Vec::new(), Some(problem.to_string())),
        };
        let trucks = trucks::fit(&files, &patched, &all_profiles, &installed)
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
            other_apps: plugin::other_apps(&game),
        })
    })
    .await
}

/// The mod built for `game` and its installed mods, and the marks of the vehicles its tablet shows nothing in
/// (`tablet::write_apart`).
fn build_mod(game: &Game) -> Result<(Vec<u8>, game_mod::Built, Vec<u64>), String> {
    let files = GameFiles::install(&game.dir).map_err(|e| e.to_string())?;
    let mods = installed_mod_files(game);
    let (mod_trucks, apart) = tablet::mod_trucks_and_marks(&mods);
    let marks = cab_mark::for_tablet(&files, &mods, &mod_trucks);
    let (bytes, built) =
        game_mod::build(&files, &mod_trucks, &game_mod::mod_screen_mods(&mods), &marks).map_err(|e| e.to_string())?;
    Ok((bytes, built, apart))
}

/// The installed mods' files (active in a profile or not), for the tablet's trucks and the mod vehicles' screens.
fn installed_mod_files(game: &Game) -> Vec<PathBuf> {
    let documents = places::game_documents();
    let installed = documents.map(|documents| profiles::installed_files(&documents, Some(game)));
    installed.unwrap_or_default().into_iter().map(|(files, _)| files).collect()
}

/// How often the installed mods are looked at for trucks the tablet isn't in yet, and mod vehicles' screens.
const MODS_LOOK: Duration = Duration::from_secs(30);

/// What WestPlay's mod was last worked out from: the game (its folder, version and DLCs) and each installed mod's
/// files (their path, size and when they last changed).
type ModsStamp = (PathBuf, Option<String>, usize, Vec<(PathBuf, u64, Option<SystemTime>)>);

/// Keeps WestPlay's mod as the installed mods want it: the tablet in every truck with its slots, the game's and its
/// installed mods' (owner 2026-10-24: "any truck, of any mod"), the screens of the mod vehicles WestPlay draws
/// on (Oyuncuyusbis' coaches: a bus mod installed, updated or taken out), and the cab's mark in those trucks'
/// dashboards (`cab_mark`: a mod's script updated, or another copy of it installed). While the game is closed, once the
/// installed mods change, the mod is made again if it isn't so (the plugin mounts it as the game starts).
pub fn watch_installed_mods(store: &SetupStore) {
    let inner = store.0.clone();
    let watch = move || {
        let mut looked = None;
        loop {
            look_at_mods(&inner, &mut looked);
            std::thread::sleep(MODS_LOOK);
        }
    };
    if let Err(error) = std::thread::Builder::new().name("westplay-mods".into()).spawn(watch) {
        log!("setup: can't watch the installed mods for WestPlay's mod: {error}");
    }
}

/// One look ([`watch_installed_mods`]); `looked` what the last one that needed nothing more was from.
fn look_at_mods(inner: &Inner, looked: &mut Option<ModsStamp>) {
    let record = inner.record();
    let Some(game) = record.game.as_deref().and_then(Game::recognise).filter(|_| record.finished) else { return };
    let Some(ours) = places::westplay_documents().map(|dir| dir.join(game_mod::FILE_NAME)) else { return };
    // Not set up yet (setup makes it), or the game has it open.
    if !ours.is_file() || game::running() {
        return;
    }
    let mods = installed_mod_files(&game);
    let mut stamps: Vec<_> = mods
        .iter()
        .map(|files| {
            let metadata = std::fs::metadata(files).ok();
            let size = metadata.as_ref().map_or(0, |metadata| metadata.len());
            (files.clone(), size, metadata.and_then(|metadata| metadata.modified().ok()))
        })
        .collect();
    stamps.sort();
    let stamp = (game.dir.clone(), game.version.map(|version| version.to_string()), game.dlcs.len(), stamps);
    if looked.as_ref() == Some(&stamp) {
        return;
    }
    let files = match GameFiles::install(&game.dir) {
        Ok(files) => files,
        Err(error) => return log!("setup: the installed mods: the game can't be read: {error}"),
    };
    let (mod_trucks, apart) = tablet::mod_trucks_and_marks(&mods);
    // The vehicles the tablet shows nothing in, as the installed mods have them now (the plugin reads them as the
    // truck changes).
    if let Some(dir) = ours.parent()
        && let Err(error) = tablet::write_apart(&apart, dir)
    {
        log!("setup: the tablet's vehicles to keep it blank in not written: {error}");
    }
    let screen_mods = game_mod::mod_screen_mods(&mods);
    let own = tablet::trucks(&files).unwrap_or_default();
    let wanted: Vec<tablet::Spot> = own
        .into_iter()
        .chain(mod_trucks.iter().cloned())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let present = tablet::trucks_in_mod(&ours);
    let screens_current = game_mod::mod_screens_current(&screen_mods, &ours);
    let marks = cab_mark::for_tablet(&files, &mods, &mod_trucks);
    let marks_current = cab_mark::current(&marks, &ours);
    if wanted != present || !screens_current || !marks_current {
        let made = game_mod::build(&files, &mod_trucks, &screen_mods, &marks).map_err(|error| error.to_string());
        let made = made.and_then(|(bytes, built)| {
            let dir = ours.parent().ok_or("no folder")?;
            game_mod::install(&bytes, dir).map_err(|error| error.to_string())?;
            Ok(built)
        });
        let built = match made {
            Ok(built) => built,
            // Tried again at the next look.
            Err(error) => return log!("setup: WestPlay's mod not made again for the installed mods: {error}"),
        };
        let screens: Vec<&str> =
            built.patched.iter().copied().filter(|layout| game_mod::mod_screen_layout(layout)).collect();
        log!(
            "setup: WestPlay's mod made again: the tablet at {} places (was {}), {} of them the mods' trucks'; the mod \
             vehicles' screens: {screens:?}; the cab's mark in {} dashboards",
            wanted.len(),
            present.len(),
            wanted.iter().filter(|spot| mod_trucks.contains(spot)).count(),
            built.cab_marks
        );
        for (script, why) in &marks.left {
            log!("setup: the cab's mark not in {script}: {why}");
        }
    }
    *looked = Some(stamp);
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

/// Installs WestPlay into the game in `dir`: its mod, built from this game, into WestPlay's own folder (the
/// plugin mounts it into the game: `game_mod`), and its plugin into the game's plugins folder (with
/// `administrator`, through Windows' prompt, for that one copy). Once both are in, WestPlay is set up in this
/// game.
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

/// The mod into WestPlay's own folder; then, from a game WestPlay 1.2 set up, its copy taken out of the game's
/// `mod` folder and out of each profile's active mods (the game closed: the companion installs only then).
/// Two copies would stop the game when a screen starts, and the plugin doesn't mount its own while the old one
/// is there.
fn install_mod(game: &Game) -> Result<(), String> {
    let (bytes, built, apart) = build_mod(game)?;
    if let Some(why) = &built.not_sharp {
        log!("setup: the 7\" Scania screen stays at the game's size: {why}");
    }
    let ours = places::westplay_documents().ok_or("noDocuments")?;
    game_mod::install(&bytes, &ours).map_err(|e| format!("Couldn't write the mod: {e}"))?;
    tablet::write_apart(&apart, &ours).map_err(|e| format!("Couldn't write the mod: {e}"))?;
    if let Some(documents) = places::game_documents() {
        westplay_setup::uninstall::remove_mod(&documents)
            .map_err(|e| format!("Couldn't take the old mod out of the game's mod folder: {e}"))?;
        let userdata = places::steam().map(|steam| steam.join("userdata"));
        let profiles = westplay_setup::uninstall::remove_from_profiles(&documents, userdata.as_deref());
        for (file, error) in &profiles.failed {
            log!("setup: WestPlay's old mod left in {}: {error}", file.display());
        }
    }
    Ok(())
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
        let mod_error = westplay_setup::uninstall::remove_mod(&documents)
            .and_then(|()| match places::westplay_documents() {
                Some(ours) => westplay_setup::uninstall::remove_mod_from(&ours),
                None => Ok(()),
            })
            .err()
            .map(|e| e.to_string());
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

/// The companion's own browser data (its window's WebView2 profile): in use until WestPlay ends.
const COMPANION_BROWSER: &str = "EBWebView";
/// How long a folder still in use (a browser that hasn't let go of its files yet) is tried again.
const IN_USE_FOR: std::time::Duration = std::time::Duration::from_secs(20);

/// WestPlay's data leaves the PC while the companion shows it going (the uninstall's spinner), once it's out of
/// the game: the engine stops (the screen's and the players' browsers, and with them the files they hold),
/// then the players' browsers go with every sign-in, and unless `keep`, the maps, the player's own wallpapers
/// and icon sets, and the settings; the setup record either way (a reinstall starts with the onboarding).
/// Each is tried again while a browser that has just closed still holds it. Left for [`leave_pc`]: what's in
/// use until WestPlay ends (the companion's own browser data, the logs), and anything still held here.
#[tauri::command]
pub async fn remove_data(keep: bool, app: AppHandle, store: State<'_, SetupStore>) -> Result<(), String> {
    let config = store.0.dir.clone();
    let data = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
    blocking(move || {
        crate::stop_host(&app);
        let until = std::time::Instant::now() + IN_USE_FOR;
        let kept = |path: &Path| {
            path.file_name().is_some_and(|name| {
                name == COMPANION_BROWSER || (keep && (name == "maps" || name == westplay_host::customize::FOLDER))
            })
        };
        let mut left = Vec::new();
        for path in std::fs::read_dir(&data).into_iter().flatten().flatten().map(|entry| entry.path()) {
            if !kept(&path) && !remove_soon(&path, until) {
                left.push(path);
            }
        }
        let settings = if keep { config.join(westplay_setup::record::FILE_NAME) } else { config };
        if !remove_soon(&settings, until) {
            left.push(settings);
        }
        if !left.is_empty() {
            let names: Vec<String> = left.iter().map(|path| path.display().to_string()).collect();
            log!("[uninstall] still in use, left for when WestPlay has ended: {}", names.join(", "));
        }
        Ok(())
    })
    .await
}

/// Removes `path` (a folder or a file), trying again while it's in use, until `until`: whether it's gone.
fn remove_soon(path: &Path, until: std::time::Instant) -> bool {
    loop {
        let removed = if path.is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) };
        match removed {
            Ok(()) => return true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return true,
            Err(_) if std::time::Instant::now() >= until => return false,
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(300)),
        }
    }
}

/// WestPlay leaves the PC (after [`remove_data`]): a hidden step waits for WestPlay to end, removes what was in
/// use until then (the companion's own browser data, the logs in Documents\WestPlay, the data and settings
/// folders themselves unless `keep`, and anything [`remove_data`] couldn't), and, if it was installed, runs its
/// uninstaller quietly, which takes the app itself away. Then it quits.
#[tauri::command]
pub fn leave_pc(keep: bool, app: AppHandle, store: State<SetupStore>) -> Result<(), String> {
    let config = store.0.dir.clone();
    let data = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
    let mut paths: Vec<PathBuf> = if keep {
        std::fs::read_dir(&data)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name().is_none_or(|name| name != "maps" && name != westplay_host::customize::FOLDER)
            })
            .chain([config.join(westplay_setup::record::FILE_NAME)])
            .collect()
    } else {
        vec![data, config]
    };
    if let Some(documents) = places::documents() {
        paths.push(documents.join("WestPlay"));
    }
    let uninstaller = std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.join("uninstall.exe")));
    after_exit(&paths, uninstaller.filter(|path| path.is_file()).as_deref())?;
    app.exit(0);
    Ok(())
}

/// Starts a step with no window at all (PowerShell, hidden): it waits for this WestPlay to end (its process
/// gone, not a guess at the time), removes `paths` (folders or files), each tried again while a browser that
/// has just closed still holds it, and starts `uninstaller` quietly, if there is one.
fn after_exit(paths: &[PathBuf], uninstaller: Option<&Path>) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut script = after_exit_script(std::process::id(), paths);
    if let Some(uninstaller) = uninstaller {
        script.push_str(&format!("Start-Process -FilePath {} -ArgumentList '/S'\n", quoted(uninstaller)));
    }
    std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden"])
        .args(["-EncodedCommand", &encoded_command(&script)])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Couldn't remove WestPlay's data: {e}"))
}

/// The step's script: wait for process `pid` to end (a minute at most), then remove each of `paths`, trying
/// again for up to half a minute while it's in use.
fn after_exit_script(pid: u32, paths: &[PathBuf]) -> String {
    let list: Vec<String> = paths.iter().map(|path| quoted(path)).collect();
    [
        "$ErrorActionPreference = 'SilentlyContinue'".to_owned(),
        format!("$westplay = Get-Process -Id {pid}"),
        "if ($westplay) { $westplay.WaitForExit(60000) | Out-Null }".to_owned(),
        format!("foreach ($path in @({})) {{", list.join(", ")),
        "  for ($try = 0; $try -lt 60 -and (Test-Path -LiteralPath $path); $try++) {".to_owned(),
        "    Remove-Item -LiteralPath $path -Recurse -Force".to_owned(),
        "    if (Test-Path -LiteralPath $path) { Start-Sleep -Milliseconds 500 }".to_owned(),
        "  }".to_owned(),
        "}".to_owned(),
        String::new(),
    ]
    .join("\n")
}

/// `path` as a PowerShell string: in single quotes, its own single quotes doubled.
fn quoted(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "''"))
}

/// `script` as PowerShell's `-EncodedCommand` takes it: its UTF-16 little-endian bytes, in base64.
fn encoded_command(script: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(chunk.get(1).copied().unwrap_or(0)) << 8)
            | u32::from(chunk.get(2).copied().unwrap_or(0));
        for (i, shift) in [18u32, 12, 6, 0].into_iter().enumerate() {
            out.push(if i <= chunk.len() { char::from(ALPHABET[(n >> shift) as usize & 63]) } else { '=' });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hands_powershell_its_script_encoded_as_it_takes_it() {
        // "ab" in UTF-16 little-endian is 61 00 62 00.
        assert_eq!(encoded_command("ab"), "YQBiAA==");
        assert_eq!(encoded_command("a"), "YQA=");
    }

    #[test]
    fn the_step_waits_for_westplay_then_removes_each_path_quoted() {
        let script = after_exit_script(4242, &[PathBuf::from(r"C:\Users\O'Brien\WestPlay")]);
        assert!(script.contains("Get-Process -Id 4242"));
        assert!(script.contains("WaitForExit"));
        assert!(script.contains(r"@('C:\Users\O''Brien\WestPlay')"));
    }

    /// The real step, hidden: it waits for a process still running (a short ping) to end, then removes a folder.
    #[test]
    #[ignore = "starts PowerShell"]
    fn the_hidden_step_removes_a_folder_once_the_process_has_ended() {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let folder = std::env::temp_dir().join(format!("westplay-after-exit-{}", std::process::id()));
        std::fs::create_dir_all(folder.join("inside")).unwrap();
        std::fs::write(folder.join("inside").join("file.txt"), "x").unwrap();
        let mut waiting = std::process::Command::new("ping")
            .args(["-n", "3", "127.0.0.1"])
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let started = std::time::Instant::now();
        let script = after_exit_script(waiting.id(), std::slice::from_ref(&folder));
        let status = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden"])
            .args(["-EncodedCommand", &encoded_command(&script)])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .unwrap();
        let _ = waiting.wait();
        assert!(status.success());
        assert!(started.elapsed() >= std::time::Duration::from_millis(1500), "it didn't wait for the process");
        assert!(!folder.exists(), "the folder is still there");
    }
}
