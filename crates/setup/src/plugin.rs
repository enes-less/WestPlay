//! WestPlay's game plugin (`westplay.dll`), in the game's `bin/win_x64/plugins` folder, where the game loads
//! SDK plugins from (and TruckersMP's client too).
//!
//! Installing writes the new plugin beside the one there, then moves it over it, so the game never finds
//! half a file. Two things can stand in the way, and each is told apart: the game running (it holds the
//! plugin it loaded: close it first), and a folder that needs administrator rights (Steam installed under
//! Program Files). For the latter WestPlay starts itself again with them, for that one copy only
//! ([`install_as_administrator`]): Windows asks the player once, and nothing else runs with those rights.

use std::io;
use std::path::{Path, PathBuf};

use windows::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject};
use windows::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;
use windows::core::{HSTRING, PCWSTR, w};

use crate::game::Game;

/// The plugin's file, in the game's plugins folder.
pub const FILE_NAME: &str = "westplay.dll";
/// The argument WestPlay is started with, as administrator, to copy the plugin and nothing else.
const COPY_ARGUMENT: &str = "--westplay-install-plugin";
/// The same, to remove it and nothing else (uninstalling).
const REMOVE_ARGUMENT: &str = "--westplay-remove-plugin";

/// Windows' errors for a file another program holds open (the game, its plugin loaded).
const ERROR_SHARING_VIOLATION: i32 = 32;
const ERROR_LOCK_VIOLATION: i32 = 33;
/// Exit codes of the copy run as administrator.
const EXIT_COPIED: u32 = 0;
const EXIT_IN_USE: u32 = 2;
const EXIT_FAILED: u32 = 1;

/// The plugin in a game, against WestPlay's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Missing,
    /// The same as WestPlay's.
    Current,
    /// Another version (an older WestPlay's, or a newer one's).
    Different,
}

/// Why the plugin couldn't be installed (or removed).
#[derive(Debug)]
pub enum InstallError {
    /// The game is running, holding the plugin it loaded: it can be replaced once the game is closed.
    InUse,
    /// The plugins folder needs administrator rights.
    NeedsAdministrator,
    /// The player said no to Windows' administrator prompt.
    Declined,
    Io(io::Error),
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InUse => write!(f, "the game is running: close it to install the plugin"),
            Self::NeedsAdministrator => write!(f, "the game's folder needs administrator rights"),
            Self::Declined => write!(f, "administrator rights weren't given"),
            Self::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for InstallError {}

impl From<io::Error> for InstallError {
    fn from(error: io::Error) -> Self {
        match error.raw_os_error() {
            Some(ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION) => Self::InUse,
            _ if error.kind() == io::ErrorKind::PermissionDenied => Self::NeedsAdministrator,
            _ => Self::Io(error),
        }
    }
}

/// The plugin installed in `game`, against WestPlay's own (`ours`, the file WestPlay ships).
pub fn state(game: &Game, ours: &Path) -> io::Result<State> {
    let installed = match std::fs::read(game.plugins_dir().join(FILE_NAME)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(State::Missing),
        Err(error) => return Err(error),
    };
    Ok(if installed == std::fs::read(ours)? { State::Current } else { State::Different })
}

/// Other CarPlay apps WestPlay can't share the trucks' screens with, by their plugin's file in the game's
/// plugins folder (lower case), and the name the companion says.
const OTHER_APPS: &[(&str, &str)] = &[
    // YMN One: its plugin takes the trucks' GPS screens for its own texture, pink while its app isn't running,
    // WestPlay's screen never seen (a player's game.log, 2026-10-27: "[mem server] Failed to init update for
    // object '/home/YamanDrive/gps.tobj'").
    ("yamandrivebridge.dll", "ymnOne"),
];

/// The other CarPlay apps installed in `game` ([`OTHER_APPS`]), each once.
pub fn other_apps(game: &Game) -> Vec<&'static str> {
    other_apps_in(&game.plugins_dir())
}

fn other_apps_in(plugins: &Path) -> Vec<&'static str> {
    let Ok(entries) = std::fs::read_dir(plugins) else { return Vec::new() };
    let names: Vec<String> = entries.flatten().map(|entry| entry.file_name().to_string_lossy().to_lowercase()).collect();
    let mut found: Vec<&'static str> = OTHER_APPS
        .iter()
        .filter(|(file, _)| names.iter().any(|name| name == file))
        .map(|(_, app)| *app)
        .collect();
    found.dedup();
    found
}

