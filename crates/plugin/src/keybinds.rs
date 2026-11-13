//! The player's keybinds (the app's settings, through the bridge): volume up, volume down, play/pause, and the
//! screen's brightness down and up.
//!
//! Each press of a bound key, in the game with WestPlay on its screen, is counted for the app, which acts on
//! it. A binding with the interaction key counts only while the player uses the screen (the key held, or
//! toggled on); one without it counts whenever, but not while the player types into WestPlay. A bound key
//! pressed while its binding counts is kept from the game, the whole press to its release (the `keyboard`
//! module asks [`take_message`] and [`taken`]), so the game never acts on it too. A mouse button bound is left to the game: it reads the mouse through DirectInput,
//! and leaves its side buttons unbound.
//!
//! The volume keys can also be held (owner 2026-11-13: one second, [`KEYBIND_HOLD_MS`]): held, it's a hold
//! (the app changes the song), counted the moment it's been held that long; let go before, it's a press,
//! counted as it's let go (as a car stereo's button: a press counted as it goes down would step the volume on
//! every hold too). Whether it counts at all is as it was when it went down.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use westplay_shared::{KEYBIND_COUNT, KEYBIND_HOLD_MS, Keybind, keybind_of};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VIRTUAL_KEY, VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_MENU, VK_RCONTROL, VK_RMENU,
    VK_RSHIFT, VK_SHIFT, VK_XBUTTON2,
};

use crate::{bridge, input, log, typing};

/// Whether each binding's key was down at the last frame.
static WAS_DOWN: [AtomicBool; KEYBIND_COUNT] = [const { AtomicBool::new(false) }; KEYBIND_COUNT];
/// Each holdable binding's key, since it went down ([`Keybind::holds`]).
static HELD: Mutex<[Held; KEYBIND_COUNT]> = Mutex::new([Held::UP; KEYBIND_COUNT]);

/// Once a frame: counts each new press (and each hold) of a bound key whose binding counts, for the app.
pub fn on_frame() {
    let Some(bridge) = bridge::get() else { return };
    let in_game = crate::d3d::game_shown();
    let now = Instant::now();
    // A press of WestPlay's whose release never came as a message (the window lost the keyboard): over once
    // the key is up.
    for (key, ours) in PRESSED_FOR_WESTPLAY.iter().enumerate() {
        // SAFETY: a plain query, no pointers.
        if ours.load(Ordering::Acquire) && unsafe { GetAsyncKeyState(key as i32) } as u16 & 0x8000 == 0 {
            ours.store(false, Ordering::Release);
        }
    }
    let mut held = HELD.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    for keybind in Keybind::ALL {
        let i = keybind as usize;
        let Some((key, with_interaction_key)) = keybind_of(bridge.host.keybinds[i].load(Ordering::Acquire)) else {
            WAS_DOWN[i].store(false, Ordering::Release);
            held[i] = Held::UP;
            continue;
        };
        // SAFETY: a plain query, no pointers.
        let down = unsafe { GetAsyncKeyState(key as i32) } as u16 & 0x8000 != 0;
        if keybind.holds() {
            match held[i].step(down, || in_game && counts(with_interaction_key), now) {
                Some(Pressed::Press) => {
                    bridge.plugin.keybind_presses[i].fetch_add(1, Ordering::AcqRel);
                    log!("keybind {keybind:?} pressed");
                }
                Some(Pressed::Hold) => {
                    bridge.plugin.keybind_holds[i].fetch_add(1, Ordering::AcqRel);
                    log!("keybind {keybind:?} held");
                }
                None => {}
            }
            continue;
        }
        // First: every frame's key state is kept, whether the press counts or not.
        if input::newly_down(&WAS_DOWN[i], down) && in_game && counts(with_interaction_key) {
            bridge.plugin.keybind_presses[i].fetch_add(1, Ordering::AcqRel);
            log!("keybind {keybind:?} pressed");
        }
    }
}

/// What a holdable key did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pressed {
    /// Let go before [`KEYBIND_HOLD_MS`].
    Press,
    /// Held that long (once, however long it's held after).
    Hold,
}

/// A holdable key, from one frame to the next: since when it's down, whether it counts (as it went down), and
/// whether it's been held long enough already.
#[derive(Clone, Copy, Debug)]
struct Held {
    since: Option<Instant>,
    counts: bool,
    held: bool,
}

impl Held {
    const UP: Held = Held { since: None, counts: false, held: false };

