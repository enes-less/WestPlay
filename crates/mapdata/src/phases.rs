//! Where a map's build spends its time, step by step: with `WESTPLAY_MAP_PHASES` set, each step's start is written to
//! the standard error with the seconds since the first, for measuring the progress shares the bar gives each step
//! (export.rs; the review of 2026-11-11, M3: stretches of a minute and more the bar never moved through); and the
//! largest of what's measured ([`most`]), for bounding work by real data. Nothing is written without it.

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("WESTPLAY_MAP_PHASES").is_some())
}

/// The largest of each thing measured, and whether it grew since it was last written.
fn largest() -> &'static Mutex<BTreeMap<&'static str, (u64, bool)>> {
    static LARGEST: OnceLock<Mutex<BTreeMap<&'static str, (u64, bool)>>> = OnceLock::new();
    LARGEST.get_or_init(Mutex::default)
}

/// Marks the start of the step `name` (when measuring), with the largest of what's measured that grew since.
pub fn mark(name: &str) {
    static START: OnceLock<Instant> = OnceLock::new();
    if on() {
        let mut largest = largest().lock().unwrap_or_else(|e| e.into_inner());
        for (what, (value, grew)) in largest.iter_mut().filter(|(_, (_, grew))| *grew) {
            eprintln!("largest {what}: {value}");
            *grew = false;
        }
        eprintln!("phase {:8.2} s  {name}", START.get_or_init(Instant::now).elapsed().as_secs_f64());
    }
}

/// Keeps `value` if it's the largest `what` yet (when measuring): written at the next [`mark`].
pub fn most(what: &'static str, value: u64) {
    if on() {
        let mut largest = largest().lock().unwrap_or_else(|e| e.into_inner());
        let kept = largest.entry(what).or_insert((value, true));
        if value > kept.0 {
            *kept = (value, true);
        }
    }
}
