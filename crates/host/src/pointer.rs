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
use crate::safari::Rect;
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
    /// Whether the mouse's events go to a web page shown in the cab (Safari's: [`WebArea`]) rather than the cabin
    /// page; and while a button is held, where it was pressed (its events stay there until it's let go, as a
    /// browser's do).
    on_web: bool,
    held_on_web: Option<bool>,
    /// The web page's last press, and how many in a row (a double click's second), for its events.
    web_press: Option<(Instant, f64, f64)>,
    web_clicks: u32,
    /// Whether the game's parking camera covered the screen with a button held that hasn't been let go since: as the
    /// camera goes, that button isn't pressed on the page (it was pressed on the camera).
    held_through_cover: bool,
}

/// A web page shown in the cab (Safari's tab), where the mouse reaches it: its rectangle of the cabin page (page
/// pixels; its own pixels from its top left), less where the cab draws over it (`covers`: the cab's there), and its
/// zoom (its CSS pixels this many of the frame's).
///
/// Its mouse goes to it as DevTools input events, which say which keys are held: none. Sent as ordinary mouse input,
/// the browser took the keys held on the player's keyboard for the click's, and the interaction key held to use the
/// screen made every click a modified one: with Left Alt, every link clicked was downloaded, as Alt-click does in a
/// browser (owner 2026-11-08: no page could be opened from Google's results); Ctrl and Shift would open links in
/// other tabs.
pub struct WebArea<'a> {
    pub view: &'a OffscreenWebView,
    pub area: Rect,
    pub covers: &'a [Rect],
    pub zoom: f64,
}

