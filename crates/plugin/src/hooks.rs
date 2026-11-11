//! Shared plumbing for the plugin's function hooks (`d3d`, `input`), and the guards that keep a panic in the
//! plugin's code from ever reaching the game (`guarded` for hooks, `contained` for what the game calls).
//!
//! MinHook patches function bodies inside system DLLs, so a hook applies to every object of that type in the
//! game, the game's own included. Hooks are created disabled and switched on together by [`enable_all`].
//!
//! A vtable hook ([`hook_vtable`]) instead swaps one entry of a class's table of functions, shared by every object
//! of the class: no code is patched, so it sits in front of whatever else hooks the function (a layer wrapping it,
//! an overlay patching its first bytes) without touching them.

use std::cell::Cell;
use std::ffi::c_void;
use std::panic::AssertUnwindSafe;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use minhook::MinHook;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_PIN,
    GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, GetModuleFileNameW, GetModuleHandleExW,
};
use windows::Win32::System::Memory::{PAGE_PROTECTION_FLAGS, PAGE_READWRITE, VirtualProtect};
use windows::core::PCWSTR;

use crate::log;

static HOOK_TARGETS: Mutex<Vec<usize>> = Mutex::new(Vec::new());

/// The vtable entries swapped ([`hook_vtable`]): each entry's address, our detour, and what it held before.
static VTABLE_HOOKS: Mutex<Vec<(usize, usize, usize)>> = Mutex::new(Vec::new());

/// A function hooked again over another's jump ([`hook_over_jump`]): its address, its first eight bytes as we left
/// them, and as they were before (the other's jump).
type OverJump = (usize, [u8; 8], [u8; 8]);

static OVER_JUMPS: Mutex<Vec<OverJump>> = Mutex::new(Vec::new());

/// Tests that swap vtable entries and take them all out (`remove_all`), one at a time.
#[cfg(test)]
pub(crate) static VTABLE_TESTS: Mutex<()> = Mutex::new(());

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
    log!("hooked {name} at {target:#x} in {}", module_of(target));
    Ok(())
}

/// The file of the module `address` is in (for the log: whose function a hook is on, the system's or a layer's
/// over it, as an upscaler's).
pub fn module_of(address: usize) -> String {
    let mut module = HMODULE::default();
    let flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;
    // SAFETY: `address` is only looked up, never followed; `module` and `path` are locals.
    unsafe {
        if GetModuleHandleExW(flags, PCWSTR(address as *const u16), &mut module).is_err() {
            return "?".into();
        }
        let mut path = [0u16; 260];
        let length = GetModuleFileNameW(Some(module), &mut path) as usize;
        String::from_utf16_lossy(&path[..length])
    }
}

/// Hooks entry `slot` of the vtable at `vtable` (shared by every object of its class: it must lie in a module's
/// image, [`in_a_module`]), on at once; `original` receives what the entry held, which the detour calls on through.
/// Under OptiScaler (2026-11-07) the swap chain's `Present` was its wrapper's, whose first bytes the Steam overlay had
/// patched; patched again by us, the three made a chain that handed Windows a wrapper as its swap chain and the game
/// crashed on its first frame. An entry swapped touches none of them.
///
/// # Safety
/// `vtable` is a live vtable with more than `slot` entries, and `detour` a function of the entry's signature that
/// calls on through `original`, ready to run on any thread from the moment it's in.
pub unsafe fn hook_vtable(
    vtable: usize,
    slot: usize,
    detour: *mut c_void,
    original: &AtomicUsize,
    name: &str,
) -> Result<(), String> {
    let entry = vtable + slot * size_of::<usize>();
    // SAFETY: the vtable's entry (the caller's contract), read before it's written.
    let before = unsafe { std::ptr::read_volatile(entry as *const usize) };
    if before == detour as usize {
        return Ok(());
    }
    original.store(before, Ordering::Release);
    // SAFETY: the entry, made writable for the one write and given its protection back.
    unsafe { write_entry(entry, detour as usize) }?;
    VTABLE_HOOKS.lock().unwrap_or_else(|e| e.into_inner()).push((entry, detour as usize, before));
    log!(
        "hooked {name} in its vtable at {entry:#x} in {}, before it {before:#x} in {}",
        module_of(vtable),
        module_of(before)
    );
    Ok(())
}

