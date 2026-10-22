//! File logging to `Documents\WestPlay\logs`. The SCS log callback may only be used on the game's main
//! thread during SDK callbacks, so everything else (render-thread hooks) goes through this file logger.
//!
//! The game's threads never touch the disk: a line logged is queued, and a thread of the plugin's own
//! writes it (and the other files, [`write_file_later`]); a write stalling on the disk would stall the
//! game's frame. That thread runs the plugin's code, so [`shutdown`] stops it before the plugin is
//! unloaded; lines logged after that are written straight away.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::Instant;

use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{FOLDERID_Documents, KF_FLAG_DEFAULT, SHGetKnownFolderPath};

/// The log file while no writer thread runs (before [`init`]'s, and after [`shutdown`]).
static LOG_FILE: Mutex<Option<File>> = Mutex::new(None);
static WRITER: Mutex<Option<Writer>> = Mutex::new(None);
static START: OnceLock<Instant> = OnceLock::new();
static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();

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
    // Without its thread, lines are written straight away.
    let fallback = file.try_clone().ok();
    match start_writer(dir.clone(), file) {
        Ok(writer) => *WRITER.lock().unwrap_or_else(|e| e.into_inner()) = Some(writer),
        Err(_) => *LOG_FILE.lock().unwrap_or_else(|e| e.into_inner()) = fallback,
    }
}

/// Stops the writer thread once everything queued is written (call before the plugin is unloaded).
pub fn shutdown() {
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
