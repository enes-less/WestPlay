//! Bridge between the game plugin (`westplay.dll`, inside ETS2/ATS) and the WestPlay app.
//!
//! - **Frames** travel through a shared D3D11 texture created by the app and opened by the plugin by
//!   name ([`frame_texture_name`]). Its keyed mutex is used as a plain lock (both sides use
//!   [`FRAME_KEY`]): the app overwrites it with every new frame, the plugin copies it whenever
//!   `frame_seq` changed. Each side holds it only for one copy and acquires with a zero timeout, so
//!   neither the game nor the app ever waits; on contention the frame is simply picked up next time.
//! - **Everything else** lives in one shared-memory block, [`Bridge`], mapped by both processes. All
//!   fields are atomics or plain data guarded by a [`SeqLock`], so either side may start first.

use std::cell::UnsafeCell;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicI32, AtomicU32, AtomicU64, Ordering, fence};

use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::System::Memory::{
    CreateFileMappingW, FILE_MAP_ALL_ACCESS, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile, PAGE_READWRITE, UnmapViewOfFile,
};
use windows::core::HSTRING;

pub const BRIDGE_NAME: &str = "Local\\WestPlay.Bridge.v16";
pub const BRIDGE_MAGIC: u32 = u32::from_le_bytes(*b"WPB7");
pub const BRIDGE_VERSION: u32 = 16;

/// Size of the app's frames, and the base size of the screen textures in the mod (`westplay_setup::game_mod`
/// builds them from these).
pub const SCREEN_WIDTH: u32 = 1280;
pub const SCREEN_HEIGHT: u32 = 720;

/// One kind of in-cabin screen: the GPS UI scripts the mod patches for it. See `screen_layouts.txt`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenLayout {
    /// Stable identifier the UI picks its screen profile by, e.g. `scania-2025`.
    pub id: &'static str,
    /// The game's scripts, e.g. `ui/dashboard/scania_2025_gps.sii`: usually one, or the same screen's in
    /// km/h and in mph (the Iveco S-Way's), sharing the layout's texture.
    pub gps_scripts: &'static [&'static str],
}

impl ScreenLayout {
    /// Height of the screen texture of the layout at `index`: `SCREEN_HEIGHT` plus the index, so each
    /// layout's texture can be recognised by its size. The extra rows repeat the frame's bottom row.
    pub fn texture_height(index: usize) -> u32 {
        SCREEN_HEIGHT + index as u32
    }
}

/// All screen layouts, in `screen_layouts.txt` order (the order defines the texture sizes).
pub fn screen_layouts() -> &'static [ScreenLayout] {
    static LAYOUTS: LazyLock<Vec<ScreenLayout>> =
        LazyLock::new(|| parse_screen_layouts(include_str!("../screen_layouts.txt")));
    &LAYOUTS
}

/// Index (in [`screen_layouts`]) of the layout whose screen texture has this size.
pub fn screen_layout_for_texture(width: u32, height: u32) -> Option<usize> {
    let index = height.checked_sub(SCREEN_HEIGHT)? as usize;
    (width == SCREEN_WIDTH && index < screen_layouts().len()).then_some(index)
}

/// The trucks WestPlay shows in on a layout that trucks it doesn't support share (SDK model ids, without the
/// `vehicle.` prefix): most trucks' interiors name the stock GPS screen (`generic`, `ui/gps.sii`), and of
/// those only the Scania S and R are WestPlay's. The other layouts are their own trucks' scripts.
const SHARED_LAYOUT_TRUCKS: &[(&str, &[&str])] = &[("generic", &["scania.s_2016", "scania.r_2016"])];

/// Whether WestPlay shows on the layout at `index` (in [`screen_layouts`]) in the truck `truck_id` (the SDK's
/// id, with or without its `vehicle.` prefix): every other truck keeps the game's own screen.
pub fn screen_supported(index: usize, truck_id: &str) -> bool {
    let Some(layout) = screen_layouts().get(index) else { return false };
    let model = truck_id.strip_prefix("vehicle.").unwrap_or(truck_id);
    SHARED_LAYOUT_TRUCKS.iter().find(|(id, _)| *id == layout.id).is_none_or(|(_, trucks)| trucks.contains(&model))
}