/// Writes `value` into the vtable entry at `entry` (in a read-only section: made writable for the write).
///
/// # Safety
/// `entry` is a live vtable entry.
unsafe fn write_entry(entry: usize, value: usize) -> Result<(), String> {
    let mut protection = PAGE_PROTECTION_FLAGS(0);
    // SAFETY: the entry's own page, its protection given back after the one aligned write.
    unsafe {
        VirtualProtect(entry as *const c_void, size_of::<usize>(), PAGE_READWRITE, &mut protection)
            .map_err(|e| format!("VirtualProtect: {e}"))?;
        std::ptr::write_volatile(entry as *mut usize, value);
        let mut was = PAGE_PROTECTION_FLAGS(0);
        let _ = VirtualProtect(entry as *const c_void, size_of::<usize>(), protection, &mut was);
    }
    Ok(())
}

/// Whether `address` lies in a module's image (a DLL or the game's executable): a vtable there is its class's,
/// shared by every object of it, where one in heap memory may be its object's own.
pub fn in_a_module(address: usize) -> bool {
    let mut module = HMODULE::default();
    let flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;
    // SAFETY: `address` is only looked up, never followed.
    unsafe { GetModuleHandleExW(flags, PCWSTR(address as *const u16), &mut module).is_ok() }
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
    // Hooked again over another's jump: its jump put back as it was, if ours is still there (else hooked over again
    // since, which calls on into ours: the plugin stays loaded). MinHook's own hook on the function is left alone
    // then: taking it out would write the function's first bytes back over the other's.
    let over: Vec<_> = OVER_JUMPS.lock().unwrap_or_else(|e| e.into_inner()).drain(..).collect();
    for &(target, ours, before) in &over {
        // SAFETY: a function this plugin patched, as when it was patched.
        if !unsafe { swap_first_bytes(target, ours, before) } {
            log!("a function at {target:#x} hooked over since ours: the plugin stays loaded");
            pin_own_module();
        }
    }
    HOOK_TARGETS.lock().unwrap_or_else(|e| e.into_inner()).retain(|target| !over.iter().any(|&(t, _, _)| t == *target));
    for (entry, detour, before) in VTABLE_HOOKS.lock().unwrap_or_else(|e| e.into_inner()).drain(..) {
        // SAFETY: an entry this plugin swapped, read to see it's still ours.
        let now = unsafe { std::ptr::read_volatile(entry as *const usize) };
        // SAFETY: as when it was swapped.
        if now == detour && unsafe { write_entry(entry, before) }.is_ok() {
            continue;
        }
        // Hooked over by another since (it calls on into ours): ours can't be taken out from under it, so the plugin's
        // code stays loaded for good rather than be called once it's gone.
        log!("a vtable entry at {entry:#x} hooked over since ours: the plugin stays loaded");
        let mut module = HMODULE::default();
        let flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_PIN;
        // SAFETY: our own detour's address, looked up to pin its module.
        let _ = unsafe { GetModuleHandleExW(flags, PCWSTR(detour as *const u16), &mut module) };
    }
    for target in HOOK_TARGETS.lock().unwrap_or_else(|e| e.into_inner()).drain(..) {
        // SAFETY: a hook this plugin made, removed once (taken out of the list).
        unsafe {
            let _ = MinHook::disable_hook(target as *mut c_void);
            let _ = MinHook::remove_hook(target as *mut c_void);
        }
    }
}

/// A function's first eight bytes.
///
/// # Safety
/// `target` is the start of a function in this process (at least eight bytes of code).
pub unsafe fn first_bytes(target: usize) -> [u8; 8] {
    // SAFETY: readable code (the caller's contract).
    unsafe { std::ptr::read_unaligned(target as *const [u8; 8]) }
}

