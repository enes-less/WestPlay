//! File logging to `Documents\WestPlay\logs`. The SCS log callback may only be used on the game's main
//! thread during SDK callbacks, so everything else (render-thread hooks) goes through this file logger.
//!
//! The game's threads never touch the disk: a line logged is queued, and a thread of the plugin's own
//! writes it (and the other files, [`write_file_later`]); a write stalling on the disk would stall the
//! game's frame. That thread runs the plugin's code, so [`shutdown`] stops it before the plugin is
//! unloaded; lines logged after that are written straight away.
//!
//! A crash is written straight away too, by the plugin's exception handler ([`on_exception`]): the game closing as
//! it starts left no line after the plugin's (players, 2026-11-06), so nothing said whose code it was in.

use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicIsize, AtomicU32, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::Instant;

use westplay_shared::crash;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{FlushFileBuffers, WriteFile};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Diagnostics::Debug::{
    AddVectoredExceptionHandler, EXCEPTION_POINTERS, LPTOP_LEVEL_EXCEPTION_FILTER, RemoveVectoredExceptionHandler,
    SetUnhandledExceptionFilter,
};
use windows::Win32::UI::Shell::{FOLDERID_Documents, KF_FLAG_DEFAULT, SHGetKnownFolderPath};

/// The log file while no writer thread runs (before [`init`]'s, and after [`shutdown`]).
static LOG_FILE: Mutex<Option<File>> = Mutex::new(None);
static WRITER: Mutex<Option<Writer>> = Mutex::new(None);
static START: OnceLock<Instant> = OnceLock::new();
static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();

/// The log file for [`on_exception`] (its handle, written with no lock; 0: none), and the copy of it that keeps
/// the handle open.
static CRASH_FILE: AtomicIsize = AtomicIsize::new(0);
static CRASH_FILE_KEPT: Mutex<Option<File>> = Mutex::new(None);
/// The exception handler while it's registered (0: not).
static EXCEPTION_HANDLER: AtomicUsize = AtomicUsize::new(0);
/// The places (instructions) exceptions were written at, each once (0: a free slot): code that catches its own
/// exceptions raises them at one place again and again (a player's OptiScaler, 2026-11-07, probing memory byte by
/// byte: 7 of the 8 lines 2.2.2 kept, the one that closed the game seconds later never written).
static EXCEPTIONS_SAID_AT: [AtomicUsize; EXCEPTION_PLACES] = [const { AtomicUsize::new(0) }; EXCEPTION_PLACES];
const EXCEPTION_PLACES: usize = 32;
/// The filter Windows called for an exception nothing caught before the plugin's ([`on_unhandled`]) (0: none).
static UNHANDLED_BEFORE: AtomicUsize = AtomicUsize::new(0);
/// Whether the plugin's [`on_unhandled`] is set (1) or not (0).
static UNHANDLED_SET: AtomicU32 = AtomicU32::new(0);

/// A file to write in the log folder: its name, and what to write, made on the writer's thread.
type FileJob = (&'static str, Box<dyn FnOnce() -> String + Send>);

enum Job {
    Line(String),
    File(FileJob),
}

/// The thread writing the logs, and the way to hand it work.
struct Writer {
    jobs: Sender<Job>,
    thread: JoinHandle<File>,
}

/// Starts the thread writing `file` (and other files in `dir`), handing the file back when its jobs end.
fn start_writer(dir: PathBuf, mut file: File) -> std::io::Result<Writer> {
    let (jobs, queue) = mpsc::channel::<Job>();
    let thread = std::thread::Builder::new().name("westplay-log".into()).spawn(move || {
        while let Ok(job) = queue.recv() {
            run(&mut file, &dir, job);
            // Whatever else is queued already, then one flush: a crash loses at most what came meanwhile.
            while let Ok(next) = queue.try_recv() {
                run(&mut file, &dir, next);
            }
            let _ = file.flush();
        }
        file
    })?;
    Ok(Writer { jobs, thread })
}

fn run(file: &mut File, dir: &Path, job: Job) {
    match job {
        Job::Line(line) => {
            let _ = writeln!(file, "{line}");
        }
        Job::File((name, contents)) => {
            let _ = fs::write(dir.join(name), contents());
        }
    }
}

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => { $crate::log::write(&format!($($arg)*)) };
}

