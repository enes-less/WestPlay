//! Bridge between the game plugin (`westplay.dll`, inside ETS2/ATS) and the WestPlay app.
//!
//! - **Frames** travel through a shared D3D11 texture created by the app and opened by the plugin by
//!   name ([`frame_texture_name`]). Its keyed mutex is used as a plain lock (both sides use
//!   [`FRAME_KEY`]): the app overwrites it with every new frame, the plugin copies it whenever
//!   `frame_seq` changed. Each side holds it only for one copy and acquires with a zero timeout, so
//!   neither the game nor the app ever waits; on contention the frame is simply picked up next time.
//! - **Everything else** lives in one shared-memory block, [`Bridge`], mapped by both processes. All
//!   fields are atomics or plain data guarded by a [`SeqLock`], so either side may start first.

pub mod cityhash;

use std::cell::UnsafeCell;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicI32, AtomicU32, AtomicU64, Ordering, fence};

use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::System::Memory::{
    CreateFileMappingW, FILE_MAP_ALL_ACCESS, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile, PAGE_READWRITE, UnmapViewOfFile,
};
use windows::core::HSTRING;

/// The bridge's name: its version in it, so an app and a plugin of different versions never share one (a
/// bridge is laid out as whichever opened it first; one of another version, still held by a game or an app,
/// would keep its layout until both are gone). [`BridgeMapping::matches`] still guards it.
pub fn bridge_name() -> String {
    format!("Local\\WestPlay.Bridge.v{BRIDGE_VERSION}")
}
pub const BRIDGE_MAGIC: u32 = u32::from_le_bytes(*b"WPB7");
pub const BRIDGE_VERSION: u32 = 24;

/// Size of the app's frames, and the base size of the screen textures in the mod (`westplay_setup::game_mod`
/// builds them from these).
pub const SCREEN_WIDTH: u32 = 1280;
pub const SCREEN_HEIGHT: u32 = 720;

/// One kind of in-cabin screen: the GPS UI scripts the mod patches for it (none for WestPlay's tablet, whose
/// model shows the texture itself). See `screen_layouts.txt`.
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

/// The height WestPlay's page is laid out at on the layout at `index` (in [`screen_layouts`]), its width
/// [`SCREEN_WIDTH`]: the frame's own, but where the game shows the frame in another shape. The stock screen
/// (`generic`, WestPlay's only in the Scania S and R's 7" display) shows it on a display of 1.615:1 (its model's
/// screen piece: PLAN.md section C), stretching 16:9 to that: laid out at 1280 x 793, of that shape, and
/// squeezed into the frame by the app, the page shows as laid out (owner 2026-10-21). WestPlay's tablet, an iPad
/// Air 5 (owner 2026-10-27), shows the frame's 720 rows whole on its glass, 23.16 x 16.25 cm (1.425:1, its
/// model's: crates/setup tablet.rs): laid out at 1280 x 898. Oyuncuyusbis' coaches' displays, each the shape of its model's screen (its corners,
/// 2026-10-24): the Travego 16 SHD's 16.2 x 9.6 cm (1.691:1, both its bands), the Setra S 531 DT's 15.2 x 9.0 cm
/// (1.682:1), the MAN Lion's Coach's 15.3 x 9.3 cm (1.646:1); the Setra S 516 HD's tablet's, its own screen's no
/// more, 20.1 x 12.2 cm (1.648:1, 2026-10-26).
pub fn page_height(index: usize) -> u32 {
    match screen_layouts().get(index).map(|layout| layout.id) {
        Some("generic") => 793,
        Some("tablet") => 898,
        Some("travego-16" | "travego-16-23") => 757,
        Some("setra-516") => 777,
        Some("setra-531") => 761,
        Some("man-lions-coach") => 778,
        _ => SCREEN_HEIGHT,
    }
}

/// WestPlay's tablet's layout: a dashboard toy any truck can have (WestPlay shows on it in every truck), its
/// model showing the layout's texture itself.
pub const TABLET: &str = "tablet";

/// The tablet's layout's index in [`screen_layouts`] (layouts are added after it: not the last).
pub fn tablet_layout() -> usize {
    screen_layouts().iter().position(|layout| layout.id == TABLET).expect("screen_layouts.txt has the tablet")
}

/// The file in WestPlay's own folder, beside `westplay.scs`, of the vehicles WestPlay's tablet shows nothing in (owner
/// 2026-10-26: the tablet a hookup any vehicle's dashboard set takes, so blank where it mustn't show): their ids'
/// hashes ([`cityhash::city_hash64`]), each in hex on a line of its own. Written by setup with the mod, read by the
/// plugin as the truck changes.
pub const TABLET_APART_FILE: &str = "tablet_apart.txt";