/// Where a function's first instruction jumps, if it's a plain jump (`jmp rel32`, as hooking tools write: RTSS's);
/// `None` for anything else.
pub fn jump_of(target: usize, bytes: [u8; 8]) -> Option<usize> {
    (bytes[0] == 0xE9).then(|| {
        let rel = i32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]);
        (target as isize + 5 + rel as isize) as usize
    })
}

/// Hooks the function at `target` again, in front of another's hook made over ours since: its first instruction is
/// that other's plain jump (RTSS writes one over any hook there before it on the game's first frame, 2026-11-08),
/// which `original` now goes on to, as the function did; the function jumps to `detour` first. Its first eight
/// bytes are changed in one go, so another thread never runs half of them; put back as they were on unload, if
/// still ours ([`remove_all`]).
///
/// # Safety
/// `target` is the start of a function in this process, and `detour` a function of the same signature that calls on
/// through `original`.
pub unsafe fn hook_over_jump(target: usize, detour: usize, original: &AtomicUsize, name: &str) -> Result<(), String> {
    // SAFETY: the function's code (the caller's contract).
    let before = unsafe { first_bytes(target) };
    let jump = jump_of(target, before).ok_or_else(|| format!("{name}: its first instruction isn't a plain jump"))?;
    // A jump of ours to `detour` from within reach of a 32-bit jump: `jmp [rip+0]` and the address after it.
    // SAFETY: memory of our own, made executable for the relay alone.
    let relay = unsafe { relay_near(target, detour) }.ok_or_else(|| format!("{name}: no memory near it"))?;
    let rel = i32::try_from(relay as isize - (target as isize + 5)).map_err(|_| format!("{name}: too far"))?;
    let mut ours = before;
    ours[0] = 0xE9;
    ours[1..5].copy_from_slice(&rel.to_le_bytes());
    // On through the other's hook, as the function did.
    original.store(jump, Ordering::Release);
    // SAFETY: the function's code, its first eight bytes swapped in one go.
    if !unsafe { swap_first_bytes(target, before, ours) } {
        return Err(format!("{name}: changed while hooking it"));
    }
    OVER_JUMPS.lock().unwrap_or_else(|e| e.into_inner()).push((target, ours, before));
    log!("hooked {name} again at {target:#x}, in front of the hook in {} over ours", module_of(jump_end(jump)));
    Ok(())
}

/// Where a jump finally lands past a relay (`jmp [rip+n]`, as hooking tools put between a function and their code),
/// for the log: whose hook it is.
fn jump_end(at: usize) -> usize {
    // SAFETY: read only if it's in a module or our memory says it's code; a relay's eight-byte address after it.
    unsafe {
        let bytes = std::ptr::read_unaligned(at as *const [u8; 6]);
        if bytes[0] == 0xFF && bytes[1] == 0x25 {
            let rel = i32::from_le_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]);
            return std::ptr::read_unaligned((at as isize + 6 + rel as isize) as *const usize);
        }
    }
    at
}

/// Swaps a function's first eight bytes from `from` to `to` in one go (they're eight-byte aligned: a function's
/// start is); `false` if they weren't `from`.
///
/// # Safety
/// `target` is the start of a function in this process.
unsafe fn swap_first_bytes(target: usize, from: [u8; 8], to: [u8; 8]) -> bool {
    use windows::Win32::System::Diagnostics::Debug::FlushInstructionCache;
    use windows::Win32::System::Memory::PAGE_EXECUTE_READWRITE;
    use windows::Win32::System::Threading::GetCurrentProcess;
    if !target.is_multiple_of(8) {
        return false;
    }
    let mut old = PAGE_PROTECTION_FLAGS(0);
    // SAFETY: the function's own page, made writable for the swap and set back; the swap is one atomic exchange.
    unsafe {
        if VirtualProtect(target as *const c_void, 8, PAGE_EXECUTE_READWRITE, &mut old).is_err() {
            return false;
        }
        let atomic = &*(target as *const std::sync::atomic::AtomicU64);
        let swapped = atomic
            .compare_exchange(u64::from_le_bytes(from), u64::from_le_bytes(to), Ordering::AcqRel, Ordering::Acquire)
            .is_ok();
        let _ = VirtualProtect(target as *const c_void, 8, old, &mut old);
        let _ = FlushInstructionCache(GetCurrentProcess(), Some(target as *const c_void), 8);
        swapped
    }
}