fn parse_screen_layouts(text: &'static str) -> Vec<ScreenLayout> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let mut fields = line.split_whitespace();
            let id = fields.next().unwrap_or_default();
            let gps_scripts: Vec<&'static str> = fields.collect();
            assert!(!gps_scripts.is_empty(), "screen_layouts.txt: expected `id gps-script...`, got {line:?}");
            // Read once, for the program's life.
            ScreenLayout { id, gps_scripts: Box::leak(gps_scripts.into_boxed_slice()) }
        })
        .collect()
}

/// Keyed-mutex key of the shared frame texture (the same for both sides: the mutex is a plain lock).
pub const FRAME_KEY: u64 = 0;

/// Name of the shared frame texture. The generation changes whenever the app recreates the texture.
pub fn frame_texture_name(generation: u32) -> String {
    format!("Local\\WestPlay.Frame.v8.{generation}")
}

#[repr(C)]
pub struct Bridge {
    pub magic: AtomicU32,
    pub version: AtomicU32,
    pub plugin: PluginBlock,
    pub host: HostBlock,
    pub route: RouteBlock,
    pub typing: TypingBlock,
}

/// Written by the game plugin.
#[repr(C)]
pub struct PluginBlock {
    pub pid: AtomicU32,
    /// Incremented on every presented game frame.
    pub heartbeat: AtomicU64,
    /// LUID of the GPU the game renders on; the app must create its frame texture on the same adapter.
    pub adapter_luid_low: AtomicU32,
    pub adapter_luid_high: AtomicI32,
    /// Layout of the most recently drawn WestPlay screen: its index in [`screen_layouts`] plus one, or 0
    /// when none has been drawn yet.
    pub screen_layout: AtomicU32,
    /// Game frame (`heartbeat` value) in which a WestPlay screen was last drawn.
    pub screen_frame: AtomicU64,
    pub telemetry: SeqLock<Telemetry>,
    /// The truck the player is driving (empty strings when there is none).
    pub truck: SeqLock<TruckInfo>,
    /// The player's job (`active` false when there is none).
    pub job: SeqLock<JobInfo>,
    /// Mouse input for the WestPlay screen while the player holds the interaction key.
    pub pointer: PointerBlock,
}

/// Single-producer, single-consumer queue of `N` key codes in shared memory (`N` a power of two, so the
/// counters wrap cleanly). When full, new keys are dropped.
#[repr(C)]
pub struct KeyQueue<const N: usize = 16> {
    head: AtomicU32,
    tail: AtomicU32,
    keys: [AtomicU32; N],
}

impl<const N: usize> KeyQueue<N> {
    const LEN: u32 = {
        assert!(N.is_power_of_two() && N <= 1 << 16);
        N as u32
    };

    /// Must only be called from one writer at a time. Returns false if the queue was full.
    pub fn push(&self, key: u32) -> bool {
        let tail = self.tail.load(Ordering::Relaxed);
        if tail.wrapping_sub(self.head.load(Ordering::Acquire)) >= Self::LEN {
            return false;
        }
        self.keys[(tail % Self::LEN) as usize].store(key, Ordering::Relaxed);
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        true
    }

    /// Must only be called from one reader at a time.
    pub fn pop(&self) -> Option<u32> {
        let head = self.head.load(Ordering::Relaxed);
        if head == self.tail.load(Ordering::Acquire) {
            return None;
        }
        let key = self.keys[(head % Self::LEN) as usize].load(Ordering::Relaxed);
        self.head.store(head.wrapping_add(1), Ordering::Release);
        Some(key)
    }
}

