//! The player's maps for the cab's Maps app, watched while WestPlay runs: a map for each of their profiles,
//! built by `westplay_setup::map` from the game WestPlay is set up in (the companion's setup chooses it), its
//! DLCs and that profile's map mods, one by one (the profile they play, from the save the game last wrote,
//! first), and the played profile's served to the cabin page from WestPlay's `maps` folder. Profiles with the
//! same map and map mods share it. A map that can't be made (a map mod WestPlay can't read) or isn't allowed
//! doesn't stop the others'. A player with no profile yet gets the base game's map, the one the profile they
//! make will play (unless they add map mods to it).
//!
//! A map is built only while the game is closed (it takes gigabytes of memory): one to build while the game
//! runs waits for it to close, the cab keeping the map it had (an older build of the same map, if there's
//! one). It waits too while the PC hasn't the memory free for it ([`MEMORY_NEEDED`]), and starts once it
//! has; a PC that hasn't that much memory at all builds it anyway (slower, Windows lending it disk). Players run WestPlay first, for its first setup, then the game. A map WestPlay doesn't allow (HVT's
//! Road to Heavy) is never built.
//!
//! With TruckersMP installed, there's one more map: TruckersMP's, which every profile plays there (no
//! profile's mods load on TruckersMP; its own packs do: westplay_setup::truckersmp), built from the game, its
//! DLCs and the packs the last TruckersMP session mounted (until there's been one, all it has), right after
//! the played profile's map; while the game's last session was TruckersMP's, it's the one played (owner
//! 2026-10-21: made as soon as TruckersMP is found, not after a first session).
//!
//! Finding the profiles' map mods opens their archives, so it's done again only when something it depends
//! on changes: another profile played, a profile's mods changed (its `profile.sii` rewritten), the game
//! updated. A map that failed is tried again once what it's built from changes.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, SystemTime};

use westplay_mapdata::export::{ExportError, Progress, Stage};
use westplay_mapdata::{locale, save};
use westplay_setup::game::{self, Game};
use westplay_setup::{map, places, profiles, truckersmp};

/// The packs the last TruckersMP session mounted, kept in the `maps` folder (truckersmp.rs).
const TRUCKERSMP_RECORD: &str = "truckersmp.json";

/// How often to look again: the profile played, the game running or not.
const CHECK_EVERY: Duration = Duration::from_secs(10);
/// How often the profile's map mods are looked at again anyway (a Workshop mod updated changes no profile).
const RECHECK_MODS: Duration = Duration::from_secs(5 * 60);

/// The memory a build wants free, bytes: the base game's whole map peaked at 2.93 GB (2026-10-17, measured
/// as it was built), and a margin for the rest of the PC.
pub const MEMORY_NEEDED: u64 = 3_500_000_000;

/// The most memory a build may take ([`memory_ceiling`]) on any PC, bytes: twice what the biggest map known
/// takes (ProMods with its Middle East, 2.25 GB peak; 2026-10-19).
const CEILING_AT_LEAST: u64 = 4 << 30;
/// What finding the sea holds a cell of its grid, bytes: the cell, its distance from land, its region's
/// verdict, its steps from the sea (westplay_mapdata::sea).
const SEA_BYTES_PER_CELL: u64 = 4;

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
    /// Another profile's map, to build after the maps before it.
    Queued,
}

/// A profile's map, for the companion's list of profiles: the profile (its folder, as the setup's check of
/// the game names it), its map's state, and the map mods left out of it (their files couldn't be read).
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileMap {
    pub dir: String,
    pub status: MapStatus,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub left_out: Vec<String>,
    /// TruckersMP's map (its `dir` TruckersMP's installation), not a profile's.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub truckersmp: bool,
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
    /// The played profile's map's state.
    pub status: MapStatus,
    /// Every profile's map's state.
    pub profiles: Vec<ProfileMap>,
}

