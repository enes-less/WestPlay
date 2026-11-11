//! Finding the games: every install of Euro Truck Simulator 2 and American Truck Simulator on the PC (Steam's,
//! in any of its libraries, and others Windows lists as installed), each with its version (its executable's)
//! and its DLCs. One WestPlay for both (owner 2026-10-28): what it does in one, it does in the other.

use std::fmt;
use std::path::{Path, PathBuf};

use windows::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VS_FIXEDFILEINFO, VerQueryValueW,
};
use windows::core::{HSTRING, w};

use crate::{places, vdf};

/// Which of SCS's truck games: they share an engine, a plugin SDK and their files' formats, and differ in what
/// they're called, where they live, and their map.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Ets2,
    Ats,
}

impl Kind {
    pub const ALL: [Kind; 2] = [Kind::Ets2, Kind::Ats];

    /// Its name: its folder's in Documents and in a Steam library, and Windows' list of programs'.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Ets2 => "Euro Truck Simulator 2",
            Kind::Ats => "American Truck Simulator",
        }
    }

    /// Its app id on Steam (its manifest, its Workshop folder, its Steam Cloud folder, its launch options).
    pub fn steam_app(self) -> &'static str {
        match self {
            Kind::Ets2 => "227300",
            Kind::Ats => "270880",
        }
    }

    /// Its executable, in its folder.
    pub fn executable(self) -> &'static str {
        match self {
            Kind::Ets2 => "bin/win_x64/eurotrucks2.exe",
            Kind::Ats => "bin/win_x64/amtrucks.exe",
        }
    }

    /// Its process, as Windows names it (TruckersMP's client runs the same).
    fn process(self) -> &'static str {
        match self {
            Kind::Ets2 => "eurotrucks2.exe",
            Kind::Ats => "amtrucks.exe",
        }
    }

    /// The game's id in the plugin SDK (`scs_telemetry_init`'s `game_id`).
    pub fn sdk_id(self) -> &'static str {
        match self {
            Kind::Ets2 => "eut2",
            Kind::Ats => "ats",
        }
    }

    /// Its base map's name (a profile's `map_path`: `/map/europe.mbd`).
    pub fn base_map(self) -> &'static str {
        match self {
            Kind::Ets2 => "europe",
            Kind::Ats => "usa",
        }
    }

    /// The time zone the SDK's game time is in, minutes from UTC: the game's clock is it plus the zone of the
    /// country the truck is in, less this (with the profile's time zones on). ETS2's central Europe's (seen
    /// 2026-10-21: an hour on in Finland); ATS's -360 (seen 2026-10-28: the SDK's 17:03 the game's 4:03 PM in
    /// Arizona, -420).
    pub fn sdk_time_zone(self) -> i32 {
        match self {
            Kind::Ets2 => 120,
            Kind::Ats => -360,
        }
    }

    /// The versions WestPlay has been played on ([`Version::matches`]). Others may well work (the plugin finds what
    /// it needs in the game by its shape), but haven't been tried. ETS2 1.62 is the owner's own (it was left out:
    /// 1.62 players were told WestPlay hadn't been tried with their game, owner 2026-11-10).
    pub fn tested_versions(self) -> &'static [&'static str] {
        match self {
            Kind::Ets2 => &["1.61.*", "1.62.*"],
            Kind::Ats => &["1.62.*"],
        }
    }

    /// The game the plugin says it's in (`PluginBlock::game`), if it's one of these.
    pub fn of_bridge(number: u32) -> Option<Kind> {
        match number {
            westplay_shared::GAME_ETS2 => Some(Kind::Ets2),
            westplay_shared::GAME_ATS => Some(Kind::Ats),
            _ => None,
        }
    }

    /// Whether a program Windows lists is this game (not its demo, nor the other game).
    pub fn is_program(self, name: &str) -> bool {
        let name = name.to_lowercase();
        name.contains(&self.name().to_lowercase()) && !name.contains("demo")
    }

    /// Whether this game is running (any install of it).
    pub fn running(self) -> bool {
        running_processes(&[self.process()])
    }
}