/// The vehicle `id` (the SDK's, with or without `vehicle.`) among `marks`: its id's hash is, or one of its parts'
/// (each between dots).
pub fn vehicle_marked(id: &str, marks: &[u64]) -> bool {
    let id = id.strip_prefix("vehicle.").unwrap_or(id);
    std::iter::once(id).chain(id.split('.')).any(|part| marks.contains(&cityhash::city_hash64(part.as_bytes())))
}

/// [`TABLET_APART_FILE`]'s text for `marks`.
pub fn write_marks(marks: &[u64]) -> String {
    marks
        .iter()
        .map(|mark| {
            format!(
                "{mark:016x}
"
            )
        })
        .collect()
}

/// The marks in [`TABLET_APART_FILE`]'s `text`; a line that isn't one is passed over.
pub fn read_marks(text: &str) -> Vec<u64> {
    text.lines().filter_map(|line| u64::from_str_radix(line.trim(), 16).ok()).collect()
}

/// Index (in [`screen_layouts`]) of the layout whose screen texture has this size.
pub fn screen_layout_for_texture(width: u32, height: u32) -> Option<usize> {
    let index = height.checked_sub(SCREEN_HEIGHT)? as usize;
    (width == SCREEN_WIDTH && index < screen_layouts().len()).then_some(index)
}

/// The cab's mark: a fully transparent texture the mod adds to the dashboard scripts of every truck the tablet goes
/// in. The game draws a dashboard's script every frame the camera is in the cab, wherever the player looks, and
/// never from outside, as it draws the GPS screens' (whose WestPlay screen tells the camera in the cab): the tablet
/// is drawn with the cab's models, left out as the player looks away from it (owner 2026-10-25: the music muffled
/// as the head turned in the interior). Its size, no screen layout's texture's.
pub const CAB_MARK_WIDTH: u32 = SCREEN_WIDTH;
pub const CAB_MARK_HEIGHT: u32 = 1;

/// [`PluginBlock::truck_supported`]: a screen of WestPlay's drawn in the player's truck.
pub const TRUCK_SUPPORTED: u32 = 1;
/// [`PluginBlock::truck_supported`]: the truck's screen isn't WestPlay's (one other trucks share, WestPlay's only in
/// some), or its cab is drawn with the electrics on and no screen of WestPlay's layouts at all (a mod truck's own).
pub const TRUCK_UNSUPPORTED: u32 = 2;

/// The trucks WestPlay shows in on a layout that trucks it doesn't support share (SDK model ids, without the
/// `vehicle.` prefix): most trucks' interiors name the stock GPS screen (`generic`, `ui/gps.sii`), and of
/// those only the Scania S and R's and the S BEV's (the S's 7" display) are WestPlay's. The other layouts are
/// their own trucks' scripts.
const SHARED_LAYOUT_TRUCKS: &[(&str, &[&str])] = &[
    ("generic", &["scania.s_2016", "scania.r_2016", "scania.s_2024e"]),
    // The game's toy GPS (Cabin Accessories), WestPlay's only on Oyuncuyusbis' Setra S 516 HD/2's tablet part.
    ("setra-516", &["setra516hd"]),
];

/// The trucks WestPlay shows in on the layout `id`, if it's one other trucks share ([`SHARED_LAYOUT_TRUCKS`]); none
/// otherwise (a layout of its own trucks' scripts).
pub fn layout_trucks(id: &str) -> &'static [&'static str] {
    SHARED_LAYOUT_TRUCKS.iter().find(|(layout, _)| *layout == id).map_or(&[], |(_, trucks)| *trucks)
}

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
            // Only the tablet is drawn by no script: any other line without one is a mistake.
            assert!(
                !gps_scripts.is_empty() || id == TABLET,
                "screen_layouts.txt: expected `id gps-script...`, got {line:?}"
            );
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
    /// Game frame in which the cab's mark ([`CAB_MARK_WIDTH`]) was last drawn: the camera in the cab then.
    pub cab_frame: AtomicU64,
    /// Whether the truck the player is in is one WestPlay shows in, as the plugin last saw ([`TRUCK_SUPPORTED`],
    /// [`TRUCK_UNSUPPORTED`]; 0: not known yet, since the truck changed).
    pub truck_supported: AtomicU32,
    pub telemetry: SeqLock<Telemetry>,
    /// The truck the player is driving (empty strings when there is none).
    pub truck: SeqLock<TruckInfo>,
    /// The player's job (`active` false when there is none).
    pub job: SeqLock<JobInfo>,
    /// Mouse input for the WestPlay screen while the player holds the interaction key.
    pub pointer: PointerBlock,
    /// Under TruckersMP: the players around the player, from TruckersMP's SDK (the plugin's `truckersmp`).
    pub nearby: SeqLock<Nearby>,
    /// Each [`Keybind`]'s presses so far: the plugin counts them, the app acts on each new one.
    pub keybind_presses: [AtomicU32; KEYBIND_COUNT],
}

