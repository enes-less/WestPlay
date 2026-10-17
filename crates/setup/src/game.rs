//! Finding the game: every install of Euro Truck Simulator 2 on the PC (Steam's, in any of its libraries, and
//! others Windows lists as installed), each with its version (its executable's) and its DLCs.

use std::fmt;
use std::path::{Path, PathBuf};

use windows::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VS_FIXEDFILEINFO, VerQueryValueW,
};
use windows::core::{HSTRING, w};

use crate::{places, vdf};

/// The game's app id on Steam (its manifest, its Workshop folder, its Steam Cloud folder).
pub const STEAM_APP: &str = "227300";
/// The game's executable, in its folder.
const EXECUTABLE: &str = "bin/win_x64/eurotrucks2.exe";
/// The game's process, as Windows names it (TruckersMP's client runs the same).
const PROCESS: &str = "eurotrucks2.exe";
/// The game versions WestPlay has been tried with. Others may well work (the plugin finds what it needs in
/// the game by its shape), but haven't been tried.
pub const TESTED_VERSIONS: &str = "1.61.*";

/// An install of the game.
#[derive(Clone, Debug, PartialEq)]
pub struct Game {
    /// Its folder (with `base.scs`).
    pub dir: PathBuf,
    pub found_by: FoundBy,
    /// Its version, from its executable (`None`: it couldn't be read).
    pub version: Option<Version>,
    /// Its DLCs' archives, by name (`dlc_balkan_e`…), in order.
    pub dlcs: Vec<String>,
}

/// How an install was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoundBy {
    /// In one of Steam's libraries.
    Steam,
    /// In Windows' list of installed programs (another store's, or SCS's own installer).
    Installed,
    /// The player chose its folder.
    Chosen,
}

/// The game's version: `1.61.1.9`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
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
    /// The game in `dir`, if it's an install of it: its archives and its executable are there.
    pub fn at(dir: &Path, found_by: FoundBy) -> Option<Self> {
        if !dir.join("base.scs").is_file() || !dir.join(EXECUTABLE).is_file() {
            return None;
        }
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
        Some(Self { dir: dir.to_path_buf(), found_by, version: executable_version(&dir.join(EXECUTABLE)), dlcs })
    }

    /// The game in `dir`, found as [`find`] finds it (Steam's, or listed as installed) or else as chosen.
    pub fn recognise(dir: &Path) -> Option<Self> {
        find().into_iter().find(|game| same_folder(&game.dir, dir)).or_else(|| Self::at(dir, FoundBy::Chosen))
    }

    /// Whether WestPlay has been tried with this version of the game ([`TESTED_VERSIONS`]).
    pub fn tested(&self) -> bool {
        self.version.is_some_and(|version| version.matches(TESTED_VERSIONS))
    }

    /// Where Steam keeps the game's Workshop mods, for an install in a Steam library (however it was found:
    /// a folder chosen there too).
    pub fn workshop(&self) -> Option<PathBuf> {
        // `<library>/steamapps/common/<game>` → `<library>/steamapps/workshop/content/227300`.
        let common = self.dir.parent()?;
        let steamapps = common.parent()?;
        let named = |path: &Path, name: &str| path.file_name().is_some_and(|n| n.eq_ignore_ascii_case(name));
        if !named(common, "common") || !named(steamapps, "steamapps") {
            return None;
        }
        let workshop = steamapps.join("workshop").join("content").join(STEAM_APP);
        workshop.is_dir().then_some(workshop)
    }

    /// Where the plugin goes.
    pub fn plugins_dir(&self) -> PathBuf {
        self.dir.join("bin").join("win_x64").join("plugins")
    }
}

/// Every install of the game found on the PC: Steam's first (in each of its libraries), then others Windows
/// lists as installed. Each folder once.
pub fn find() -> Vec<Game> {
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
            let manifest = std::fs::read_to_string(steamapps.join(format!("appmanifest_{STEAM_APP}.acf"))).ok();
            if let Some(folder) = manifest.as_deref().and_then(steam_install_dir) {
                add(Game::at(&steamapps.join("common").join(folder), FoundBy::Steam));
            }
        }
    }
    for (name, location) in places::installed_programs() {
        if is_the_game(&name) {
            // Steam lists its games too: those are found already.
            add(Game::at(&location, FoundBy::Installed));
        }
    }
    found
}

/// Whether the game is running (any install of it).
pub fn running() -> bool {
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
        if String::from_utf16_lossy(&entry.szExeFile[..length]).eq_ignore_ascii_case(PROCESS) {
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

/// The game's folder name in its Steam library, from its `appmanifest_227300.acf`.
pub fn steam_install_dir(manifest: &str) -> Option<String> {
    let file = vdf::parse(manifest)?;
    let state = file.get("AppState")?;
    (state.get("appid")?.text()? == STEAM_APP).then_some(())?;
    Some(state.get("installdir")?.text()?.to_owned())
}

/// Whether a program Windows lists is the game (not its demo, nor American Truck Simulator).
pub fn is_the_game(name: &str) -> bool {
    let name = name.to_lowercase();
    name.contains("euro truck simulator 2") && !name.contains("demo")
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
        let workshop = library.join("workshop").join("content").join(STEAM_APP);
        std::fs::create_dir_all(&workshop).unwrap();
        let game = |dir: PathBuf| Game { dir, found_by: FoundBy::Chosen, version: None, dlcs: vec![] };
        // Chosen there by hand, it's still that library's game.
        assert_eq!(game(library.join("common").join("Euro Truck Simulator 2")).workshop(), Some(workshop));
        // Anywhere else, there's none.
        assert_eq!(game(root.join("Games").join("Euro Truck Simulator 2")).workshop(), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn tells_a_tested_version() {
        let game = |version| Game { dir: PathBuf::new(), found_by: FoundBy::Steam, version, dlcs: vec![] };
        assert!(game(Some(Version([1, 61, 1, 9]))).tested());
        assert!(!game(Some(Version([1, 62, 0, 1]))).tested());
        assert!(!game(None).tested());
    }

    #[test]
    fn reads_the_folder_steam_installed_the_game_in() {
        let manifest = "\"AppState\"\n{\n\t\"appid\"\t\t\"227300\"\n\t\"installdir\"\t\t\"Euro Truck Simulator 2\"\n}";
        assert_eq!(steam_install_dir(manifest).as_deref(), Some("Euro Truck Simulator 2"));
        // Another game's manifest.
        assert_eq!(steam_install_dir("\"AppState\" { \"appid\" \"270880\" \"installdir\" \"American\" }"), None);
    }

    #[test]
    fn knows_the_game_in_windows_list_of_programs() {
        assert!(is_the_game("Euro Truck Simulator 2"));
        assert!(!is_the_game("Euro Truck Simulator 2 Demo"));
        assert!(!is_the_game("American Truck Simulator"));
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
        for game in find() {
            println!(
                "{} ({:?}): version {:?}, {} DLCs, workshop {:?}",
                game.dir.display(),
                game.found_by,
                game.version.map(|v| v.to_string()),
                game.dlcs.len(),
                game.workshop()
            );
        }
    }
}