impl WebArea<'_> {
    fn contains(&self, x: f32, y: f32) -> bool {
        self.area.contains(x, y) && !self.covers.iter().any(|cover| cover.contains(x, y))
    }
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
            on_web: false,
            held_on_web: None,
            web_press: None,
            web_clicks: 1,
            held_through_cover: false,
        }
    }

    /// The input from the plugin (`active`: the interaction key held; `buttons` held), as the page gets it while the
    /// game's parking camera covers the screen or not (`covered`): under the camera none (as the key let go: buttons
    /// released, the mouse gone from the page, the cursor hidden), and after it the buttons held through it not
    /// pressed on the page until they're let go (owner 2026-11-13: under the parking camera, WestPlay takes no
    /// clicks).
    fn uncovered(&mut self, active: bool, buttons: u32, covered: bool) -> (bool, u32) {
        if covered {
            self.held_through_cover |= buttons != 0;
            return (false, 0);
        }
        if buttons == 0 {
            self.held_through_cover = false;
        }
        (active, if self.held_through_cover { 0 } else { buttons })
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

    /// Takes the pointer input the plugin collected since the last call and sends it to the page (or to the web
    /// page shown in it, where the mouse is over it: `web`); none while the game's parking camera covers the screen
    /// (`covered`). Returns the mouse as it is now, for the cursor drawn.
    pub fn update(
        &mut self,
        block: &PointerBlock,
        webview: &OffscreenWebView,
        web: Option<WebArea<'_>>,
        covered: bool,
    ) -> Mouse {
        let active = block.active.load(Ordering::Acquire) == 1;
        let held = if active { block.buttons.load(Ordering::Acquire) } else { 0 };
        let (active, buttons) = self.uncovered(active, held, covered);
        // Taken whether or not the page gets them: the movement under the camera isn't the page's after it.
        let input = Input {
            active,
            dx: block.dx.swap(0, Ordering::AcqRel),
            dy: block.dy.swap(0, Ordering::AcqRel),
            wheel: block.wheel.swap(0, Ordering::AcqRel),
            buttons,
        };
        let buttons = self.buttons;
        let events = self.step(input, Instant::now());
        let to_web = self.route(web.as_ref().map(|web| web.contains(self.x, self.y)));
        if to_web != self.on_web {
            // Off one, onto the other: the one left is told, as a browser tells a page the mouse left it.
            let leave = COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_NONE;
            if self.on_web {
                if let Some(web) = &web {
                    let _ = web.view.send_mouse(COREWEBVIEW2_MOUSE_EVENT_KIND_LEAVE, leave, 0, POINT::default());
                }
            } else {
                let _ = webview.send_mouse(COREWEBVIEW2_MOUSE_EVENT_KIND_LEAVE, leave, 0, POINT::default());
            }
            self.on_web = to_web;
        }
        let keys = self.virtual_keys();
        let now = Instant::now();
        for (kind, data) in events {
            match (&web, to_web) {
                (Some(web), true) => {
                    let zoom = web.zoom.max(0.1);
                    let (x, y) = (f64::from(self.x - web.area.x) / zoom, f64::from(self.y - web.area.y) / zoom);
                    if kind == LEFT_DOWN || kind == RIGHT_DOWN {
                        self.web_clicks = clicks_after(self.web_press, self.web_clicks, now, x, y);
                        self.web_press = Some((now, x, y));
                    }
                    if let Some(event) = web_event(kind, data, x, y, self.buttons, self.web_clicks) {
                        let _ = web.view.devtools("Input.dispatchMouseEvent", &event);
                    } else if kind == COREWEBVIEW2_MOUSE_EVENT_KIND_LEAVE {
                        let _ =
                            web.view.send_mouse(kind, COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS_NONE, 0, POINT::default());
                    }
                }
                (None, true) => {}
                (_, false) => {
                    let point = POINT { x: self.x as i32, y: self.y as i32 };
                    let _ = webview.send_mouse(kind, keys, data, point);
                }
            }
        }
        // A button pressed stays where it was pressed until it's let go.
        self.held_on_web = match (self.buttons, self.held_on_web) {
            (0, _) => None,
            (_, None) => Some(to_web),
            (_, held) => held,
        };
        Mouse {
            active: self.active,
            x: self.x,
            y: self.y,
            pressed: self.buttons & PointerBlock::LEFT != 0,
            used: input.dx != 0 || input.dy != 0 || input.wheel != 0 || self.buttons != buttons,
        }
    }

    /// Whether the mouse's events go to the web page (`over_web`: whether the cursor is over it; `None`: none
    /// shown): where a button held was pressed, else where the cursor is. A page gone while a button pressed on it
    /// is held: its events go nowhere until it's let go.
    fn route(&self, over_web: Option<bool>) -> bool {
        match (self.held_on_web, over_web) {
            (Some(held), _) => held,
            (None, over) => over.unwrap_or(false),
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

/// A double click's second press: this soon after the first (ms), this near it (CSS pixels).
const DOUBLE_CLICK: (Duration, f64) = (Duration::from_millis(500), 6.0);

/// How many presses in a row a press at `(x, y)` makes: one more than the last's if it was soon and near, else one.
fn clicks_after(last: Option<(Instant, f64, f64)>, count: u32, now: Instant, x: f64, y: f64) -> u32 {
    match last {
        Some((at, lx, ly)) if now.duration_since(at) <= DOUBLE_CLICK.0 && (x - lx).hypot(y - ly) <= DOUBLE_CLICK.1 => {
            count + 1
        }
        _ => 1,
    }
}

/// A mouse event for a web page as DevTools' `Input.dispatchMouseEvent` takes it (JSON): at `(x, y)` (CSS pixels),
/// the buttons held after it (`buttons`, the bridge's), `clicks` presses in a row; no keys held (`modifiers` 0). `None`
/// for one it has no kind for (the mouse leaving: sent as such).
fn web_event(
    kind: COREWEBVIEW2_MOUSE_EVENT_KIND,
    data: u32,
    x: f64,
    y: f64,
    buttons: u32,
    clicks: u32,
) -> Option<String> {
    use serde_json::json;
    let mask = u32::from(buttons & PointerBlock::LEFT != 0) | (u32::from(buttons & PointerBlock::RIGHT != 0) << 1);
    let held = if buttons & PointerBlock::LEFT != 0 {
        "left"
    } else if buttons & PointerBlock::RIGHT != 0 {
        "right"
    } else {
        "none"
    };
    let event = match kind {
        COREWEBVIEW2_MOUSE_EVENT_KIND_MOVE => {
            json!({ "type": "mouseMoved", "x": x, "y": y, "button": held, "buttons": mask, "modifiers": 0 })
        }
        LEFT_DOWN
        | RIGHT_DOWN
        | COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_UP
        | COREWEBVIEW2_MOUSE_EVENT_KIND_RIGHT_BUTTON_UP => {
            let pressed = kind == LEFT_DOWN || kind == RIGHT_DOWN;
            let left = kind == LEFT_DOWN || kind == COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_UP;
            json!({
                "type": if pressed { "mousePressed" } else { "mouseReleased" },
                "x": x,
                "y": y,
                "button": if left { "left" } else { "right" },
                "buttons": mask,
                "clickCount": clicks,
                "modifiers": 0,
            })
        }
        // The wheel's notches (120 each, positive away from the player: up the page) as pixels down the page.
        COREWEBVIEW2_MOUSE_EVENT_KIND_WHEEL => json!({
            "type": "mouseWheel",
            "x": x,
            "y": y,
            "deltaX": 0,
            "deltaY": -f64::from(data as i32),
            "modifiers": 0,
        }),
        _ => return None,
    };
    Some(event.to_string())
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
    fn a_press_stays_with_the_page_it_was_pressed_on() {
        let mut pointer = Pointer::new(1280, 720);
        // Over the web page, nothing held: its events go there; off it, to the cabin page.
        assert!(pointer.route(Some(true)));
        assert!(!pointer.route(Some(false)));
        assert!(!pointer.route(None));
        // Pressed on the web page and dragged off it (or the page gone): still the web page's.
        pointer.held_on_web = Some(true);
        assert!(pointer.route(Some(false)));
        assert!(pointer.route(None));
        // Pressed on the cabin page and dragged onto the web page: still the cabin page's.
        pointer.held_on_web = Some(false);
        assert!(!pointer.route(Some(true)));
    }

    #[test]
    fn a_web_pages_mouse_says_no_keys_held() {
        let read = |kind, data, buttons, clicks| {
            serde_json::from_str::<serde_json::Value>(&web_event(kind, data, 100.5, 40.0, buttons, clicks).unwrap())
                .unwrap()
        };
        let press = read(LEFT_DOWN, 0, PointerBlock::LEFT, 1);
        assert_eq!(press["type"], "mousePressed");
        assert_eq!(press["button"], "left");
        assert_eq!((press["x"].as_f64(), press["y"].as_f64()), (Some(100.5), Some(40.0)));
        assert_eq!(press["modifiers"], 0, "no key held, whatever the player's keyboard has down");
        let release = read(COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_UP, 0, 0, 2);
        assert_eq!((release["type"].as_str(), release["clickCount"].as_u64()), (Some("mouseReleased"), Some(2)));
        let dragging = read(COREWEBVIEW2_MOUSE_EVENT_KIND_MOVE, 0, PointerBlock::LEFT, 1);
        assert_eq!((dragging["button"].as_str(), dragging["buttons"].as_u64()), (Some("left"), Some(1)));
        // A notch away from the player: up the page.
        let wheel = read(COREWEBVIEW2_MOUSE_EVENT_KIND_WHEEL, 120, 0, 1);
        assert_eq!(wheel["deltaY"].as_f64(), Some(-120.0));
        let wheel = read(COREWEBVIEW2_MOUSE_EVENT_KIND_WHEEL, (-120i32) as u32, 0, 1);
        assert_eq!(wheel["deltaY"].as_f64(), Some(120.0));
        assert!(web_event(COREWEBVIEW2_MOUSE_EVENT_KIND_LEAVE, 0, 0.0, 0.0, 0, 1).is_none());
    }

    #[test]
    fn counts_a_double_clicks_presses() {
        let now = Instant::now();
        assert_eq!(clicks_after(None, 1, now, 10.0, 10.0), 1);
        let first = Some((now, 10.0, 10.0));
        assert_eq!(clicks_after(first, 1, now + Duration::from_millis(200), 12.0, 11.0), 2);
        assert_eq!(clicks_after(first, 1, now + Duration::from_millis(700), 10.0, 10.0), 1, "too late");
        assert_eq!(clicks_after(first, 1, now + Duration::from_millis(200), 40.0, 10.0), 1, "too far");
    }

    /// One update as the plugin's input comes (`active`, `buttons`) with the camera over the screen or not: the events
    /// the page gets.
    fn under(
        pointer: &mut Pointer,
        active: bool,
        buttons: u32,
        covered: bool,
    ) -> Vec<(COREWEBVIEW2_MOUSE_EVENT_KIND, u32)> {
        let (active, buttons) = pointer.uncovered(active, buttons, covered);
        pointer.step(Input { active, dx: 5, dy: 0, wheel: 120, buttons }, Instant::now())
    }

    #[test]
    fn the_parking_camera_over_the_screen_takes_its_clicks() {
        let left = PointerBlock::LEFT;
        let mut pointer = Pointer::new(1280, 720);
        under(&mut pointer, true, 0, false);
        under(&mut pointer, true, left, false);
        // The camera comes up with the button held: let go on the page, and the mouse gone from it.
        let events = under(&mut pointer, true, left, true);
        assert_eq!(
            events,
            vec![(COREWEBVIEW2_MOUSE_EVENT_KIND_LEFT_BUTTON_UP, 0), (COREWEBVIEW2_MOUSE_EVENT_KIND_LEAVE, 0)]
        );
        // Under it, clicks, moves and the wheel reach nothing.
        assert!(under(&mut pointer, true, 0, true).is_empty());
        assert!(under(&mut pointer, true, left, true).is_empty());
        // The camera gone with the button still held: the mouse back, the button not pressed on the page...
        let events = under(&mut pointer, true, left, false);
        assert!(!events.iter().any(|(kind, _)| *kind == LEFT_DOWN), "{events:?}");
        assert!(events.contains(&(COREWEBVIEW2_MOUSE_EVENT_KIND_MOVE, 0)));
        // ...until it's let go: then a press is a press.
        under(&mut pointer, true, 0, false);
        let events = under(&mut pointer, true, left, false);
        assert!(events.contains(&(LEFT_DOWN, 0)), "{events:?}");
    }

    #[test]
    fn the_parking_camera_shown_lately_covers_the_screen() {
        use westplay_shared::{PARKING_SHOWN_FRAMES, parking_camera_shown};
        assert!(!parking_camera_shown(0, 500), "never drawn");
        assert!(parking_camera_shown(500, 500));
        assert!(parking_camera_shown(500, 500 + PARKING_SHOWN_FRAMES));
        assert!(!parking_camera_shown(500, 501 + PARKING_SHOWN_FRAMES));
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
