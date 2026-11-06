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
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, SystemTime};

use westplay_mapdata::export::{Progress, Stage};
use westplay_mapdata::{locale, save};
use westplay_setup::game::{Game, Kind};
use westplay_setup::{map, map_builder, places, profiles, truckersmp};

use crate::map_process;
use crate::settings::SharedSettings;

/// The packs the last TruckersMP session in the game `kind` mounted, kept in the `maps` folder (truckersmp.rs):
/// ETS2's as before ATS, ATS's beside it; a ProMods server's session's apart (`promods`), so each of the two maps
/// stays as it was while the player goes from one kind of server to the other.
fn truckersmp_record(kind: Kind, promods: bool) -> &'static str {
    match (kind, promods) {
        (Kind::Ets2, false) => "truckersmp.json",
        (Kind::Ats, false) => "truckersmp-ats.json",
        (Kind::Ets2, true) => "truckersmp-promods.json",
        (Kind::Ats, true) => "truckersmp-ats-promods.json",
    }
}

/// How often to look again: the profile played, the game running or not.
const CHECK_EVERY: Duration = Duration::from_secs(10);
/// How often the profile's map mods are looked at again anyway (a Workshop mod updated changes no profile).
const RECHECK_MODS: Duration = Duration::from_secs(5 * 60);

/// The memory a build wants free, bytes: the base game's whole map peaked at 2.93 GB (2026-10-17, measured
/// as it was built), and a margin for the rest of the PC.
pub const MEMORY_NEEDED: u64 = 3_500_000_000;

// No build is stopped for the memory it takes (owner 2026-11-06: the biggest map mods need more than any share of a
// PC WestPlay would set; Windows lends a build the disk). Prepare Maps Faster fills four fifths of it with blocks
// drawn at once, and no more (westplay_mapdata::threads::room_on).

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
    /// Its game: `ets2` or `ats`.
    pub game: Kind,
    pub dir: String,
    /// Its map's folder, by name: profiles sharing a map share it (what [`SetUpGame::prepare_maps_again`] asks by).
    pub folder: String,
    pub status: MapStatus,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub left_out: Vec<String>,
    /// TruckersMP's map (its `dir` TruckersMP's installation), not a profile's.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub truckersmp: bool,
    /// TruckersMP's ProMods servers' map, beside its other servers'.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub promods: bool,
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

/// The games WestPlay is set up in (their folders: one a game, ETS2's and ATS's), set by the companion's setup, and
/// the player's asks to prepare the maps again (the companion's Map page), from any thread: the watch looks at once
/// at either.
#[derive(Debug, Default)]
pub struct SetUpGame {
    state: Mutex<SetUp>,
    changed: Condvar,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct SetUp {
    dirs: Vec<PathBuf>,
    /// How many times the player has asked to prepare the maps again.
    asked_again: u64,
    /// The maps asked for since the watch last looked: `None` once it has.
    asked: Option<Asked>,
}

/// The maps the player asked to prepare again.
#[derive(Clone, Debug, PartialEq)]
enum Asked {
    All,
    /// Those in these folders (their names: `ProfileMap::folder`).
    Maps(std::collections::BTreeSet<String>),
}

impl Asked {
    /// This ask and `other`, made before the watch looked.
    fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::Maps(mut these), Self::Maps(those)) => {
                these.extend(those);
                Self::Maps(these)
            }
            _ => Self::All,
        }
    }

    fn has(&self, folder: &Path) -> bool {
        match self {
            Self::All => true,
            Self::Maps(names) => folder.file_name().is_some_and(|name| names.contains(&*name.to_string_lossy())),
        }
    }
}

