//! Screen interaction: DirectInput hooks.
//!
//! The game reads the mouse through DirectInput 8. While the player holds the interaction key, these hooks
//! take it away from the game: it sees no mouse movement or buttons, so the head view stays on the screen
//! (but a button it saw go down still comes up for it: none stays held). The mouse goes to the app through
//! the bridge's pointer block instead, which drives the WestPlay screen with it. (The keyboard is the
//! `keyboard` module's: the game reads it from window messages. Should a game read a keyboard through
//! DirectInput too, it's kept from the same keys there while the player types into WestPlay, and only then.)
//!
//! Both ways DirectInput reports input are covered: immediate (`GetDeviceState`) and buffered
//! (`GetDeviceData`). A mouse read both ways reports each movement to both, so while one is read buffered its
//! movement is taken from there alone (under TruckersMP, which reads ETS2's mouse immediately as the game
//! reads it buffered, the cursor went twice as far). One hook covers every device (all share DirectInput's
//! device implementation); each device is classified once by its capabilities. The game reads the mouse in its
//! own data format, so where each axis and button sits in it is asked from DirectInput rather than assumed
//! (see [`MouseLayout`]).

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use westplay_shared::{DEFAULT_INTERACTION_KEY, PointerBlock};
use windows::Win32::Devices::HumanInterfaceDevice::{
    DI8DEVTYPE_KEYBOARD, DI8DEVTYPE_MOUSE, DIDEVCAPS, DIDEVICEOBJECTDATA, DIDEVICEOBJECTINSTANCEA,
    DIDEVICEOBJECTINSTANCEW, DIGDD_PEEK, DIPH_BYOFFSET, DIRECTINPUT_VERSION, DirectInput8Create, GUID_Button,
    GUID_SysMouse, GUID_XAxis, GUID_YAxis, GUID_ZAxis, IDirectInput8A, IDirectInput8W, IDirectInputDevice8A,
    IDirectInputDevice8W,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows::core::{GUID, HRESULT, Interface};

use crate::hooks::{guarded, hook, vtable_entry};
use crate::{bridge, log, typing};

// IDirectInputDevice8 vtable slots (IUnknown = 0..2).
const VTBL_GET_DEVICE_STATE: usize = 9;
const VTBL_GET_DEVICE_DATA: usize = 10;

/// Offsets probed for objects in a mouse's data format. DirectInput's mouse formats are 16 or 20 bytes, but a
/// game may read its mouse in a bigger one: ETS2 uses a joystick-style format with the buttons past byte 64
/// (`DIJOYSTATE2` is 272 bytes).
const MAX_MOUSE_FORMAT_SIZE: u32 = 512;
/// Mouse buttons WestPlay uses: left, right, middle.
const MOUSE_BUTTONS: usize = 3;
/// `DIDFT_PSHBUTTON | DIDFT_TGLBUTTON`: the object is a button.
const DIDFT_BUTTON: u32 = 0x0c;

/// An object of a data format: its offset, type (`GUID_XAxis`, `GUID_Button`, ...) and `dwType`.
type FormatObject = (u32, GUID, u32);
const KEYBOARD_STATE_SIZE: u32 = 256;

type GetDeviceStateFn = unsafe extern "system" fn(*mut c_void, u32, *mut c_void) -> HRESULT;
type GetDeviceDataFn = unsafe extern "system" fn(*mut c_void, u32, *mut DIDEVICEOBJECTDATA, *mut u32, u32) -> HRESULT;

// The ANSI and Unicode device interfaces may or may not share these functions; each gets its own hook if not.
static ORIG_GET_DEVICE_STATE_W: AtomicUsize = AtomicUsize::new(0);
static ORIG_GET_DEVICE_STATE_A: AtomicUsize = AtomicUsize::new(0);
static ORIG_GET_DEVICE_DATA_W: AtomicUsize = AtomicUsize::new(0);
static ORIG_GET_DEVICE_DATA_A: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeviceKind {
    Mouse(MouseLayout),
    Keyboard,
    Other,
}

/// Where a mouse's objects sit in the data format the game set for it: offsets into its device state, which
/// are also the `dwOfs` of its buffered events. Axes are 4-byte relative counts, buttons one byte (pressed
/// = 0x80).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct MouseLayout {
    x: Option<u32>,
    y: Option<u32>,
    wheel: Option<u32>,
    buttons: [Option<u32>; MOUSE_BUTTONS],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MouseObject {
    X,
    Y,
    Wheel,
    Button(usize),
}

impl MouseLayout {
    /// The object at this offset of the data format, if it's one WestPlay uses.
    fn object(&self, offset: u32) -> Option<MouseObject> {
        let at = Some(offset);
        if at == self.x {
            Some(MouseObject::X)
        } else if at == self.y {
            Some(MouseObject::Y)
        } else if at == self.wheel {
            Some(MouseObject::Wheel)
        } else {
            self.buttons.iter().position(|button| *button == at).map(MouseObject::Button)
        }
    }

    /// Asks DirectInput what the device's current data format holds at each offset.
    ///
    /// # Safety
    /// `device` is a live DirectInput device (the game's, as its own call passed it).
    unsafe fn probe(device: *mut c_void) -> Self {
        // SAFETY: a live DirectInput device (the caller's contract).
        unsafe {
            let objects: Vec<FormatObject> = (0..MAX_MOUSE_FORMAT_SIZE)
                .filter_map(|offset| object_at(device, offset).map(|(kind, object_type)| (offset, kind, object_type)))
                .collect();
            log!(
                "mouse data format: {}",
                objects
                    .iter()
                    .map(|(offset, kind, t)| format!("{offset}: {kind:?} {t:#x}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            Self::from_objects(&objects)
        }
    }

    /// The layout of a data format holding these objects. Buttons are told apart by their order: DirectInput
    /// numbers a mouse's objects in one sequence (the system mouse's buttons come after its three axes), so
    /// the lowest-numbered button is the left one, then right, then middle.
    fn from_objects(objects: &[FormatObject]) -> Self {
        let mut layout = Self::default();
        let mut buttons = Vec::new();
        for &(offset, kind, object_type) in objects {
            match kind {
                kind if kind == GUID_XAxis => layout.x = Some(offset),
                kind if kind == GUID_YAxis => layout.y = Some(offset),
                kind if kind == GUID_ZAxis => layout.wheel = Some(offset),
                kind if kind == GUID_Button || object_type & DIDFT_BUTTON != 0 => {
                    // DIDFT_GETINSTANCE: the object's number.
                    buttons.push(((object_type >> 8) & 0xffff, offset));
                }
                _ => {}
            }
        }
        buttons.sort_unstable();
        for (slot, (_, offset)) in layout.buttons.iter_mut().zip(buttons) {
            *slot = Some(offset);
        }
        layout
    }
}

/// The type (`GUID_XAxis`, `GUID_Button`, ...) and `dwType` of the object at `offset` of the device's data
/// format. The Unicode and ANSI interfaces take differently sized structures; the device is one or the other.
///
/// # Safety
/// As [`MouseLayout::probe`].
unsafe fn object_at(device: *mut c_void, offset: u32) -> Option<(GUID, u32)> {
    // SAFETY: a live DirectInput device (the caller's contract); the infos are locals with their sizes set, as
    // the calls require (all zeros is valid for them).
    unsafe {
        let unicode = IDirectInputDevice8W::from_raw_borrowed(&device)?;
        let mut info: DIDEVICEOBJECTINSTANCEW = std::mem::zeroed();
        info.dwSize = size_of::<DIDEVICEOBJECTINSTANCEW>() as u32;
        if unicode.GetObjectInfo(&mut info, offset, DIPH_BYOFFSET).is_ok() {
            return Some((info.guidType, info.dwType));
        }
        let ansi = IDirectInputDevice8A::from_raw_borrowed(&device)?;
        let mut info: DIDEVICEOBJECTINSTANCEA = std::mem::zeroed();
        info.dwSize = size_of::<DIDEVICEOBJECTINSTANCEA>() as u32;
        ansi.GetObjectInfo(&mut info, offset, DIPH_BYOFFSET).ok()?;
        Some((info.guidType, info.dwType))
    }
}

static DEVICE_KINDS: LazyLock<Mutex<HashMap<usize, DeviceKind>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
/// The mouse buttons each device's buffered input has told the game are down and not yet up (bits by their
/// index in the device's [`MouseLayout`]): while the player uses the screen, their releases still reach it.
static GAME_HELD: LazyLock<Mutex<HashMap<usize, u8>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
static ACTIVE: AtomicBool = AtomicBool::new(false);
/// When each mouse was last read buffered for real (not peeked), by anyone: its movement and wheel are then
/// taken from those reads alone, its immediate reads giving the screen only its buttons.
static BUFFERED_READ: LazyLock<Mutex<HashMap<usize, Instant>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
/// How recently a buffered read leaves a mouse's immediate movement out (the game reads it every frame).
const BUFFERED_RECENT: Duration = Duration::from_millis(500);

/// Whether a mouse's immediate read moves the screen's cursor: not while it's read buffered too (each way
/// reports every movement).
fn immediate_moves(last_buffered: Option<Instant>, now: Instant) -> bool {
    last_buffered.is_none_or(|at| now.saturating_duration_since(at) >= BUFFERED_RECENT)
}

/// # Safety
/// In the game's process, before the hooks are switched on ([`crate::hooks::enable_all`]).
pub unsafe fn install() -> Result<(), String> {
    // SAFETY: throwaway DirectInput objects made here, only to read where their functions are: dinput8.dll's
    // own, hooked with detours of their exact signatures (below), and valid after the objects go.
    unsafe {
        let instance = GetModuleHandleW(None).map_err(|e| format!("GetModuleHandleW: {e}"))?;
        let mut unicode: Option<IDirectInput8W> = None;
        DirectInput8Create(
            instance.into(),
            DIRECTINPUT_VERSION,
            &IDirectInput8W::IID,
            &mut unicode as *mut _ as _,
            None,
        )
        .map_err(|e| format!("DirectInput8Create (W): {e}"))?;
        let mut ansi: Option<IDirectInput8A> = None;
        DirectInput8Create(instance.into(), DIRECTINPUT_VERSION, &IDirectInput8A::IID, &mut ansi as *mut _ as _, None)
            .map_err(|e| format!("DirectInput8Create (A): {e}"))?;
        let (Some(unicode), Some(ansi)) = (unicode, ansi) else {
            return Err("DirectInput8Create returned null".into());
        };

        let mut device_w: Option<IDirectInputDevice8W> = None;
        unicode.CreateDevice(&GUID_SysMouse, &mut device_w, None).map_err(|e| format!("CreateDevice (W): {e}"))?;
        let mut device_a: Option<IDirectInputDevice8A> = None;
        ansi.CreateDevice(&GUID_SysMouse, &mut device_a, None).map_err(|e| format!("CreateDevice (A): {e}"))?;
        let (Some(device_w), Some(device_a)) = (device_w, device_a) else {
            return Err("CreateDevice returned null".into());
        };

        // The functions live in dinput8.dll and stay valid after these throwaway objects are released.
        let state_w = vtable_entry(device_w.as_raw(), VTBL_GET_DEVICE_STATE);
        let state_a = vtable_entry(device_a.as_raw(), VTBL_GET_DEVICE_STATE);
        let data_w = vtable_entry(device_w.as_raw(), VTBL_GET_DEVICE_DATA);
        let data_a = vtable_entry(device_a.as_raw(), VTBL_GET_DEVICE_DATA);

        hook(
            state_w,
            get_device_state_w as *mut c_void,
            &ORIG_GET_DEVICE_STATE_W,
            "IDirectInputDevice8W::GetDeviceState",
        )?;
        hook(data_w, get_device_data_w as *mut c_void, &ORIG_GET_DEVICE_DATA_W, "IDirectInputDevice8W::GetDeviceData")?;
        if state_a != state_w {
            hook(
                state_a,
                get_device_state_a as *mut c_void,
                &ORIG_GET_DEVICE_STATE_A,
                "IDirectInputDevice8A::GetDeviceState",
            )?;
        }
        if data_a != data_w {
            hook(
                data_a,
                get_device_data_a as *mut c_void,
                &ORIG_GET_DEVICE_DATA_A,
                "IDirectInputDevice8A::GetDeviceData",
            )?;
        }
        Ok(())
    }
}

/// # Safety
/// Called in place of `IDirectInputDevice8W::GetDeviceState`, with its arguments.
unsafe extern "system" fn get_device_state_w(device: *mut c_void, size: u32, data: *mut c_void) -> HRESULT {
    // SAFETY: as `get_device_state` asks: its own trampoline, and the game's arguments.
    unsafe { get_device_state(&ORIG_GET_DEVICE_STATE_W, device, size, data) }
}

/// # Safety
/// Called in place of `IDirectInputDevice8A::GetDeviceState`, with its arguments.
unsafe extern "system" fn get_device_state_a(device: *mut c_void, size: u32, data: *mut c_void) -> HRESULT {
    // SAFETY: as `get_device_state` asks: its own trampoline, and the game's arguments.
    unsafe { get_device_state(&ORIG_GET_DEVICE_STATE_A, device, size, data) }
}

/// # Safety
/// Called in place of `IDirectInputDevice8W::GetDeviceData`, with its arguments.
unsafe extern "system" fn get_device_data_w(
    device: *mut c_void,
    object_size: u32,
    data: *mut DIDEVICEOBJECTDATA,
    count: *mut u32,
    flags: u32,
) -> HRESULT {
    // SAFETY: as `get_device_data` asks: its own trampoline, and the game's arguments.
    unsafe { get_device_data(&ORIG_GET_DEVICE_DATA_W, device, object_size, data, count, flags) }
}

/// # Safety
/// Called in place of `IDirectInputDevice8A::GetDeviceData`, with its arguments.
unsafe extern "system" fn get_device_data_a(
    device: *mut c_void,
    object_size: u32,
    data: *mut DIDEVICEOBJECTDATA,
    count: *mut u32,
    flags: u32,
) -> HRESULT {
    // SAFETY: as `get_device_data` asks: its own trampoline, and the game's arguments.
    unsafe { get_device_data(&ORIG_GET_DEVICE_DATA_A, device, object_size, data, count, flags) }
}

/// # Safety
/// `original` holds a `GetDeviceState` trampoline; the rest are the game's arguments to it.
unsafe fn get_device_state(original: &AtomicUsize, device: *mut c_void, size: u32, data: *mut c_void) -> HRESULT {
    // SAFETY: the trampoline was stored before the hooks were switched on (`install`, then `enable_all`), so it's
    // the original function, called with the caller's arguments as they came. The state is looked at only once
    // it's filled (`data` set).
    unsafe {
        let original: GetDeviceStateFn = std::mem::transmute(original.load(Ordering::Acquire));
        let result = original(device, size, data);
        if result.is_ok() && !data.is_null() {
            guarded(|| on_device_state(device, size, data as *mut u8));
        }
        result
    }
}

/// # Safety
/// `original` holds a `GetDeviceData` trampoline; the rest are the game's arguments to it.
unsafe fn get_device_data(
    original: &AtomicUsize,
    device: *mut c_void,
    object_size: u32,
    data: *mut DIDEVICEOBJECTDATA,
    count: *mut u32,
    flags: u32,
) -> HRESULT {
    // SAFETY: the trampoline was stored before the hooks were switched on (`install`, then `enable_all`), so it's
    // the original function, called with the caller's arguments as they came. The events are looked at only
    // once counted (`count` set).
    unsafe {
        let original: GetDeviceDataFn = std::mem::transmute(original.load(Ordering::Acquire));
        let result = original(device, object_size, data, count, flags);
        if result.is_ok() && !count.is_null() {
            guarded(|| on_device_data(device, object_size, data, count, flags));
        }
        result
    }
}

/// Immediate input: the device's whole current state.
///
/// # Safety
/// `state` is `size` bytes of `device`'s state, just filled by the original `GetDeviceState`.
unsafe fn on_device_state(device: *mut c_void, size: u32, state: *mut u8) {
    // SAFETY: `state` is `size` bytes the original call just filled; every read and write stays within them
    // (each axis and button checked against `size`).
    unsafe {
        let kind = device_kind(device);
        // While the player types into WestPlay, the keyboard's keys are its (but the game's own).
        if kind == DeviceKind::Keyboard && size == KEYBOARD_STATE_SIZE && typing::typing() {
            for scancode in (0..KEYBOARD_STATE_SIZE).filter(|&code| !typing::stays_the_games_scancode(code)) {
                *state.add(scancode as usize) = 0;
            }
        }
        if !interaction_active() {
            return;
        }
        if let DeviceKind::Mouse(layout) = kind {
            let axis = |offset: Option<u32>| match offset {
                Some(offset) if offset + 4 <= size => (state.add(offset as usize) as *const i32).read_unaligned(),
                _ => 0,
            };
            let (x, y) = (axis(layout.x), axis(layout.y));
            let buffered = BUFFERED_READ.lock().unwrap_or_else(|e| e.into_inner()).get(&(device as usize)).copied();
            let moves = immediate_moves(buffered, Instant::now());
            SOURCES.state(x, y, moves);
            if moves {
                take_mouse(x, y, axis(layout.wheel));
            }
            let mut buttons = 0;
            for (index, offset) in layout.buttons.iter().enumerate() {
                if let Some(offset) = *offset
                    && offset < size
                    && *state.add(offset as usize) & 0x80 != 0
                {
                    buttons |= 1 << index;
                }
            }
            set_buttons(buttons);
            std::ptr::write_bytes(state, 0, size as usize);
        }
    }
}

/// Buffered input: events since the last read. While interacting, the mouse's go to the screen, and the
/// game gets none of them but the releases of buttons it saw go down; while the player types into WestPlay, a
/// keyboard's lose every key but the game's own.
///
/// # Safety
/// The game's arguments after the original `GetDeviceData` filled them: `count` set, and `data` (if
/// any) holding that many events of `object_size` bytes.
unsafe fn on_device_data(
    device: *mut c_void,
    object_size: u32,
    data: *mut DIDEVICEOBJECTDATA,
    count: *mut u32,
    flags: u32,
) {
    // SAFETY: `count` events of `object_size` bytes each are what the original call just wrote into `data`
    // (their size checked to hold an event's head); only those are read, and fewer kept.
    unsafe {
        let kind = device_kind(device);
        if kind == DeviceKind::Other {
            return;
        }
        if matches!(kind, DeviceKind::Mouse(_)) && flags & DIGDD_PEEK == 0 {
            BUFFERED_READ.lock().unwrap_or_else(|e| e.into_inner()).insert(device as usize, Instant::now());
        }
        let interacting = interaction_active();
        // No events to look at (the game counting or flushing them), or too short to read: none, while
        // interacting.
        if data.is_null() || (object_size as usize) < EVENT_HEAD {
            if interacting {
                *count = 0;
            }
            return;
        }
        let events = std::slice::from_raw_parts_mut(data as *mut u8, *count as usize * object_size as usize);
        // A peek leaves the events in the buffer: they're taken when the game reads them for real.
        let peek = flags & DIGDD_PEEK != 0;
        let kept = match kind {
            DeviceKind::Mouse(layout) => {
                let mut held = GAME_HELD.lock().unwrap_or_else(|e| e.into_inner());
                let held = held.entry(device as usize).or_default();
                let mut now_held = *held;
                if interacting && !peek {
                    SOURCES.buffered_read();
                }
                let kept = retain_events(events, object_size as usize, |offset, value| {
                    let object = layout.object(offset);
                    if interacting && !peek {
                        if matches!(object, Some(MouseObject::X | MouseObject::Y)) {
                            SOURCES.buffered_event(value as i32);
                        }
                        to_screen(object, value);
                    }
                    game_gets(object, value & 0x80 != 0, interacting, &mut now_held)
                });
                if !peek {
                    *held = now_held;
                }
                kept
            }
            DeviceKind::Keyboard => {
                let typing = typing::typing();
                retain_events(events, object_size as usize, |offset, _| {
                    !typing || typing::stays_the_games_scancode(offset)
                })
            }
            DeviceKind::Other => return,
        };
        *count = kept as u32;
    }
}

/// The bytes of a buffered event read: its object's offset and its value, the fields every version of the
/// struct starts with (it may be DirectX 3's shorter one).
const EVENT_HEAD: usize = 8;

/// Keeps the buffered events `keep` says yes to (given each one's object offset and value), packed at the
/// front of `events` in their order (each `size` bytes, at least [`EVENT_HEAD`]); returns how many.
fn retain_events(events: &mut [u8], size: usize, mut keep: impl FnMut(u32, u32) -> bool) -> usize {
    let mut kept = 0;
    for index in 0..events.len() / size {
        let at = index * size;
        let word =
            |from: usize| u32::from_le_bytes([events[from], events[from + 1], events[from + 2], events[from + 3]]);
        let (offset, value) = (word(at), word(at + 4));
        if keep(offset, value) {
            if kept != index {
                events.copy_within(at..at + size, kept * size);
            }
            kept += 1;
        }
    }
    kept
}

/// Whether a mouse event goes to the game, keeping track in `held` of the buttons it has seen go down: all
/// of them while the player isn't using the screen; while they are, only the release of a button it holds.
fn game_gets(object: Option<MouseObject>, pressed: bool, interacting: bool, held: &mut u8) -> bool {
    match object {
        Some(MouseObject::Button(index)) => {
            let bit = 1 << index;
            if !interacting {
                if pressed {
                    *held |= bit
                } else {
                    *held &= !bit
                }
                true
            } else if !pressed && *held & bit != 0 {
                *held &= !bit;
                true
            } else {
                false
            }
        }
        _ => !interacting,
    }
}

/// Where the screen's mouse movement came from while the player used the screen, logged when they stop: the
/// immediate reads (`GetDeviceState`) and the buffered ones (`GetDeviceData`), how many and how much movement
/// (mouse counts, x and y added up unsigned), and the immediate movement left out, the mouse being read
/// buffered too ([`immediate_moves`]).
struct MouseSources {
    state_reads: AtomicU64,
    state_moving: AtomicU64,
    state_counts: AtomicU64,
    state_left_out: AtomicU64,
    buffered_reads: AtomicU64,
    buffered_events: AtomicU64,
    buffered_counts: AtomicU64,
}

static SOURCES: MouseSources = MouseSources {
    state_reads: AtomicU64::new(0),
    state_moving: AtomicU64::new(0),
    state_counts: AtomicU64::new(0),
    state_left_out: AtomicU64::new(0),
    buffered_reads: AtomicU64::new(0),
    buffered_events: AtomicU64::new(0),
    buffered_counts: AtomicU64::new(0),
};

impl MouseSources {
    fn state(&self, x: i32, y: i32, taken: bool) {
        self.state_reads.fetch_add(1, Ordering::Relaxed);
        if x != 0 || y != 0 {
            let counts = u64::from(x.unsigned_abs() + y.unsigned_abs());
            self.state_moving.fetch_add(1, Ordering::Relaxed);
            self.state_counts.fetch_add(counts, Ordering::Relaxed);
            if !taken {
                self.state_left_out.fetch_add(counts, Ordering::Relaxed);
            }
        }
    }

    fn buffered_read(&self) {
        self.buffered_reads.fetch_add(1, Ordering::Relaxed);
    }

    fn buffered_event(&self, value: i32) {
        self.buffered_events.fetch_add(1, Ordering::Relaxed);
        self.buffered_counts.fetch_add(u64::from(value.unsigned_abs()), Ordering::Relaxed);
    }

    /// Logs what came since the last time, and starts again.
    fn log_and_reset(&self) {
        let take = |counter: &AtomicU64| counter.swap(0, Ordering::Relaxed);
        log!(
            "mouse while interacting: immediate reads {} ({} with movement, {} counts, {} left out), buffered reads {} ({} movement events, {} counts)",
            take(&self.state_reads),
            take(&self.state_moving),
            take(&self.state_counts),
            take(&self.state_left_out),
            take(&self.buffered_reads),
            take(&self.buffered_events),
            take(&self.buffered_counts)
        );
    }
}

/// A mouse event while the player uses the screen: to the screen.
fn to_screen(object: Option<MouseObject>, value: u32) {
    match object {
        Some(MouseObject::X) => take_mouse(value as i32, 0, 0),
        Some(MouseObject::Y) => take_mouse(0, value as i32, 0),
        Some(MouseObject::Wheel) => take_mouse(0, 0, value as i32),
        Some(MouseObject::Button(index)) => {
            if let Some(pointer) = pointer_block() {
                if value & 0x80 != 0 {
                    pointer.buttons.fetch_or(1 << index, Ordering::AcqRel);
                } else {
                    pointer.buttons.fetch_and(!(1 << index), Ordering::AcqRel);
                }
            }
        }
        None => {}
    }
}

/// Whether the player is using the screen: WestPlay is on it and the interaction key (the player's choice,
/// through the bridge) is held, or toggled on. Publishes changes to the app. Without WestPlay on the screen,
/// the key takes nothing from the game.
pub fn interaction_active() -> bool {
    // Never while the game is paused: its menu and its map are used with the mouse (owner 2026-10-21: toggled
    // on, the menu couldn't be used).
    let active = bridge::screen_on() && !crate::telemetry::paused() && interaction_key_held();
    if ACTIVE.swap(active, Ordering::AcqRel) != active {
        log!("screen interaction {}", if active { "on" } else { "off" });
        if !active {
            SOURCES.log_and_reset();
        }
        if let Some(pointer) = pointer_block() {
            if !active {
                pointer.buttons.store(0, Ordering::Release);
            }
            pointer.active.store(active as u32, Ordering::Release);
        }
    }
    active
}

/// Whether the interaction key says the screen is used, whether or not WestPlay is on it: held, or (the
/// player's choice) toggled on.
fn interaction_key_held() -> bool {
    if toggle_mode() { TOGGLED.load(Ordering::Acquire) } else { interaction_key_down() }
}

/// Whether the interaction key is down.
fn interaction_key_down() -> bool {
    let key = match bridge::get().map(|bridge| bridge.host.interaction_key.load(Ordering::Acquire)) {
        Some(key) if key != 0 => key,
        _ => DEFAULT_INTERACTION_KEY,
    };
    // SAFETY: a plain query, no pointers.
    unsafe { GetAsyncKeyState(key as i32) as u16 & 0x8000 != 0 }
}

/// Whether the interaction key is pressed once to use the screen and again to stop, rather than held.
fn toggle_mode() -> bool {
    bridge::get().is_some_and(|bridge| bridge.host.interaction_toggle.load(Ordering::Acquire) == 1)
}

/// In toggle mode: whether the player has turned using the screen on.
static TOGGLED: AtomicBool = AtomicBool::new(false);
/// Whether the interaction key was down at the last frame.
static KEY_WAS_DOWN: AtomicBool = AtomicBool::new(false);

/// Once a frame: in toggle mode, each press of the interaction key turns using the screen on or off. It's
/// off whenever WestPlay isn't on the screen, and as the game pauses.
pub fn on_frame() {
    let pressed = newly_down(&KEY_WAS_DOWN, interaction_key_down());
    let off = if !toggle_mode() {
        Some("held mode")
    } else if !bridge::screen_on() {
        Some("WestPlay not on the screen")
    } else if crate::telemetry::paused() {
        // Its menu wants the mouse; back in the game, a press uses the screen again.
        Some("the game paused")
    } else {
        None
    };
    if let Some(why) = off {
        if TOGGLED.swap(false, Ordering::AcqRel) {
            log!("screen use toggled off: {why}");
        }
        return;
    }
    if pressed {
        TOGGLED.fetch_xor(true, Ordering::AcqRel);
    }
}

/// Whether a key is down now and wasn't at the last look (`was_down`, which takes `down` for the next). The
/// last look is kept whatever it was: kept only while the key was down (`down && !was_down.swap(down)`, the
/// swap skipped while it's up), the first release was never seen, and no press counted after the first
/// (owner 2026-10-21: toggled on, the key couldn't toggle off).
pub fn newly_down(was_down: &AtomicBool, down: bool) -> bool {
    let before = was_down.swap(down, Ordering::AcqRel);
    down && !before
}

fn take_mouse(dx: i32, dy: i32, wheel: i32) {
    if let Some(pointer) = pointer_block() {
        pointer.dx.fetch_add(dx, Ordering::AcqRel);
        pointer.dy.fetch_add(dy, Ordering::AcqRel);
        pointer.wheel.fetch_add(wheel, Ordering::AcqRel);
    }
}

fn set_buttons(bits: u32) {
    if let Some(pointer) = pointer_block() {
        pointer
            .buttons
            .store(bits & (PointerBlock::LEFT | PointerBlock::RIGHT | PointerBlock::MIDDLE), Ordering::Release);
    }
}

fn pointer_block() -> Option<&'static PointerBlock> {
    bridge::get().map(|bridge| &bridge.plugin.pointer)
}

/// What kind of device this is, from its capabilities (looked up once per device).
///
/// # Safety
/// `device` is a live DirectInput device (the game's, as its own call passed it).
unsafe fn device_kind(device: *mut c_void) -> DeviceKind {
    // SAFETY: a live DirectInput device (the caller's contract); `caps` is a local with its size set.
    unsafe {
        let mut kinds = DEVICE_KINDS.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(kind) = kinds.get(&(device as usize)) {
            return *kind;
        }
        // GetCapabilities has the same slot and signature on the ANSI and Unicode interfaces.
        let mut caps = DIDEVCAPS { dwSize: size_of::<DIDEVCAPS>() as u32, ..Default::default() };
        let kind = match IDirectInputDevice8W::from_raw_borrowed(&device).map(|d| d.GetCapabilities(&mut caps)) {
            Some(Ok(())) => match caps.dwDevType & 0xff {
                DI8DEVTYPE_MOUSE => DeviceKind::Mouse(MouseLayout::probe(device)),
                DI8DEVTYPE_KEYBOARD => DeviceKind::Keyboard,
                _ => DeviceKind::Other,
            },
            _ => DeviceKind::Other,
        };
        log!(
            "dinput device {device:p}: {kind:?} (type {:#x}, {} axes, {} buttons)",
            caps.dwDevType,
            caps.dwAxes,
            caps.dwButtons
        );
        kinds.insert(device as usize, kind);
        kind
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_press_counts_once_however_many_frames_it_lasts() {
        let was_down = AtomicBool::new(false);
        let frames = [false, true, true, false, false, true, false, true, true, true, false];
        let presses: Vec<bool> = frames.iter().map(|&down| newly_down(&was_down, down)).collect();
        assert_eq!(presses.iter().filter(|&&pressed| pressed).count(), 3);
        assert_eq!(presses, [false, true, false, false, false, true, false, true, false, false, false]);
    }

    #[test]
    fn a_mouse_read_buffered_too_moves_the_cursor_from_there_alone() {
        let now = Instant::now();
        // Never read buffered (a game reading it immediately only): its immediate reads move the cursor.
        assert!(immediate_moves(None, now));
        // Read buffered a frame ago (ETS2, while TruckersMP reads it immediately): left out.
        assert!(!immediate_moves(Some(now - Duration::from_millis(16)), now));
        // Not read buffered for a while (the game stopped reading it so): taken again.
        assert!(immediate_moves(Some(now - BUFFERED_RECENT), now));
    }

    #[test]
    fn orders_buttons_by_their_number_wherever_numbering_starts() {
        // ETS2's mouse: axes at 0/4/8 numbered 0-2, buttons in a joystick-style format numbered from 3.
        let button = |offset, number: u32| (offset, GUID_Button, (number << 8) | 0x04);
        let objects = [
            (0, GUID_XAxis, 0x001),
            (4, GUID_YAxis, 0x101),
            (8, GUID_ZAxis, 0x201),
            button(49, 4),
            button(48, 3),
            button(50, 5),
            button(51, 6),
        ];
        let layout = MouseLayout::from_objects(&objects);
        assert_eq!((layout.x, layout.y, layout.wheel), (Some(0), Some(4), Some(8)));
        assert_eq!(layout.buttons, [Some(48), Some(49), Some(50)]);
    }

    /// Buffered events as DirectInput lays them out: offset, value, then (in the full struct) the time, a
    /// sequence number and app data.
    fn events(list: &[(u32, u32)], size: usize) -> Vec<u8> {
        let mut bytes = vec![0xee; list.len() * size];
        for (index, (offset, value)) in list.iter().enumerate() {
            bytes[index * size..index * size + 4].copy_from_slice(&offset.to_le_bytes());
            bytes[index * size + 4..index * size + 8].copy_from_slice(&value.to_le_bytes());
            // Tag the rest so a moved event can be checked whole.
            for byte in &mut bytes[index * size + 8..(index + 1) * size] {
                *byte = index as u8;
            }
        }
        bytes
    }

    #[test]
    fn keeps_the_events_wanted_packed_in_order_whole() {
        for size in [16, 20, 24] {
            let mut data = events(&[(0, 5), (12, 0x80), (4, 7), (12, 0)], size);
            let kept = retain_events(&mut data, size, |offset, _| offset == 12);
            assert_eq!(kept, 2);
            // The two button events, moved to the front with the rest of their struct.
            assert_eq!(&data[0..8], &[12, 0, 0, 0, 0x80, 0, 0, 0]);
            assert!(data[8..size].iter().all(|&byte| byte == 1));
            assert_eq!(&data[size..size + 8], &[12, 0, 0, 0, 0, 0, 0, 0]);
            assert!(data[size + 8..2 * size].iter().all(|&byte| byte == 3));
        }
        let mut none = events(&[(0, 1)], 16);
        assert_eq!(retain_events(&mut none, 16, |_, _| false), 0);
        assert_eq!(retain_events(&mut [], 16, |_, _| true), 0);
    }

    #[test]
    fn a_button_held_before_using_the_screen_still_comes_up_for_the_game() {
        let left = Some(MouseObject::Button(0));
        let mut held = 0;
        // Driving: the game gets everything, and sees the left button go down.
        assert!(game_gets(Some(MouseObject::X), false, false, &mut held));
        assert!(game_gets(left, true, false, &mut held));
        // Then the interaction key: movement and new presses are the screen's...
        assert!(!game_gets(Some(MouseObject::X), false, true, &mut held));
        assert!(!game_gets(Some(MouseObject::Button(1)), true, true, &mut held));
        // ...but the left button's release still reaches the game, once.
        assert!(game_gets(left, false, true, &mut held));
        assert!(!game_gets(left, true, true, &mut held));
        assert!(!game_gets(left, false, true, &mut held));
        // The right button went down on the screen: its release is the screen's too.
        assert!(!game_gets(Some(MouseObject::Button(1)), false, true, &mut held));
        assert_eq!(held, 0);
    }

    #[test]
    fn recognises_buttons_by_type_when_the_format_names_no_guid() {
        let layout = MouseLayout::from_objects(&[(12, GUID::zeroed(), 0x0c)]);
        assert_eq!(layout.buttons, [Some(12), None, None]);
    }

    #[test]
    fn finds_objects_wherever_the_data_format_puts_them() {
        // A format with the buttons first and the axes after them, unlike DirectInput's own.
        let layout = MouseLayout { x: Some(8), y: Some(12), wheel: Some(16), buttons: [Some(0), Some(1), None] };
        assert_eq!(layout.object(8), Some(MouseObject::X));
        assert_eq!(layout.object(16), Some(MouseObject::Wheel));
        assert_eq!(layout.object(1), Some(MouseObject::Button(1)));
        assert_eq!(layout.object(2), None);
        assert_eq!(MouseLayout::default().object(0), None);
    }
}