/// Mouse input taken away from the game while the player holds the interaction key (written by the plugin).
#[repr(C)]
pub struct PointerBlock {
    /// 1 while the interaction key is held: the game sees no mouse or keyboard input, WestPlay gets the mouse.
    pub active: AtomicU32,
    /// Movement (DirectInput counts, x right, y down) and wheel since the app last took them: the plugin
    /// adds, the app swaps them with 0.
    pub dx: AtomicI32,
    pub dy: AtomicI32,
    pub wheel: AtomicI32,
    /// Held mouse buttons: bit 0 left, bit 1 right, bit 2 middle.
    pub buttons: AtomicU32,
}

impl PointerBlock {
    pub const LEFT: u32 = 1;
    pub const RIGHT: u32 = 2;
    pub const MIDDLE: u32 = 4;
}

/// Written by the app.
#[repr(C)]
pub struct HostBlock {
    pub pid: AtomicU32,
    /// Incremented regularly while the app runs.
    pub heartbeat: AtomicU64,
    /// Generation of the shared frame texture ([`frame_texture_name`]); 0 when there is none.
    pub frame_generation: AtomicU32,
    pub frame_width: AtomicU32,
    pub frame_height: AtomicU32,
    /// Incremented after each frame written to the shared texture.
    pub frame_seq: AtomicU64,
    /// 1 while WestPlay should cover the truck's screen; 0 shows the game's original screen.
    pub screen_enabled: AtomicU32,
    /// The key the player holds to use the screen (a Windows virtual-key code, from the app's settings);
    /// 0 until the app has set it, meaning [`DEFAULT_INTERACTION_KEY`].
    pub interaction_key: AtomicU32,
}

/// Typing on the player's own keyboard into the WestPlay text field selected: the app says which selection
/// (the page's), the plugin whether it types into it and what was typed. It types while the truck stands
/// still, from a selection made then; once the truck moves the keys are the game's again until the player
/// selects a field anew (W, the throttle, must never type a "w" at a red light).
#[repr(C)]
pub struct TypingBlock {
    /// The app: the field selection, a number that changes with every selection (0: no field selected).
    pub selection: AtomicU32,
    /// The plugin: a [`TypingState`] code.
    pub state: AtomicU32,
    /// The plugin: what was typed, oldest first: characters (Unicode scalar values) and [`TypedKey`] codes.
    pub keys: KeyQueue<64>,
}

/// Whether the player's keyboard types into the selected field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum TypingState {
    /// No field selected: the keys are the game's.
    Off = 0,
    /// The keys type into the field (none reach the game but the few that stay its own: F-keys, Print
    /// Screen, Windows).
    Typing = 1,
    /// A field is selected but the keys are the game's: the truck moved, or it was selected while moving.
    Paused = 2,
}

impl TypingState {
    pub fn from_code(code: u32) -> Self {
        match code {
            1 => Self::Typing,
            2 => Self::Paused,
            _ => Self::Off,
        }
    }
}

/// Keys typed that aren't characters, as queued (above any Unicode scalar value).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum TypedKey {
    Backspace = 0x8000_0001,
    /// Ctrl+Backspace: the word before.
    WordBackspace = 0x8000_0002,
    Enter = 0x8000_0003,
    Escape = 0x8000_0004,
    /// Ctrl+V: the app pastes what the PC's clipboard holds.
    Paste = 0x8000_0005,
}

impl TypedKey {
    pub fn from_code(code: u32) -> Option<Self> {
        [Self::Backspace, Self::WordBackspace, Self::Enter, Self::Escape, Self::Paste]
            .into_iter()
            .find(|key| *key as u32 == code)
    }
}

/// A change to the game's GPS route the app asks for (Maps' search: go, add a stop, end the route): the app
/// writes the request, the plugin the answer. The game's own route: its waypoints, on its map, planned by
/// its GPS, changed there as any other.
#[repr(C)]
pub struct RouteBlock {
    /// What to do (a [`RouteAction`] code) and at which map node (its uid, as in the map's files), both
    /// written before `request` changes.
    pub action: AtomicU32,
    pub node: AtomicU64,
    /// Incremented by the app for each new request.
    pub request: AtomicU32,
    /// The plugin: the `request` it last answered, and how (a [`RouteAnswer`] code), `answer` written first.
    pub answered: AtomicU32,
    pub answer: AtomicI32,
}

