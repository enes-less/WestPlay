//! The app's log: each line printed (the console, in development) and kept in `Documents\WestPlay\logs\app.log`
//! beside the plugin's `plugin.log`, written anew each time WestPlay starts. Release builds have no console,
//! so without the file what the host says (a service that failed, the game's memory not found) would be lost.
//! Never anything of the player's accounts or what they play: errors, and how long things took.

use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use windows::Win32::System::SystemInformation::GetLocalTime;

static FILE: Mutex<Option<File>> = Mutex::new(None);

/// Where the log is kept: `Documents\WestPlay\logs`, none if Documents can't be found.
pub fn dir() -> Option<PathBuf> {
    Some(westplay_setup::places::documents()?.join("WestPlay").join("logs"))
}

/// Starts the log file, empty (lines before it are only printed). Without a Documents folder, or if the file
/// can't be written, lines are only printed. A panic in any thread is kept in it too (`log_panics`).
pub fn init() {
    log_panics();
    let Some(dir) = dir() else { return };
    let _ = fs::create_dir_all(&dir);
    if let Ok(file) = File::create(dir.join("app.log")) {
        *FILE.lock().unwrap_or_else(|e| e.into_inner()) = Some(file);
    }
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
