//! Screen interaction: drives the cabin page with the mouse input the plugin takes from the game while the
//! player holds the interaction key (the bridge's pointer block).
//!
//! The cursor lives in page pixels: mouse movement moves it across the screen, whatever truck or camera, and
//! clicks and the wheel reach the page as ordinary mouse events at its position. The host draws the cursor
//! (`cursor.rs`) where this puts it, into the frame the game takes, as it moves: WebView2 renders no system
//! cursor offscreen, and the page's own showed late.
//!
//! Movement is accelerated (precise when the hand moves slowly, quick across the screen when it moves fast),
//! and a pressed button holds the cursor still until the mouse really drags, so a click lands where it was
//! aimed even if the hand nudges the mouse while pressing (a touchscreen's "tap slop").

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_MOUSE_EVENT_KIND, COREWEBVIEW2_MOUSE_EVENT_KIND_LEAVE, COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_DOWN,
    COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_UP, COREWEBVIEW2_MOUSE_EVENT_KIND_MOVE,
    COREWEBVIEW2_MOUSE_EVENT_KIND_RIGHT_BUTTON_DOWN, COREWEBVIEW2_MOUSE_EVENT_KIND_RIGHT_BUTTON_UP,
    COREWEBVIEW2_MOUSE_EVENT_KIND_WHEEL, COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS,
    COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_LEFT_BUTTON, COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_NONE,
    COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_RIGHT_BUTTON,
};
use westplay_shared::PointerBlock;
use windows::Win32::Foundation::POINT;

use crate::cursor::Mouse;
use crate::webview::OffscreenWebView;

/// Page pixels per mouse count at rest, at sensitivity 1 (the player's sensitivity scales the whole curve).
const BASE_GAIN: f32 = 1.3;
/// How much faster the cursor gets per count/ms of hand speed...
const ACCELERATION: f32 = 0.55;
/// ...up to this speed (counts per millisecond); beyond it the gain stays constant.
const MAX_ACCELERATED_SPEED: f32 = 4.0;
/// Movement batches arrive once per game frame; a longer gap means the hand started from rest.
const MOVEMENT_INTERVAL: (Duration, Duration) = (Duration::from_millis(4), Duration::from_millis(50));
/// How far (page pixels) the mouse must travel with a button held before the cursor follows: a drag.
const TAP_SLOP: f32 = 10.0;

/// Page pixels per mouse count at this hand speed (counts per millisecond).
fn gain(speed: f32) -> f32 {
    BASE_GAIN * (1.0 + ACCELERATION * speed.clamp(0.0, MAX_ACCELERATED_SPEED))
}

/// Input collected by the plugin since the last update.
#[derive(Clone, Copy, Debug, Default)]
struct Input {
    active: bool,
    dx: i32,
    dy: i32,
    wheel: i32,
    buttons: u32,
}

pub struct Pointer {
    width: f32,
    height: f32,
    x: f32,
    y: f32,
    sensitivity: f32,
    active: bool,
    buttons: u32,
    last_movement: Option<Instant>,
    /// While a button is held and the mouse hasn't dragged yet: how far it has travelled.
    slop_travel: Option<f32>,
}

impl Pointer {
    /// A cursor for a page of this size, starting in its centre.
    pub fn new(width: u32, height: u32) -> Self {
        let (width, height) = (width as f32, height as f32);
        Self {
            width,
            height,
            x: width / 2.0,
            y: height / 2.0,
            sensitivity: 1.0,
            active: false,
            buttons: 0,
            last_movement: None,
            slop_travel: None,
        }
    }

    /// The page laid out at another size (`webview::Stage::set_page_height`): the cursor stays where it was on
    /// the screen, the same share of the way across and down.
    pub fn set_size(&mut self, width: u32, height: u32) {
        let (width, height) = (width as f32, height as f32);
        self.x = (self.x * width / self.width).clamp(0.0, width - 1.0);
        self.y = (self.y * height / self.height).clamp(0.0, height - 1.0);
        (self.width, self.height) = (width, height);
    }

    /// How fast the cursor moves: a multiplier on its speed curve (`settings::Sensitivity`).
    pub fn set_sensitivity(&mut self, sensitivity: f32) {
        self.sensitivity = sensitivity;
    }

