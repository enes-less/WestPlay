//! Where things are on the player's PC: their Documents folder, Steam's folder and its libraries, and the
//! programs Windows lists as installed.

use std::path::{Path, PathBuf};

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, RRF_RT_REG_SZ, RegCloseKey, RegEnumKeyExW, RegGetValueW,
    RegOpenKeyExW,
};
use windows::Win32::UI::Shell::{FOLDERID_Documents, KF_FLAG_DEFAULT, SHGetKnownFolderPath};
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

/// The game's own folder in Documents: its profiles, mods and settings.
pub fn game_documents() -> Option<PathBuf> {
    Some(documents()?.join("Euro Truck Simulator 2"))
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
}
