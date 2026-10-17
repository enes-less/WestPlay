//! Typing on the player's own keyboard into the WestPlay text field selected (the bridge's
//! [`westplay_shared::TypingBlock`]).
//!
//! While the truck stands still, a field selected then takes the keyboard: every key press becomes what
//! the player's Windows keyboard layout types (Turkish Q types ı ğ ü, AltGr and accent keys work), queued for
//! the app, and the game gets none of them but the few that stay its own ([`stays_the_games`]). The keys are
//! the game's again when the field is no longer selected (submitted, a result chosen, the keyboard put away,
//! Esc) or the truck moves; after moving, only a new selection takes them again, so the throttle never types
//! a "w" at a red light ([`Gate`]).

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use westplay_shared::{TypedKey, TypingState};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyboardLayout, GetKeyboardState, ToUnicodeEx, VK_APPS, VK_BACK, VK_CONTROL, VK_ESCAPE, VK_F1, VK_F24, VK_LWIN,
    VK_MENU, VK_RETURN, VK_RWIN, VK_SNAPSHOT,
};

use crate::{bridge, log};

/// Faster than this, the truck is moving, km/h.
const MOVING_KMH: f32 = 1.0;

/// Whether the keys type into the field right now (read by the keyboard hooks, on any thread).
static TYPING: AtomicBool = AtomicBool::new(false);
static GATE: Mutex<Gate> = Mutex::new(Gate { seen: 0, armed: 0 });

/// Whether the player's keys type into WestPlay's field (and so none reach the game but its own).
pub fn typing() -> bool {
    TYPING.load(Ordering::Acquire)
}

/// Once a frame (the truck's `speed`, m/s, and whether the game is paused): whether the keys type, from
/// the app's field selection; published for the app.
pub fn update(speed: f32, paused: bool) {
    let Some(bridge) = bridge::get() else { return };
    let block = &bridge.typing;
    let selection = if bridge::screen_on() { block.selection.load(Ordering::Acquire) } else { 0 };
    let moving = speed.abs() * 3.6 > MOVING_KMH;
    let state = GATE.lock().unwrap_or_else(|e| e.into_inner()).update(selection, moving);
    // Paused (a menu, the game's map): its keys are its own, without letting go of the selection.
    let typing = state == TypingState::Typing && !paused;
    if TYPING.swap(typing, Ordering::AcqRel) != typing {
        log!("typing on the keyboard {}", if typing { "on" } else { "off" });
    }
    block.state.store(state as u32, Ordering::Release);
}

/// When the keys type: from a selection made while the truck stands still, until it moves; then not until a
/// new selection (the app's number changes with each).
#[derive(Debug, PartialEq)]
struct Gate {
    /// The selection last seen, and the one typing is allowed for (0: none).
    seen: u32,
    armed: u32,
}

impl Gate {
    fn update(&mut self, selection: u32, moving: bool) -> TypingState {
        if selection == 0 {
            (self.seen, self.armed) = (0, 0);
            return TypingState::Off;
        }
        if selection != self.seen {
            self.seen = selection;
            self.armed = if moving { 0 } else { selection };
        }
        if moving {
            self.armed = 0;
        }
        if self.armed == selection { TypingState::Typing } else { TypingState::Paused }
    }
}

/// Keys that stay the game's while typing: the F-keys, Print Screen, the Windows keys and the menu key
/// (screenshots, the game's own F-key functions, switching windows).
pub fn stays_the_games(key: u32) -> bool {
    (VK_F1.0 as u32..=VK_F24.0 as u32).contains(&key)
        || [VK_SNAPSHOT, VK_LWIN, VK_RWIN, VK_APPS].iter().any(|vk| vk.0 as u32 == key)
}

/// The same keys as DirectInput names them (scan codes, extended ones with 0x80): F1-F15, Print Screen
/// (SysRq), the Windows and menu keys.
pub fn stays_the_games_scancode(scancode: u32) -> bool {
    matches!(scancode, 0x3b..=0x44 | 0x57 | 0x58 | 0x64..=0x66 | 0xb7 | 0xdb..=0xdd)
}

/// A key pressed while typing (`key`, its scan code): queued for the app as what it types.
///
/// # Safety
/// On the game's window thread (the keyboard state read is that thread's).
pub unsafe fn press(key: u32, scancode: u32) {
    // SAFETY: on the thread whose keyboard state is asked (the caller's contract).
    unsafe {
        let typed = typed(key, scancode);
        let Some(bridge) = bridge::get() else { return };
        for code in typed {
            if !bridge.typing.keys.push(code) {
                log!("typing queue full, a key dropped");
            }
        }
    }
}

