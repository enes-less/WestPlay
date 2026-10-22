//! Where things are on the player's PC: their Documents folder, Steam's folder and its libraries, and the
//! programs Windows lists as installed.

use std::path::{Path, PathBuf};

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, RRF_RT_REG_SZ, RegCloseKey, RegEnumKeyExW, RegGetValueW,
    RegOpenKeyExW,
};
use windows::Win32::UI::Shell::{FOLDERID_Documents, FOLDERID_RoamingAppData, KF_FLAG_DEFAULT, SHGetKnownFolderPath};
use windows::core::{HSTRING, PCWSTR, PWSTR, w};

use crate::vdf;

/// The player's Documents folder, wherever it is (it may be moved, to OneDrive say).
pub fn documents() -> Option<PathBuf> {
    // SAFETY: Windows hands back a CoTaskMem string, read before it's freed here.
    unsafe {
        let path = SHGetKnownFolderPath(&FOLDERID_Documents, KF_FLAG_DEFAULT, None).ok()?;
        let result = path.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(path.0 as *const _));
        result
    }
}

/// The player's roaming application data folder (`%APPDATA%`), where TruckersMP installs itself.
pub fn roaming() -> Option<PathBuf> {
    // SAFETY: Windows hands back a CoTaskMem string, read before it's freed here.
    unsafe {
        let path = SHGetKnownFolderPath(&FOLDERID_RoamingAppData, KF_FLAG_DEFAULT, None).ok()?;
        let result = path.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(path.0 as *const _));
        result
    }
}

/// The game's folder's name in Documents.
const GAME_FOLDER: &str = "Euro Truck Simulator 2";

/// Every folder that may be the player's Documents, Windows' first: Windows' own (wherever it's moved, to
/// OneDrive say), the user's `Documents`, and each OneDrive's folders (OneDrive names Documents in the
/// player's language: Belgeler, Dokumente…). Windows and the game can disagree: Documents backed up to
/// OneDrive then not, or a game folder OneDrive brought from another PC.
pub fn documents_folders() -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = documents().into_iter().collect();
    if let Some(home) = std::env::var_os("USERPROFILE") {
        folders.push(PathBuf::from(home).join("Documents"));
    }
    for variable in ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"] {
        let Some(root) = std::env::var_os(variable) else { continue };
        let entries = std::fs::read_dir(root).into_iter().flatten().flatten();
        folders.extend(entries.map(|entry| entry.path()).filter(|path| path.is_dir()));
    }
    // A home the game is told to keep elsewhere, in its Steam launch options (`-homedir`): the folder given,
    // and the one it's in (the folder given may be the game's own folder, or hold it).
    for home in launch_homes() {
        folders.extend(home.parent().map(Path::to_path_buf));
        folders.push(home);
    }
    let mut seen = Vec::new();
    folders.retain(|folder| {
        let key = folder.to_string_lossy().to_lowercase();
        let new = !seen.contains(&key);
        seen.push(key);
        new
    });
    folders
}

/// The homes the game is told to keep in its Steam launch options (`-homedir <folder>`), for each of Steam's
/// users on this PC (owner 2026-10-21: a player's game lived in `D:\ETS2`; WestPlay read an old profile with
/// no mods from Documents).
fn launch_homes() -> Vec<PathBuf> {
    let Some(steam) = steam() else { return Vec::new() };
    let users = std::fs::read_dir(steam.join("userdata")).into_iter().flatten().flatten();
    users
        .filter_map(|user| std::fs::read_to_string(user.path().join("config").join("localconfig.vdf")).ok())
        .filter_map(|text| homedir(launch_options(&vdf::parse(&text)?)?))
        .collect()
}

/// The game's launch options in a Steam user's `localconfig.vdf` (its keys in any case).
fn launch_options(config: &vdf::Value) -> Option<&str> {
    let steam = config.get("UserLocalConfigStore")?.get("Software")?.get("Valve")?.get("Steam")?;
    steam.get("apps")?.get(crate::game::STEAM_APP)?.get("LaunchOptions")?.text()
}

