//! The player's keybinds (the app's settings, through the bridge): volume up, volume down, play/pause.
//!
//! Each press of a bound key, in the game with WestPlay on its screen, is counted for the app, which acts on
//! it. A binding with the interaction key counts only while the player uses the screen (the key held, or
//! toggled on); one without it counts whenever, but not while the player types into WestPlay. While its
//! binding counts, a bound key is kept from the game (the `keyboard` module asks [`taken`]), so the game
//! never acts on it too. A mouse button bound is left to the game: it reads the mouse through DirectInput,
//! and leaves its side buttons unbound.

use std::sync::atomic::{AtomicBool, Ordering};

use westplay_shared::{KEYBIND_COUNT, Keybind, keybind_of};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VIRTUAL_KEY, VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_MENU, VK_RCONTROL, VK_RMENU,
    VK_RSHIFT, VK_SHIFT, VK_XBUTTON2,
};

use crate::{bridge, input, log, typing};

/// Whether each binding's key was down at the last frame.
static WAS_DOWN: [AtomicBool; KEYBIND_COUNT] = [const { AtomicBool::new(false) }; KEYBIND_COUNT];

/// Once a frame: counts each new press of a bound key whose binding counts, for the app.
pub fn on_frame() {
    let Some(bridge) = bridge::get() else { return };
    let in_game = crate::d3d::game_shown();
    for keybind in Keybind::ALL {
        let i = keybind as usize;
        let Some((key, with_interaction_key)) = keybind_of(bridge.host.keybinds[i].load(Ordering::Acquire)) else {
            WAS_DOWN[i].store(false, Ordering::Release);
            continue;
        };
        // SAFETY: a plain query, no pointers.
        let down = unsafe { GetAsyncKeyState(key as i32) } as u16 & 0x8000 != 0;
        // First: every frame's key state is kept, whether the press counts or not.
        if input::newly_down(&WAS_DOWN[i], down) && in_game && counts(with_interaction_key) {
            bridge.plugin.keybind_presses[i].fetch_add(1, Ordering::AcqRel);
            log!("keybind {keybind:?} pressed");
        }
    }
}

/// Whether a binding counts now: with WestPlay on the screen, and either the screen in use (one with the
/// interaction key) or the player not typing into WestPlay (one without it).
fn counts(with_interaction_key: bool) -> bool {
    bridge::screen_on() && if with_interaction_key { input::interaction_active() } else { !typing::typing() }
}

/// Whether the game is kept from `key` (a virtual-key code; the generic Shift, Ctrl and Alt stand for both
/// sides) now: a keyboard key bound whose binding counts.
pub fn taken(key: u32) -> bool {
    let Some(bridge) = bridge::get() else { return false };
    let sides = sides(key);
    bridge.host.keybinds.iter().filter_map(|value| keybind_of(value.load(Ordering::Acquire))).any(
        |(bound, with_interaction_key)| {
            !is_mouse_button(bound) && sides.contains(&bound) && counts(with_interaction_key)
        },
    )
}

/// The keys `key` stands for: both sides for the generic Shift, Ctrl and Alt, else itself.
fn sides(key: u32) -> [u32; 2] {
    match key {
        k if k == vk(VK_SHIFT) => [vk(VK_LSHIFT), vk(VK_RSHIFT)],
        k if k == vk(VK_CONTROL) => [vk(VK_LCONTROL), vk(VK_RCONTROL)],
        k if k == vk(VK_MENU) => [vk(VK_LMENU), vk(VK_RMENU)],
        k => [k, k],
    }
}

/// The side of a key message's Shift, Ctrl or Alt (its `wParam` is the generic one), from its `lParam`:
/// right Ctrl and Alt are extended keys, right Shift has its own scan code. Any other key as it is.
pub fn sided(key: u32, lparam: isize) -> u32 {
    let extended = (lparam >> 24) & 1 != 0;
    let scancode = (lparam >> 16) & 0xff;
    match key {
        k if k == vk(VK_SHIFT) => vk(if scancode == 0x36 { VK_RSHIFT } else { VK_LSHIFT }),
        k if k == vk(VK_CONTROL) => vk(if extended { VK_RCONTROL } else { VK_LCONTROL }),
        k if k == vk(VK_MENU) => vk(if extended { VK_RMENU } else { VK_LMENU }),
        k => k,
    }
}

fn vk(key: VIRTUAL_KEY) -> u32 {
    u32::from(key.0)
}

/// Mouse buttons' virtual-key codes: left, right, (cancel,) middle, back, forward.
fn is_mouse_button(key: u32) -> bool {
    (1..=vk(VK_XBUTTON2)).contains(&key) && key != 3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_messages_shift_ctrl_and_alt_get_their_side() {
        // Right Alt (AltGr) and right Ctrl are extended keys.
        assert_eq!(sided(0x12, 1 << 24 | 0x38 << 16), 0xa5);
        assert_eq!(sided(0x12, 0x38 << 16), 0xa4);
        assert_eq!(sided(0x11, 1 << 24 | 0x1d << 16), 0xa3);
        assert_eq!(sided(0x10, 0x36 << 16), 0xa1);
        assert_eq!(sided(0x10, 0x2a << 16), 0xa0);
        // Any other key as it is.
        assert_eq!(sided(0x31, 0x02 << 16), 0x31);
    }

    #[test]
    fn the_generic_modifiers_stand_for_both_sides() {
        assert_eq!(sides(0x11), [0xa2, 0xa3]);
        assert_eq!(sides(0x31), [0x31, 0x31]);
    }

    #[test]
    fn tells_mouse_buttons_from_keys() {
        assert!(is_mouse_button(0x05) && is_mouse_button(0x06) && is_mouse_button(0x04));
        assert!(!is_mouse_button(0x03) && !is_mouse_button(0x31) && !is_mouse_button(0xa4));
    }
}
