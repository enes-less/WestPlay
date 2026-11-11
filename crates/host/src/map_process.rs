//! A map built in WestPlay's map builder, `westplay-maps.exe` (westplay_setup::map_builder): started for the one
//! map, told what to build on its standard input, heard on its standard output, and ended with WestPlay (a job
//! object that ends what's in it once WestPlay's last handle on it is gone, WestPlay ending or crashing). What it
//! says on its standard error (a panic, a native crash) goes into WestPlay's log; a build that ends without
//! saying how is that map failing, the others still made.
//!
//! Without the builder (a development build that hasn't built it) the map is built in WestPlay's own process, as it
//! was before 2.2.2, with Windows' allocator either way.

use std::io::{BufRead, BufReader, Write};
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use westplay_setup::map_builder::{self, ALLOCATOR_VARIABLE, Failure, MIMALLOC, Request, Said};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation, SetInformationJobObject,
};
use windows::Win32::System::Threading::TerminateProcess;

/// No console window for the builder (Windows' `CREATE_NO_WINDOW`).
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// How many of the builder's last lines on its standard error are kept, for why it ended.
const LAST_LINES: usize = 40;

/// A watch's builds (`maps::MapWatch`), stopped together: the builder running ended, none started after.
#[derive(Default)]
pub struct Builds {
    stopped: AtomicBool,
    /// The builder running, by its process's handle (kept open by its `Child` while it's here).
    running: Mutex<Option<isize>>,
}

impl Builds {
    /// Stops them: the builder running ended (its map failing, not kept as a failure: `maps::carry_out`).
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        let running = self.running.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(handle) = *running {
            // SAFETY: the builder's process handle, open while it's in `running` (taken out before its `Child`
            // goes, under this lock).
            let _ = unsafe { TerminateProcess(HANDLE(handle as *mut _), 1) };
        }
    }

    pub fn stopped(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }
}

/// Builds `request`'s map with the builder `builder` (or in this process, without one), telling `said` how it
/// goes; how it ended: [`Said::Built`] or [`Said::Failed`]. Stopped (`builds`), the builder is ended.
pub fn build(builder: Option<&Path>, request: &Request, builds: &Builds, said: &mut dyn FnMut(Said)) -> Said {
    if builds.stopped() {
        return Said::Failed(Failure::Error { error: "stopped".into() });
    }
    match builder {
        Some(builder) => in_builder(builder, request, builds, said),
        None => here(request, said),
    }
}

/// In WestPlay's own process: a panic is this map failing.
fn here(request: &Request, said: &mut dyn FnMut(Said)) -> Said {
    let _effort = request.fast.then(crate::maps::total_memory).flatten().map(|total| {
        westplay_mapdata::threads::Effort::full(westplay_mapdata::threads::room_on(total, crate::maps::process_memory))
    });
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        map_builder::build(request, said, &crate::maps::process_memory)
    }));
    built.unwrap_or_else(|panic| {
        let what = crate::maps::panic_message(&*panic);
        Said::Failed(Failure::Error { error: format!("WestPlay couldn't make it ({what})") })
    })
}

/// In the builder's own process, mimalloc its allocator for a faster build.
fn in_builder(builder: &Path, request: &Request, builds: &Builds, said: &mut dyn FnMut(Said)) -> Said {
    let failed = |error: String| Said::Failed(Failure::Error { error });
    let mut command = Command::new(builder);
    command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).creation_flags(CREATE_NO_WINDOW);
    if request.fast {
        command.env(ALLOCATOR_VARIABLE, MIMALLOC);
    } else {
        command.env_remove(ALLOCATOR_VARIABLE);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return failed(format!("the map builder couldn't start: {error}")),
    };
    end_with_westplay(child.as_raw_handle());
    // Known to `builds`, to be ended if they stop; stopped meanwhile: ended now.
    *builds.running.lock().unwrap_or_else(|e| e.into_inner()) = Some(child.as_raw_handle() as isize);
    if builds.stopped() {
        builds.stop();
    }
    let asked = serde_json::to_string(request).map_err(std::io::Error::other).and_then(|line| {
        let mut stdin = child.stdin.take().ok_or_else(|| std::io::Error::other("no standard input"))?;
        stdin.write_all(line.as_bytes())?;
        stdin.write_all(b"\n")
    });
    if let Err(error) = asked {
        let _ = child.kill();
        let _ = child.wait();
        return failed(format!("the map builder couldn't be asked: {error}"));
    }
    // Its standard error, read beside (a pipe left unread would stall it), the last lines kept.
    let errors = child.stderr.take().map(|stderr| {
        std::thread::spawn(move || {
            let mut last: Vec<String> = Vec::new();
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if last.len() == LAST_LINES {
                    last.remove(0);
                }
                last.push(line);
            }
            last
        })
    });
    let mut ended = None;
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            match serde_json::from_str::<Said>(&line) {
                Ok(progress @ Said::Progress { .. }) => said(progress),
                Ok(end) => ended = Some(end),
                Err(error) => log!("map builder: a line it said isn't understood ({error}): {line}"),
            }
        }
    }
    let status = child.wait();
    *builds.running.lock().unwrap_or_else(|e| e.into_inner()) = None;
    let errors = errors.and_then(|thread| thread.join().ok()).unwrap_or_default();
    for line in &errors {
        log!("map builder: {line}");
    }
    ended.unwrap_or_else(|| {
        let code =
            status.ok().and_then(|status| status.code()).map_or("?".into(), |code| format!("{:#X}", code as u32));
        let last = said_last(&errors).map_or(String::new(), |line| format!(": {line}"));
        failed(format!("the map builder ended without finishing it (exit code {code}){last}"))
    })
}