/// The folder `-homedir` names in launch `options` (`-homedir D:\ETS2`, or quoted when it has spaces).
fn homedir(options: &str) -> Option<PathBuf> {
    // The options as words, a quoted one whole.
    let mut words = Vec::new();
    let (mut word, mut quoted) = (String::new(), false);
    for c in options.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
            }
            c => word.push(c),
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    let at = words.iter().position(|word| word.eq_ignore_ascii_case("-homedir"))?;
    words.get(at + 1).filter(|folder| !folder.starts_with('-')).map(PathBuf::from)
}

/// WestPlay's own folder in Documents (in Windows' Documents, as the plugin finds it): its logs, and its
/// screens mod, which the plugin mounts into the game itself (`game_mod`).
pub fn westplay_documents() -> Option<PathBuf> {
    Some(documents()?.join("WestPlay"))
}

/// The game's own folder in Documents: its profiles, mods and settings. Of the folders that may be Documents
/// ([`documents_folders`]), the one the game wrote its settings in last; Windows' Documents' if it has none
/// yet (a game never started).
pub fn game_documents() -> Option<PathBuf> {
    choose_game_documents(&documents_folders()).or_else(|| Some(documents()?.join(GAME_FOLDER)))
}

/// Of `folders`, the game's folder whose `config.cfg` the game wrote last (the earlier folder on a tie).
fn choose_game_documents(folders: &[PathBuf]) -> Option<PathBuf> {
    let written = |game: &Path| std::fs::metadata(game.join("config.cfg")).and_then(|m| m.modified()).ok();
    let mut latest: Option<(std::time::SystemTime, PathBuf)> = None;
    for game in folders.iter().map(|folder| folder.join(GAME_FOLDER)) {
        if let Some(at) = written(&game)
            && latest.as_ref().is_none_or(|(best, _)| at > *best)
        {
            latest = Some((at, game));
        }
    }
    latest.map(|(_, game)| game)
}

/// Steam's folder, as Steam records it (for this user, else for the machine), named as Windows names it:
/// Steam keeps it lowercase with forward slashes (`c:/program files (x86)/steam`).
pub fn steam() -> Option<PathBuf> {
    let path = registry_text(HKEY_CURRENT_USER, w!("Software\\Valve\\Steam"), w!("SteamPath"))
        .or_else(|| registry_text(HKEY_LOCAL_MACHINE, w!("SOFTWARE\\WOW6432Node\\Valve\\Steam"), w!("InstallPath")))
        .map(PathBuf::from)?;
    path.is_dir().then(|| windows_name(&path))
}

/// `path` as Windows names it: each folder with its own capitals, joined by backslashes, in the plain form
/// ([`plain`]); as given if it can't be looked up (it isn't there).
pub fn windows_name(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).map_or_else(|_| path.to_path_buf(), |real| plain(&real))
}

/// `path` in its plain form: `\\?\C:\x` as `C:\x`, `\\?\UNC\server\share\x` as `\\server\share\x`; any other
/// path as it is. The long form is what `canonicalize` gives, and WebView2 serves no files from a folder named
/// that way.
pub fn plain(path: &Path) -> PathBuf {
    use std::path::{Component, Prefix};
    let mut components = path.components();
    let start = match components.next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::VerbatimDisk(letter) => format!("{}:\\", char::from(letter)),
            Prefix::VerbatimUNC(server, share) => {
                format!(r"\\{}\{}\", server.to_string_lossy(), share.to_string_lossy())
            }
            _ => return path.to_path_buf(),
        },
        _ => return path.to_path_buf(),
    };
    let mut plain = PathBuf::from(start);
    plain.extend(components.filter(|component| !matches!(component, Component::RootDir)));
    plain
}

/// Steam's library folders (each with a `steamapps` folder), from its `libraryfolders.vdf`; Steam's own
/// folder first.
pub fn steam_libraries(steam: &Path) -> Vec<PathBuf> {
    let file = std::fs::read_to_string(steam.join("steamapps").join("libraryfolders.vdf")).ok();
    let mut libraries = vec![steam.to_path_buf()];
    libraries.extend(file.as_deref().map(library_paths).unwrap_or_default().iter().map(|path| windows_name(path)));
    let mut seen = Vec::new();
    libraries.retain(|path| {
        let key = path.to_string_lossy().to_lowercase().replace('/', "\\").trim_end_matches('\\').to_owned();
        if seen.contains(&key) {
            return false;
        }
        seen.push(key);
        true
    });
    libraries
}