    /// Takes the pointer input the plugin collected since the last call and sends it to the page. Returns the
    /// mouse as it is now, for the cursor drawn.
    pub fn update(&mut self, block: &PointerBlock, webview: &OffscreenWebView) -> Mouse {
        let active = block.active.load(Ordering::Acquire) == 1;
        let input = Input {
            active,
            dx: block.dx.swap(0, Ordering::AcqRel),
            dy: block.dy.swap(0, Ordering::AcqRel),
            wheel: block.wheel.swap(0, Ordering::AcqRel),
            buttons: if active { block.buttons.load(Ordering::Acquire) } else { 0 },
        };
        let buttons = self.buttons;
        for (kind, data) in self.step(input, Instant::now()) {
            let point = POINT { x: self.x as i32, y: self.y as i32 };
            let _ = webview.send_mouse(kind, self.virtual_keys(), data, point);
        }
        Mouse {
            active: self.active,
            x: self.x,
            y: self.y,
            pressed: self.buttons & PointerBlock::LEFT != 0,
            used: input.dx != 0 || input.dy != 0 || input.wheel != 0 || self.buttons != buttons,
        }
    }

    /// Applies one batch of input; returns the mouse events for the page, in order.
    fn step(&mut self, input: Input, now: Instant) -> Vec<(COREWEBVIEW2_MOUSE_EVENT_KIND, u32)> {
        let mut events = Vec::new();
        if !input.active {
            if self.active {
                self.active = false;
                self.set_buttons(0, &mut events);
                events.push((COREWEBVIEW2_MOUSE_EVENT_KIND_LEAVE, 0));
            }
            self.last_movement = None;
            return events;
        }
        let entered = !self.active;
        self.active = true;

        // Movement first: in a batch, the hand moved before it clicked.
        if self.apply_movement(input.dx, input.dy, now) || entered {
            events.push((COREWEBVIEW2_MOUSE_EVENT_KIND_MOVE, 0));
        }
        self.set_buttons(input.buttons, &mut events);
        if input.wheel != 0 {
            // DirectInput and WebView2 both count 120 per wheel notch, positive away from the user.
            events.push((COREWEBVIEW2_MOUSE_EVENT_KIND_WHEEL, input.wheel as u32));
        }
        events
    }

    /// Moves the cursor (accelerated, held still within the tap slop). Returns whether it moved.
    fn apply_movement(&mut self, dx: i32, dy: i32, now: Instant) -> bool {
        if dx == 0 && dy == 0 {
            return false;
        }
        let counts = (dx as f32).hypot(dy as f32);
        let interval = self
            .last_movement
            .map_or(MOVEMENT_INTERVAL.1, |last| now.duration_since(last))
            .clamp(MOVEMENT_INTERVAL.0, MOVEMENT_INTERVAL.1);
        self.last_movement = Some(now);
        let gain = gain(counts / interval.as_secs_f32() / 1000.0) * self.sensitivity;
        let (mx, my) = (dx as f32 * gain, dy as f32 * gain);

        if let Some(travel) = self.slop_travel.as_mut() {
            *travel += mx.hypot(my);
            if *travel < TAP_SLOP {
                return false;
            }
            // A drag: from here on the cursor follows.
            self.slop_travel = None;
        }
        let (x, y) = ((self.x + mx).clamp(0.0, self.width - 1.0), (self.y + my).clamp(0.0, self.height - 1.0));
        let moved = x != self.x || y != self.y;
        (self.x, self.y) = (x, y);
        moved
    }

    fn set_buttons(&mut self, buttons: u32, events: &mut Vec<(COREWEBVIEW2_MOUSE_EVENT_KIND, u32)>) {
        for (bit, down, up) in [
            (
                PointerBlock::LEFT,
                COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_DOWN,
                COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_UP,
            ),
            (
                PointerBlock::RIGHT,
                COREWEBVIEW2_MOUSE_EVENT_KIND_RIGHT_BUTTON_DOWN,
                COREWEBVIEW2_MOUSE_EVENT_KIND_RIGHT_BUTTON_UP,
            ),
        ] {
            if buttons & bit != self.buttons & bit {
                self.buttons ^= bit;
                events.push((if buttons & bit != 0 { down } else { up }, 0));
            }
        }
        self.slop_travel = match (self.buttons, self.slop_travel) {
            (0, _) => None,
            (_, None) if events.iter().any(|(kind, _)| [LEFT_DOWN, RIGHT_DOWN].contains(kind)) => Some(0.0),
            (_, travel) => travel,
        };
    }

    fn virtual_keys(&self) -> COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS {
        let mut keys = COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_NONE.0;
        if self.buttons & PointerBlock::LEFT != 0 {
            keys |= COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_LEFT_BUTTON.0;
        }
        if self.buttons & PointerBlock::RIGHT != 0 {
            keys |= COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_RIGHT_BUTTON.0;
        }
        COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS(keys)
    }
}