/// The game WestPlay is set up in (its folder), set by the companion's setup, and the player's asks to prepare
/// the maps again (the companion's Map page), from any thread: the watch looks at once at either.
#[derive(Debug, Default)]
pub struct SetUpGame {
    state: Mutex<SetUp>,
    changed: Condvar,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct SetUp {
    dir: Option<PathBuf>,
    /// How many times the player has asked to prepare the maps again.
    asked_again: u64,
}

impl SetUpGame {
    pub fn new(dir: Option<PathBuf>) -> Self {
        Self { state: Mutex::new(SetUp { dir, asked_again: 0 }), changed: Condvar::new() }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, SetUp> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn set(&self, dir: Option<PathBuf>) {
        self.state().dir = dir;
        self.changed.notify_all();
    }

    pub fn get(&self) -> Option<PathBuf> {
        self.state().dir.clone()
    }

    /// Asks for every profile's map to be prepared again, failed ones included: each is built anew in turn
    /// (as ever, while the game is closed), the cab keeping the map it has meanwhile.
    pub fn prepare_maps_again(&self) {
        self.state().asked_again += 1;
        self.changed.notify_all();
    }

    /// Waits up to `timeout` for the game, or the player's asks, to be other than `known`.
    fn wait_for_another(&self, known: &SetUp, timeout: Duration) {
        let state = self.state();
        let _ = self.changed.wait_timeout_while(state, timeout, |state| state == known);
    }
}

pub struct MapWatch {
    pub updates: Receiver<MapUpdate>,
    /// The map the played profile plays, as worked out.
    pub played: PlayedMap,
}

/// The game and the profile played (its map and its map mods): what the cab's map is built from, and so
/// what the route's junctions are read from (route.rs). `None` until worked out, and without a game.
pub type PlayedMap = Arc<Mutex<Option<(Game, profiles::Profile)>>>;

impl MapWatch {
    /// Watches the maps of the profiles in `game`, building each into `maps` when it needs it and the game
    /// is closed.
    pub fn start(maps: PathBuf, game: Arc<SetUpGame>) -> Self {
        let (sender, updates) = mpsc::channel();
        let played = PlayedMap::default();
        let telling = played.clone();
        let _ = std::thread::Builder::new()
            .name("westplay-maps".into())
            .spawn(move || watch(&maps, &game, &sender, &telling));
        Self { updates, played }
    }
}

/// TruckersMP's installation, and its packs with when each was written.
type TruckersMpPacks = (PathBuf, Vec<(PathBuf, Option<SystemTime>)>);

/// What the profiles' maps were worked out from: when none of it changes, the maps stay what they were.
#[derive(Clone, Debug, PartialEq)]
struct Basis {
    /// The game's folder (WestPlay set up in another).
    game: PathBuf,
    /// The profile the game has selected, by name (its log), and the folder of the one with the newest save.
    selected: Option<String>,
    profile: Option<PathBuf>,
    /// Every profile, and when its `profile.sii` (its mods) was last written.
    profiles: Vec<(PathBuf, Option<SystemTime>)>,
    game_written: Option<SystemTime>,
    /// TruckersMP's installation, if it's installed, with its packs and when each was written (its launcher
    /// updates them), and the packs the game's last session mounted from it (its log; `None`: it wasn't
    /// TruckersMP's).
    truckersmp: Option<TruckersMpPacks>,
    session: Option<Vec<PathBuf>>,
    /// Which stretch of `RECHECK_MODS` it is.
    round: u64,
}

/// Maps that couldn't be made, by folder: what each was built from then, and why. Tried again once what it's
/// built from changes, not every few seconds.
type Failures = HashMap<PathBuf, (serde_json::Value, String)>;

fn watch(maps: &Path, set_up: &SetUpGame, sender: &Sender<MapUpdate>, played: &PlayedMap) {
    let documents = places::game_documents();
    let userdata = places::steam().map(|steam| steam.join("userdata"));
    let mut sent: Option<MapUpdate> = None;
    let mut worked_out: Option<(Basis, Vec<Want>)> = None;
    let mut failed = Failures::new();
    // Maps the player asked to prepare again (by folder), each built anew in its turn, and the last ask seen.
    let mut again: HashSet<PathBuf> = HashSet::new();
    let mut asked_again = 0;
    // Unused maps not removed whole yet, told once (tried again at each look).
    let mut not_removed: HashSet<PathBuf> = HashSet::new();
    loop {
        let set_up_in = set_up.state().clone();
        let game = set_up_in.dir.as_deref().and_then(Game::recognise);
        let asked_now = set_up_in.asked_again != asked_again;
        if asked_now {
            asked_again = set_up_in.asked_again;
            // Everything looked at afresh, failures forgotten.
            worked_out = None;
            failed.clear();
        }
        let (update, more) = match (&game, &documents) {
            (None, _) | (_, None) => {
                *played.lock().unwrap_or_else(|e| e.into_inner()) = None;
                let update = MapUpdate { folder: None, built: None, status: MapStatus::NoGame, profiles: Vec::new() };
                (update, false)
            }
            (Some(game), Some(documents)) => {
                let basis = basis(game, documents, userdata.as_deref());
                // A TruckersMP session's packs, kept for its map until the next session.
                if let Some(session) = &basis.session
                    && truckersmp::remember(&maps.join(TRUCKERSMP_RECORD), session)
                {
                    log!("TruckersMP: its last session mounted {} packs", session.len());
                }
                let wants = match worked_out.take() {
                    Some((known, wants)) if known == basis => wants,
                    _ => wants(game, documents, userdata.as_deref(), &basis, maps),
                };
                let profile = &wants[0].profile;
                let mut shared = played.lock().unwrap_or_else(|e| e.into_inner());
                if shared.as_ref().is_none_or(|(known_game, known)| known_game != game || known != profile) {
                    *shared = Some((game.clone(), profile.clone()));
                }
                drop(shared);
                if asked_now {
                    again.extend(wants.iter().filter(|want| want.allowed).map(|want| want.folder.clone()));
                }
                let done = carry_out(game, &wants, maps, &mut failed, &mut again, sender, &mut sent);
                // The maps no one wants any more gone (none is being built between two looks).
                remove_unused(maps, &wants, &mut not_removed);
                worked_out = Some((basis, wants));
                done
            }
        };
        if sent.as_ref() != Some(&update) {
            if sender.send(update.clone()).is_err() {
                return;
            }
            sent = Some(update);
        }
        // A map just built: on to the next at once.
        if !more {
            set_up.wait_for_another(&set_up_in, CHECK_EVERY);
        }
    }
}

fn basis(game: &Game, documents: &Path, userdata: Option<&Path>) -> Basis {
    let profile =
        save::latest(documents, userdata).and_then(|save| save::profile_of(&save)?.parent().map(Path::to_path_buf));
    let written = |path: &Path| std::fs::metadata(path).and_then(|m| m.modified()).ok();
    Basis {
        game: game.dir.clone(),
        selected: profiles::selected(documents),
        profile,
        profiles: profile_dirs(documents, userdata)
            .into_iter()
            .map(|dir| {
                let at = written(&dir.join("profile.sii"));
                (dir, at)
            })
            .collect(),
        game_written: written(&game.dir.join("bin").join("win_x64").join("eurotrucks2.exe")),
        truckersmp: truckersmp::installation().map(|installation| {
            let packs = truckersmp::packs(&installation).into_iter().map(|pack| {
                let at = written(&pack);
                (pack, at)
            });
            (installation.clone(), packs.collect())
        }),
        session: std::fs::read(documents.join("game.log.txt"))
            .ok()
            .and_then(|log| truckersmp::mounted(&String::from_utf8_lossy(&log))),
        round: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs())
            / RECHECK_MODS.as_secs(),
    }
}

/// Every profile's folder, as `profiles::find` looks for them (the game's Documents folder's, and Steam
/// Cloud's under Steam's `userdata`), sorted.
fn profile_dirs(documents: &Path, userdata: Option<&Path>) -> Vec<PathBuf> {
    let mut folders = vec![documents.join("profiles"), documents.join("steam_profiles")];
    for user in userdata.into_iter().flat_map(|dir| std::fs::read_dir(dir).into_iter().flatten().flatten()) {
        folders.push(user.path().join(game::STEAM_APP).join("remote").join("profiles"));
    }
    let mut dirs: Vec<PathBuf> = folders
        .iter()
        .flat_map(|folder| std::fs::read_dir(folder).into_iter().flatten().flatten())
        .map(|entry| entry.path())
        .filter(|dir| dir.join("profile.sii").is_file())
        .collect();
    dirs.sort();
    dirs
}

/// A map some profiles want: its folder, what it's built from, a profile to build it for, and every profile
/// that plays it.
struct Want {
    profile: profiles::Profile,
    dirs: Vec<PathBuf>,
    folder: PathBuf,
    source: serde_json::Value,
    language: Option<String>,
    allowed: bool,
    /// TruckersMP's map, not a profile's.
    truckersmp: bool,
}

/// The maps the profiles want, the played profile's first (the profile last saved in; before any save, the
/// most recently made one; before any profile, the one a new player makes), then the others', the most
/// recently played first. Profiles playing the same map with the same map mods share one.
fn wants(game: &Game, documents: &Path, userdata: Option<&Path>, basis: &Basis, maps: &Path) -> Vec<Want> {
    let mut all = profiles::find(documents, userdata, Some(game));
    // The profile played: the one the game has selected (its log; two of the same name, the one with the
    // newest save first), else the one with the newest save, else the newest.
    let named = basis.selected.as_ref().and_then(|name| {
        let mut same: Vec<usize> = (0..all.len()).filter(|&index| &all[index].name == name).collect();
        same.sort_by_key(|&index| basis.profile.as_ref() != Some(&all[index].dir));
        same.first().copied()
    });
    let played = match (named, &basis.profile) {
        (Some(index), _) => Some(index),
        (None, Some(dir)) => all.iter().position(|profile| &profile.dir == dir),
        (None, None) => (!all.is_empty()).then_some(0),
    };
    let first = match played {
        Some(index) => all.remove(index),
        None => profiles::Profile::new_player(),
    };
    let language = std::fs::read_to_string(documents.join("config.cfg")).ok().and_then(|c| locale::game_language(&c));
    let mut wants: Vec<Want> = Vec::new();
    for profile in std::iter::once(first).chain(all) {
        let folder = map::folder(maps, &profile);
        if let Some(want) = wants.iter_mut().find(|want| want.folder == folder) {
            want.dirs.push(profile.dir.clone());
            continue;
        }
        wants.push(Want {
            dirs: vec![profile.dir.clone()],
            source: map::source(game, &profile, language.as_deref()),
            allowed: !profile.plays_hvt_map(),
            folder,
            profile,
            language: language.clone(),
            truckersmp: false,
        });
    }
    // TruckersMP's, one for every profile: the one played while the game's last session was TruckersMP's,
    // else right after the played profile's.
    if let Some((installation, on_disk)) = &basis.truckersmp {
        let packs = truckersmp::remembered(&maps.join(TRUCKERSMP_RECORD))
            .unwrap_or_else(|| on_disk.iter().map(|(pack, _)| pack.clone()).collect());
        let profile = truckersmp::profile(installation, &packs);
        let want = Want {
            dirs: vec![installation.clone()],
            source: map::source(game, &profile, language.as_deref()),
            folder: map::folder(maps, &profile),
            allowed: true,
            profile,
            language,
            truckersmp: true,
        };
        let at = if basis.session.is_some() { 0 } else { 1 };
        wants.insert(at.min(wants.len()), want);
    }
    wants
}

/// Removes the maps in `maps` no profile wants any more (owner 2026-10-22): a profile deleted, its map mods
/// changed, a mod once taken for a map mod (a truck mod, before a368b80). Only WestPlay's own map folders
/// ([`map_folder_name`]); nothing while no profile has been found (the profiles' folders unread for a moment
/// would leave none wanted). Each is first renamed `<name>.removing` (at once: no longer a map, and known for
/// what it is), then removed; Windows may hold a file of it a moment (a virus scanner going through the
/// files), so one not removed whole is tried again at each look, said once (`told`).
fn remove_unused(maps: &Path, wants: &[Want], told: &mut HashSet<PathBuf>) {
    if wants.iter().any(|want| want.dirs.iter().any(|dir| dir.as_os_str().is_empty())) {
        return;
    }
    let wanted: HashSet<&Path> = wants.iter().map(|want| want.folder.as_path()).collect();
    // The listing read whole first: Windows won't rename a folder while its parent's listing is open.
    let entries: Vec<(PathBuf, String)> = std::fs::read_dir(maps)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| (entry.path(), entry.file_name().to_string_lossy().into_owned()))
        .collect();
    for (mut folder, name) in entries {
        let removing = name.strip_suffix(REMOVING).is_some_and(map_folder_name);
        if !removing {
            if wanted.contains(folder.as_path()) || !is_map_folder(&folder) {
                continue;
            }
            let renamed = maps.join(format!("{name}{REMOVING}"));
            if let Err(error) = std::fs::rename(&folder, &renamed) {
                if told.insert(folder.clone()) {
                    log!("map {name}: no profile plays it any more; not removed yet (tried again): {error}");
                }
                continue;
            }
            folder = renamed;
        }
        match remove_tree(&folder) {
            Ok(()) => log!("map {name}: removed, no profile plays it any more"),
            Err(error) => {
                if told.insert(folder) {
                    log!("map {name}: no profile plays it any more; not removed whole yet (tried again): {error}");
                }
            }
        }
    }
}

