//! Shared plumbing for the plugin's function hooks (`d3d`, `input`), and the guards that keep a panic in the
//! plugin's code from ever reaching the game (`guarded` for hooks, `contained` for what the game calls).
//!
//! MinHook patches function bodies inside system DLLs, so a hook applies to every object of that type in the
//! game, the game's own included. Hooks are created disabled and switched on together by [`enable_all`].

use std::cell::Cell;
use std::ffi::c_void;
use std::panic::AssertUnwindSafe;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use minhook::MinHook;

use crate::log;

static HOOK_TARGETS: Mutex<Vec<usize>> = Mutex::new(Vec::new());

/// Hooks the function at `target`; `original` receives the trampoline that calls the original function.
///
/// # Safety
/// `target` is the start of a function in this process, and `detour` a function of the same signature that
/// calls on through `original`; created disabled, on until [`enable_all`].
pub unsafe fn hook(target: usize, detour: *mut c_void, original: &AtomicUsize, name: &str) -> Result<(), String> {
    // SAFETY: a function and its detour of the same signature (the caller's contract).
    let trampoline = unsafe { MinHook::create_hook(target as *mut c_void, detour) }
        .map_err(|e| format!("create_hook {name}: {e:?}"))?;
    original.store(trampoline as usize, Ordering::Release);
    HOOK_TARGETS.lock().unwrap_or_else(|e| e.into_inner()).push(target);
    log!("hooked {name} at {target:#x}");
    Ok(())
}

/// Switches every hook on.
///
/// # Safety
/// Every hook created was made as [`hook`] requires, and its detour is ready to run (on any thread).
pub unsafe fn enable_all() -> Result<(), String> {
    // SAFETY: hooks made as `hook` requires (the caller's contract).
    unsafe { MinHook::enable_all_hooks() }.map_err(|e| format!("enable_all_hooks: {e:?}"))
}

/// Switches every hook off and removes it, the functions as they were.
///
/// # Safety
/// No detour may be running on another thread meanwhile, or run after (the caller waits for them to return
/// before the plugin can be unloaded: `scs_telemetry_shutdown`).
pub unsafe fn remove_all() {
    for target in HOOK_TARGETS.lock().unwrap_or_else(|e| e.into_inner()).drain(..) {
        // SAFETY: a hook this plugin made, removed once (taken out of the list).
        unsafe {
            let _ = MinHook::disable_hook(target as *mut c_void);
            let _ = MinHook::remove_hook(target as *mut c_void);
        }
    }
}

/// The function a vtable slot points to.
///
/// # Safety
/// `object` is a live COM object whose vtable has more than `slot` entries.
pub unsafe fn vtable_entry(object: *mut c_void, slot: usize) -> usize {
    // SAFETY: a COM object starts with its vtable's address, and the slot is within it (the caller's contract).
    unsafe { *(*(object as *const *const usize)).add(slot) }
}

thread_local! {
    static IN_WESTPLAY: Cell<bool> = const { Cell::new(false) };
}

/// Runs plugin code from a hook. Re-entrant calls are skipped (our own calls can land in hooked functions),
/// and panics are contained so they can never unwind into or abort the game.
pub fn guarded(f: impl FnOnce()) {
    if IN_WESTPLAY.with(|flag| flag.replace(true)) {
        return;
    }
    if std::panic::catch_unwind(AssertUnwindSafe(f)).is_err() {
        log!("panic inside a hook was contained");
    }
    IN_WESTPLAY.with(|flag| flag.set(false));
}

/// Runs plugin code the game calls directly (the SDK's init and shutdown, its telemetry callbacks): a panic is
/// contained and `failed` returned instead, so it can never unwind into or abort the game.
pub fn contained<T>(what: &str, failed: T, f: impl FnOnce() -> T) -> T {
    std::panic::catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| {
        log!("panic in {what} was contained");
        failed
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_the_game_called_into_comes_back_as_failed() {
        assert_eq!(contained("a test", -1, || 0), 0);
        assert_eq!(contained("a test", -1, || panic!("a bug in the plugin")), -1);
    }

    #[test]
    fn a_panic_in_a_hook_stops_there_and_the_next_call_still_runs() {
        guarded(|| panic!("a bug in a hook"));
        let mut ran = false;
        guarded(|| ran = true);
        assert!(ran);
    }
}