/// Starts the log afresh (`plugin.log` emptied), unless it's running already: the game's telemetry and
/// TruckersMP's SDK each start the plugin, in either order, and the second mustn't empty the first's lines.
/// After [`shutdown`] (an `sdk reload`), it starts afresh again.
pub fn init() {
    if WRITER.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
        return;
    }
    START.get_or_init(Instant::now);
    let dir = log_dir();
    let _ = fs::create_dir_all(dir);
    let Ok(file) = OpenOptions::new().create(true).write(true).truncate(true).open(dir.join("plugin.log")) else {
        return;
    };
    catch_crashes(&file);
    // Without its thread, lines are written straight away.
    let fallback = file.try_clone().ok();
    match start_writer(dir.clone(), file) {
        Ok(writer) => *WRITER.lock().unwrap_or_else(|e| e.into_inner()) = Some(writer),
        Err(_) => *LOG_FILE.lock().unwrap_or_else(|e| e.into_inner()) = fallback,
    }
}

/// Stops the writer thread once everything queued is written, and the exception handler (call before the plugin
/// is unloaded: both run the plugin's code).
pub fn shutdown() {
    stop_catching_crashes();
    let writer = WRITER.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(Writer { jobs, thread }) = writer {
        drop(jobs);
        if let Ok(file) = thread.join() {
            *LOG_FILE.lock().unwrap_or_else(|e| e.into_inner()) = Some(file);
        }
    }
}

pub fn write(message: &str) {
    let elapsed = START.get_or_init(Instant::now).elapsed();
    let line = format!("[{:>9.3}] {message}", elapsed.as_secs_f64());
    let line = match queue(Job::Line(line)) {
        Ok(()) => return,
        Err(Job::Line(line)) => line,
        Err(Job::File(_)) => return,
    };
    let mut guard = LOG_FILE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(file) = guard.as_mut() {
        let _ = writeln!(file, "{line}");
        let _ = file.flush();
    }
}

/// Writes a crash into `file` (the log) as it happens ([`on_exception`]): registered once, until [`shutdown`].
fn catch_crashes(file: &File) {
    if let Ok(kept) = file.try_clone() {
        CRASH_FILE.store(kept.as_raw_handle() as isize, Ordering::Release);
        *CRASH_FILE_KEPT.lock().unwrap_or_else(|e| e.into_inner()) = Some(kept);
    }
    if EXCEPTION_HANDLER.load(Ordering::Acquire) != 0 {
        return;
    }
    // SAFETY: `on_exception` is the plugin's own, valid until [`stop_catching_crashes`] takes it out before the
    // plugin is unloaded.
    let handler = unsafe { AddVectoredExceptionHandler(1, Some(on_exception)) };
    EXCEPTION_HANDLER.store(handler as usize, Ordering::Release);
    // SAFETY: `on_unhandled` likewise; the filter it replaces is kept, called after it, and put back.
    let before = unsafe { SetUnhandledExceptionFilter(Some(on_unhandled)) };
    UNHANDLED_BEFORE.store(before.map_or(0, |filter| filter as usize), Ordering::Release);
    UNHANDLED_SET.store(1, Ordering::Release);
}