/// An install of a game. Serialized for the map builder's process (`map_builder`).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Game {
    pub kind: Kind,
    /// Its folder (with `base.scs`).
    pub dir: PathBuf,
    pub found_by: FoundBy,
    /// Its version, from its executable (`None`: it couldn't be read).
    pub version: Option<Version>,
    /// Its DLCs' archives, by name (`dlc_balkan_e`…), in order.
    pub dlcs: Vec<String>,
}

/// How an install was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FoundBy {
    /// In one of Steam's libraries.
    Steam,
    /// In Windows' list of installed programs (another store's, or SCS's own installer).
    Installed,
    /// The player chose its folder.
    Chosen,
}

/// The game's version: `1.61.1.9`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct Version(pub [u16; 4]);

impl Version {
    /// Whether a version pattern of the game's (`1.61.*`, `1.61.1.9`) takes in this version.
    pub fn matches(&self, pattern: &str) -> bool {
        let parts: Vec<&str> = pattern.trim().split('.').collect();
        parts.iter().enumerate().all(|(index, part)| {
            *part == "*" || (index < 4 && part.parse::<u16>().is_ok_and(|number| number == self.0[index]))
        }) && !parts.is_empty()
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [a, b, c, d] = self.0;
        write!(f, "{a}.{b}.{c}.{d}")
    }
}

impl Game {
    /// The game in `dir`, if it's an install of one: its archives and its executable are there (which says
    /// which game: [`Self::listed_as`]).
    pub fn at(dir: &Path, found_by: FoundBy) -> Option<Self> {
        Self::listed_as(dir, found_by, None)
    }

    /// The game in `dir`, `listed` as game `kind` (by its Steam manifest, or in Windows' programs) if it is.
    ///
    /// Which game it is: the listed one if its executable is there (Steam knows which game it installed), else
    /// the one whose executable is; a folder with both executables and no listing is the game whose base map
    /// its archives have (`map/europe.mbd` or `map/usa.mbd`). A player's ATS folder had ETS2's executable copied
    /// in (2026-11-07): taken by its executable for ETS2, it was left out of the ATS installs found, and set up as
    /// ETS2 when chosen by hand (ETS2's mod written, ATS's plugin finding none).
    pub fn listed_as(dir: &Path, found_by: FoundBy, listed: Option<Kind>) -> Option<Self> {
        if !dir.join("base.scs").is_file() {
            return None;
        }
        let present: Vec<Kind> = Kind::ALL.into_iter().filter(|kind| dir.join(kind.executable()).is_file()).collect();
        let kind = match (listed, &present[..]) {
            (Some(listed), _) if present.contains(&listed) => listed,
            (_, []) => return None,
            (_, [only]) => *only,
            _ => base_map_kind(dir)?,
        };
        let mut dlcs: Vec<String> = std::fs::read_dir(dir)
            .ok()?
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_str()?.to_owned();
                let dlc = name.strip_prefix("dlc_").is_some() && name.ends_with(".scs");
                dlc.then(|| name.trim_end_matches(".scs").to_owned())
            })
            .collect();
        dlcs.sort();
        let version = executable_version(&dir.join(kind.executable()));
        Some(Self { kind, dir: dir.to_path_buf(), found_by, version, dlcs })
    }

    /// The game in `dir`, found as [`find`] finds it (Steam's, or listed as installed) or else as chosen.
    pub fn recognise(dir: &Path) -> Option<Self> {
        let mut found = Kind::ALL.into_iter().flat_map(find);
        found.find(|game| same_folder(&game.dir, dir)).or_else(|| Self::at(dir, FoundBy::Chosen))
    }

    /// Whether WestPlay has been tried with this version of the game ([`Kind::tested_versions`]).
    pub fn tested(&self) -> bool {
        self.version.is_some_and(|version| self.kind.tested_versions().iter().any(|pattern| version.matches(pattern)))
    }

    /// Where Steam keeps the game's Workshop mods, for an install in a Steam library (however it was found:
    /// a folder chosen there too).
    pub fn workshop(&self) -> Option<PathBuf> {
        // `<library>/steamapps/common/<game>` → `<library>/steamapps/workshop/content/<its app>`.
        let common = self.dir.parent()?;
        let steamapps = common.parent()?;
        let named = |path: &Path, name: &str| path.file_name().is_some_and(|n| n.eq_ignore_ascii_case(name));
        if !named(common, "common") || !named(steamapps, "steamapps") {
            return None;
        }
        let workshop = steamapps.join("workshop").join("content").join(self.kind.steam_app());
        workshop.is_dir().then_some(workshop)
    }

    /// Where the plugin goes.
    pub fn plugins_dir(&self) -> PathBuf {
        self.dir.join("bin").join("win_x64").join("plugins")
    }
}

