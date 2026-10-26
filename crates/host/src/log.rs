//! The app's log: each line printed (the console, in development) and kept in `Documents\WestPlay\logs\app.log`
//! beside the plugin's `plugin.log`, written anew each time WestPlay starts. Release builds have no console,
//! so without the file what the host says (a service that failed, the game's memory not found) would be lost.
//! Where Documents refuses it, it's kept in WestPlay's own data folder instead, saying why ([`init`]).
//! Never anything of the player's accounts or what they play: errors, and how long things took.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use windows::Win32::System::SystemInformation::GetLocalTime;

static FILE: Mutex<Option<File>> = Mutex::new(None);

/// Where this run's `app.log` is, once [`init`] made it.
static PLACE: OnceLock<PathBuf> = OnceLock::new();

/// Where the log is kept: `Documents\WestPlay\logs`, none if Documents can't be found.
pub fn dir() -> Option<PathBuf> {
    Some(westplay_setup::places::documents()?.join("WestPlay").join("logs"))
}

/// Where the log is kept when Documents refuses it: WestPlay's own data folder (`%LOCALAPPDATA%\app.westplay`,
/// where the screens' browsers and the maps are, so WestPlay can write there whenever it runs at all). A
/// player's ransomware protection (Windows Security's Controlled folder access, 2026-10-24) let the game write
/// its `plugin.log` in Documents but not WestPlay: there was no `app.log`, and nothing said why.
fn fallback_dir() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("app.westplay").join("logs"))
}

/// Where this run's `app.log` is (the companion's About shows it); `None` if it couldn't be made anywhere.
pub fn place() -> Option<&'static Path> {
    PLACE.get().map(PathBuf::as_path)
}

/// Starts the log file, empty (lines before it are only printed), in Documents; if that's refused, in
/// WestPlay's own folder, its first line saying what Documents answered. Where neither can be written, lines
/// are only printed. A panic in any thread is kept in it too (`log_panics`).
pub fn init() {
    log_panics();
    let (made, refused) = open(dir(), fallback_dir());
    let Some((file, path)) = made else { return };
    *FILE.lock().unwrap_or_else(|e| e.into_inner()) = Some(file);
    let _ = PLACE.set(path);
    if let Some(refused) = refused {
        write(&format!("the log is kept here, as {refused}"));
    }
}

/// `app.log` made anew in `dir`, or in `fallback` if `dir` refuses it; and, when it's not in `dir`, why.
fn open(dir: Option<PathBuf>, fallback: Option<PathBuf>) -> (Option<(File, PathBuf)>, Option<String>) {
    let refused = match &dir {
        Some(dir) => match create(dir) {
            Ok(made) => return (Some(made), None),
            Err(e) => format!("{} couldn't be written: {e}", dir.display()),
        },
        None => "the Documents folder wasn't found".to_string(),
    };
    (fallback.and_then(|fallback| create(&fallback).ok()), Some(refused))
}

/// `dir\app.log`, made anew (and `dir` with it).
fn create(dir: &Path) -> std::io::Result<(File, PathBuf)> {
    fs::create_dir_all(dir)?;
    let path = dir.join("app.log");
    Ok((File::create(&path)?, path))
}

/// Keeps every panic in the log: which thread, what and where, and the calls that led to it. A release build
/// has no console, so a thread that panicked (the maps' watch, 2.0.2: its first state never came, the
/// Companion's Prepare Maps Again hidden) had stopped without a word. Printed as before too.
fn log_panics() {
    let printed = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let backtrace = std::backtrace::Backtrace::force_capture();
        write(&format!("panic in thread {}: {info}\n{backtrace}", thread.name().unwrap_or("unnamed")));
        printed(info);
    }));
}

/// Prints `line`, and keeps it in the log with the time.
pub fn write(line: &str) {
    println!("{line}");
    let mut file = FILE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(file) = file.as_mut() {
        // SAFETY: a plain query of the time.
        let now = unsafe { GetLocalTime() };
        let _ =
            writeln!(file, "{:02}:{:02}:{:02}.{:03}  {line}", now.wHour, now.wMinute, now.wSecond, now.wMilliseconds);
    }
}

/// Logs a line, formatted as `format!` does.
#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => { $crate::log::write(&format!($($arg)*)) };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tests here each put a file of their own in as the log: one at a time.
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

    #[test]
    fn keeps_a_panic_in_the_file_with_its_thread_and_place() {
        let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("westplay-log-panic-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("app.log");
        *FILE.lock().unwrap() = Some(File::create(&path).unwrap());
        log_panics();
        let thread = std::thread::Builder::new().name("westplay-test".into()).spawn(|| panic!("a test's panic"));
        assert!(thread.unwrap().join().is_err());
        // Back to the default hook, for the tests after.
        drop(std::panic::take_hook());
        *FILE.lock().unwrap() = None;
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("panic in thread westplay-test: "), "not logged: {text}");
        assert!(text.contains("a test's panic"), "no message: {text}");
        assert!(text.contains("log.rs:"), "no place: {text}");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn keeps_the_log_in_documents_when_it_can() {
        let root = std::env::temp_dir().join(format!("westplay-log-documents-test-{}", std::process::id()));
        let (documents, fallback) = (root.join("Documents").join("logs"), root.join("fallback"));
        let (made, refused) = open(Some(documents.clone()), Some(fallback.clone()));
        assert_eq!(made.map(|(_, path)| path), Some(documents.join("app.log")));
        assert_eq!(refused, None);
        assert!(!fallback.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn keeps_the_log_in_its_own_folder_saying_why_when_documents_refuses() {
        let root = std::env::temp_dir().join(format!("westplay-log-refused-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        // A file where the folder would be: Documents refuses, as a ransomware protection's would.
        fs::write(root.join("Documents"), "").unwrap();
        let (documents, fallback) = (root.join("Documents").join("logs"), root.join("fallback"));
        let (made, refused) = open(Some(documents.clone()), Some(fallback.clone()));
        let (mut file, path) = made.expect("no log in the fallback");
        assert_eq!(path, fallback.join("app.log"));
        writeln!(file, "a line").unwrap();
        drop(file);
        assert_eq!(fs::read_to_string(&path).unwrap(), "a line\n");
        let refused = refused.unwrap();
        assert!(refused.starts_with(&format!("{} couldn't be written: ", documents.display())), "{refused}");
        // No Documents at all: said too.
        let (made, refused) = open(None, Some(fallback));
        assert!(made.is_some());
        assert_eq!(refused.as_deref(), Some("the Documents folder wasn't found"));
        // Neither writable: no log, only printing.
        assert!(open(Some(documents.clone()), Some(documents)).0.is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn keeps_each_line_in_the_file_with_the_time() {
        let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("westplay-log-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("app.log");
        *FILE.lock().unwrap() = Some(File::create(&path).unwrap());
        log!("route: the navigator wasn't found ({})", "a new game version?");
        *FILE.lock().unwrap() = None;
        let text = fs::read_to_string(&path).unwrap();
        // Other tests may log meanwhile (they run in parallel): this test's line is among them.
        let line =
            text.lines().find(|line| line.ends_with("  route: the navigator wasn't found (a new game version?)"));
        let line = line.unwrap_or_else(|| panic!("not logged: {text}"));
        assert_eq!(line.as_bytes()[2], b':');
        fs::remove_dir_all(dir).unwrap();
    }
}
