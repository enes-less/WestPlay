//! The app's log: each line printed (the console, in development) and kept in `Documents\WestPlay\logs\app.log`
//! beside the plugin's `plugin.log`, written anew each time WestPlay starts. Release builds have no console,
//! so without the file what the host says (a service that failed, the game's memory not found) would be lost.
//! Where Documents refuses it, it's kept in WestPlay's own data folder instead, saying why ([`init`]). The run
//! before's is kept beside it as `app.old.log`: a player who starts WestPlay again after it closed would otherwise
//! send the new run's log, the crash's gone (2026-11-06). Its first line is WestPlay's version, and a native crash
//! is its last ([`catch_crashes`]).
//! Never anything of the player's accounts or what they play: errors, and how long things took.

use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::Write;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Mutex, OnceLock};

use westplay_shared::crash;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{FlushFileBuffers, WriteFile};
use windows::Win32::System::Console::{GetStdHandle, STD_ERROR_HANDLE, SetStdHandle};
use windows::Win32::System::Diagnostics::Debug::{EXCEPTION_POINTERS, SetUnhandledExceptionFilter};
use windows::Win32::System::SystemInformation::GetLocalTime;

static FILE: Mutex<Option<File>> = Mutex::new(None);

/// Where this run's `app.log` is, once [`init`] made it.
static PLACE: OnceLock<PathBuf> = OnceLock::new();

/// The log file's handle, for [`on_crash`] to write with no lock (0: none).
static CRASH_FILE: AtomicIsize = AtomicIsize::new(0);

/// The run before's log, kept beside this run's.
const OLD_LOG: &str = "app.old.log";

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
/// WestPlay's own folder, saying what Documents answered. Its first line is WestPlay's version. Where neither can
/// be written, lines are only printed. A panic in any thread is kept in it too (`log_panics`), and a native
/// crash ([`catch_crashes`]).
pub fn init() {
    log_panics();
    let (made, refused) = open(dir(), fallback_dir());
    let Some((file, path)) = made else { return };
    let handle = file.as_raw_handle() as isize;
    *FILE.lock().unwrap_or_else(|e| e.into_inner()) = Some(file);
    let _ = PLACE.set(path);
    write(&format!("WestPlay {}", env!("CARGO_PKG_VERSION")));
    if let Some(refused) = refused {
        write(&format!("the log is kept here, as {refused}"));
    }
    catch_crashes(handle);
}

/// Keeps a native crash in the log, the file's `handle`: one that isn't a Rust panic ends WestPlay at once and
/// said nothing (players: "it closes as I press Install", 2026-11-06). Windows' last chance for an exception
/// nothing caught writes what it was and whose code it was in ([`on_crash`]); and Rust's own last words, a stack
/// that overflowed or memory that couldn't be had (each ending the process without a panic), go to the standard
/// error, which a release build (no console) points at the log.
fn catch_crashes(handle: isize) {
    CRASH_FILE.store(handle, Ordering::Release);
    // SAFETY: `on_crash` stays valid for the process's life (this executable's own code); the standard error is
    // set only where there's none, to the log file's handle, which lives as long (`FILE` is never let go).
    unsafe {
        SetUnhandledExceptionFilter(Some(on_crash));
        let console = GetStdHandle(STD_ERROR_HANDLE).is_ok_and(|current| !current.is_invalid() && !current.0.is_null());
        if !console {
            let _ = SetStdHandle(STD_ERROR_HANDLE, HANDLE(handle as *mut core::ffi::c_void));
        }
    }
}