/// The most of TruckersMP's players around the player the bridge carries (its streaming range holds far fewer).
pub const NEARBY_MAX: usize = 128;

/// A TruckersMP player around the player, as TruckersMP's SDK gives them: their number on the server (the
/// same as its map feed's `PlayerId`), where their vehicle is (world x and z, metres) and its heading (the
/// game's, in turns, 0..1).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NearbyPlayer {
    pub id: i32,
    pub heading: f32,
    pub x: f64,
    pub z: f64,
}

/// The players around the player under TruckersMP, written each few frames by the plugin: the game server's
/// id (TruckersMP's; 0: not connected), the player's own number on it (`-1`: not known), and the first
/// `count` of `players`. `frame` is the game frame it was written in (the plugin's heartbeat): one standing
/// still is from a game gone, or not under TruckersMP.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Nearby {
    pub frame: u64,
    pub server: u64,
    pub local: i32,
    pub count: u32,
    pub players: [NearbyPlayer; NEARBY_MAX],
}

impl Default for Nearby {
    fn default() -> Self {
        Self { frame: 0, server: 0, local: -1, count: 0, players: [NearbyPlayer::default(); NEARBY_MAX] }
    }
}

impl Nearby {
    /// The players it holds.
    pub fn players(&self) -> &[NearbyPlayer] {
        &self.players[..(self.count as usize).min(NEARBY_MAX)]
    }
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
    /// 1: the interaction key is pressed once to use the screen and again to stop (toggled), not held.
    pub interaction_toggle: AtomicU32,
    /// Each [`Keybind`]'s key ([`keybind_value`]): 0 until the app has set them, meaning none.
    pub keybinds: [AtomicU32; KEYBIND_COUNT],
}

/// What WestPlay does from the game by a key of the player's choosing (the app's settings).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Keybind {
    VolumeUp = 0,
    VolumeDown = 1,
    PlayPause = 2,
}

impl Keybind {
    pub const ALL: [Keybind; KEYBIND_COUNT] = [Keybind::VolumeUp, Keybind::VolumeDown, Keybind::PlayPause];
}

pub const KEYBIND_COUNT: usize = 3;

/// A keybind as the bridge carries it: its key's virtual-key code, and whether it's pressed while holding the
/// interaction key (using the screen) rather than alone.
pub fn keybind_value(key: u32, with_interaction_key: bool) -> u32 {
    (key & 0xffff) | (u32::from(with_interaction_key) << 16)
}

/// A keybind's key and whether it's with the interaction key, from [`keybind_value`] (`None`: none set).
pub fn keybind_of(value: u32) -> Option<(u32, bool)> {
    let key = value & 0xffff;
    (key != 0).then_some((key, value & (1 << 16) != 0))
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
    /// [`RouteAction::Follow`]'s waypoints (map node uids), in route order: the first `via_count`, written
    /// before `request` changes.
    pub vias: [AtomicU64; MAX_VIAS],
    pub via_count: AtomicU32,
}

/// The most waypoints a [`RouteAction::Follow`] holds a route with: the game takes ten, the destination one of
/// them.
pub const MAX_VIAS: usize = 9;

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
    /// The route to the node through the waypoints `vias` (in order), as WestPlay planned it: the route the
    /// player picked, held by them. Not while on a job.
    Follow = 4,
}