/// What the app asks of the game's route.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum RouteAction {
    /// A waypoint at the node, before a job's destination.
    AddStop = 1,
    /// The route to the node alone (as the game's map with a single waypoint); not while on a job, whose
    /// destination the game keeps.
    Go = 2,
    /// No more waypoints: to a job's destination still, or no route at all.
    End = 3,
}

impl RouteAction {
    pub fn from_code(code: u32) -> Option<Self> {
        [Self::AddStop, Self::Go, Self::End].into_iter().find(|action| *action as u32 == code)
    }
}

/// How the plugin answered a route request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum RouteAnswer {
    Done = 1,
    /// Already one of the route's waypoints.
    AlreadyThere = 2,
    /// On a job, whose destination the game keeps: only stops can be added.
    OnJob = -5,
    /// The route has as many waypoints as the game allows.
    Full = -3,
    /// The game has no node with that uid.
    NoSuchNode = -2,
    /// The game's code isn't the version WestPlay knows: nothing was done.
    Unsupported = -1,
    /// The game's route wasn't as expected, or the request made no sense: nothing was done.
    Unexpected = -4,
}

impl RouteAnswer {
    pub fn from_code(code: i32) -> Option<Self> {
        [Self::Done, Self::AlreadyThere, Self::OnJob, Self::Full, Self::NoSuchNode, Self::Unsupported, Self::Unexpected]
            .into_iter()
            .find(|answer| *answer as i32 == code)
    }
}

/// Left Alt (`VK_LMENU`): the interaction key until the app says otherwise. The game binds no Alt by default.
pub const DEFAULT_INTERACTION_KEY: u32 = 0xa4;

/// Snapshot of the game state the UI needs. Plain data, published once per game frame.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct Telemetry {
    /// World position (metres) and orientation (heading/pitch/roll in turns, 0..1).
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub heading: f32,
    pub pitch: f32,
    pub roll: f32,
    /// Metres per second; negative when reversing.
    pub speed: f32,
    pub engine_rpm: f32,
    pub gear: i32,
    pub displayed_gear: i32,
    pub cruise_control_speed: f32,
    /// Litres.
    pub fuel: f32,
    /// Kilometres.
    pub fuel_range: f32,
    /// Navigation: metres and seconds to the route target, speed limit in m/s (0 = none).
    pub navigation_distance: f32,
    pub navigation_time: f32,
    pub navigation_speed_limit: f32,
    /// Minutes since the start of the game world (day 0, 00:00).
    pub game_time: u32,
    /// Time/distance scale of the current area (see the SDK readme).
    pub local_scale: f32,
    /// 0..1, follows the dashboard backlight setting.
    pub dashboard_backlight: f32,
    pub electric_enabled: bool,
    pub engine_enabled: bool,
    pub parking_brake: bool,
    pub low_beam: bool,
    pub high_beam: bool,
    pub left_blinker: bool,
    pub right_blinker: bool,
    pub paused: bool,
    /// The driver's head in the cab, in the game's cab space (metres: x right, y up, z back; the interior
    /// model's own space): where it is by default plus where the player moved it, and which way it faces
    /// (turns, as `heading` and `pitch` above: heading to the left, pitch up).
    pub head_x: f32,
    pub head_y: f32,
    pub head_z: f32,
    pub head_heading: f32,
    pub head_pitch: f32,
    /// Where the game's camera is in the world (metres, as `x`, `y`, `z`), when the plugin knows (it reads
    /// it from the game: plugin/src/camera.rs).
    pub camera_known: bool,
    pub camera_x: f32,
    pub camera_y: f32,
    pub camera_z: f32,
    /// Where the cab turns in the truck (metres, the truck's own space from `x`/`y`/`z`: x right, y up, z
    /// back): the cab's space starts there (the SDK's `cabin.position`), so the head in the truck is this plus
    /// `head_*`.
    pub cabin_x: f32,
    pub cabin_y: f32,
    pub cabin_z: f32,
    /// The steering as the truck has it (-1 to 1, counterclockwise: left positive): the game turns the camera
    /// with it (its "steering-follow camera rotation").
    pub steering: f32,
}