/// A relay to `detour` (`jmp [rip+0]`, then its address) in memory of our own within reach of a 32-bit jump from
/// `near`, or `None`.
///
/// # Safety
/// Plain allocation; the memory is ours and made executable once written.
unsafe fn relay_near(near: usize, detour: usize) -> Option<usize> {
    use windows::Win32::System::Memory::{MEM_COMMIT, MEM_RESERVE, PAGE_EXECUTE_READ, VirtualAlloc};
    const STEP: usize = 0x1_0000;
    const REACH: usize = 0x7000_0000;
    let start = (near & !(STEP - 1)).saturating_sub(REACH);
    for offset in (0..2 * REACH).step_by(STEP) {
        let at = start + offset;
        // SAFETY: memory asked for at an address of our choosing; none given back if it's taken.
        let page = unsafe { VirtualAlloc(Some(at as *const c_void), STEP, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE) };
        if page.is_null() {
            continue;
        }
        let page = page as usize;
        if page.abs_diff(near) > REACH {
            continue;
        }
        let mut relay = [0u8; 14];
        relay[..6].copy_from_slice(&[0xFF, 0x25, 0, 0, 0, 0]);
        relay[6..].copy_from_slice(&(detour as u64).to_le_bytes());
        let mut old = PAGE_PROTECTION_FLAGS(0);
        // SAFETY: the page just made, ours; written, then made executable and no longer writable.
        unsafe {
            std::ptr::copy_nonoverlapping(relay.as_ptr(), page as *mut u8, relay.len());
            let _ = VirtualProtect(page as *const c_void, STEP, PAGE_EXECUTE_READ, &mut old);
        }
        return Some(page);
    }
    None
}

/// Keeps the plugin's own code loaded for good (another's hook calls into ours).
fn pin_own_module() {
    let mut module = HMODULE::default();
    let flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_PIN;
    // SAFETY: an address in this module's code, looked up to pin it.
    let _ = unsafe { GetModuleHandleExW(flags, PCWSTR(pin_own_module as *const () as *const u16), &mut module) };
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

    /// A vtable's entry swapped: calls through it reach the detour, which calls on to what was there; taken out, the
    /// entry is as it was.
    #[test]
    fn swaps_a_vtable_entry_and_puts_it_back() {
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        static ORIGINAL: AtomicUsize = AtomicUsize::new(0);
        extern "system" fn first(x: u32) -> u32 {
            x + 1
        }
        extern "system" fn second(x: u32) -> u32 {
            x + 2
        }
        extern "system" fn detour(x: u32) -> u32 {
            CALLS.fetch_add(1, Ordering::Relaxed);
            // SAFETY: what the entry held, a function of this signature.
            let original: extern "system" fn(u32) -> u32 =
                unsafe { std::mem::transmute(ORIGINAL.load(Ordering::Acquire)) };
            original(x) * 10
        }
        let _one_at_a_time = VTABLE_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        // A table of functions in this test's image, as a class's vtable is in its DLL's.
        static mut TABLE: [usize; 2] = [0, 0];
        // SAFETY: the test's own table, used by this test alone.
        unsafe {
            let table = &raw mut TABLE;
            (*table)[0] = first as *const () as usize;
            (*table)[1] = second as *const () as usize;
            let vtable = table as usize;
            assert!(in_a_module(vtable));
            hook_vtable(vtable, 1, detour as *mut c_void, &ORIGINAL, "a test's second").unwrap();
            let call: extern "system" fn(u32) -> u32 = std::mem::transmute((*table)[1]);
            assert_eq!(call(1), 30);
            assert_eq!(CALLS.load(Ordering::Relaxed), 1);
            assert_eq!((*table)[0], first as *const () as usize, "the other entry untouched");
            remove_all();
            assert_eq!((*table)[1], second as *const () as usize);
        }
    }

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