impl RouteAction {
    pub fn from_code(code: u32) -> Option<Self> {
        [Self::AddStop, Self::Go, Self::End, Self::Follow].into_iter().find(|action| *action as u32 == code)
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
    /// In a Convoy session, the session's time less the player's own (`game_time`), in-game minutes (the
    /// SDK's `multiplayer.time.offset`; 0 out of one): the cab's clock shows `game_time` plus this. A job's
    /// times are the player's own.
    pub multiplayer_time_offset: i32,
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
    /// Where the driver's head is by default (the SDK's `head.position`, cab space, metres): the interior
    /// model's origin, which the speakers of a truck's parts are placed from (the host's speakers.rs).
    #[cfg_attr(feature = "serde", serde(skip))]
    pub head: [f32; 3],
    /// Its licence plate as the game shows it (`license.plate`), to find it among the save's vehicles.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub plate: FixedStr<32>,
    /// The plate's country (`license.plate.country.id`, e.g. `italy`).
    #[cfg_attr(feature = "serde", serde(skip))]
    pub plate_country: FixedStr<32>,
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
// SAFETY: integers and floats only (see `Plain`); a count read torn is bounded by `Nearby::players`.
unsafe impl Plain for Nearby {}

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
                &HSTRING::from(bridge_name()),
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

    /// Whether the bridge is laid out as this build's ([`BRIDGE_VERSION`]): whichever side opened it first set
    /// its own version, and a plugin and an app of different versions read each other's blocks at the wrong
    /// places (a dev plugin with an older app took the driving keys for typing). Neither uses one that
    /// doesn't match.
    pub fn matches(&self) -> bool {
        self.bridge().version.load(Ordering::Acquire) == BRIDGE_VERSION
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
    #[test]
    fn marks_a_vehicle_by_its_ids_hash_or_a_parts() {
        let marks =
            read_marks(&write_marks(&[cityhash::city_hash64(b"truckx"), cityhash::city_hash64(b"brand.model")]));
        assert_eq!(marks.len(), 2);
        assert!(super::vehicle_marked("vehicle.truckx", &marks) && super::vehicle_marked("mod.truckx", &marks));
        assert!(super::vehicle_marked("brand.model", &marks) && !super::vehicle_marked("brand.other", &marks));
        assert!(
            read_marks(
                "not one

"
            )
            .is_empty()
        );
    }

    #[test]
    fn the_bridges_name_carries_its_version() {
        assert_eq!(super::bridge_name(), format!("Local\\WestPlay.Bridge.v{}", super::BRIDGE_VERSION));
    }

    use super::*;

    #[test]
    fn the_7_inch_screens_page_takes_its_displays_shape() {
        let index = |id: &str| screen_layouts().iter().position(|layout| layout.id == id).unwrap();
        // 1280 x 793: 1.614, the display's 1.615.
        assert_eq!(page_height(index("generic")), 793);
        // The coaches' displays, each its shape.
        for (layout, height) in [
            ("travego-16", 757),
            ("travego-16-23", 757),
            ("setra-516", 777),
            ("setra-531", 761),
            ("man-lions-coach", 778),
        ] {
            assert_eq!(page_height(index(layout)), height, "{layout}");
        }
        assert_eq!(page_height(index("daf-2021")), SCREEN_HEIGHT);
        assert_eq!(page_height(screen_layouts().len()), SCREEN_HEIGHT);
    }

    #[test]
    fn a_keybind_carries_its_key_and_whether_its_with_the_interaction_key() {
        assert_eq!(keybind_of(keybind_value(0x31, true)), Some((0x31, true)));
        assert_eq!(keybind_of(keybind_value(0x05, false)), Some((0x05, false)));
        // Nothing set yet: none.
        assert_eq!(keybind_of(0), None);
    }

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
            head: [-0.722, 1.696, 0.686],
            plate: FixedStr::new("CA 324KD"),
            plate_country: FixedStr::new("italy"),
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
        assert!(screen_supported(generic, "vehicle.scania.s_2024e"));
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
    fn the_tablet_is_westplays_in_every_truck() {
        let tablet = tablet_layout();
        // Where it was first put (its texture 1280 x 727, the size the plugin knows it by); layouts added since
        // come after it, so it and those before it keep their textures' sizes.
        assert_eq!(tablet, 7);
        assert_eq!(screen_layouts()[tablet].id, TABLET);
        assert!(screen_layouts()[tablet].gps_scripts.is_empty());
        for truck in ["vehicle.mercedes.actros2014", "vehicle.volvo.fh16_2012", "scania.s_2016", "vehicle.daf.2021"] {
            assert!(screen_supported(tablet, truck), "{truck}");
        }
        // Laid out in its glass's shape (the iPad's).
        assert_eq!(page_height(tablet), 898);
    }

    #[test]
    fn the_games_toy_gps_is_westplays_only_on_the_setra_516s_tablet() {
        let setra = screen_layouts().iter().position(|layout| layout.id == "setra-516").unwrap();
        assert_eq!(screen_layouts()[setra].gps_scripts, ["ui/dashboard/gps.dlc_toys.sii"]);
        assert!(screen_supported(setra, "vehicle.setra516hd"));
        // Any other truck with the toy GPS keeps the game's own.
        for truck in ["vehicle.daf.2021", "vehicle.scania.s_2016", "vehicle.setradt531"] {
            assert!(!screen_supported(setra, truck), "{truck}");
        }
        assert_eq!(layout_trucks("setra-516"), ["setra516hd"]);
        assert!(layout_trucks("daf-2021").is_empty());
        // Laid out at the tablet's 1.648:1.
        assert_eq!(page_height(setra), 777);
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