/// Takes the exception handler out (the plugin is being unloaded: its code is going).
fn stop_catching_crashes() {
    let handler = EXCEPTION_HANDLER.swap(0, Ordering::AcqRel);
    if handler != 0 {
        // SAFETY: the handle `AddVectoredExceptionHandler` gave, taken out once.
        unsafe { RemoveVectoredExceptionHandler(handler as *const core::ffi::c_void) };
    }
    if UNHANDLED_SET.swap(0, Ordering::AcqRel) != 0 {
        let before = UNHANDLED_BEFORE.swap(0, Ordering::AcqRel);
        // SAFETY: the filter that was there before the plugin's put back as it was (none, 0); one set over the
        // plugin's since is left where it is (it may call the plugin's, which can't be known).
        unsafe {
            let before: LPTOP_LEVEL_EXCEPTION_FILTER = (before != 0).then(|| {
                std::mem::transmute::<usize, unsafe extern "system" fn(*const EXCEPTION_POINTERS) -> i32>(before)
            });
            let current = SetUnhandledExceptionFilter(before);
            if current.is_some_and(|filter| filter as usize != on_unhandled as *const () as usize) {
                SetUnhandledExceptionFilter(current);
            }
        }
    }
}

/// Each of the game's exceptions as it's raised, before anything catches it: one that ends a process
/// (`crash::FATAL`) is written into the log at once, with whose code it was in (the plugin's, the game's, another
/// plugin's), once for each place it's raised at, up to [`EXCEPTION_PLACES`]. Written straight to the file, with no
/// lock and no memory taken: the game may be failing. Nothing else is done: the game's own handling goes on (and may
/// catch it: [`on_unhandled`] says the one nothing did).
///
/// # Safety
/// Called by Windows with the exception's pointers.
unsafe extern "system" fn on_exception(info: *mut EXCEPTION_POINTERS) -> i32 {
    /// Windows' `EXCEPTION_CONTINUE_SEARCH`: the exception goes on to whatever handles it.
    const CONTINUE_SEARCH: i32 = 0;
    // SAFETY: Windows' pointers to the exception, only read; the handle is the log file's, kept open.
    unsafe {
        let Some(record) = info.as_ref().and_then(|info| info.ExceptionRecord.as_ref()) else {
            return CONTINUE_SEARCH;
        };
        let handle = CRASH_FILE.load(Ordering::Acquire);
        if handle == 0 || !crash::is_fatal(record.ExceptionCode.0 as u32) || !first_at(record.ExceptionAddress as usize)
        {
            return CONTINUE_SEARCH;
        }
        let elapsed = START.get().map_or(0.0, |start| start.elapsed().as_secs_f64());
        let mut line = crash::Line::new();
        let _ = write!(line, "[{elapsed:>9.3}] an exception, caught or not (said once for its place): ");
        crash::describe(record, &mut line);
        let _ = line.write_str("\n");
        let file = HANDLE(handle as *mut core::ffi::c_void);
        let _ = WriteFile(file, Some(line.as_bytes()), None, None);
        write_callers(file, record.ExceptionAddress as usize);
        let _ = FlushFileBuffers(file);
    }
    CONTINUE_SEARCH
}

/// Writes into the log (`file`) the calls that led to an exception at `fault`, as [`on_exception`] writes: on the
/// stack, no lock taken (`crash::callers`).
fn write_callers(file: HANDLE, fault: usize) {
    let mut frames = [core::ptr::null_mut(); crash::CALLERS];
    let found = crash::callers(fault, &mut frames);
    let mut line = crash::Line::new();
    let _ = line.write_str("            called from: ");
    crash::write_callers(found, &mut line);
    let _ = line.write_str("\n");
    // SAFETY: the log file's handle, kept open; a line on the stack.
    let _ = unsafe { WriteFile(file, Some(line.as_bytes()), None, None) };
}

/// Whether `place` (an exception's instruction) is new: taken into [`EXCEPTIONS_SAID_AT`], while there's room.
fn first_at(place: usize) -> bool {
    for slot in &EXCEPTIONS_SAID_AT {
        match slot.compare_exchange(0, place, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return true,
            Err(taken) if taken == place => return false,
            Err(_) => {}
        }
    }
    false
}

