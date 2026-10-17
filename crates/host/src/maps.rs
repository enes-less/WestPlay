//! The player's map for the cab's Maps app, watched while WestPlay runs: the map of the profile they play
//! (from the save the game last wrote), built by `westplay_setup::map` from the game WestPlay is set up in
//! (the companion's setup chooses it), its DLCs and that profile's map mods, and served to the cabin page
//! from WestPlay's `maps` folder. A player with no profile yet gets the base game's map, the one the profile
//! they make will play (unless they add map mods to it).
//!
//! A map is built only while the game is closed (it takes gigabytes of memory): one to build while the game
//! runs waits for it to close, the cab keeping the map it had (an older build of the same map, if there's
//! one). It waits too while the PC hasn't the memory free for it ([`MEMORY_NEEDED`]), and starts once it
//! has; a PC that hasn't that much memory at all builds it anyway (slower, Windows lending it disk). Players run WestPlay first, for its first setup, then the game. A map WestPlay doesn't allow (HVT's
//! Road to Heavy) is never built.
//!
//! Finding the profile's map mods opens their archives, so it's done again only when something it depends
//! on changes: another profile played, its mods changed (its `profile.sii` rewritten), the game updated.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, SystemTime};

use westplay_mapdata::export::{Progress, Stage};
use westplay_mapdata::{locale, save};
use westplay_setup::game::{self, Game};
use westplay_setup::{map, places, profiles};

/// How often to look again: the profile played, the game running or not.
const CHECK_EVERY: Duration = Duration::from_secs(10);
/// How often the profile's map mods are looked at again anyway (a Workshop mod updated changes no profile).
const RECHECK_MODS: Duration = Duration::from_secs(5 * 60);

/// The memory a build wants free, bytes: the base game's whole map peaked at 2.93 GB (2026-10-17, measured
/// as it was built), and a margin for the rest of the PC.
pub const MEMORY_NEEDED: u64 = 3_500_000_000;

/// The map's state, for the companion window.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum MapStatus {
    /// Not looked yet.
    #[default]
    Unknown,
    /// WestPlay isn't set up in a game yet, or the game isn't there any more.
    NoGame,
    Ready,
    /// Being built: how far, 0 to 1.
    Building {
        done: f32,
        /// What it's doing now.
        stage: MapStage,
    },
    /// To build, once the game is closed.
    WaitingForGame,
    /// To build, once the PC has `needed` bytes of memory free (it has `free` now).
    WaitingForMemory {
        needed: u64,
        free: u64,
    },
    /// A map WestPlay doesn't allow.
    NotAllowed,
    Failed {
        error: String,
    },
}

/// What a map being built is doing, for the companion to tell the player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MapStage {
    /// The map's sectors and definitions, from the game, its DLCs and the map mods.
    Reading,
    /// Roads, junctions, water and land, drawn as shapes.
    Drawing,
    /// The shapes cut into tiles.
    Tiles,
    /// The rest written (the road signs, the places, the map's manifest).
    Saving,
}

impl MapStage {
    fn of(progress: &Progress) -> Self {
        match progress.stage {
            Stage::Reading => Self::Reading,
            Stage::Drawing => Self::Drawing,
            // The tiles are written by 0.95 of the way.
            Stage::Tiles if progress.done < 0.95 => Self::Tiles,
            Stage::Tiles | Stage::Done => Self::Saving,
        }
    }
}

/// What the cab shows and the companion says.
#[derive(Clone, Debug, PartialEq)]
pub struct MapUpdate {
    /// The map's folder in the `maps` folder, for the cab (`None`: no map to show).
    pub folder: Option<String>,
    /// When the map shown was built (seconds since 1970): a map built again keeps its folder.
    pub built: Option<u64>,
    pub status: MapStatus,
}

/// The game WestPlay is set up in (its folder), set by the companion's setup, from any thread: the watch
/// looks at once when it's set.
#[derive(Debug, Default)]
pub struct SetUpGame {
    dir: Mutex<Option<PathBuf>>,
    changed: Condvar,
}

impl SetUpGame {
    pub fn new(dir: Option<PathBuf>) -> Self {
        Self { dir: Mutex::new(dir), changed: Condvar::new() }
    }

    pub fn set(&self, dir: Option<PathBuf>) {
        *self.dir.lock().unwrap_or_else(|e| e.into_inner()) = dir;
        self.changed.notify_all();
    }