/// Identity of the current truck, from the SDK's "truck" configuration.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct TruckInfo {
    /// Brand for configuration purposes, e.g. `daf` (C-identifier characters).
    pub brand_id: FixedStr<32>,
    /// Brand for display, localized by the game, e.g. `DAF`.
    pub brand: FixedStr<64>,
    /// Truck model id, e.g. `vehicle.daf.2021` (C-identifier characters and dots).
    pub id: FixedStr<64>,
    /// Model name for display, localized by the game, e.g. `XG+`.
    pub name: FixedStr<64>,
}

/// The player's current job, from the SDK's "job" configuration. Names come localized by the game.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct JobInfo {
    /// There is a job (the game sends no attributes when there isn't).
    pub active: bool,
    /// The cargo is on: on the way to the destination (otherwise to the source, to pick it up).
    pub cargo_loaded: bool,
    /// A special transport (no companies, only cities).
    pub special: bool,
    /// The cargo, for display, e.g. "Tomatoes".
    pub cargo: FixedStr<64>,
    /// kg.
    pub cargo_mass: f32,
    pub source_city: FixedStr<64>,
    pub source_company: FixedStr<64>,
    pub destination_city: FixedStr<64>,
    pub destination_company: FixedStr<64>,
    /// When the delivery window ends: minutes since day 0, 00:00 (as the telemetry's game time).
    pub delivery_time: u32,
    /// The job's planned distance, simulated km.
    pub planned_distance_km: u32,
    /// The expected income, without penalties, in the game's currency.
    pub income: u64,
}

/// A UTF-8 string stored inline in shared memory: NUL-padded, truncated at a character boundary.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct FixedStr<const N: usize>([u8; N]);

impl<const N: usize> FixedStr<N> {
    pub fn new(value: &str) -> Self {
        let mut end = value.len().min(N);
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        let mut bytes = [0u8; N];
        bytes[..end].copy_from_slice(&value.as_bytes()[..end]);
        Self(bytes)
    }

    pub fn as_str(&self) -> &str {
        let len = self.0.iter().position(|&b| b == 0).unwrap_or(N);
        // Bytes always come from a &str cut at a character boundary, but shared memory is written by
        // another process; fall back to the longest valid prefix rather than trusting it.
        match std::str::from_utf8(&self.0[..len]) {
            Ok(text) => text,
            Err(error) => std::str::from_utf8(&self.0[..error.valid_up_to()]).unwrap_or_default(),
        }
    }
}

impl<const N: usize> Default for FixedStr<N> {
    fn default() -> Self {
        Self([0; N])
    }
}

impl<const N: usize> std::fmt::Debug for FixedStr<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self.as_str(), f)
    }
}

#[cfg(feature = "serde")]
impl<const N: usize> serde::Serialize for FixedStr<N> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// Single-writer sequence lock for plain data shared across processes. The writer's process may die in the
/// middle of a write (the game crashing), leaving the sequence odd: readers then give up rather than wait
/// for ever ([`SeqLock::read`]), and the next writer starts from an even sequence again.
#[repr(C)]
pub struct SeqLock<T: Plain> {
    sequence: AtomicU32,
    value: UnsafeCell<T>,
}

/// Plain data a [`SeqLock`] may hold: a read that overlapped a write (thrown away after) still makes a valid
/// value, each of its bytes taken whole from one of two valid values. Integers, floats, `bool`s (a byte each)
/// and byte strings, in `#[repr(C)]` structs, qualify; enums, references and pointers don't.
///
/// # Safety
/// Any mix of the bytes of two valid values, byte by byte, must be a valid value.
pub unsafe trait Plain: Copy {}

