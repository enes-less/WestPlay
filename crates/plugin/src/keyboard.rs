//! Screen interaction: the keyboard, for typing into WestPlay.
//!
//! The game reads keys from its window messages, and polls some with `GetAsyncKeyState`/`GetKeyState`. While
//! the player types into a WestPlay field (`typing`), every key but the few that stay the game's own is kept
//! from it: its messages are taken out of the game's message loop before it handles them (so they never
//! become key presses or typed characters for it), and polling reports it as up. Each press goes to the app
//! as what it types. Otherwise every key reaches the game, the interaction key held or not, so keyboard
//! drivers can steer and brake while using the screen; but the keys bound to WestPlay (`keybinds`), while
//! their bindings count.

use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::Win32::UI::WindowsAndMessaging::{
    MSG, PM_REMOVE, WM_CHAR, WM_DEADCHAR, WM_KEYDOWN, WM_KEYUP, WM_NULL, WM_SYSCHAR, WM_SYSDEADCHAR, WM_SYSKEYDOWN,
    WM_SYSKEYUP,
};
use windows::core::{BOOL, PCSTR, s, w};

use crate::hooks::{guarded, hook};
use crate::{keybinds, typing};

type PeekMessageWFn = unsafe extern "system" fn(*mut MSG, HWND, u32, u32, u32) -> BOOL;
type GetMessageWFn = unsafe extern "system" fn(*mut MSG, HWND, u32, u32) -> BOOL;
type GetKeyStateFn = unsafe extern "system" fn(i32) -> i16;

static ORIG_PEEK_MESSAGE_W: AtomicUsize = AtomicUsize::new(0);
static ORIG_GET_MESSAGE_W: AtomicUsize = AtomicUsize::new(0);
static ORIG_GET_ASYNC_KEY_STATE: AtomicUsize = AtomicUsize::new(0);
static ORIG_GET_KEY_STATE: AtomicUsize = AtomicUsize::new(0);

/// # Safety
/// In the game's process, before the hooks are switched on ([`crate::hooks::enable_all`]).
pub unsafe fn install() -> Result<(), String> {
    // SAFETY: user32's own exports, hooked with detours of their exact signatures (below).
    unsafe {
        let user32 = GetModuleHandleW(w!("user32.dll")).map_err(|e| format!("user32.dll: {e}"))?;
        let functions: [(PCSTR, *mut c_void, &AtomicUsize, &str); 4] = [
            (s!("PeekMessageW"), peek_message_w as *mut c_void, &ORIG_PEEK_MESSAGE_W, "PeekMessageW"),
            (s!("GetMessageW"), get_message_w as *mut c_void, &ORIG_GET_MESSAGE_W, "GetMessageW"),
            (s!("GetAsyncKeyState"), get_async_key_state as *mut c_void, &ORIG_GET_ASYNC_KEY_STATE, "GetAsyncKeyState"),
            (s!("GetKeyState"), get_key_state as *mut c_void, &ORIG_GET_KEY_STATE, "GetKeyState"),
        ];
        for (export, detour, original, name) in functions {
            let target = GetProcAddress(user32, export).ok_or_else(|| format!("{name} not found in user32.dll"))?;
            hook(target as usize, detour, original, name)?;
        }
        Ok(())
    }
}

/// # Safety
/// Called by Windows' callers in place of `PeekMessageW`, with its arguments.
unsafe extern "system" fn peek_message_w(msg: *mut MSG, window: HWND, min: u32, max: u32, remove: u32) -> BOOL {
    // SAFETY: the trampoline was stored before the hooks were switched on (`install`, then `enable_all`), so it's
    // the original function; `msg` is the caller's, written by it, and filtered only when set.
    unsafe {
        let original: PeekMessageWFn = std::mem::transmute(ORIG_PEEK_MESSAGE_W.load(Ordering::Acquire));
        let result = original(msg, window, min, max, remove);
        // A message only peeked at stays in the queue; it is filtered when the game removes it.
        if result.as_bool() && !msg.is_null() && remove & PM_REMOVE.0 != 0 {
            guarded(|| filter(&mut *msg));
        }
        result
    }
}

/// # Safety
/// Called in place of `GetMessageW`, with its arguments.
unsafe extern "system" fn get_message_w(msg: *mut MSG, window: HWND, min: u32, max: u32) -> BOOL {
    // SAFETY: as `peek_message_w`: the original function, and the caller's `msg` once it's set.
    unsafe {
        let original: GetMessageWFn = std::mem::transmute(ORIG_GET_MESSAGE_W.load(Ordering::Acquire));
        let result = original(msg, window, min, max);
        // 0 is WM_QUIT, -1 an error.
        if result.0 > 0 && !msg.is_null() {
            guarded(|| filter(&mut *msg));
        }
        result
    }
}

/// # Safety
/// Called in place of `GetAsyncKeyState`, with its argument.
unsafe extern "system" fn get_async_key_state(key: i32) -> i16 {
    // SAFETY: the trampoline was stored before the hooks were switched on: the original function.
    unsafe {
        let original: GetKeyStateFn = std::mem::transmute(ORIG_GET_ASYNC_KEY_STATE.load(Ordering::Acquire));
        let mut state = original(key);
        // Our own calls (inside `guarded`) get the real state.
        guarded(|| {
            if hidden(key as u32) {
                state = 0;
            }
        });
        state
    }
}

/// # Safety
/// Called in place of `GetKeyState`, with its argument.
unsafe extern "system" fn get_key_state(key: i32) -> i16 {
    // SAFETY: the trampoline was stored before the hooks were switched on: the original function.
    unsafe {
        let original: GetKeyStateFn = std::mem::transmute(ORIG_GET_KEY_STATE.load(Ordering::Acquire));
        let mut state = original(key);
        guarded(|| {
            if hidden(key as u32) {
                state = 0;
            }
        });
        state
    }
}

/// Whether the game's polling sees `key` up whatever it is: any key but its own while the player types, and a
/// key bound to WestPlay while its binding counts (`keybinds`).
fn hidden(key: u32) -> bool {
    (typing::typing() && !typing::stays_the_games(key)) || keybinds::taken(key)
}

/// While the player types, takes every key's messages (but the game's own) away from the game, turning them
/// into `WM_NULL`, each press queued as what it types; and a bound key's presses while its binding counts.
fn filter(msg: &mut MSG) {
    let key = msg.wParam.0 as u32;
    let typed = typing::typing()
        && matches!(
            msg.message,
            WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP | WM_CHAR | WM_SYSCHAR | WM_DEADCHAR | WM_SYSDEADCHAR
        )
        && !typing::stays_the_games(key);
    if typed && matches!(msg.message, WM_KEYDOWN | WM_SYSKEYDOWN) {
        // The scan code, with 0x100 for an extended key (bits 16-23 and 24 of the message's lParam).
        let scancode = ((msg.lParam.0 >> 16) & 0xff) as u32 | (((msg.lParam.0 >> 24) & 1) as u32) << 8;
        // SAFETY: a key message of the game's window, filtered on its own thread (`press`'s contract).
        unsafe { typing::press(key, scancode) };
    }
    // A key bound to WestPlay: its presses aren't the game's (a release it gets is harmless).
    let bound = !typed
        && matches!(msg.message, WM_KEYDOWN | WM_SYSKEYDOWN)
        && keybinds::taken(keybinds::sided(key, msg.lParam.0));
    if typed || bound {
        msg.message = WM_NULL;
        msg.wParam = WPARAM(0);
        msg.lParam = LPARAM(0);
    }
}