    pub fn get(&self) -> Option<PathBuf> {
        self.dir.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Waits up to `timeout` for the game to be other than `known`.
    fn wait_for_another(&self, known: &Option<PathBuf>, timeout: Duration) {
        let dir = self.dir.lock().unwrap_or_else(|e| e.into_inner());
        let _ = self.changed.wait_timeout_while(dir, timeout, |dir| dir == known);
    }
}

pub struct MapWatch {
    pub updates: Receiver<MapUpdate>,
}

impl MapWatch {
    /// Watches the map of the profile played in `game`, building it into `maps` when it needs it and the game
    /// is closed.
    pub fn start(maps: PathBuf, game: Arc<SetUpGame>) -> Self {
        let (sender, updates) = mpsc::channel();
        let _ = std::thread::Builder::new().name("westplay-maps".into()).spawn(move || watch(&maps, &game, &sender));
        Self { updates }
    }
}

/// What the profile's map was worked out from: when none of it changes, the map stays what it was.
#[derive(Clone, Debug, PartialEq)]
struct Basis {
    /// The game's folder (WestPlay set up in another).
    game: PathBuf,
    profile: Option<PathBuf>,
    profile_written: Option<SystemTime>,
    game_written: Option<SystemTime>,
    /// Which stretch of `RECHECK_MODS` it is.
    round: u64,
}

fn watch(maps: &Path, set_up: &SetUpGame, sender: &Sender<MapUpdate>) {
    let documents = places::game_documents();
    let userdata = places::steam().map(|steam| steam.join("userdata"));
    let mut sent: Option<MapUpdate> = None;
    let mut worked_out: Option<(Basis, Want)> = None;
    // A build that failed, tried again only once something it's built from changes (or every
    // `RECHECK_MODS`), not every few seconds.
    let mut failed: Option<(Basis, MapUpdate)> = None;
    loop {
        let set_up_in = set_up.get();
        let game = set_up_in.as_deref().and_then(Game::recognise);
        let update = match (&game, &documents) {
            (None, _) | (_, None) => MapUpdate { folder: None, built: None, status: MapStatus::NoGame },
            (Some(game), Some(documents)) => {
                let basis = basis(game, documents, userdata.as_deref());
                match failed.as_ref() {
                    Some((at, update)) if *at == basis => update.clone(),
                    _ => {
                        let want = match worked_out.take() {
                            Some((known, want)) if known == basis => want,
                            _ => want(game, documents, userdata.as_deref(), &basis, maps),
                        };
                        let update = carry_out(game, &want, maps, sender, &mut sent);
                        failed =
                            matches!(update.status, MapStatus::Failed { .. }).then(|| (basis.clone(), update.clone()));
                        worked_out = Some((basis, want));
                        update
                    }
                }
            }
        };
        if sent.as_ref() != Some(&update) {
            if sender.send(update.clone()).is_err() {
                return;
            }
            sent = Some(update);
        }
        set_up.wait_for_another(&set_up_in, CHECK_EVERY);
    }
}

fn basis(game: &Game, documents: &Path, userdata: Option<&Path>) -> Basis {
    let profile =
        save::latest(documents, userdata).and_then(|save| save::profile_of(&save)?.parent().map(Path::to_path_buf));
    let written = |path: &Path| std::fs::metadata(path).and_then(|m| m.modified()).ok();
    Basis {
        game: game.dir.clone(),
        profile_written: profile.as_deref().and_then(|dir| written(&dir.join("profile.sii"))),
        profile,
        game_written: written(&game.dir.join("bin").join("win_x64").join("eurotrucks2.exe")),
        round: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs())
            / RECHECK_MODS.as_secs(),
    }
}

/// The map a profile wants: its folder, what it's built from, and the profile (to build it).
struct Want {
    profile: profiles::Profile,
    folder: PathBuf,
    source: serde_json::Value,
    language: Option<String>,
    allowed: bool,
}

fn want(game: &Game, documents: &Path, userdata: Option<&Path>, basis: &Basis, maps: &Path) -> Want {
    let all = profiles::find(documents, userdata, Some(game));
    // The profile last saved in; before any save, the most recently made one; before any profile, the one a
    // new player makes.
    let profile = match &basis.profile {
        Some(dir) => all.into_iter().find(|profile| &profile.dir == dir),
        None => all.into_iter().next(),
    }
    .unwrap_or_else(profiles::Profile::new_player);
    let language = std::fs::read_to_string(documents.join("config.cfg")).ok().and_then(|c| locale::game_language(&c));
    Want {
        folder: map::folder(maps, &profile),
        source: map::source(game, &profile, language.as_deref()),
        allowed: !profile.plays_hvt_map(),
        profile,
        language,
    }
}

/// Shows the wanted map if it's built, builds it if it isn't and the game is closed, or waits.
fn carry_out(
    game: &Game,
    want: &Want,
    maps: &Path,
    sender: &Sender<MapUpdate>,
    sent: &mut Option<MapUpdate>,
) -> MapUpdate {
    let name = want.folder.file_name().map(|name| name.to_string_lossy().into_owned());
    let built = built_at(&want.folder);
    let update = |folder: Option<String>, built: Option<u64>, status| MapUpdate { folder, built, status };
    if !want.allowed {
        return update(None, None, MapStatus::NotAllowed);
    }
    if !map::needs_building(&want.folder, &want.source) {
        return update(name, built, MapStatus::Ready);
    }
    // An older build of the same map meanwhile, if there's one.
    let shown = name.clone().filter(|_| built.is_some());
    if game::running() {
        return update(shown, built, MapStatus::WaitingForGame);
    }
    if let Some(free) = memory_short(memory()) {
        return update(shown, built, MapStatus::WaitingForMemory { needed: MEMORY_NEEDED, free });
    }
    let mut last_told = -1.0f32;
    let mut last_stage = None;
    let mut tell = |progress: Progress| {
        let stage = MapStage::of(&progress);
        let update = update(shown.clone(), built, MapStatus::Building { done: progress.done, stage });
        // Each new stage, and steps of the way within one.
        if (Some(stage) != last_stage || progress.done - last_told >= 0.05) && sender.send(update.clone()).is_ok() {
            last_told = progress.done;
            last_stage = Some(stage);
            *sent = Some(update);
        }
    };
    tell(Progress { stage: Stage::Reading, done: 0.0 });
    match map::build(game, &want.profile, want.language.as_deref(), maps, &mut tell) {
        Ok(_) => update(name, built_at(&want.folder), MapStatus::Ready),
        // The game started meanwhile: once it's closed.
        Err(map::BuildError::GameRunning) => update(shown, built, MapStatus::WaitingForGame),
        Err(error) => update(shown, built, MapStatus::Failed { error: error.to_string() }),
    }
}