// SAFETY: integers, floats and `bool`s only (see `Plain`).
unsafe impl Plain for Telemetry {}
// SAFETY: byte strings only (see `Plain`).
unsafe impl Plain for TruckInfo {}
// SAFETY: byte strings, integers, floats and `bool`s only (see `Plain`).
unsafe impl Plain for JobInfo {}

// SAFETY: the value is only ever written by one writer at a time (`write`'s contract) and read whole, a
// read that overlapped a write being thrown away (`read`).
unsafe impl<T: Plain> Sync for SeqLock<T> {}

/// How many times [`SeqLock::read`] tries before deciding a write will never finish.
pub const SEQLOCK_READ_TRIES: u32 = 100_000;

impl<T: Plain> SeqLock<T> {
    /// Must only be called from one writer at a time.
    pub fn write(&self, value: &T) {
        // Even, whatever a writer that died mid-write left (odd), so odd means "being written" again.
        let sequence = self.sequence.load(Ordering::Relaxed) & !1;
        self.sequence.store(sequence.wrapping_add(1), Ordering::Relaxed);
        fence(Ordering::Release);
        // SAFETY: the cell is ours to write: one writer at a time, readers only copying it out.
        unsafe { std::ptr::write_volatile(self.value.get(), *value) };
        fence(Ordering::Release);
        self.sequence.store(sequence.wrapping_add(2), Ordering::Release);
    }

    /// A consistent snapshot, retrying while a write is in progress; `None` if one never finishes (its
    /// writer died in the middle of it), after [`SEQLOCK_READ_TRIES`] tries (a write takes well under a
    /// microsecond).
    pub fn read(&self) -> Option<T> {
        for _ in 0..SEQLOCK_READ_TRIES {
            let before = self.sequence.load(Ordering::Acquire);
            if before % 2 == 1 {
                std::hint::spin_loop();
                continue;
            }
            // SAFETY: a copy of plain data (`Plain`): valid even if a write overlapped it, and then thrown away.
            let value = unsafe { std::ptr::read_volatile(self.value.get()) };
            fence(Ordering::Acquire);
            if self.sequence.load(Ordering::Relaxed) == before {
                return Some(value);
            }
        }
        None
    }
}

/// A mapping of the [`Bridge`] shared memory. Whichever process maps it first creates it (zeroed).
pub struct BridgeMapping {
    handle: HANDLE,
    view: MEMORY_MAPPED_VIEW_ADDRESS,
}

// SAFETY: the mapping's handle and view may be used from any thread; what's in it is shared through atomics
// and `SeqLock`s.
unsafe impl Send for BridgeMapping {}
// SAFETY: as for `Send`: the bridge's fields are atomics and `SeqLock`s, made to be shared.
unsafe impl Sync for BridgeMapping {}

impl BridgeMapping {
    pub fn open() -> windows::core::Result<Self> {
        let size = size_of::<Bridge>();
        // SAFETY: the mapping is the bridge's size, so the view holds one `Bridge`, zeroed when new (a valid
        // `Bridge`: atomics and plain data); it's closed if it can't be viewed.
        unsafe {
            let handle = CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                None,
                PAGE_READWRITE,
                0,
                size as u32,
                &HSTRING::from(BRIDGE_NAME),
            )?;
            let view = MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, size);
            if view.Value.is_null() {
                let error = windows::core::Error::from_thread();
                let _ = CloseHandle(handle);
                return Err(error);
            }
            let mapping = Self { handle, view };
            let bridge = mapping.bridge();
            if bridge.magic.load(Ordering::Acquire) != BRIDGE_MAGIC {
                bridge.version.store(BRIDGE_VERSION, Ordering::Relaxed);
                bridge.magic.store(BRIDGE_MAGIC, Ordering::Release);
            }
            Ok(mapping)
        }
    }

    pub fn bridge(&self) -> &Bridge {
        // SAFETY: the view holds a `Bridge` (see `open`), mapped as long as `self`, which the borrow can't
        // outlive; it's shared only through atomics and `SeqLock`s.
        unsafe { &*(self.view.Value as *const Bridge) }
    }
}