/// An exception nothing caught, the one that ends the game (Windows' last chance before its own crash handling):
/// written into the log as [`on_exception`] writes, then handed to the filter that was there before the plugin's.
///
/// # Safety
/// Called by Windows with the exception's pointers.
unsafe extern "system" fn on_unhandled(info: *const EXCEPTION_POINTERS) -> i32 {
    /// Windows' `EXCEPTION_CONTINUE_SEARCH`: its own handling goes on.
    const CONTINUE_SEARCH: i32 = 0;
    // SAFETY: Windows' pointers to the exception, only read; the handle is the log file's, kept open; the filter
    // before the plugin's is called with what Windows gave this one.
    unsafe {
        let handle = CRASH_FILE.load(Ordering::Acquire);
        if handle != 0
            && let Some(record) = info.as_ref().and_then(|info| info.ExceptionRecord.as_ref())
        {
            let elapsed = START.get().map_or(0.0, |start| start.elapsed().as_secs_f64());
            let mut line = crash::Line::new();
            let _ = write!(line, "[{elapsed:>9.3}] the game is crashing, nothing caught it: ");
            crash::describe(record, &mut line);
            let _ = line.write_str("\n");
            let file = HANDLE(handle as *mut core::ffi::c_void);
            let _ = WriteFile(file, Some(line.as_bytes()), None, None);
            let _ = FlushFileBuffers(file);
        }
        // The calls that led there were written with its first sighting ([`on_exception`]): from here, Windows'
        // walk of the stack doesn't reach them.
        match UNHANDLED_BEFORE.load(Ordering::Acquire) {
            0 => CONTINUE_SEARCH,
            before => {
                let before =
                    std::mem::transmute::<usize, unsafe extern "system" fn(*const EXCEPTION_POINTERS) -> i32>(before);
                before(info)
            }
        }
    }
}

/// Writes the file `name` in the log folder with what `contents` makes, on the writer's thread (nothing
/// without it: such files are diagnostics, not worth a game thread's time).
pub fn write_file_later(name: &'static str, contents: impl FnOnce() -> String + Send + 'static) {
    let _ = queue(Job::File((name, Box::new(contents))));
}

/// Hands a job to the writer thread, or back if there's none.
fn queue(job: Job) -> Result<(), Job> {
    match WRITER.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        Some(writer) => writer.jobs.send(job).map_err(|mpsc::SendError(job)| job),
        None => Err(job),
    }
}

pub fn log_dir() -> &'static PathBuf {
    LOG_DIR.get_or_init(|| documents_dir().join("WestPlay").join("logs"))
}

/// The player's Documents folder (Windows' own, wherever it's moved).
pub fn documents_dir() -> PathBuf {
    // SAFETY: Windows hands back a CoTaskMem string, read before it's freed here.
    unsafe {
        match SHGetKnownFolderPath(&FOLDERID_Documents, KF_FLAG_DEFAULT, None) {
            Ok(pwstr) => {
                let path = pwstr.to_string().map(PathBuf::from);
                CoTaskMemFree(Some(pwstr.0 as *const _));
                path.unwrap_or_else(|_| fallback_documents_dir())
            }
            Err(_) => fallback_documents_dir(),
        }
    }
}