/// Every install of the game `kind` found on the PC: Steam's first (in each of its libraries), then others
/// Windows lists as installed. Each folder once.
pub fn find(kind: Kind) -> Vec<Game> {
    let mut found: Vec<Game> = Vec::new();
    let mut add = |game: Option<Game>| {
        if let Some(game) = game
            && !found.iter().any(|known| same_folder(&known.dir, &game.dir))
        {
            found.push(game);
        }
    };
    if let Some(steam) = places::steam() {
        for library in places::steam_libraries(&steam) {
            let steamapps = library.join("steamapps");
            let manifest = format!("appmanifest_{}.acf", kind.steam_app());
            let manifest = std::fs::read_to_string(steamapps.join(manifest)).ok();
            if let Some(folder) = manifest.as_deref().and_then(|manifest| steam_install_dir(manifest, kind)) {
                add(Game::listed_as(&steamapps.join("common").join(folder), FoundBy::Steam, Some(kind)));
            }
        }
    }
    for (name, location) in places::installed_programs() {
        if kind.is_program(&name) {
            // Steam lists its games too: those are found already.
            add(Game::listed_as(&location, FoundBy::Installed, Some(kind)));
        }
    }
    // A folder of the other game's, listed under this one's name: not this game.
    found.retain(|game| game.kind == kind);
    found
}

/// Whether a game WestPlay works with is running (either, any install of it).
pub fn running() -> bool {
    running_processes(&Kind::ALL.map(Kind::process))
}

/// Whether a process of one of these names (as Windows names them) is running.
fn running_processes(names: &[&str]) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
    };
    // SAFETY: a snapshot of the PC's processes, closed below.
    let Ok(snapshot) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else { return false };
    let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
    let mut found = false;
    // SAFETY: the snapshot is open; `entry` is a local with its size set, as the call requires.
    let mut more = unsafe { Process32FirstW(snapshot, &mut entry) }.is_ok();
    while more {
        let length = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
        let name = String::from_utf16_lossy(&entry.szExeFile[..length]);
        if names.iter().any(|wanted| name.eq_ignore_ascii_case(wanted)) {
            found = true;
            break;
        }
        // SAFETY: as above: the open snapshot, and `entry` with its size set.
        more = unsafe { Process32NextW(snapshot, &mut entry) }.is_ok();
    }
    // SAFETY: the snapshot is closed once, after its last use.
    unsafe {
        let _ = CloseHandle(snapshot);
    }
    found
}

/// The game whose base map the install in `dir` has in its archives (`None`: neither, or they can't be read).
fn base_map_kind(dir: &Path) -> Option<Kind> {
    let files = westplay_mapdata::files::GameFiles::open(dir).ok()?;
    let has = |kind: Kind| files.read(&format!("map/{}.mbd", kind.base_map())).ok().flatten().is_some();
    let mut kinds = Kind::ALL.into_iter().filter(|kind| has(*kind));
    match (kinds.next(), kinds.next()) {
        (Some(kind), None) => Some(kind),
        _ => None,
    }
}

