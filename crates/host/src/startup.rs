//! Startup timing: each stage of a launch logged once (`log.rs`: the console in development, and the app's
//! log), with the local time and how long since the process started, to see where a slow launch goes. Start
//! `npm run dev` with the time printed (PowerShell: `Get-Date -Format HH:mm:ss.fff; npm run dev`), and the
//! gap to the first line is the dev server and the build before the app.

use std::collections::HashSet;
use std::sync::Mutex;

use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::SystemInformation::{GetLocalTime, GetSystemTimeAsFileTime};
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

static SEEN: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// Prints `stage` with the time, the first time it's reached (later calls with the same stage do nothing).
pub fn mark(stage: &str) {
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if !seen.get_or_insert_with(HashSet::new).insert(stage.to_owned()) {
        return;
    }
    // SAFETY: a plain query of the time.
    let now = unsafe { GetLocalTime() };
    let uptime = uptime_ms().map_or_else(|| "?".to_owned(), |ms| ms.to_string());
    log!(
        "[startup] {:02}:{:02}:{:02}.{:03}  +{uptime:>5} ms  {stage}",
        now.wHour,
        now.wMinute,
        now.wSecond,
        now.wMilliseconds
    );
}

/// Milliseconds since this process was created.
fn uptime_ms() -> Option<u64> {
    let (mut created, mut exited, mut kernel, mut user) = Default::default();
    // SAFETY: this process's own times, into locals.
    unsafe { GetProcessTimes(GetCurrentProcess(), &mut created, &mut exited, &mut kernel, &mut user) }.ok()?;
    // SAFETY: a plain query of the time.
    let now = unsafe { GetSystemTimeAsFileTime() };
    // FILETIMEs count 100 ns steps.
    let ticks = |time: FILETIME| (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
    Some(ticks(now).saturating_sub(ticks(created)) / 10_000)
}