impl Drop for BridgeMapping {
    fn drop(&mut self) {
        // SAFETY: the view and the handle are this mapping's own, unmapped and closed once, as it goes.
        unsafe {
            let _ = UnmapViewOfFile(self.view);
            let _ = CloseHandle(self.handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // SAFETY: integers only (see `Plain`).
    unsafe impl Plain for [u64; 8] {}
    // SAFETY: an integer (see `Plain`).
    unsafe impl Plain for u32 {}

    #[test]
    fn seqlock_round_trip() {
        let lock = SeqLock { sequence: AtomicU32::new(0), value: UnsafeCell::new(Telemetry::default()) };
        let value = Telemetry { speed: 22.5, game_time: 600, electric_enabled: true, ..Default::default() };
        lock.write(&value);
        assert_eq!(lock.read(), Some(value));
    }

    #[test]
    fn readers_never_see_a_write_half_done() {
        // A value whose halves always match when written whole.
        let lock = std::sync::Arc::new(SeqLock { sequence: AtomicU32::new(0), value: UnsafeCell::new([0u64; 8]) });
        let writer = {
            let lock = lock.clone();
            std::thread::spawn(move || {
                for i in 1..=200_000u64 {
                    lock.write(&[i; 8]);
                }
            })
        };
        let mut read = 0;
        while !writer.is_finished() {
            if let Some(value) = lock.read() {
                assert!(value.iter().all(|&part| part == value[0]), "torn: {value:?}");
                read += 1;
            }
        }
        writer.join().unwrap();
        assert!(read > 0);
        assert_eq!(lock.read(), Some([200_000; 8]));
    }

    #[test]
    fn a_writer_dying_mid_write_neither_hangs_readers_nor_breaks_the_next_writer() {
        let lock = SeqLock { sequence: AtomicU32::new(0), value: UnsafeCell::new(7u32) };
        lock.write(&1);
        // The game crashed between the two sequence stores: odd, and never finished.
        lock.sequence.fetch_add(1, Ordering::Relaxed);
        let started = std::time::Instant::now();
        assert_eq!(lock.read(), None);
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        // The next session writes: readers get its value, and stay right for the writes after.
        lock.write(&2);
        assert_eq!(lock.read(), Some(2));
        lock.write(&3);
        assert_eq!(lock.read(), Some(3));
        assert_eq!(lock.sequence.load(Ordering::Relaxed) % 2, 0);
    }

    #[test]
    fn fixed_str_round_trip() {
        let text = FixedStr::<16>::new("XG+");
        assert_eq!(text.as_str(), "XG+");
        assert_eq!(FixedStr::<16>::default().as_str(), "");
    }

    #[test]
    fn fixed_str_truncates_at_char_boundary() {
        // "ö" is two bytes; a 4-byte buffer cannot hold "Kröl" (5 bytes) and must not split the "ö".
        assert_eq!(FixedStr::<4>::new("Kröl").as_str(), "Krö");
        assert_eq!(FixedStr::<3>::new("Kröl").as_str(), "Kr");
    }

    #[test]
    fn fixed_str_tolerates_invalid_bytes() {
        let mut bytes = [0u8; 8];
        bytes[..4].copy_from_slice(&[b'D', b'A', 0xC3, b'F']);
        assert_eq!(FixedStr::<8>(bytes).as_str(), "DA");
    }

    #[test]
    fn truck_info_through_seqlock() {
        let lock = SeqLock { sequence: AtomicU32::new(0), value: UnsafeCell::new(TruckInfo::default()) };
        let truck = TruckInfo {
            brand_id: FixedStr::new("daf"),
            brand: FixedStr::new("DAF"),
            id: FixedStr::new("vehicle.daf.2021"),
            name: FixedStr::new("XG+"),
        };
        lock.write(&truck);
        assert_eq!(lock.read(), Some(truck));
    }

    #[test]
    fn screen_layouts_are_well_formed() {
        let layouts = screen_layouts();
        assert_eq!(layouts[0].id, "generic", "the generic layout must keep the exact frame size");
        for (index, layout) in layouts.iter().enumerate() {
            assert!(layout.gps_scripts.iter().all(|script| script.ends_with(".sii")), "{layout:?}");
            assert!(layouts[..index].iter().all(|other| other.id != layout.id), "duplicate id {}", layout.id);
        }
    }

    #[test]
    fn screen_layouts_are_recognised_by_texture_size() {
        for index in 0..screen_layouts().len() {
            assert_eq!(screen_layout_for_texture(SCREEN_WIDTH, ScreenLayout::texture_height(index)), Some(index));
        }
        let past_last = ScreenLayout::texture_height(screen_layouts().len());
        assert_eq!(screen_layout_for_texture(SCREEN_WIDTH, past_last), None);
        assert_eq!(screen_layout_for_texture(SCREEN_WIDTH, SCREEN_HEIGHT - 1), None);
        assert_eq!(screen_layout_for_texture(SCREEN_WIDTH + 1, SCREEN_HEIGHT), None);
    }

    #[test]
    fn the_stock_screen_is_westplays_only_in_the_scania_s_and_r() {
        let layout = |id: &str| screen_layouts().iter().position(|layout| layout.id == id).unwrap();
        let generic = layout("generic");
        assert!(screen_supported(generic, "vehicle.scania.s_2016"));
        assert!(screen_supported(generic, "scania.r_2016"));
        // The Actros MP4 (and the game's other trucks) name the same stock screen.
        assert!(!screen_supported(generic, "vehicle.mercedes.actros2014"));
        assert!(!screen_supported(generic, "vehicle.volvo.fh16_2012"));
        assert!(!screen_supported(generic, ""));
        // A truck's own layout is its own.
        assert!(screen_supported(layout("daf-2021"), "vehicle.daf.2021"));
        assert!(screen_supported(layout("iveco-sway"), "vehicle.iveco.sway"));
        assert!(!screen_supported(screen_layouts().len(), "vehicle.scania.s_2016"));
    }

    #[test]
    fn key_queue_keeps_order_and_drops_when_full() {
        // SAFETY: a key queue is atomics only: all zeros is a valid, empty queue.
        let queue: KeyQueue = unsafe { std::mem::zeroed() };
        assert_eq!(queue.pop(), None);
        for key in 0..16 {
            assert!(queue.push(key));
        }
        assert!(!queue.push(99), "full");
        assert_eq!(queue.pop(), Some(0));
        assert!(queue.push(100));
        let rest: Vec<u32> = std::iter::from_fn(|| queue.pop()).collect();
        assert_eq!(rest.first(), Some(&1));
        assert_eq!(rest.last(), Some(&100));
        assert_eq!(rest.len(), 16);
        // A longer one holds as many.
        // SAFETY: a key queue is atomics only: all zeros is a valid, empty queue.
        let typed: KeyQueue<64> = unsafe { std::mem::zeroed() };
        assert_eq!((0..100).filter(|&key| typed.push(key)).count(), 64);
    }

    #[test]
    fn key_queue_survives_counter_wraparound() {
        // SAFETY: a key queue is atomics only: all zeros is a valid, empty queue.
        let queue: KeyQueue = unsafe { std::mem::zeroed() };
        queue.head.store(u32::MAX - 1, Ordering::Relaxed);
        queue.tail.store(u32::MAX - 1, Ordering::Relaxed);
        for key in 1..=4 {
            assert!(queue.push(key));
        }
        assert_eq!(std::iter::from_fn(|| queue.pop()).collect::<Vec<_>>(), [1, 2, 3, 4]);
    }

    #[test]
    fn two_mappings_share_memory() {
        let a = BridgeMapping::open().unwrap();
        let b = BridgeMapping::open().unwrap();
        a.bridge().host.frame_seq.store(42, Ordering::Release);
        assert_eq!(b.bridge().host.frame_seq.load(Ordering::Acquire), 42);
        assert_eq!(b.bridge().magic.load(Ordering::Acquire), BRIDGE_MAGIC);
    }
}