fn fallback_documents_dir() -> PathBuf {
    std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("Documents")).unwrap_or_else(std::env::temp_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This test's name, for running it again as a process that crashes.
    const CRASH_TEST: &str = "log::tests::writes_a_crash_into_the_log_as_it_happens_and_lets_it_go_on";

    /// A bad memory access in a process of its own (this test run again, told by `WESTPLAY_CRASH_LOG`): the log
    /// says what it was and where, the process still ends as it would have. Taken out, the handler is gone.
    #[test]
    fn writes_a_crash_into_the_log_as_it_happens_and_lets_it_go_on() {
        if let Ok(path) = std::env::var("WESTPLAY_CRASH_LOG") {
            let file = File::create(path).unwrap();
            catch_crashes(&file);
            // SAFETY: none: a write to an address nothing is at, on purpose.
            unsafe { std::ptr::without_provenance_mut::<u8>(0x10).write_volatile(1) };
            unreachable!("the bad access didn't end the process");
        }
        let dir = std::env::temp_dir().join(format!("westplay-plugin-crash-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("plugin.log");
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", CRASH_TEST, "--nocapture", "--test-threads=1"])
            .env("WESTPLAY_CRASH_LOG", &path)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(!status.success(), "the crash was caught: the game would have gone on wrongly");
        let text = fs::read_to_string(&path).unwrap();
        // Seen as it's raised, then, nothing catching it, as what ends the process: both at its place.
        assert!(
            text.contains("] an exception, caught or not (said once for its place): access violation (0xC0000005) at "),
            "{text}"
        );
        assert!(
            text.contains("] the game is crashing, nothing caught it: access violation (0xC0000005) at "),
            "{text}"
        );
        // With the calls that led there, from the faulting function's caller down to the thread's start: the test
        // harness's, in this test's executable, then Windows'.
        let called = text.lines().find(|line| line.starts_with("            called from: ")).unwrap_or_default();
        assert!(called.contains(".exe+0x") && called.contains(" <- ") && called.contains("ntdll.dll+0x"), "{text}");
        assert_eq!(text.matches("            called from: ").count(), 1, "{text}");
        assert_eq!(text.matches(", writing 0x0000000000000010").count(), 2, "{text}");
        let _ = fs::remove_dir_all(&dir);
        // In this process: registered once, taken out by the shutdown.
        let file = File::create(std::env::temp_dir().join(format!("westplay-plugin-crash-{}.log", std::process::id())))
            .unwrap();
        catch_crashes(&file);
        let first = EXCEPTION_HANDLER.load(Ordering::Acquire);
        catch_crashes(&file);
        assert!(first != 0 && EXCEPTION_HANDLER.load(Ordering::Acquire) == first, "registered once");
        stop_catching_crashes();
        assert_eq!(EXCEPTION_HANDLER.load(Ordering::Acquire), 0);
        assert_eq!(UNHANDLED_SET.load(Ordering::Acquire), 0);
    }

    /// An exception raised at one place again and again (code catching its own) is written once, and others still
    /// get their line after it.
    #[test]
    fn says_each_place_once() {
        assert!(first_at(0x7000_0001));
        assert!(!first_at(0x7000_0001));
        assert!(!first_at(0x7000_0001));
        assert!(first_at(0x7000_0002));
    }

    #[test]
    fn writes_off_the_callers_thread_in_order_and_finishes_before_handing_the_file_back() {
        let dir = std::env::temp_dir().join(format!("westplay-log-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = File::create(dir.join("plugin.log")).unwrap();
        let writer = start_writer(dir.clone(), file).unwrap();
        for i in 0..1000 {
            writer.jobs.send(Job::Line(format!("line {i}"))).unwrap();
        }
        // A file's contents are made on the writer's thread, not the caller's (a game thread, in the game).
        let caller = std::thread::current().id();
        let (made_on, made) = mpsc::channel();
        writer
            .jobs
            .send(Job::File((
                "textures.txt",
                Box::new(move || {
                    made_on.send(std::thread::current().id()).unwrap();
                    "kinds".to_string()
                }),
            )))
            .unwrap();
        // Stopping waits for everything queued.
        let Writer { jobs, thread } = writer;
        drop(jobs);
        drop(thread.join().unwrap());
        assert_ne!(made.recv().unwrap(), caller);
        let lines: Vec<String> =
            fs::read_to_string(dir.join("plugin.log")).unwrap().lines().map(String::from).collect();
        assert_eq!(lines.len(), 1000);
        assert!(lines.iter().enumerate().all(|(i, line)| *line == format!("line {i}")));
        assert_eq!(fs::read_to_string(dir.join("textures.txt")).unwrap(), "kinds");
        let _ = fs::remove_dir_all(&dir);
    }
}