/// The game `kind`'s folder name in its Steam library, from its `appmanifest_<app>.acf`.
pub fn steam_install_dir(manifest: &str, kind: Kind) -> Option<String> {
    let file = vdf::parse(manifest)?;
    let state = file.get("AppState")?;
    (state.get("appid")?.text()? == kind.steam_app()).then_some(())?;
    Some(state.get("installdir")?.text()?.to_owned())
}

fn same_folder(a: &Path, b: &Path) -> bool {
    let key = |path: &Path| {
        std::fs::canonicalize(path)
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .to_lowercase()
            .trim_end_matches(['\\', '/'])
            .to_owned()
    };
    key(a) == key(b)
}

/// An executable's file version (its version resource's), if it has one.
fn executable_version(path: &Path) -> Option<Version> {
    let file = HSTRING::from(path.as_os_str());
    // SAFETY: `data` is the size Windows asked for and is read only through it; the fixed info is checked to be
    // there and long enough before it's read.
    unsafe {
        let size = GetFileVersionInfoSizeW(&file, None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(&file, None, size, data.as_mut_ptr().cast()).ok()?;
        let mut info: *mut core::ffi::c_void = std::ptr::null_mut();
        let mut length = 0u32;
        if !VerQueryValueW(data.as_ptr().cast(), w!("\\"), &mut info, &mut length).as_bool()
            || info.is_null()
            || (length as usize) < size_of::<VS_FIXEDFILEINFO>()
        {
            return None;
        }
        // Within `data`, a byte buffer: read as it lies, whatever its alignment.
        let info = std::ptr::read_unaligned(info as *const VS_FIXEDFILEINFO);
        let (ms, ls) = (info.dwFileVersionMS, info.dwFileVersionLS);
        Some(Version([(ms >> 16) as u16, ms as u16, (ls >> 16) as u16, ls as u16]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_workshop_of_the_steam_library_the_game_is_in() {
        let root = std::env::temp_dir().join(format!("westplay-workshop-{}", std::process::id()));
        let library = root.join("SteamLibrary").join("steamapps");
        let workshop = library.join("workshop").join("content").join("227300");
        std::fs::create_dir_all(&workshop).unwrap();
        let game =
            |dir: PathBuf| Game { kind: Kind::Ets2, dir, found_by: FoundBy::Chosen, version: None, dlcs: vec![] };
        // Chosen there by hand, it's still that library's game.
        assert_eq!(game(library.join("common").join("Euro Truck Simulator 2")).workshop(), Some(workshop));
        // Anywhere else, there's none.
        assert_eq!(game(root.join("Games").join("Euro Truck Simulator 2")).workshop(), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Which game a folder holds: the listed one if its executable is there, else the one whose executable is;
    /// with both and no listing, the one whose base map is in its archives.
    #[test]
    fn tells_which_game_a_folder_is_with_both_executables() {
        let root = std::env::temp_dir().join(format!("westplay-game-kind-{}", std::process::id()));
        let install = |name: &str, executables: &[Kind], map: Option<&str>| {
            let dir = root.join(name);
            std::fs::create_dir_all(dir.join("bin").join("win_x64")).unwrap();
            for kind in executables {
                std::fs::write(dir.join(kind.executable()), b"").unwrap();
            }
            let files: Vec<(String, Vec<u8>)> =
                map.map(|map| vec![(format!("map/{map}.mbd"), b"mbd".to_vec())]).unwrap_or_default();
            std::fs::write(dir.join("base.scs"), crate::game_mod::zip(&files)).unwrap();
            dir
        };
        let kind = |dir: &Path, listed| Game::listed_as(dir, FoundBy::Chosen, listed).map(|game| game.kind);
        // ATS, ETS2's executable copied in too.
        let ats = install("ats", &[Kind::Ets2, Kind::Ats], Some("usa"));
        assert_eq!(kind(&ats, Some(Kind::Ats)), Some(Kind::Ats), "Steam's ATS install");
        assert_eq!(kind(&ats, None), Some(Kind::Ats), "chosen by hand: its map says");
        assert_eq!(Game::at(&ats, FoundBy::Chosen).map(|game| game.kind), Some(Kind::Ats));
        // One executable: that game, whatever it's listed as (the listing then not this game: `find` leaves it).
        let ets2 = install("ets2", &[Kind::Ets2], Some("europe"));
        assert_eq!(kind(&ets2, None), Some(Kind::Ets2));
        assert_eq!(kind(&ets2, Some(Kind::Ats)), Some(Kind::Ets2));
        // Both executables, no listing and no base map to tell: not taken for either.
        let unknown = install("unknown", &[Kind::Ets2, Kind::Ats], None);
        assert_eq!(kind(&unknown, None), None);
        // No executable: no game.
        assert_eq!(kind(&install("none", &[], Some("europe")), None), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn tells_a_tested_version() {
        let game = |kind, version| Game { kind, dir: PathBuf::new(), found_by: FoundBy::Steam, version, dlcs: vec![] };
        assert!(game(Kind::Ets2, Some(Version([1, 61, 1, 9]))).tested());
        assert!(game(Kind::Ets2, Some(Version([1, 62, 0, 1]))).tested());
        assert!(!game(Kind::Ets2, Some(Version([1, 59, 1, 3]))).tested());
        assert!(!game(Kind::Ets2, None).tested());
        assert!(game(Kind::Ats, Some(Version([1, 62, 0, 4]))).tested());
        assert!(!game(Kind::Ats, Some(Version([1, 61, 3, 18]))).tested());
    }

    #[test]
    fn reads_the_folder_steam_installed_the_game_in() {
        let manifest = "\"AppState\"\n{\n\t\"appid\"\t\t\"227300\"\n\t\"installdir\"\t\t\"Euro Truck Simulator 2\"\n}";
        assert_eq!(steam_install_dir(manifest, Kind::Ets2).as_deref(), Some("Euro Truck Simulator 2"));
        // Another game's manifest: that game's.
        let ats = "\"AppState\" { \"appid\" \"270880\" \"installdir\" \"American\" }";
        assert_eq!(steam_install_dir(ats, Kind::Ets2), None);
        assert_eq!(steam_install_dir(ats, Kind::Ats).as_deref(), Some("American"));
    }

    #[test]
    fn knows_the_game_the_plugin_says() {
        for kind in Kind::ALL {
            assert_eq!(Kind::of_bridge(westplay_shared::game_number(kind.sdk_id())), Some(kind));
        }
        assert_eq!(Kind::of_bridge(0), None);
    }

    #[test]
    fn knows_each_game_in_windows_list_of_programs() {
        assert!(Kind::Ets2.is_program("Euro Truck Simulator 2"));
        assert!(!Kind::Ets2.is_program("Euro Truck Simulator 2 Demo"));
        assert!(!Kind::Ets2.is_program("American Truck Simulator"));
        assert!(Kind::Ats.is_program("American Truck Simulator"));
        assert!(!Kind::Ats.is_program("Euro Truck Simulator 2"));
    }

    #[test]
    fn matches_the_games_version_patterns() {
        let version = Version([1, 61, 1, 9]);
        assert_eq!(version.to_string(), "1.61.1.9");
        assert!(version.matches("1.61.*") && version.matches("1.61.1.9") && version.matches("1.*"));
        assert!(!version.matches("1.60.*") && !version.matches("1.6.*") && !version.matches(""));
    }

    /// The real thing, on this PC: prints what's found. `cargo test -p westplay-setup -- --ignored --nocapture`
    #[test]
    #[ignore = "looks at the PC's installs"]
    fn finds_the_game_on_this_pc() {
        for game in Kind::ALL.into_iter().flat_map(find) {
            println!(
                "{:?} {} ({:?}): version {:?}, {} DLCs, workshop {:?}, its base map {:?}",
                game.kind,
                game.dir.display(),
                game.found_by,
                game.version.map(|v| v.to_string()),
                game.dlcs.len(),
                game.workshop(),
                base_map_kind(&game.dir)
            );
            assert_eq!(base_map_kind(&game.dir), Some(game.kind), "its archives' map is its own");
        }
    }
}