/// What a map no one wants is renamed with on its way out.
const REMOVING: &str = ".removing";

/// Removes `dir` and all in it, a file and a folder at a time, going on past one it can't (the first such
/// error returned): `std::fs::remove_dir_all` stopped at once with "used by another process" on old maps'
/// folders that these plain deletes took whole (2026-10-22, 16,795 tiles).
fn remove_tree(dir: &Path) -> std::io::Result<()> {
    let entries: Vec<std::fs::DirEntry> = std::fs::read_dir(dir)?.flatten().collect();
    let mut first_error = None;
    for entry in entries {
        let path = entry.path();
        let removed = if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            remove_tree(&path)
        } else {
            std::fs::remove_file(&path)
        };
        if let Err(error) = removed {
            first_error.get_or_insert(error);
        }
    }
    match first_error {
        Some(error) => Err(error),
        None => std::fs::remove_dir(dir),
    }
}

/// Whether a folder's name is one `map::folder` gives: `<map>-<16 hex digits>`.
fn map_folder_name(name: &str) -> bool {
    name.rsplit_once('-')
        .is_some_and(|(map, hash)| !map.is_empty() && hash.len() == 16 && hash.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Whether `folder` is one of WestPlay's maps: named as `map::folder` names them, a map's manifest in it or
/// its tiles (one an earlier removal left part of).
fn is_map_folder(folder: &Path) -> bool {
    let named = folder.file_name().and_then(|name| name.to_str()).is_some_and(map_folder_name);
    let tiles = || {
        std::fs::read_dir(folder)
            .into_iter()
            .flatten()
            .flatten()
            .any(|entry| entry.file_name().to_string_lossy().starts_with("tiles-") && entry.path().is_dir())
    };
    named && (folder.join("map.json").is_file() || tiles())
}

/// A map's state without building it: not allowed, failed (and nothing it's built from changed since),
/// built, or to build (the player asked for it again, `again`).
fn state_of(want: &Want, failed: &Failures, again: &HashSet<PathBuf>) -> MapStatus {
    if !want.allowed {
        return MapStatus::NotAllowed;
    }
    if again.contains(&want.folder) {
        return MapStatus::Queued;
    }
    if let Some((source, error)) = failed.get(&want.folder)
        && *source == want.source
    {
        return MapStatus::Failed { error: error.clone() };
    }
    if map::needs_building(&want.folder, &want.source) { MapStatus::Queued } else { MapStatus::Ready }
}

/// Every profile's map's state, for the companion.
fn profile_maps(wants: &[Want], states: &[MapStatus]) -> Vec<ProfileMap> {
    wants
        .iter()
        .zip(states)
        .flat_map(|(want, status)| {
            let left_out: Vec<String> = match status {
                MapStatus::Ready => map::left_out(&want.folder).into_iter().map(|left| left.name).collect(),
                _ => Vec::new(),
            };
            want.dirs.iter().filter(|dir| !dir.as_os_str().is_empty()).map(move |dir| ProfileMap {
                dir: dir.to_string_lossy().into_owned(),
                status: status.clone(),
                left_out: left_out.clone(),
                truckersmp: want.truckersmp,
            })
        })
        .collect()
}

/// Shows the played profile's map if it's built, and builds the next map that isn't (the played profile's
/// first) if the game is closed, or waits. Returns what to show and say, and whether it built one (so the next
/// is looked at at once).
fn carry_out(
    game: &Game,
    wants: &[Want],
    maps: &Path,
    failed: &mut Failures,
    again: &mut HashSet<PathBuf>,
    sender: &Sender<MapUpdate>,
    sent: &mut Option<MapUpdate>,
) -> (MapUpdate, bool) {
    let played = &wants[0];
    let name = played.folder.file_name().map(|name| name.to_string_lossy().into_owned());
    let built = built_at(&played.folder);
    // The played profile's map once built; meanwhile an older build of it, if there's one.
    let older = name.clone().filter(|_| built.is_some());
    let update = |states: &[MapStatus]| {
        let (folder, built) = match states[0] {
            MapStatus::Ready => (name.clone(), built_at(&played.folder)),
            MapStatus::NotAllowed => (None, None),
            _ => (older.clone(), built),
        };
        MapUpdate { folder, built, status: states[0].clone(), profiles: profile_maps(wants, states) }
    };
    let mut states: Vec<MapStatus> = wants.iter().map(|want| state_of(want, failed, again)).collect();
    let Some(next) = states.iter().position(|state| *state == MapStatus::Queued) else {
        return (update(&states), false);
    };
    if game::running() {
        states[next] = MapStatus::WaitingForGame;
        return (update(&states), false);
    }
    if let Some(free) = memory_short(memory()) {
        states[next] = MapStatus::WaitingForMemory { needed: MEMORY_NEEDED, free };
        return (update(&states), false);
    }
    let want = &wants[next];
    let mut during = states.clone();
    let mut last_told = -1.0f32;
    let mut last_stage = None;
    // What's built, how big it is and the memory it takes as it goes, in the log: a build that takes too long
    // or too much says where.
    let folder = want.folder.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let mods: Vec<&str> = want.profile.map_mods().map(|m| m.package.as_str()).collect();
    let started = std::time::Instant::now();
    log!(
        "map {folder}: building for {:?} ({}, map mods {mods:?}); {} free",
        want.profile.name,
        want.profile.map,
        memory().map_or("?".into(), |(free, total)| format!("{} of {}", gigabytes(free), gigabytes(total)))
    );
    let mut logged_at = -1.0f32;
    let mut logged_stage = None;
    // A build taking more than this is stopped, not left to page the PC to a halt.
    let ceiling = memory().map_or(CEILING_AT_LEAST, |(_, total)| memory_ceiling(total));
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let mut tell = |progress: Progress| {
        if let Some(taken) = process_memory()
            && taken > ceiling
            && !stopping.swap(true, Ordering::Relaxed)
        {
            log!("map {folder}: stopped, taking {} (more than {})", gigabytes(taken), gigabytes(ceiling));
        }
        // A map whose sea grid alone wouldn't fit under the ceiling: stopped before it's made (it's made at
        // once, between two looks at the memory).
        if let Some(size) = progress.size
            && size.sea_cells.saturating_mul(SEA_BYTES_PER_CELL) > ceiling
            && !stopping.swap(true, Ordering::Relaxed)
        {
            log!("map {folder}: stopped, its sea grid alone needing more than {}", gigabytes(ceiling));
        }
        if let Some(size) = progress.size {
            let [x0, z0, x1, z1] = size.extent;
            log!(
                "map {folder}: {} items, roads reaching {:.0} x {:.0} km, sea grid {:.0} million cells of {} m",
                size.items,
                (x1 - x0) / 1000.0,
                (z1 - z0) / 1000.0,
                size.sea_cells as f64 / 1e6,
                size.sea_cell
            );
        }
        if Some(progress.stage) != logged_stage || progress.done - logged_at >= 0.05 {
            logged_at = progress.done;
            logged_stage = Some(progress.stage);
            log!(
                "map {folder}: {:?} {:.0}% at {:.0} s, WestPlay using {}",
                progress.stage,
                progress.done * 100.0,
                started.elapsed().as_secs_f32(),
                process_memory().map_or("?".into(), gigabytes)
            );
        }
        let stage = MapStage::of(&progress);
        during[next] = MapStatus::Building { done: progress.done, stage };
        let update = update(&during);
        // Each new stage, and steps of the way within one.
        if (Some(stage) != last_stage || progress.done - last_told >= 0.01) && sender.send(update.clone()).is_ok() {
            last_told = progress.done;
            last_stage = Some(stage);
            *sent = Some(update);
        }
    };
    tell(Progress { stage: Stage::Reading, done: 0.0, size: None });
    let built = map::build(game, &want.profile, want.language.as_deref(), maps, Some(stop), &mut tell);
    for left in built.iter().flat_map(|built| &built.left_out) {
        log!("map {}: built without {}, which couldn't be read: {}", want.folder.display(), left.name, left.error);
    }
    if let Some(error) = built.as_ref().ok().and_then(|built| built.summary.time_zones_unread.as_ref()) {
        log!("map {}: built without time zones (the cab's clock the game's time): {error}", want.folder.display());
    }
    match built {
        Ok(_) if !map::needs_building(&want.folder, &want.source) => states[next] = MapStatus::Ready,
        // Built, yet not from what it was asked for (what it's built from changed meanwhile): not built again
        // and again at once, but as a failure, tried again once it changes.
        Ok(_) => {
            let error = "the map built isn't the one asked for".to_owned();
            failed.insert(want.folder.clone(), (want.source.clone(), error.clone()));
            states[next] = MapStatus::Failed { error };
        }
        // The game started meanwhile: once it's closed.
        Err(map::BuildError::GameRunning) => {
            states[next] = MapStatus::WaitingForGame;
            return (update(&states), false);
        }
        // Stopped at the ceiling: a failure, tried again once what it's built from changes (not again and
        // again).
        Err(map::BuildError::Export(ExportError::Stopped)) => {
            let error =
                format!("it needed more than {} of memory, more than WestPlay lets a map take", gigabytes(ceiling));
            log!("map {}: {error}", want.folder.display());
            failed.insert(want.folder.clone(), (want.source.clone(), error.clone()));
            states[next] = MapStatus::Failed { error };
        }
        Err(error) => {
            let error = error.to_string();
            log!("map {}: {error}", want.folder.display());
            failed.insert(want.folder.clone(), (want.source.clone(), error.clone()));
            states[next] = MapStatus::Failed { error };
        }
    }
    // Asked for again: done (or, failing, kept as a failure).
    again.remove(&want.folder);
    (update(&states), true)
}

/// The memory WestPlay has taken (its private bytes, what it holds whether in RAM or paged out).
fn process_memory() -> Option<u64> {
    use windows::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
    };
    use windows::Win32::System::Threading::GetCurrentProcess;
    let mut counters =
        PROCESS_MEMORY_COUNTERS_EX { cb: size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32, ..Default::default() };
    // SAFETY: `counters` is a local with its size set, read as the larger form the size says it is.
    unsafe {
        GetProcessMemoryInfo(GetCurrentProcess(), (&raw mut counters).cast::<PROCESS_MEMORY_COUNTERS>(), counters.cb)
    }
    .ok()?;
    Some(counters.PrivateUsage as u64)
}

fn gigabytes(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / (1u64 << 30) as f64)
}