impl SetUpGame {
    pub fn new(dirs: Vec<PathBuf>) -> Self {
        Self { state: Mutex::new(SetUp { dirs, asked_again: 0, asked: None }), changed: Condvar::new() }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, SetUp> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn set(&self, dirs: Vec<PathBuf>) {
        self.state().dirs = dirs;
        self.changed.notify_all();
    }

    pub fn get(&self) -> Vec<PathBuf> {
        self.state().dirs.clone()
    }

    /// Asks for the maps in `folders` (their names: `ProfileMap::folder`; `None`: every map) to be prepared again,
    /// failed ones included: each is built anew in turn (as ever, while the game is closed), the cab keeping the
    /// map it has meanwhile (owner 2026-11-06: a player with many profiles' maps picks those to draw again).
    pub fn prepare_maps_again(&self, folders: Option<Vec<String>>) {
        let asked = folders.map_or(Asked::All, |folders| Asked::Maps(folders.into_iter().collect()));
        let mut state = self.state();
        state.asked_again += 1;
        state.asked = Some(match state.asked.take() {
            Some(before) => before.and(asked),
            None => asked,
        });
        drop(state);
        self.changed.notify_all();
    }

    /// The maps asked for since the last look, forgotten as they're taken.
    fn take_asked(&self) -> Option<Asked> {
        self.state().asked.take()
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

/// The game played and its profile played (its map and its map mods): what the cab's map is built from, and so
/// what the route's junctions are read from (route.rs). `None` until worked out, and without a game.
pub type PlayedMap = Arc<Mutex<Option<(Game, profiles::Profile)>>>;

impl MapWatch {
    /// Watches the maps of the profiles in the games set up (`game`), building each into `maps` when it needs it
    /// and the games are closed.
    /// How hard each build works is read from `settings` as it starts (`MapSettings::fast`); each is built by the
    /// map builder `builder` (`map_process`), or in WestPlay's own process without one.
    pub fn start(maps: PathBuf, game: Arc<SetUpGame>, settings: Arc<SharedSettings>, builder: Option<PathBuf>) -> Self {
        let (sender, updates) = mpsc::channel();
        let played = PlayedMap::default();
        let telling = played.clone();
        let _ = std::thread::Builder::new()
            .name("westplay-maps".into())
            .spawn(move || watch(&maps, &game, &settings, builder.as_deref(), &sender, &telling));
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

fn watch(
    maps: &Path,
    set_up: &SetUpGame,
    settings: &SharedSettings,
    builder: Option<&Path>,
    sender: &Sender<MapUpdate>,
    played: &PlayedMap,
) {
    let userdata = places::steam().map(|steam| steam.join("userdata"));
    // Each set-up game's own folder in Documents, looked for once a game.
    let mut documents_of: HashMap<Kind, Option<PathBuf>> = HashMap::new();
    let mut sent: Option<MapUpdate> = None;
    // Each game's maps as last worked out (by the game's folder), and from what.
    let mut worked_out: HashMap<PathBuf, (Basis, Vec<Want>)> = HashMap::new();
    // A game's maps that couldn't be worked out (a panic), from what, and why.
    let mut wants_failed: HashMap<PathBuf, (Basis, String)> = HashMap::new();
    let mut failed = Failures::new();
    // Maps the player asked to prepare again (by folder), each built anew in its turn, and the last ask seen.
    let mut again: HashSet<PathBuf> = HashSet::new();
    let mut asked_again = 0;
    // Unused maps not removed whole yet, told once (tried again at each look).
    let mut not_removed: HashSet<PathBuf> = HashSet::new();
    loop {
        let set_up_in = set_up.state().clone();
        let games: Vec<(Game, PathBuf)> = set_up_in
            .dirs
            .iter()
            .filter_map(|dir| Game::recognise(dir))
            .filter_map(|game| {
                let documents = documents_of.entry(game.kind).or_insert_with(|| places::game_documents(game.kind));
                let documents = documents.clone()?;
                Some((game, documents))
            })
            .collect();
        let games = played_first(games, userdata.as_deref());
        let asked_now = set_up_in.asked_again != asked_again;
        let mut asked = None;
        if asked_now {
            asked_again = set_up_in.asked_again;
            asked = set_up.take_asked();
            // Everything looked at afresh; the maps asked for, their failures forgotten.
            worked_out.clear();
            wants_failed.clear();
            if let Some(asked) = &asked {
                failed.retain(|folder, _| !asked.has(folder));
            }
        }
        // Every game's maps in one list, the played game's first (its played profile's map the one the cab shows);
        // whether each game's were worked out, and why the played game's couldn't be.
        let mut wants: Vec<Want> = Vec::new();
        let mut all_worked_out = true;
        let mut played_failed: Option<String> = None;
        for (index, (game, documents)) in games.iter().enumerate() {
            let basis = basis(game, documents, userdata.as_deref());
            // A TruckersMP session's packs, kept for its map until the next session: once the game has closed, its
            // log whole. While it starts, the log has only the packs mounted so far (23 of 24), which made another
            // map of TruckersMP's, the one it had removed, and the whole one built again at every quit (owner
            // 2026-11-03).
            if let Some(session) = &basis.session
                && !game.kind.running()
            {
                let promods = truckersmp::promods(session);
                if truckersmp::remember(&maps.join(truckersmp_record(game.kind, promods)), session) {
                    let server = if promods { "a ProMods server" } else { "a server without ProMods" };
                    log!("TruckersMP: its last session ({server}) mounted {} packs", session.len());
                }
            }
            match work_out(game, documents, userdata.as_deref(), basis, maps, &mut worked_out, &mut wants_failed) {
                Ok(game_wants) => wants.extend(game_wants),
                // A game whose maps couldn't be worked out leaves the others' to be made (owner 2026-10-18: one
                // profile's map failing must not stop the others').
                Err(error) => {
                    all_worked_out = false;
                    if index == 0 {
                        played_failed = Some(error);
                    }
                }
            }
        }
        // Those of games set up no more (or gone) forgotten.
        worked_out.retain(|dir, _| games.iter().any(|(game, _)| &game.dir == dir));
        wants_failed.retain(|dir, _| games.iter().any(|(game, _)| &game.dir == dir));
        let (update, more) = if games.is_empty() {
            *played.lock().unwrap_or_else(|e| e.into_inner()) = None;
            let update = MapUpdate { folder: None, built: None, status: MapStatus::NoGame, profiles: Vec::new() };
            (update, false)
        } else if let Some(error) = played_failed {
            // The played game's failed: said so (the Companion's map step and its Prepare Maps Again have a state
            // to show), the cab keeping the map it has.
            let (folder, built) = sent.as_ref().map_or((None, None), |sent| (sent.folder.clone(), sent.built));
            let status = MapStatus::Failed { error };
            (MapUpdate { folder, built, status, profiles: Vec::new() }, false)
        } else {
            let (game, profile) = (&wants[0].game, &wants[0].profile);
            let mut shared = played.lock().unwrap_or_else(|e| e.into_inner());
            if shared.as_ref().is_none_or(|(known_game, known)| known_game != game || known != profile) {
                *shared = Some((game.clone(), profile.clone()));
            }
            drop(shared);
            if let Some(asked) = &asked {
                again.extend(
                    wants.iter().filter(|want| want.allowed && asked.has(&want.folder)).map(|want| want.folder.clone()),
                );
            }
            let fast = settings.get().map.fast;
            let (mut update, more) = carry_out(&wants, maps, fast, builder, &mut failed, &mut again, sender, &mut sent);
            // A game running that WestPlay isn't set up in (ATS beside an ETS2 set up before it, its plugin put
            // there by hand): no map in its cab, never the other game's.
            if Kind::ALL.iter().any(|kind| kind.running() && !games.iter().any(|(game, _)| game.kind == *kind)) {
                update.folder = None;
                update.built = None;
            }
            (update, more)
        };
        if sent.as_ref() != Some(&update) {
            if sender.send(update.clone()).is_err() {
                return;
            }
            sent = Some(update);
        }
        // The maps no one wants any more gone (none is being built between two looks), once the maps' state is
        // told: a big map takes minutes to remove, and until the first state the companion has none to show
        // (its Prepare Maps Again hidden, 2.0). Only once every game's maps are known: another game's maps are
        // wanted too.
        if !games.is_empty() && all_worked_out {
            remove_unused(maps, &wants, &mut not_removed);
        }
        // A map just built: on to the next at once.
        if !more {
            set_up.wait_for_another(&set_up_in, CHECK_EVERY);
        }
    }
}

/// The games set up (each with its folder in Documents), the one played first: the one running, else the one
/// saved in last.
fn played_first(mut games: Vec<(Game, PathBuf)>, userdata: Option<&Path>) -> Vec<(Game, PathBuf)> {
    games.sort_by_cached_key(|(game, documents)| {
        let saved = save::latest(documents, userdata, game.kind.steam_app())
            .and_then(|save| std::fs::metadata(save).ok()?.modified().ok());
        (std::cmp::Reverse(game.kind.running()), std::cmp::Reverse(saved))
    });
    games
}

/// The maps `game`'s profiles want ([`wants`]): as last worked out while what they're worked out from (`basis`)
/// is the same, else worked out anew (kept in `worked_out`); why not, if that failed (a panic, kept in
/// `wants_failed`: not tried again until something of it changes).
fn work_out(
    game: &Game,
    documents: &Path,
    userdata: Option<&Path>,
    basis: Basis,
    maps: &Path,
    worked_out: &mut HashMap<PathBuf, (Basis, Vec<Want>)>,
    wants_failed: &mut HashMap<PathBuf, (Basis, String)>,
) -> Result<Vec<Want>, String> {
    if let Some((known, wants)) = worked_out.get(&game.dir)
        && *known == basis
    {
        return Ok(wants.clone());
    }
    if let Some((known, error)) = wants_failed.get(&game.dir)
        && *known == basis
    {
        return Err(error.clone());
    }
    // Worked out anew (each profile's map mods opened: a long list takes a while), timed in the log, so a look
    // that never ends shows where it stopped. A panic (a mod's archive read wrong, 2.0.3) is this look failing,
    // not the maps' watch stopping for good.
    let started = std::time::Instant::now();
    log!("maps: working out each profile's map");
    let worked =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| wants(game, documents, userdata, &basis, maps)));
    match worked {
        Ok(wants) => {
            log!(
                "maps: {} map(s) for the profiles, worked out in {:.1} s",
                wants.len(),
                started.elapsed().as_secs_f32()
            );
            wants_failed.remove(&game.dir);
            worked_out.insert(game.dir.clone(), (basis, wants.clone()));
            Ok(wants)
        }
        Err(panic) => {
            let error = format!("couldn't read the profiles' mods ({})", panic_message(&*panic));
            log!("maps: {error}");
            worked_out.remove(&game.dir);
            wants_failed.insert(game.dir.clone(), (basis, error.clone()));
            Err(error)
        }
    }
}

fn basis(game: &Game, documents: &Path, userdata: Option<&Path>) -> Basis {
    let profile = save::latest(documents, userdata, game.kind.steam_app())
        .and_then(|save| save::profile_of(&save)?.parent().map(Path::to_path_buf));
    let written = |path: &Path| std::fs::metadata(path).and_then(|m| m.modified()).ok();
    Basis {
        game: game.dir.clone(),
        selected: profiles::selected(documents),
        profile,
        profiles: profile_dirs(game.kind, documents, userdata)
            .into_iter()
            .map(|dir| {
                let at = written(&dir.join("profile.sii"));
                (dir, at)
            })
            .collect(),
        game_written: written(&game.dir.join(game.kind.executable())),
        truckersmp: truckersmp::installation().map(|installation| {
            let packs = truckersmp::packs(&installation, game.kind).into_iter().map(|pack| {
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

/// Every profile's folder of the game `kind`, as `profiles::find` looks for them (its Documents folder's, and
/// Steam Cloud's under Steam's `userdata`), sorted.
fn profile_dirs(kind: Kind, documents: &Path, userdata: Option<&Path>) -> Vec<PathBuf> {
    let mut folders = vec![documents.join("profiles"), documents.join("steam_profiles")];
    for user in userdata.into_iter().flat_map(|dir| std::fs::read_dir(dir).into_iter().flatten().flatten()) {
        folders.push(user.path().join(kind.steam_app()).join("remote").join("profiles"));
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

/// A map some profiles want: its game, its folder, what it's built from, a profile to build it for, and every
/// profile that plays it.
#[derive(Clone)]
struct Want {
    game: Game,
    profile: profiles::Profile,
    dirs: Vec<PathBuf>,
    folder: PathBuf,
    source: serde_json::Value,
    language: Option<String>,
    allowed: bool,
    /// TruckersMP's map, not a profile's.
    truckersmp: bool,
    /// TruckersMP's ProMods servers' map.
    promods: bool,
}

/// The maps the profiles want, the played profile's first (the profile last saved in; before any save, the
/// most recently made one; before any profile, the one a new player makes), then the others', the most
/// recently played first. Profiles playing the same map with the same map mods share one.
fn wants(game: &Game, documents: &Path, userdata: Option<&Path>, basis: &Basis, maps: &Path) -> Vec<Want> {
    let mut all = profiles::find(game.kind, documents, userdata, Some(game));
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
        None => profiles::Profile::new_player(game.kind),
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
            game: game.clone(),
            source: map::source(game, &profile, language.as_deref()),
            allowed: !profile.plays_hvt_map(),
            folder,
            profile,
            language: language.clone(),
            truckersmp: false,
            promods: false,
        });
    }
    // TruckersMP's, one for every profile: its other servers' map, and its ProMods servers' once a session was
    // on one; the one the game's last session played first (TruckersMP's), else right after the played
    // profile's.
    if let Some((installation, on_disk)) = &basis.truckersmp {
        let last = basis.session.as_deref().map(truckersmp::promods);
        let made = |packs: Vec<PathBuf>, promods: bool| {
            let profile = truckersmp::profile(game.kind, installation, &packs);
            Want {
                game: game.clone(),
                dirs: vec![installation.clone()],
                source: map::source(game, &profile, language.as_deref()),
                folder: map::folder(maps, &profile),
                allowed: true,
                profile,
                language: language.clone(),
                truckersmp: true,
                promods,
            }
        };
        let others = truckersmp::remembered(&maps.join(truckersmp_record(game.kind, false)))
            .unwrap_or_else(|| on_disk.iter().map(|(pack, _)| pack.clone()).collect());
        let mut truckersmp = vec![made(others, false)];
        if let Some(packs) = truckersmp::remembered(&maps.join(truckersmp_record(game.kind, true))) {
            truckersmp.push(made(packs, true));
        }
        // The last session's first.
        truckersmp.sort_by_key(|want| Some(want.promods) != last);
        let at = if last.is_some() { 0 } else { 1 }.min(wants.len());
        wants.splice(at..at, truckersmp);
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
            log!("map {name}: no profile plays it any more; removing it");
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
            let folder = want.folder.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
            want.dirs.iter().filter(|dir| !dir.as_os_str().is_empty()).map(move |dir| ProfileMap {
                game: want.game.kind,
                dir: dir.to_string_lossy().into_owned(),
                folder: folder.clone(),
                status: status.clone(),
                left_out: left_out.clone(),
                truckersmp: want.truckersmp,
                promods: want.promods,
            })
        })
        .collect()
}

/// Shows the played profile's map if it's built, and builds the next map that isn't (the played profile's
/// first) if the game is closed, or waits. Returns what to show and say, and whether it built one (so the next
/// is looked at at once).
#[allow(clippy::too_many_arguments)]
fn carry_out(
    wants: &[Want],
    maps: &Path,
    fast: bool,
    builder: Option<&Path>,
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
    if westplay_setup::game::running() {
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
        "map {folder}: building for {:?} ({}, map mods {mods:?}){}, {}; {} free",
        want.profile.name,
        want.profile.map,
        if fast { ", faster: every thread, blocks at once up to four fifths of the memory" } else { "" },
        match (builder.is_some(), fast) {
            (true, true) => "in the map builder, mimalloc its allocator",
            (true, false) => "in the map builder",
            (false, _) => "in WestPlay itself (no map builder)",
        },
        memory().map_or("?".into(), |(free, total)| format!("{} of {}", gigabytes(free), gigabytes(total)))
    );
    let mut logged_at = -1.0f32;
    let mut logged_stage = None;
    // `memory`: the build's, as it says (the builder's own process, or WestPlay's).
    let mut tell = |progress: Progress, memory: Option<u64>| {
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
                memory.map_or("?".into(), gigabytes)
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
    tell(Progress { stage: Stage::Reading, done: 0.0, size: None }, None);
    let request = map_builder::Request {
        game: want.game.clone(),
        profile: want.profile.clone(),
        language: want.language.clone(),
        maps: maps.to_owned(),
        fast,
    };
    // A build failing (a mod read wrong, a panic, the builder crashing) is this map failing, as any other error is:
    // the others are still made (owner 2026-10-18: one profile's map failing must not stop the others').
    let ended = map_process::build(builder, &request, &mut |said| {
        if let map_builder::Said::Progress { stage, done, size, memory } = said
            && let Some(progress) = map_builder::progress_of(&stage, done, size)
        {
            tell(progress, memory);
        }
    });
    match ended {
        map_builder::Said::Built(outcome) => {
            for left in &outcome.left_out {
                log!(
                    "map {}: built without {}, which couldn't be read: {}",
                    want.folder.display(),
                    left.name,
                    left.error
                );
            }
            if let Some(error) = &outcome.time_zones_unread {
                log!(
                    "map {}: built without time zones (the cab's clock the game's time): {error}",
                    want.folder.display()
                );
            }
            match outcome.not_owned {
                Some((guarded, elsewhere)) => log!(
                    "map {}: only what the game has: {guarded} items of map DLCs it hasn't and {elsewhere} in states it hasn't left out",
                    want.folder.display()
                ),
                None if want.game.kind == Kind::Ats => log!(
                    "map {}: drawn whole: the game's list of its map DLCs wasn't found in its executable",
                    want.folder.display()
                ),
                None => {}
            }
            if let Some((fit, used)) = outcome.world {
                log!(
                    "map {}: the game's world picture fits {:.1}% of its roads ({})",
                    want.folder.display(),
                    100.0 * fit,
                    if used { "used for its sea and beyond" } else { "not used: sea by the roads alone" }
                );
            }
            if map::needs_building(&want.folder, &want.source) {
                // Built, yet not from what it was asked for (what it's built from changed meanwhile): not built again
                // and again at once, but as a failure, tried again once it changes.
                let error = "the map built isn't the one asked for".to_owned();
                failed.insert(want.folder.clone(), (want.source.clone(), error.clone()));
                states[next] = MapStatus::Failed { error };
            } else {
                states[next] = MapStatus::Ready;
            }
        }
        // The game started meanwhile: once it's closed.
        map_builder::Said::Failed(map_builder::Failure::GameRunning) => {
            states[next] = MapStatus::WaitingForGame;
            return (update(&states), false);
        }
        map_builder::Said::Failed(map_builder::Failure::Error { error }) => {
            log!("map {}: {error}", want.folder.display());
            failed.insert(want.folder.clone(), (want.source.clone(), error.clone()));
            states[next] = MapStatus::Failed { error };
        }
        map_builder::Said::Progress { .. } => unreachable!("a build ends built or failed"),
    }
    // Asked for again: done (or, failing, kept as a failure).
    again.remove(&want.folder);
    (update(&states), true)
}

/// What a panic said (its message, if it gave one as text).
pub(crate) fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|text| text.to_string())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "an error WestPlay didn't expect".into())
}

/// The memory WestPlay has taken (its private bytes, what it holds whether in RAM or paged out).
pub(crate) fn process_memory() -> Option<u64> {
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

/// The PC's memory in all (bytes).
pub(crate) fn total_memory() -> Option<u64> {
    memory().map(|(_, total)| total)
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
    fn a_panic_caught_says_what_it_said() {
        let caught = |run: fn()| panic_message(&*std::panic::catch_unwind(run).unwrap_err());
        // The player's (2.0.3): a clamp's own message, formatted.
        assert_eq!(
            caught(|| panic!("min > max. min = {}, max = {}", 65536, 42304)),
            "min > max. min = 65536, max = 42304"
        );
        assert_eq!(caught(|| panic!("a plain message")), "a plain message");
        assert_eq!(caught(|| std::panic::panic_any(7)), "an error WestPlay didn't expect");
    }

    #[test]
    fn tells_the_cab_its_maps_folder() {
        assert_eq!(map_message(Some("europe-1"), Some(7)), r#"{"built":7,"folder":"europe-1","type":"map"}"#);
        assert_eq!(map_message(None, None), r#"{"built":null,"folder":null,"type":"map"}"#);
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
            setter.set(vec![PathBuf::from("game")]);
        });
        set_up.wait_for_another(&SetUp::default(), Duration::from_secs(10));
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(set_up.get(), [PathBuf::from("game")]);
        thread.join().unwrap();
    }

    #[test]
    fn looks_at_once_when_the_player_asks_to_prepare_the_maps_again() {
        let set_up = Arc::new(SetUpGame::new(vec![PathBuf::from("game")]));
        let known = set_up.state().clone();
        let asker = set_up.clone();
        let started = std::time::Instant::now();
        let thread = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            asker.prepare_maps_again(None);
        });
        set_up.wait_for_another(&known, Duration::from_secs(10));
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(set_up.state().asked_again, 1);
        thread.join().unwrap();
    }

    #[test]
    fn asks_made_before_a_look_add_up() {
        let set_up = SetUpGame::new(Vec::new());
        let names = |names: &[&str]| Some(names.iter().map(|name| name.to_string()).collect::<Vec<_>>());
        set_up.prepare_maps_again(names(&["europe-1"]));
        set_up.prepare_maps_again(names(&["europe-2"]));
        let asked = set_up.take_asked().unwrap();
        assert!(asked.has(Path::new("maps/europe-1")) && asked.has(Path::new("maps/europe-2")));
        assert!(!asked.has(Path::new("maps/europe-3")));
        assert_eq!(set_up.take_asked(), None, "taken once");
        // Every map asked for, with some: every map.
        set_up.prepare_maps_again(names(&["europe-1"]));
        set_up.prepare_maps_again(None);
        assert!(set_up.take_asked().unwrap().has(Path::new("maps/europe-3")));
        assert_eq!(set_up.state().asked_again, 4);
    }

    fn a_want(dirs: &[&str], folder: &str, allowed: bool) -> Want {
        Want {
            game: Game {
                kind: Kind::Ets2,
                dir: PathBuf::from("game"),
                found_by: westplay_setup::game::FoundBy::Steam,
                version: None,
                dlcs: vec![],
            },
            profile: profiles::Profile::new_player(Kind::Ets2),
            dirs: dirs.iter().map(PathBuf::from).collect(),
            folder: PathBuf::from(folder),
            source: serde_json::json!({ "mods": [] }),
            language: None,
            allowed,
            truckersmp: false,
            promods: false,
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
        std::fs::write(maps.join(truckersmp_record(Kind::Ets2, false)), "[]").unwrap();
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
        assert!(maps.join(truckersmp_record(Kind::Ets2, false)).is_file());
        let _ = std::fs::remove_dir_all(&maps);
    }

    #[test]
    fn every_profile_has_its_maps_state() {
        // The played profile's map, one two profiles share, and a new player's (no profile folder).
        // And an ATS profile's, after ETS2's.
        let mut ats = a_want(&["ats/d"], "maps/usa-1", true);
        ats.game.kind = Kind::Ats;
        let wants = [a_want(&["p/a"], "maps/europe-1", true), a_want(&["p/b", "p/c"], "maps/europe-2", true), ats];
        let states = [MapStatus::Ready, MapStatus::Queued, MapStatus::Queued];
        let listed: Vec<(Kind, String, MapStatus)> =
            profile_maps(&wants, &states).into_iter().map(|each| (each.game, each.dir, each.status)).collect();
        assert_eq!(
            listed,
            [
                (Kind::Ets2, "p/a".to_owned(), MapStatus::Ready),
                (Kind::Ets2, "p/b".to_owned(), MapStatus::Queued),
                (Kind::Ets2, "p/c".to_owned(), MapStatus::Queued),
                (Kind::Ats, "ats/d".to_owned(), MapStatus::Queued),
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