/// The PC's memory: free now, and in all (bytes); `None` if Windows doesn't say.
fn memory() -> Option<(u64, u64)> {
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut status = MEMORYSTATUSEX { dwLength: size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    // SAFETY: `status` is a local with its size set, as the call requires.
    unsafe { GlobalMemoryStatusEx(&mut status) }.ok()?;
    Some((status.ullAvailPhys, status.ullTotalPhys))
}

/// What's free, when a build has to wait for more: the PC has enough memory in all but not free now. One that
/// hasn't enough in all doesn't wait (it never would have): it builds, slower.
fn memory_short(memory: Option<(u64, u64)>) -> Option<u64> {
    let (free, total) = memory?;
    (total >= MEMORY_NEEDED && free < MEMORY_NEEDED).then_some(free)
}

/// When the map in `folder` was built (its manifest written), if it has been.
fn built_at(folder: &Path) -> Option<u64> {
    let written = std::fs::metadata(folder.join("map.json")).and_then(|m| m.modified()).ok()?;
    written.duration_since(SystemTime::UNIX_EPOCH).ok().map(|since| since.as_secs())
}

/// `{"type":"map","folder":"europe-cbf29ce484222325","built":1792112933}`: the map the cab shows, in WestPlay's
/// maps folder (served at `https://westplay.maps/`), and when it was built; `null` for none.
pub fn map_message(folder: Option<&str>, built: Option<u64>) -> String {
    serde_json::json!({ "type": "map", "folder": folder, "built": built }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_the_cab_its_maps_folder() {
        assert_eq!(map_message(Some("europe-1"), Some(7)), r#"{"built":7,"folder":"europe-1","type":"map"}"#);
        assert_eq!(map_message(None, None), r#"{"built":null,"folder":null,"type":"map"}"#);
    }

    #[test]
    fn looks_at_once_when_setup_sets_the_game() {
        let set_up = Arc::new(SetUpGame::default());
        let setter = set_up.clone();
        let started = std::time::Instant::now();
        let thread = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            setter.set(Some(PathBuf::from("game")));
        });
        set_up.wait_for_another(&None, Duration::from_secs(10));
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(set_up.get(), Some(PathBuf::from("game")));
        thread.join().unwrap();
    }

    #[test]
    fn tells_what_the_build_is_doing() {
        let at = |stage, done| MapStage::of(&Progress { stage, done });
        assert_eq!(at(Stage::Reading, 0.0), MapStage::Reading);
        assert_eq!(at(Stage::Drawing, 0.15), MapStage::Drawing);
        assert_eq!(at(Stage::Tiles, 0.6), MapStage::Tiles);
        assert_eq!(at(Stage::Tiles, 0.95), MapStage::Saving);
        assert_eq!(at(Stage::Done, 1.0), MapStage::Saving);
    }

    #[test]
    fn tells_the_companion_the_maps_state() {
        let json = |status: MapStatus| serde_json::to_value(status).unwrap();
        assert_eq!(
            json(MapStatus::Building { done: 0.5, stage: MapStage::Drawing }),
            serde_json::json!({ "state": "building", "done": 0.5, "stage": "drawing" })
        );
        assert_eq!(json(MapStatus::WaitingForGame), serde_json::json!({ "state": "waitingForGame" }));
        assert_eq!(
            json(MapStatus::WaitingForMemory { needed: 3, free: 1 }),
            serde_json::json!({ "state": "waitingForMemory", "needed": 3, "free": 1 })
        );
    }

    /// A PC with the memory free builds; one with enough but not free waits; one without enough at all
    /// builds anyway (it would wait for ever).
    #[test]
    fn waits_for_memory_only_when_it_will_come() {
        const GB: u64 = 1_000_000_000;
        assert_eq!(memory_short(Some((10 * GB, 32 * GB))), None);
        assert_eq!(memory_short(Some((2 * GB, 32 * GB))), Some(2 * GB));
        assert_eq!(memory_short(Some((GB, 3 * GB))), None);
        assert_eq!(memory_short(None), None);
        let (free, total) = memory().expect("Windows says");
        assert!(free > 0 && total >= free);
    }
}