const LEFT_DOWN: COREWEBVIEW2_MOUSE_EVENT_KIND = COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_DOWN;
const RIGHT_DOWN: COREWEBVIEW2_MOUSE_EVENT_KIND = COREWEBVIEW2_MOUSE_EVENT_KIND_RIGHT_BUTTON_DOWN;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_taller_page_keeps_the_cursor_where_it_was_on_the_screen() {
        let mut pointer = Pointer::new(1280, 720);
        pointer.y = 360.0;
        pointer.set_size(1280, 793);
        assert_eq!((pointer.x, pointer.y), (640.0, 396.5));
        // And within it, at its foot.
        pointer.y = 792.0;
        pointer.set_size(1280, 720);
        assert!(pointer.y <= 719.0);
    }

    fn input(dx: i32, dy: i32, buttons: u32) -> Input {
        Input { active: true, dx, dy, wheel: 0, buttons }
    }

    #[test]
    fn accelerates_with_hand_speed_up_to_a_limit() {
        assert_eq!(gain(0.0), BASE_GAIN);
        assert!(gain(1.0) > gain(0.2));
        assert_eq!(gain(MAX_ACCELERATED_SPEED), gain(MAX_ACCELERATED_SPEED * 10.0));
    }

    #[test]
    fn slow_movement_is_precise_and_fast_movement_covers_more_ground() {
        let start = Instant::now();
        // The same 10 counts, 50 ms after the previous movement (0.2 counts/ms) or 4 ms after it (2.5).
        let travel = |gap_ms| {
            let mut pointer = Pointer::new(1280, 720);
            pointer.step(input(1, 0, 0), start);
            let before = pointer.x;
            pointer.step(input(10, 0, 0), start + Duration::from_millis(gap_ms));
            pointer.x - before
        };
        let (slow, fast) = (travel(50), travel(4));
        assert!((slow - 10.0 * gain(0.2)).abs() < 1e-3, "slow {slow}");
        assert!((fast - 10.0 * gain(2.5)).abs() < 1e-3, "fast {fast}");
        assert!(fast > slow * 2.0);
    }

    #[test]
    fn sensitivity_scales_movement() {
        let start = Instant::now();
        let travel = |sensitivity| {
            let mut pointer = Pointer::new(1280, 720);
            pointer.set_sensitivity(sensitivity);
            pointer.step(input(1, 0, 0), start);
            let before = pointer.x;
            pointer.step(input(20, 0, 0), start + Duration::from_millis(20));
            pointer.x - before
        };
        assert!((travel(2.0) - 2.0 * travel(1.0)).abs() < 1e-3);
    }

    #[test]
    fn a_click_lands_where_it_was_aimed() {
        let now = Instant::now();
        let mut pointer = Pointer::new(1280, 720);
        pointer.step(input(0, 0, 0), now);
        let events = pointer.step(input(2, 1, PointerBlock::LEFT), now);
        // The nudge that came with the press moves the cursor (it happened before the press)...
        assert_eq!(events.last().map(|e| e.0), Some(LEFT_DOWN));
        let (x, y) = (pointer.x, pointer.y);
        // ...but while the button is held, small movements don't.
        let events = pointer.step(input(3, -2, PointerBlock::LEFT), now);
        assert!(events.is_empty());
        assert_eq!((pointer.x, pointer.y), (x, y));
        let events = pointer.step(input(0, 0, 0), now);
        assert_eq!(events, vec![(COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_UP, 0)]);
    }

    #[test]
    fn a_drag_moves_the_cursor_once_past_the_slop() {
        let now = Instant::now();
        let mut pointer = Pointer::new(1280, 720);
        pointer.step(input(0, 0, 0), now);
        pointer.step(input(0, 0, PointerBlock::LEFT), now);
        let events = pointer.step(input(40, 0, PointerBlock::LEFT), now);
        assert_eq!(events, vec![(COREWEBVIEW2_MOUSE_EVENT_KIND_MOVE, 0)]);
        assert!(pointer.x > 640.0);
    }

    #[test]
    fn letting_go_of_the_key_releases_buttons_and_leaves() {
        let now = Instant::now();
        let mut pointer = Pointer::new(1280, 720);
        pointer.step(input(0, 0, PointerBlock::LEFT), now);
        let events = pointer.step(Input::default(), now);
        assert_eq!(
            events,
            vec![(COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_UP, 0), (COREWEBVIEW2_MOUSE_EVENT_KIND_LEAVE, 0)]
        );
    }
}