    /// The key `down` or not at `now`: a press as it's let go before the hold's time, a hold once it's down that
    /// long. `counts` is asked as it goes down.
    fn step(&mut self, down: bool, counts: impl FnOnce() -> bool, now: Instant) -> Option<Pressed> {
        match (down, self.since) {
            (true, None) => {
                *self = Held { since: Some(now), counts: counts(), held: false };
                None
            }
            (true, Some(since)) => {
                let long = now.duration_since(since) >= Duration::from_millis(KEYBIND_HOLD_MS);
                (self.counts && !self.held && long).then(|| {
                    self.held = true;
                    Pressed::Hold
                })
            }
            (false, Some(_)) => {
                let press = (self.counts && !self.held).then_some(Pressed::Press);
                *self = Held::UP;
                press
            }
            (false, None) => None,
        }
    }
}

/// Whether a binding counts now: with WestPlay on the screen, and either the screen in use (one with the
/// interaction key) or the player not typing into WestPlay (one without it).
fn counts(with_interaction_key: bool) -> bool {
    bridge::screen_on() && if with_interaction_key { input::interaction_active() } else { !typing::typing() }
}

/// The bound keys whose press is WestPlay's, by virtual-key code: down while their binding counted, until let
/// go. The game is kept from all of that press, not only its going down: its release and its state as it's
/// held too, however the binding counts meanwhile (the interaction key let go first). In ATS 1.62 Alt+1 left the
/// cab's camera: the 1's release, or its state once Alt was up, still reached the game.
static PRESSED_FOR_WESTPLAY: [AtomicBool; 256] = [const { AtomicBool::new(false) }; 256];

/// A key message's verdict (`key` its side's virtual-key code, `down` whether it's a key going down): whether
/// the game is kept from it. Going down while its binding counts, the press becomes WestPlay's
/// ([`PRESSED_FOR_WESTPLAY`]) and every message of it after is kept too; going up, it's WestPlay's no more.
pub fn take_message(key: u32, down: bool) -> bool {
    let Some(ours) = PRESSED_FOR_WESTPLAY.get(key as usize) else { return false };
    if !down {
        return ours.swap(false, Ordering::AcqRel);
    }
    if counts_now(key) {
        ours.store(true, Ordering::Release);
    }
    ours.load(Ordering::Acquire)
}

/// Whether the game is kept from `key` (a virtual-key code; the generic Shift, Ctrl and Alt stand for both
/// sides) now, as its polling asks: a keyboard key bound whose binding counts, or whose press is WestPlay's.
pub fn taken(key: u32) -> bool {
    let ours = sides(key)
        .iter()
        .any(|side| PRESSED_FOR_WESTPLAY.get(*side as usize).is_some_and(|k| k.load(Ordering::Acquire)));
    ours || counts_now(key)
}

/// Whether `key` (as [`taken`] takes it) is a keyboard key bound whose binding counts now.
fn counts_now(key: u32) -> bool {
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
    fn a_volume_key_let_go_is_a_press_and_held_a_second_a_hold() {
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        // Pressed and let go: a press, as it's let go (not as it went down).
        let mut key = Held::UP;
        assert_eq!(key.step(true, || true, at(0)), None);
        assert_eq!(key.step(true, || true, at(300)), None);
        assert_eq!(key.step(false, || true, at(320)), Some(Pressed::Press));
        // Held: a hold at a second, once; let go after, no press.
        assert_eq!(key.step(true, || true, at(1000)), None);
        assert_eq!(key.step(true, || true, at(1990)), None);
        assert_eq!(key.step(true, || true, at(2000)), Some(Pressed::Hold));
        assert_eq!(key.step(true, || true, at(6000)), None);
        assert_eq!(key.step(false, || true, at(6100)), None);
        // Down where it doesn't count (WestPlay off its screen): neither.
        assert_eq!(key.step(true, || false, at(7000)), None);
        assert_eq!(key.step(true, || true, at(9500)), None);
        assert_eq!(key.step(false, || true, at(9600)), None);
        // Up and staying up: nothing.
        assert_eq!(key.step(false, || true, at(9700)), None);
    }

    #[test]
    fn a_press_of_westplays_is_kept_from_the_game_to_its_release() {
        // No bridge here, so no binding counts: a press of 2 isn't WestPlay's, the game gets it.
        assert!(!take_message(0x32, true) && !take_message(0x32, false));
        // A press of 1 that went down as WestPlay's: its repeats, its state and its release are kept too, the
        // binding counting or not by then; after its release, nothing.
        PRESSED_FOR_WESTPLAY[0x31].store(true, Ordering::Release);
        assert!(take_message(0x31, true));
        assert!(taken(0x31));
        assert!(take_message(0x31, false));
        assert!(!taken(0x31) && !take_message(0x31, false) && !take_message(0x31, true));
    }

    #[test]
    fn tells_mouse_buttons_from_keys() {
        assert!(is_mouse_button(0x05) && is_mouse_button(0x06) && is_mouse_button(0x04));
        assert!(!is_mouse_button(0x03) && !is_mouse_button(0x31) && !is_mouse_button(0xa4));
    }
}