/// The most memory a build may take on a PC with `total` memory, bytes: three fifths of it, and at least
/// [`CEILING_AT_LEAST`] (a PC with less than that pages to build a map anyway).
fn memory_ceiling(total: u64) -> u64 {
    (total / 5 * 3).max(CEILING_AT_LEAST)
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
    fn stops_a_build_taking_more_than_three_fifths_of_the_pc() {
        let gb = 1u64 << 30;
        assert_eq!(memory_ceiling(32 * gb), 32 * gb / 5 * 3);
        assert_eq!(memory_ceiling(16 * gb), 16 * gb / 5 * 3);
        // A small PC: at least 4 GB (its builds page, but finish).
        assert_eq!(memory_ceiling(4 * gb), 4 * gb);
    }

    #[test]
    fn logs_the_memory_a_build_takes() {
        // This process's own: something, and less than the PC has.
        let taken = process_memory().unwrap();
        assert!(taken > 0 && taken < memory().unwrap().1, "{taken}");
        assert_eq!(gigabytes(6 * (1 << 30) + (1 << 29)), "6.5 GB");
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
        set_up.wait_for_another(&SetUp::default(), Duration::from_secs(10));
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(set_up.get(), Some(PathBuf::from("game")));
        thread.join().unwrap();
    }

    #[test]
    fn looks_at_once_when_the_player_asks_to_prepare_the_maps_again() {
        let set_up = Arc::new(SetUpGame::new(Some(PathBuf::from("game"))));
        let known = set_up.state().clone();
        let asker = set_up.clone();
        let started = std::time::Instant::now();
        let thread = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            asker.prepare_maps_again();
        });
        set_up.wait_for_another(&known, Duration::from_secs(10));
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(set_up.state().asked_again, 1);
        thread.join().unwrap();
    }

    fn a_want(dirs: &[&str], folder: &str, allowed: bool) -> Want {
        Want {
            profile: profiles::Profile::new_player(),
            dirs: dirs.iter().map(PathBuf::from).collect(),
            folder: PathBuf::from(folder),
            source: serde_json::json!({ "mods": [] }),
            language: None,
            allowed,
            truckersmp: false,
        }
    }

    #[test]
    fn removes_the_maps_no_profile_wants() {
        let maps = std::env::temp_dir().join(format!("westplay-unused-maps-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&maps);
        let map = |name: &str| {
            let folder = maps.join(name);
            std::fs::create_dir_all(folder.join("tiles-1")).unwrap();
            std::fs::write(folder.join("map.json"), "{}").unwrap();
            folder
        };
        // Two maps wanted (a profile's, TruckersMP's), one a truck mod once made (unwanted), a folder named as a
        // map with no manifest, another of the player's, TruckersMP's record.
        let (played, truckersmp, old) =
            (map("europe-cbf29ce484222325"), map("europe-6350e2ab6a0e4959"), map("europe-9d0794bb7c00b91b"));
        std::fs::create_dir_all(maps.join("europe-0123456789abcdef")).unwrap();
        std::fs::create_dir_all(maps.join("my maps")).unwrap();
        // One an earlier removal left part of (its manifest gone, tiles left), and one renamed on its way out.
        let half = maps.join("europe-6c17f8cb18ac028c");
        std::fs::create_dir_all(half.join("tiles-2").join("14")).unwrap();
        let leaving = maps.join("europe-1111111111111111.removing");
        std::fs::create_dir_all(leaving.join("tiles-3")).unwrap();
        std::fs::write(maps.join(TRUCKERSMP_RECORD), "[]").unwrap();
        let wants = [
            a_want(&["profiles/57"], played.to_str().unwrap(), true),
            a_want(&["TruckersMP"], truckersmp.to_str().unwrap(), true),
        ];
        // No profile found yet (a new player's map only): nothing removed.
        let new_player = [a_want(&[""], played.to_str().unwrap(), true)];
        remove_unused(&maps, &new_player, &mut HashSet::new());
        assert!(old.is_dir());
        remove_unused(&maps, &wants, &mut HashSet::new());
        assert!(!old.exists() && !half.exists() && !leaving.exists());
        assert!(!maps.join("europe-9d0794bb7c00b91b.removing").exists());
        assert!(played.join("tiles-1").is_dir() && truckersmp.join("map.json").is_file());
        assert!(maps.join("europe-0123456789abcdef").is_dir() && maps.join("my maps").is_dir());
        assert!(maps.join(TRUCKERSMP_RECORD).is_file());
        let _ = std::fs::remove_dir_all(&maps);
    }

    #[test]
    fn every_profile_has_its_maps_state() {
        // The played profile's map, one two profiles share, and a new player's (no profile folder).
        let wants = [a_want(&["p/a"], "maps/europe-1", true), a_want(&["p/b", "p/c"], "maps/europe-2", true)];
        let states = [MapStatus::Ready, MapStatus::Queued];
        let listed: Vec<(String, MapStatus)> =
            profile_maps(&wants, &states).into_iter().map(|each| (each.dir, each.status)).collect();
        assert_eq!(
            listed,
            [
                ("p/a".to_owned(), MapStatus::Ready),
                ("p/b".to_owned(), MapStatus::Queued),
                ("p/c".to_owned(), MapStatus::Queued),
            ]
        );
        assert!(profile_maps(&[a_want(&[""], "maps/europe-1", true)], &[MapStatus::Ready]).is_empty());
    }

    #[test]
    fn a_failed_map_is_tried_again_only_once_what_its_built_from_changes() {
        let mut want = a_want(&["p/a"], "maps-that-are-not-here/europe-1", true);
        let mut failed = Failures::new();
        let none = HashSet::new();
        assert_eq!(state_of(&want, &failed, &none), MapStatus::Queued);
        failed.insert(want.folder.clone(), (want.source.clone(), "sector version 903".into()));
        assert_eq!(state_of(&want, &failed, &none), MapStatus::Failed { error: "sector version 903".into() });
        // The player asks for it again: built again, the failure aside.
        assert_eq!(state_of(&want, &failed, &HashSet::from([want.folder.clone()])), MapStatus::Queued);
        // A mod updated: built again.
        want.source = serde_json::json!({ "mods": ["updated"] });
        assert_eq!(state_of(&want, &failed, &none), MapStatus::Queued);
        // HVT's: never, even asked for again.
        let hvt = a_want(&["p/h"], "maps/hvtmap-1", false);
        assert_eq!(state_of(&hvt, &failed, &HashSet::from([hvt.folder.clone()])), MapStatus::NotAllowed);
    }

    #[test]
    fn tells_what_the_build_is_doing() {
        let at = |stage, done| MapStage::of(&Progress { stage, done, size: None });
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