/// Windows' last chance for an exception nothing caught: its line in the log, then on to Windows' own handling
/// (the process ends). No locks and no memory taken: the process is failing.
///
/// # Safety
/// Called by Windows with the exception's pointers.
unsafe extern "system" fn on_crash(info: *const EXCEPTION_POINTERS) -> i32 {
    /// Windows' `EXCEPTION_CONTINUE_SEARCH`: its own handling goes on.
    const CONTINUE_SEARCH: i32 = 0;
    let handle = CRASH_FILE.load(Ordering::Acquire);
    // SAFETY: Windows' pointers to the exception, only read; the handle is the log file's, written once.
    unsafe {
        if handle == 0 || info.is_null() || (*info).ExceptionRecord.is_null() {
            return CONTINUE_SEARCH;
        }
        let now = GetLocalTime();
        let mut line = crash::Line::new();
        let _ = write!(
            line,
            "{:02}:{:02}:{:02}.{:03}  WestPlay crashed: ",
            now.wHour, now.wMinute, now.wSecond, now.wMilliseconds
        );
        crash::describe(&*(*info).ExceptionRecord, &mut line);
        let _ = line.write_str("\n");
        let file = HANDLE(handle as *mut core::ffi::c_void);
        let _ = WriteFile(file, Some(line.as_bytes()), None, None);
        let _ = FlushFileBuffers(file);
    }
    CONTINUE_SEARCH
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

/// `dir\app.log`, made anew (and `dir` with it), the run before's kept as [`OLD_LOG`].
fn create(dir: &Path) -> std::io::Result<(File, PathBuf)> {
    fs::create_dir_all(dir)?;
    let path = dir.join("app.log");
    if path.is_file() {
        let _ = fs::rename(&path, dir.join(OLD_LOG));
    }
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

    /// This test's name, for running it again as a process that crashes.
    const CRASH_TEST: &str = "log::tests::keeps_a_native_crash_and_rusts_last_words_in_the_file";

    /// Each way WestPlay can end without a panic, in a process of its own (this test run again, told how by
    /// `WESTPLAY_CRASH_TEST`), as a release build has it (no standard error): its log says what ended it.
    #[test]
    fn keeps_a_native_crash_and_rusts_last_words_in_the_file() {
        if let (Ok(how), Ok(path)) = (std::env::var("WESTPLAY_CRASH_TEST"), std::env::var("WESTPLAY_CRASH_LOG")) {
            crash_here(&how, Path::new(&path));
        }
        let dir = std::env::temp_dir().join(format!("westplay-log-crash-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let ways = [
            ("access", "WestPlay crashed: access violation (0xC0000005) at "),
            ("overflow", "has overflowed its stack"),
            ("memory", "memory allocation of 70368744177664 bytes failed"),
        ];
        for (how, said) in ways {
            let path = dir.join(format!("{how}.log"));
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", CRASH_TEST, "--nocapture", "--test-threads=1"])
                .env("WESTPLAY_CRASH_TEST", how)
                .env("WESTPLAY_CRASH_LOG", &path)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap();
            assert!(!status.success(), "{how}: it didn't end");
            let text = fs::read_to_string(&path).unwrap();
            assert!(text.contains(said), "{how}: not in the log: {text}");
        }
        // The bad access, said with where it was: this test's executable.
        let access = fs::read_to_string(dir.join("access.log")).unwrap();
        assert!(access.contains(".exe+0x") && access.contains(", writing 0x0000000000000010"), "{access}");
        fs::remove_dir_all(dir).unwrap();
    }

    /// Ends this process `how`, its log at `path` as a release build's (no standard error of its own).
    fn crash_here(how: &str, path: &Path) -> ! {
        let file = File::create(path).unwrap();
        let handle = file.as_raw_handle() as isize;
        *FILE.lock().unwrap() = Some(file);
        // SAFETY: setting this process's standard error to none, as a release build (no console) has it.
        unsafe {
            let _ = SetStdHandle(STD_ERROR_HANDLE, HANDLE::default());
        }
        catch_crashes(handle);
        match how {
            // SAFETY: none: a write to an address nothing is at, on purpose.
            "access" => unsafe { std::ptr::without_provenance_mut::<u8>(0x10).write_volatile(1) },
            "overflow" => {
                #[allow(unconditional_recursion)]
                fn deeper(depth: u64) -> u64 {
                    std::hint::black_box([depth; 64]);
                    deeper(depth + 1) + 1
                }
                deeper(0);
            }
            _ => {
                let layout = std::alloc::Layout::from_size_align(1 << 46, 1).unwrap();
                std::alloc::handle_alloc_error(layout);
            }
        }
        unreachable!("{how} didn't end the process")
    }

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
    fn keeps_the_run_befores_log_beside_this_runs() {
        let dir = std::env::temp_dir().join(format!("westplay-log-old-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("app.log"), "the run before").unwrap();
        fs::write(dir.join(OLD_LOG), "the run before that").unwrap();
        let (mut file, path) = create(&dir).unwrap();
        writeln!(file, "this run").unwrap();
        drop(file);
        assert_eq!(fs::read_to_string(&path).unwrap(), "this run\n");
        assert_eq!(fs::read_to_string(dir.join(OLD_LOG)).unwrap(), "the run before");
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