/// What the builder said last of why it ended, from its standard error: a panic's own message (the line after
/// "panicked at …", or that line), never Rust's note under it ("note: run with `RUST_BACKTRACE=1`…": all that
/// players saw, 2026-11-10); else its last line.
fn said_last(errors: &[String]) -> Option<String> {
    let useful =
        |line: &&String| !line.trim().is_empty() && !line.trim_start().starts_with("note: run with `RUST_BACKTRACE");
    if let Some(at) = errors.iter().rposition(|line| line.contains(" panicked at ")) {
        let place = errors[at].trim();
        let message = errors.get(at + 1).filter(useful).map(|line| line.trim());
        return Some(match message {
            Some(message) => format!("{message} ({place})"),
            None => place.to_owned(),
        });
    }
    errors.iter().rev().find(useful).map(|line| line.trim().to_owned())
}

/// Puts the process `child` in the job WestPlay's builders run in, which ends them once WestPlay's handle on it
/// goes (WestPlay ending, or crashing): a builder never goes on alone.
fn end_with_westplay(child: std::os::windows::io::RawHandle) {
    static JOB: OnceLock<Option<isize>> = OnceLock::new();
    let job = JOB.get_or_init(|| {
        // SAFETY: a new job of WestPlay's, its limits set from a local of the size the call is told; the handle is
        // kept (never closed) for WestPlay's life.
        unsafe {
            let job = CreateJobObjectW(None, None).ok()?;
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
            .ok()?;
            Some(job.0 as isize)
        }
    });
    match job {
        // SAFETY: the job's handle, kept open, and the child's, alive while `child` is.
        Some(job) => unsafe {
            if let Err(error) = AssignProcessToJobObject(HANDLE(*job as *mut _), HANDLE(child)) {
                log!("map builder: not put in WestPlay's job ({error}): it would outlive WestPlay");
            }
        },
        None => log!("map builder: WestPlay's job couldn't be made: a builder would outlive WestPlay"),
    }
}

#[cfg(test)]
mod said_last_tests {
    use super::said_last;