/// Installs WestPlay's plugin (`ours`) in `game`: written beside the one there, then moved over it.
pub fn install(game: &Game, ours: &Path) -> Result<PathBuf, InstallError> {
    copy_into(ours, &game.plugins_dir())
}

/// Takes the plugin out of `game` (only WestPlay's own file, and a copy left half-written).
pub fn uninstall(game: &Game) -> Result<(), InstallError> {
    remove_from(&game.plugins_dir())
}

fn remove_from(plugins: &Path) -> Result<(), InstallError> {
    let target = plugins.join(FILE_NAME);
    // The game holding it (running) told apart from a folder needing rights, as when installing.
    if target.exists() {
        std::fs::OpenOptions::new().write(true).open(&target)?;
    }
    for file in [target, plugins.join(format!("{FILE_NAME}.partial"))] {
        match std::fs::remove_file(&file) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn copy_into(ours: &Path, plugins: &Path) -> Result<PathBuf, InstallError> {
    let target = plugins.join(FILE_NAME);
    // The game holds the plugin it loaded: tell that apart from a folder needing rights (moving over a
    // loaded file is refused as "access denied" too). Opening it to write, as nobody may while it's loaded,
    // says "in use"; it's closed again untouched. (Append alone isn't enough: Windows lets a loaded DLL be
    // opened for that.)
    if target.exists() {
        std::fs::OpenOptions::new().write(true).open(&target)?;
    }
    std::fs::create_dir_all(plugins)?;
    let partial = plugins.join(format!("{FILE_NAME}.partial"));
    std::fs::copy(ours, &partial)?;
    if let Err(error) = std::fs::rename(&partial, &target) {
        let _ = std::fs::remove_file(&partial);
        return Err(error.into());
    }
    Ok(target)
}

/// Installs the plugin with administrator rights: WestPlay starts itself again with them (Windows asks the
/// player), copies the plugin, and ends; this waits for it.
pub fn install_as_administrator(game: &Game, ours: &Path) -> Result<(), InstallError> {
    as_administrator(format!("{COPY_ARGUMENT} {} {}", quoted(ours), quoted(&game.plugins_dir())))
}

/// Removes the plugin with administrator rights, as [`install_as_administrator`] copies it.
pub fn uninstall_as_administrator(game: &Game) -> Result<(), InstallError> {
    as_administrator(format!("{REMOVE_ARGUMENT} {}", quoted(&game.plugins_dir())))
}

/// WestPlay started again with administrator rights and `arguments` (Windows asks the player), waited for.
fn as_administrator(arguments: String) -> Result<(), InstallError> {
    let exe = std::env::current_exe()?;
    let (exe, arguments) = (HSTRING::from(exe.as_os_str()), HSTRING::from(arguments));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(exe.as_ptr()),
        lpParameters: PCWSTR(arguments.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    // SAFETY: `info` is complete and its strings live through the call.
    if let Err(error) = unsafe { ShellExecuteExW(&mut info) } {
        return Err(if error.code() == ERROR_CANCELLED.to_hresult() {
            InstallError::Declined
        } else {
            InstallError::Io(io::Error::other(error))
        });
    }
    let mut code = EXIT_FAILED;
    // SAFETY: the process handle came from the call above (`SEE_MASK_NOCLOSEPROCESS`) and is closed once.
    unsafe {
        if WaitForSingleObject(info.hProcess, INFINITE) == WAIT_OBJECT_0 {
            let _ = GetExitCodeProcess(info.hProcess, &mut code);
        }
        let _ = CloseHandle(info.hProcess);
    }
    match code {
        EXIT_COPIED => Ok(()),
        EXIT_IN_USE => Err(InstallError::InUse),
        _ => Err(InstallError::Io(io::Error::other("the plugin couldn't be copied or removed"))),
    }
}

/// For WestPlay's `main`, first thing: when it was started to copy (or remove) the plugin as administrator,
/// does that and returns the exit code (WestPlay then ends); otherwise `None`, and it starts as usual.
pub fn run_as_administrator_copy(args: &[String]) -> Option<i32> {
    let done = match args {
        [_, argument, ours, plugins] if argument == COPY_ARGUMENT => {
            copy_into(Path::new(ours), Path::new(plugins)).map(|_| ())
        }
        [_, argument, plugins] if argument == REMOVE_ARGUMENT => remove_from(Path::new(plugins)),
        _ => return None,
    };
    let code = match done {
        Ok(()) => EXIT_COPIED,
        Err(InstallError::InUse) => EXIT_IN_USE,
        Err(_) => EXIT_FAILED,
    };
    Some(code as i32)
}

/// A path as one command-line argument.
fn quoted(path: &Path) -> String {
    format!("\"{}\"", path.display())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::FoundBy;

    #[test]
    fn finds_other_carplay_apps_by_their_plugin() {
        let (game, _, root) = setup("others");
        let plugins = game.plugins_dir();
        std::fs::create_dir_all(&plugins).unwrap();
        std::fs::write(plugins.join(FILE_NAME), b"ours").unwrap();
        assert!(other_apps(&game).is_empty());
        // As its installer names it, whatever the case.
        std::fs::write(plugins.join("YamanDriveBridge.dll"), b"theirs").unwrap();
        assert_eq!(other_apps(&game), ["ymnOne"]);
        // No plugins folder at all: none.
        std::fs::remove_dir_all(&plugins).unwrap();
        assert!(other_apps(&game).is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    /// A game folder (temporary) and WestPlay's plugin beside it.
    fn setup(name: &str) -> (Game, PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("westplay-plugin-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let ours = root.join("ours.dll");
        std::fs::write(&ours, b"the plugin, version 2").unwrap();
        let game = Game { dir: root.join("game"), found_by: FoundBy::Chosen, version: None, dlcs: vec![] };
        (game, ours, root)
    }

    #[test]
    fn installs_replaces_and_takes_out_the_plugin() {
        let (game, ours, root) = setup("cycle");
        assert_eq!(state(&game, &ours).unwrap(), State::Missing);
        let installed = install(&game, &ours).unwrap();
        assert_eq!(installed, game.plugins_dir().join(FILE_NAME));
        assert_eq!(state(&game, &ours).unwrap(), State::Current);
        // An older one there: replaced whole.
        std::fs::write(&installed, b"the plugin, version 1").unwrap();
        assert_eq!(state(&game, &ours).unwrap(), State::Different);
        install(&game, &ours).unwrap();
        assert_eq!(std::fs::read(&installed).unwrap(), b"the plugin, version 2");
        assert!(!game.plugins_dir().join(format!("{FILE_NAME}.partial")).exists());
        uninstall(&game).unwrap();
        assert_eq!(state(&game, &ours).unwrap(), State::Missing);
        uninstall(&game).unwrap();
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn says_so_while_the_game_holds_the_plugin() {
        use windows::Win32::Foundation::FreeLibrary;
        use windows::Win32::System::LibraryLoader::LoadLibraryW;
        let (game, ours, root) = setup("in-use");
        // Loaded as the game loads a plugin (a real DLL: Windows' own small version.dll, copied in under the
        // plugin's name), which no file lock stands in for: Windows lets a loaded DLL be opened to append.
        let installed = game.plugins_dir().join(FILE_NAME);
        std::fs::create_dir_all(game.plugins_dir()).unwrap();
        std::fs::copy(r"C:\Windows\System32\version.dll", &installed).unwrap();
        // SAFETY: a copy of a system DLL, loaded only to hold the file open, and freed below.
        let module = unsafe { LoadLibraryW(&HSTRING::from(installed.as_os_str())) }.unwrap();
        assert!(matches!(install(&game, &ours), Err(InstallError::InUse)));
        assert!(matches!(uninstall(&game), Err(InstallError::InUse) | Err(InstallError::NeedsAdministrator)));
        // SAFETY: the module was loaded above and is freed once.
        unsafe { FreeLibrary(module) }.unwrap();
        install(&game, &ours).unwrap();
        assert_eq!(state(&game, &ours).unwrap(), State::Current);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn copies_as_administrator_only_when_started_for_it() {
        let (game, ours, root) = setup("admin");
        let plugins = game.plugins_dir();
        let args = |argument: &str| {
            vec!["westplay.exe".into(), argument.into(), ours.display().to_string(), plugins.display().to_string()]
        };
        assert_eq!(run_as_administrator_copy(&["westplay.exe".into()]), None);
        assert_eq!(run_as_administrator_copy(&args("--something-else")), None);
        assert_eq!(run_as_administrator_copy(&args(COPY_ARGUMENT)), Some(0));
        assert_eq!(state(&game, &ours).unwrap(), State::Current);
        let _ = std::fs::remove_dir_all(root);
    }
}