/// The libraries' paths in a `libraryfolders.vdf`.
pub fn library_paths(text: &str) -> Vec<PathBuf> {
    let Some(file) = vdf::parse(text) else { return Vec::new() };
    let Some(folders) = file.get("libraryfolders") else { return Vec::new() };
    folders.entries().iter().filter_map(|(_, folder)| folder.get("path")?.text()).map(PathBuf::from).collect()
}

/// Every program Windows lists as installed (for this user and the machine, 64- and 32-bit): its name and
/// where it's installed, when it says.
pub fn installed_programs() -> Vec<(String, PathBuf)> {
    let roots = [
        (HKEY_LOCAL_MACHINE, w!("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall")),
        (HKEY_LOCAL_MACHINE, w!("SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall")),
        (HKEY_CURRENT_USER, w!("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall")),
    ];
    let mut programs = Vec::new();
    for (hive, path) in roots {
        for name in subkeys(hive, path) {
            // SAFETY: `path` is one of the static registry paths below.
            let key = HSTRING::from(format!("{}\\{name}", unsafe { path.to_string() }.unwrap_or_default()));
            let key = PCWSTR(key.as_ptr());
            let (Some(display), Some(location)) =
                (registry_text(hive, key, w!("DisplayName")), registry_text(hive, key, w!("InstallLocation")))
            else {
                continue;
            };
            if !location.trim().is_empty() {
                programs.push((display, PathBuf::from(location.trim().trim_matches('"'))));
            }
        }
    }
    programs
}

/// The names of a registry key's subkeys.
fn subkeys(hive: HKEY, path: PCWSTR) -> Vec<String> {
    let mut key = HKEY::default();
    // SAFETY: the hive is a predefined key and `path` a static string; `key` is a local the call writes.
    if unsafe { RegOpenKeyExW(hive, path, None, KEY_READ, &mut key) } != ERROR_SUCCESS {
        return Vec::new();
    }
    let mut names = Vec::new();
    for index in 0.. {
        let mut buffer = [0u16; 256];
        let mut length = buffer.len() as u32;
        let status =
            // SAFETY: the key is open; `buffer` is ours and `length` says how much of it the call may write.
            unsafe { RegEnumKeyExW(key, index, Some(PWSTR(buffer.as_mut_ptr())), &mut length, None, None, None, None) };
        if status != ERROR_SUCCESS {
            break;
        }
        names.push(String::from_utf16_lossy(&buffer[..length as usize]));
    }
    // SAFETY: the key was opened above and is closed once, after its last use.
    unsafe {
        let _ = RegCloseKey(key);
    }
    names
}