    #[test]
    fn says_the_panic_not_rusts_note_under_it() {
        let errors: Vec<String> = [
            "thread 'main' (4242) panicked at crates\\mapdata\\src\\sector.rs:120:9:",
            "index out of bounds: the len is 3 but the index is 7",
            "note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(
            said_last(&errors).unwrap(),
            "index out of bounds: the len is 3 but the index is 7 (thread 'main' (4242) panicked at \
             crates\\mapdata\\src\\sector.rs:120:9:)"
        );
        // A native crash: its own line, the last.
        let crashed = vec!["westplay-maps crashed: access violation in westplay-maps.exe+0x1234".to_owned()];
        assert_eq!(said_last(&crashed).unwrap(), crashed[0]);
        // Only the note: nothing to say.
        let note = vec!["note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace".to_owned()];
        assert_eq!(said_last(&note), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use westplay_setup::game::{self, Kind};
    use westplay_setup::profiles::Profile;

    fn request(game: game::Game, maps: PathBuf, fast: bool) -> Request {
        Request { game, profile: Profile::new_player(Kind::Ets2), language: None, maps, fast }
    }

    fn any_game() -> game::Game {
        game::Game {
            kind: Kind::Ets2,
            dir: PathBuf::from("nowhere"),
            found_by: game::FoundBy::Chosen,
            version: None,
            dlcs: Vec::new(),
        }
    }

    /// A builder that says nothing it should and ends: this map failing, with why, never WestPlay waiting on it.
    #[test]
    fn a_builder_that_ends_without_finishing_fails_the_map_saying_so() {
        let not_a_builder = PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32").join("whoami.exe");
        let mut progress = 0;
        let builds = Builds::default();
        let ended =
            build(Some(&not_a_builder), &request(any_game(), std::env::temp_dir(), false), &builds, &mut |_| {
                progress += 1
            });
        assert_eq!(progress, 0);
        let Said::Failed(Failure::Error { error }) = ended else { panic!("{ended:?}") };
        assert!(error.starts_with("the map builder ended without finishing it (exit code 0x0)"), "{error}");
        // A builder that isn't there.
        let ended = build(
            Some(Path::new("no-such-builder.exe")),
            &request(any_game(), std::env::temp_dir(), false),
            &builds,
            &mut |_| {},
        );
        let Said::Failed(Failure::Error { error }) = ended else { panic!("{ended:?}") };
        assert!(error.starts_with("the map builder couldn't start: "), "{error}");
    }

    /// A builder still building when its watch stops is ended then, never left building; stopped, none starts.
    #[test]
    fn stopping_ends_the_builder_building() {
        // A builder that never says anything nor ends: a loop in a batch file, in its one process.
        let dir = std::env::temp_dir().join(format!("westplay-stop-builder-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let builder = dir.join("builds-forever.bat");
        std::fs::write(&builder, "@echo off\r\n:again\r\ngoto again\r\n").unwrap();
        let builds = std::sync::Arc::new(Builds::default());
        let stopper = builds.clone();
        let started = std::time::Instant::now();
        let thread = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(500));
            stopper.stop();
        });
        let ended = build(Some(&builder), &request(any_game(), dir.clone(), false), &builds, &mut |_| {});
        thread.join().unwrap();
        assert!(started.elapsed() < std::time::Duration::from_secs(10), "{:?}", started.elapsed());
        assert!(matches!(ended, Said::Failed(_)), "{ended:?}");
        assert!(builds.running.lock().unwrap().is_none());
        // Stopped: the next isn't started.
        let ended = build(Some(&builder), &request(any_game(), dir.clone(), false), &builds, &mut |_| {});
        let Said::Failed(Failure::Error { error }) = ended else { panic!("{ended:?}") };
        assert_eq!(error, "stopped");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The base game's map built by the real builder (`WESTPLAY_MAP_BUILDER`: its file, e.g. the release build's),
    /// into `WESTPLAY_MAP_OUT`, faster with `WESTPLAY_MAP_FAST`: its progress heard, and built.
    #[test]
    #[ignore = "builds a whole map with the installed game: minutes"]
    fn the_builder_builds_the_base_games_map() {
        let builder = PathBuf::from(std::env::var_os("WESTPLAY_MAP_BUILDER").expect("WESTPLAY_MAP_BUILDER"));
        let out = PathBuf::from(std::env::var_os("WESTPLAY_MAP_OUT").expect("WESTPLAY_MAP_OUT"));
        let fast = std::env::var_os("WESTPLAY_MAP_FAST").is_some();
        let game = game::find(Kind::Ets2).into_iter().next().expect("ETS2 installed");
        // In the game's language, as WestPlay asks.
        let documents = westplay_setup::places::game_documents(Kind::Ets2).expect("the game's documents");
        let language = std::fs::read_to_string(documents.join("config.cfg"))
            .ok()
            .and_then(|config| westplay_mapdata::locale::game_language(&config));
        let started = std::time::Instant::now();
        let (mut said, mut most) = (0, 0u64);
        let builds = Builds::default();
        let request = Request { language, ..request(game, out, fast) };
        let ended = build(Some(&builder), &request, &builds, &mut |progress| {
            if let Said::Progress { memory, .. } = progress {
                said += 1;
                most = most.max(memory.unwrap_or(0));
            }
        });
        println!(
            "{ended:?} in {:.1?}, {said} progress lines, the builder's memory at most {:.2} GB",
            started.elapsed(),
            most as f64 / (1u64 << 30) as f64
        );
        assert!(matches!(ended, Said::Built(_)), "{ended:?}");
        assert!(said > 50);
    }
}
