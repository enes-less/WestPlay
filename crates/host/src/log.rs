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
/// can't be written, lines are only printed.
pub fn init() {
    let Some(dir) = dir() else { return };
    let _ = fs::create_dir_all(&dir);
    if let Ok(file) = File::create(dir.join("app.log")) {
        *FILE.lock().unwrap_or_else(|e| e.into_inner()) = Some(file);
    }
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

    #[test]
    fn keeps_each_line_in_the_file_with_the_time() {
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