/// A text value of the registry, if it's there.
fn registry_text(hive: HKEY, path: PCWSTR, name: PCWSTR) -> Option<String> {
    let mut buffer = [0u16; 1024];
    let mut size = (buffer.len() * 2) as u32;
    // SAFETY: the buffer is ours and `size` says how much of it the call may write, in bytes.
    let status = unsafe {
        RegGetValueW(hive, path, name, RRF_RT_REG_SZ, None, Some(buffer.as_mut_ptr().cast()), Some(&mut size))
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let length = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    Some(String::from_utf16_lossy(&buffer[..length]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_game_folder_the_game_uses_wherever_documents_is() {
        let root = std::env::temp_dir().join(format!("westplay-places-{}", std::process::id()));
        let (windows, onedrive, empty) =
            (root.join("Documents"), root.join("OneDrive").join("Belgeler"), root.join("x"));
        for folder in [&windows, &onedrive] {
            std::fs::create_dir_all(folder.join(GAME_FOLDER)).unwrap();
        }
        std::fs::create_dir_all(&empty).unwrap();
        // Only OneDrive's has the game's settings: it's the one.
        std::fs::write(onedrive.join(GAME_FOLDER).join("config.cfg"), "uset g_lang \"tr_tr\"").unwrap();
        let folders = [windows.clone(), onedrive.clone(), empty.clone()];
        assert_eq!(choose_game_documents(&folders), Some(onedrive.join(GAME_FOLDER)));
        // Both: the one written last.
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::fs::write(windows.join(GAME_FOLDER).join("config.cfg"), "uset g_lang \"en_us\"").unwrap();
        assert_eq!(choose_game_documents(&folders), Some(windows.join(GAME_FOLDER)));
        // None has played yet.
        assert_eq!(choose_game_documents(&[empty]), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn names_folders_plainly() {
        assert_eq!(plain(Path::new(r"\\?\C:\Users\a\WestPlay\ui")), Path::new(r"C:\Users\a\WestPlay\ui"));
        assert_eq!(plain(Path::new(r"\\?\UNC\server\share\ui")), Path::new(r"\\server\share\ui"));
        assert_eq!(plain(Path::new(r"C:\Users\a\ui")), Path::new(r"C:\Users\a\ui"));
        assert_eq!(plain(Path::new(r"\\server\share\ui")), Path::new(r"\\server\share\ui"));
    }

    #[test]
    fn names_a_folder_as_windows_does() {
        // Written as Steam writes its own folder: lowercase, forward slashes.
        let windows = std::env::var("SystemRoot").unwrap();
        let as_steam_writes = windows.to_lowercase().replace('\\', "/");
        assert_eq!(windows_name(Path::new(&as_steam_writes)), Path::new(&windows));
        // A folder that isn't there stays as given.
        assert_eq!(windows_name(Path::new("q:/not/there")), Path::new("q:/not/there"));
    }

    #[test]
    fn reads_the_libraries_steam_lists() {
        let text = "\"libraryfolders\" { \"0\" { \"path\" \"C:\\\\Program Files (x86)\\\\Steam\" } \"1\" { \"path\" \"D:\\\\SteamLibrary\" } }";
        assert_eq!(
            library_paths(text),
            vec![PathBuf::from(r"C:\Program Files (x86)\Steam"), PathBuf::from(r"D:\SteamLibrary")]
        );
        assert!(library_paths("not a vdf {").is_empty());
    }

    #[test]
    fn reads_the_home_the_game_is_told_to_keep() {
        assert_eq!(homedir(r"-homedir D:\ETS2"), Some(PathBuf::from(r"D:\ETS2")));
        assert_eq!(homedir(r#"-nointro -HOMEDIR "D:\My Games\ETS2" -64bit"#), Some(PathBuf::from(r"D:\My Games\ETS2")));
        assert_eq!(homedir("-nointro -64bit"), None);
        assert_eq!(homedir("-homedir -nointro"), None);
        assert_eq!(homedir(""), None);
        // As Steam keeps them, its quotes and backslashes escaped, its keys in its own case.
        let config = vdf::parse(
            r#""UserLocalConfigStore" { "Software" { "Valve" { "Steam" { "apps" {
                "270880" { "LaunchOptions" "-homedir E:\\ATS" }
                "227300" { "LastPlayed" "1792500000" "LaunchOptions" "-homedir \"D:\\My Games\\ETS2\"" }
            } } } } }"#,
        )
        .unwrap();
        assert_eq!(launch_options(&config).and_then(homedir), Some(PathBuf::from(r"D:\My Games\ETS2")));
    }

    #[test]
    fn a_home_given_as_the_game_folder_or_the_one_holding_it_is_found() {
        let root = std::env::temp_dir().join(format!("westplay-homedir-{}", std::process::id()));
        let (documents, home) = (root.join("Documents"), root.join("ETS2"));
        std::fs::create_dir_all(documents.join(GAME_FOLDER)).unwrap();
        std::fs::create_dir_all(home.join(GAME_FOLDER)).unwrap();
        // An old game folder in Documents, and the one the game uses now in the home it's given.
        std::fs::write(documents.join(GAME_FOLDER).join("config.cfg"), "old").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::fs::write(home.join(GAME_FOLDER).join("config.cfg"), "now").unwrap();
        // `-homedir <root>\ETS2`: the folder given and the one it's in are candidates.
        let given = |folder: PathBuf| vec![documents.clone(), folder.parent().unwrap().to_path_buf(), folder];
        assert_eq!(choose_game_documents(&given(home.clone())), Some(home.join(GAME_FOLDER)));
        // `-homedir "<root>\ETS2\Euro Truck Simulator 2"`: the same folder.
        assert_eq!(choose_game_documents(&given(home.join(GAME_FOLDER))), Some(home.join(GAME_FOLDER)));
        let _ = std::fs::remove_dir_all(&root);
    }
}