/// What a key press types: a key with its own meaning ([`key_meaning`]), or the characters the player's
/// keyboard layout gives (none for an accent key, whose accent waits for the next letter).
///
/// # Safety
/// As [`press`].
unsafe fn typed(key: u32, scancode: u32) -> Vec<u32> {
    // SAFETY: plain Windows calls with locals; the keyboard state is this thread's (the caller's contract).
    unsafe {
        let mut state = [0u8; 256];
        if GetKeyboardState(&mut state).is_err() {
            return Vec::new();
        }
        let down = |vk: u16| state[vk as usize] & 0x80 != 0;
        // Ctrl alone (Ctrl with Alt is AltGr, which types characters).
        let ctrl = down(VK_CONTROL.0) && !down(VK_MENU.0);
        if let Some(meaning) = key_meaning(key, ctrl) {
            return meaning.map(|key| key as u32).into_iter().collect();
        }
        let mut buffer = [0u16; 8];
        let layout = GetKeyboardLayout(0);
        let count = ToUnicodeEx(key, scancode, &state, &mut buffer, 0, Some(layout));
        if count <= 0 {
            return Vec::new();
        }
        String::from_utf16_lossy(&buffer[..count as usize]).chars().filter(|c| !c.is_control()).map(u32::from).collect()
    }
}

/// A key with a meaning of its own while typing: `Some(Some(key))`; `Some(None)` for one that types nothing
/// (other Ctrl shortcuts); `None` for one the layout turns into characters.
fn key_meaning(key: u32, ctrl: bool) -> Option<Option<TypedKey>> {
    match key {
        k if k == VK_BACK.0 as u32 => Some(Some(if ctrl { TypedKey::WordBackspace } else { TypedKey::Backspace })),
        k if k == VK_RETURN.0 as u32 => Some(Some(TypedKey::Enter)),
        k if k == VK_ESCAPE.0 as u32 => Some(Some(TypedKey::Escape)),
        k if k == u32::from(b'V') && ctrl => Some(Some(TypedKey::Paste)),
        _ if ctrl => Some(None),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_from_a_selection_made_standing_still_until_the_truck_moves() {
        let mut gate = Gate { seen: 0, armed: 0 };
        assert_eq!(gate.update(0, false), TypingState::Off);
        // Selected while stopped: typing.
        assert_eq!(gate.update(1, false), TypingState::Typing);
        assert_eq!(gate.update(1, false), TypingState::Typing);
        // It moves: the keys are the game's, and stay so when it stops again (a red light).
        assert_eq!(gate.update(1, true), TypingState::Paused);
        assert_eq!(gate.update(1, false), TypingState::Paused);
        // Selected anew, standing still: typing again.
        assert_eq!(gate.update(2, false), TypingState::Typing);
        // Let go (submitted, Esc): off.
        assert_eq!(gate.update(0, false), TypingState::Off);
    }

    #[test]
    fn a_selection_made_while_moving_never_types() {
        let mut gate = Gate { seen: 0, armed: 0 };
        assert_eq!(gate.update(5, true), TypingState::Paused);
        assert_eq!(gate.update(5, false), TypingState::Paused);
        assert_eq!(gate.update(6, false), TypingState::Typing);
    }

    #[test]
    fn keys_with_meanings_of_their_own() {
        assert_eq!(key_meaning(VK_BACK.0 as u32, false), Some(Some(TypedKey::Backspace)));
        assert_eq!(key_meaning(VK_BACK.0 as u32, true), Some(Some(TypedKey::WordBackspace)));
        assert_eq!(key_meaning(VK_RETURN.0 as u32, false), Some(Some(TypedKey::Enter)));
        assert_eq!(key_meaning(VK_ESCAPE.0 as u32, false), Some(Some(TypedKey::Escape)));
        assert_eq!(key_meaning(u32::from(b'V'), true), Some(Some(TypedKey::Paste)));
        // A letter is the layout's; with Ctrl, nothing.
        assert_eq!(key_meaning(u32::from(b'V'), false), None);
        assert_eq!(key_meaning(u32::from(b'C'), true), Some(None));
    }

    #[test]
    fn some_keys_stay_the_games() {
        assert!(stays_the_games(VK_F1.0 as u32) && stays_the_games(VK_SNAPSHOT.0 as u32));
        assert!(!stays_the_games(u32::from(b'W')) && !stays_the_games(VK_RETURN.0 as u32));
        // F5 and the left Windows key as DirectInput names them; W (0x11) not.
        assert!(stays_the_games_scancode(0x3f) && stays_the_games_scancode(0xdb));
        assert!(!stays_the_games_scancode(0x11));
    }
}
