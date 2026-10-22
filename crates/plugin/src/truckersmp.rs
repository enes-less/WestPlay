//! TruckersMP's SDK (docs.truckersmp.com/client-sdk, github.com/TruckersMP/GameClientSDK, C layer of SDK
//! 1.1.0): under TruckersMP, its client loads the plugin a second time as one of its own plugins
//! (`truckersmp_init`), and through it the plugin passes the players around the player to the app: their
//! number on the server, where their vehicle is and its heading, every frame (owner 2026-10-21: the map's
//! trucks around the player move as in the game; further ones come from TruckersMP's map feed, in the cab).
//! Only the SDK's public modules are used (players, vehicles, network: no intent), nothing of an account but a
//! player's number on the server.
//!
//! As the SDK asks: the players streamed in are taken once at the start (`GetAllPlayers`) and followed through
//! its stream-in and stream-out events; their vehicles are read in its pre-render event; every call is made on
//! the game's thread, from inside its callbacks, and each returns at once.

use std::collections::HashSet;
use std::ffi::{c_char, c_void};
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use westplay_shared::{NEARBY_MAX, Nearby, NearbyPlayer};

use crate::{bridge, hooks, log};

/// The SDK version the plugin is built against (`TRUCKERSMP_SDK_VERSION`, 1.1.0).
const SDK_VERSION: u32 = 0x0100_0100;
/// The SDK's function tables and events, by their ids (`TRUCKERSMP_API_*`, `TRUCKERSMP_EVT_*`).
const API_CORE: u64 = 0x611c_12f8_a745_65ec;
const API_PLAYERS: u64 = 0x91bf_40df_ad9c_41eb;
const API_PLAYER_HANDLE: u64 = 0x8ee2_7b0c_cfae_e795;
const API_VEHICLE: u64 = 0x246b_095f_1033_3d2f;
const API_NETWORK: u64 = 0x9c2c_7fb5_0b69_537d;
const EVT_PLAYER_STREAM_IN: u64 = 0x095c_1192_f5da_21cd;
const EVT_PLAYER_STREAM_OUT: u64 = 0x702c_3c15_a6df_f3ee;
const EVT_PRE_RENDER: u64 = 0x62b8_aff3_0797_6b31;
/// `TruckersMP_Result_Ok`, and `TruckersMP_LogLevel_Info`.
const RESULT_OK: i32 = 0;
const LOG_INFO: i32 = 0;

type Context = *mut c_void;
type EventFn = unsafe extern "C" fn(user: *mut c_void, event: u64, payload: *mut c_void, size: u32);

/// `TruckersMP_Host`: what the client hands the plugin at its start.
#[repr(C)]
pub struct Host {
    size: u32,
    ctx: Context,
    declare_version: Option<unsafe extern "C" fn(Context, u32) -> i32>,
    get_api: Option<unsafe extern "C" fn(Context, u64) -> *const c_void>,
    free: Option<unsafe extern "C" fn(Context, *mut c_void)>,
}

/// `TruckersMP_PluginDesc`: the plugin as the client lists it.
#[repr(C)]
pub struct PluginDesc {
    size: u32,
    name: [u8; 64],
    author: [u8; 64],
    version: [u8; 32],
    description: [u8; 512],
}

/// `TruckersMP_List`: `count` elements of `stride` bytes.
#[repr(C)]
struct List {
    items: *const c_void,
    count: u32,
    stride: u32,
}

/// `TruckersMP_String`: borrowed bytes, not nul-terminated.
#[repr(C)]
struct SdkString {
    ptr: *const c_char,
    size: u32,
}

/// `TruckersMP_Core`.
#[repr(C)]
struct Core {
    add_listener: unsafe extern "C" fn(Context, u64, EventFn, *mut c_void) -> i32,
    remove_listener: unsafe extern "C" fn(Context, u64, EventFn, *mut c_void) -> i32,
    _unlock_intent: usize,
    log_message: unsafe extern "C" fn(Context, i32, SdkString) -> i32,
    _game_id: usize,
    _client_version: usize,
}

/// `TruckersMP_Players`. A player handle is a `uint64_t` (0: none), passed by value.
#[repr(C)]
struct Players {
    get_local_player: unsafe extern "C" fn(Context, *mut u64) -> bool,
    _get_player_by_id: usize,
    get_all_players: unsafe extern "C" fn(Context, *mut List) -> bool,
}

/// `TruckersMP_PlayerHandle`: its getters in order, those the plugin uses typed.
#[repr(C)]
struct PlayerHandle {
    get_player_id: unsafe extern "C" fn(Context, u64, *mut i32) -> bool,
    /// From GetAccountID to GetNetworkLatency: accounts, names, tags, ranks, streaming, latency (not read).
    _between: [usize; 12],
    get_vehicle: unsafe extern "C" fn(Context, u64, *mut u64) -> bool,
}

/// `TruckersMP_Vehicle`.
#[repr(C)]
struct Vehicle {
    _get_trailer: usize,
    get_placement: unsafe extern "C" fn(Context, u64, *mut Placement) -> bool,
}

/// `TruckersMP_Network`.
#[repr(C)]
struct Network {
    _is_connected: usize,
    _get_last_ping: usize,
    get_game_server_id: unsafe extern "C" fn(Context, *mut u64) -> bool,
}

/// `TruckersMP_Placement`: a world position (metres) and rotation (`w`, `x`, `y`, `z`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct Placement {
    x: f64,
    y: f64,
    z: f64,
    w: f32,
    qx: f32,
    qy: f32,
    qz: f32,
}

/// The game's heading (turns, 0..1: 0 north, which is −z, turning west, −x) of a rotation: the angle about
/// the vertical turning the game's forward (−z) to where it faces.
fn heading_of(p: &Placement) -> f32 {
    let yaw = (2.0 * (p.w * p.qy + p.qx * p.qz)).atan2(1.0 - 2.0 * (p.qx * p.qx + p.qy * p.qy));
    (yaw / std::f32::consts::TAU).rem_euclid(1.0)
}

/// The SDK session: the client's context and tables (alive while the plugin is loaded as TruckersMP's), and
/// the players streamed in.
struct Session {
    ctx: Context,
    core: &'static Core,
    players: &'static Players,
    handle: &'static PlayerHandle,
    vehicle: &'static Vehicle,
    network: Option<&'static Network>,
    streamed: HashSet<u64>,
}

// SAFETY: the context and tables are the client's, used only on the game's thread (every SDK callback runs
// there); the mutex only hands them from the start to the callbacks.
unsafe impl Send for Session {}

static SESSION: Mutex<Option<Session>> = Mutex::new(None);

fn session() -> std::sync::MutexGuard<'static, Option<Session>> {
    SESSION.lock().unwrap_or_else(|e| e.into_inner())
}

/// Copies `text` into a fixed field, nul-terminated (cut to fit).
fn fill(field: &mut [u8], text: &str) {
    let bytes = &text.as_bytes()[..text.len().min(field.len() - 1)];
    field[..bytes.len()].copy_from_slice(bytes);
    field[bytes.len()] = 0;
}

/// The client starting the plugin as one of TruckersMP's. Returns whether it stays loaded; a panic declines.
///
/// # Safety
/// Called by TruckersMP's client on the game's thread, `host` and `desc` valid through the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn truckersmp_init(host: *const Host, desc: *mut PluginDesc) -> bool {
    // SAFETY: as the client calls this (the contract above).
    hooks::contained("truckersmp_init", false, || unsafe { init(host, desc) })
}

/// # Safety
/// As [`truckersmp_init`].
unsafe fn init(host: *const Host, desc: *mut PluginDesc) -> bool {
    log::init();
    // SAFETY: `host` and `desc` are the client's, valid through the call; each table it hands back is the
    // client's, laid out as the SDK's header declares, alive while the plugin is loaded; only its own
    // functions are called, with its context.
    unsafe {
        let Some(host) = host.as_ref() else { return false };
        let (Some(declare_version), Some(get_api)) = (host.declare_version, host.get_api) else { return false };
        if (host.size as usize) < size_of::<Host>() {
            log!("TruckersMP: its client hands a host older than SDK 1.1: not used");
            return false;
        }
        if let Some(desc) = desc.as_mut()
            && desc.size as usize >= size_of::<PluginDesc>()
        {
            fill(&mut desc.name, "WestPlay");
            fill(&mut desc.author, "WestPlay");
            fill(&mut desc.version, env!("CARGO_PKG_VERSION"));
            fill(&mut desc.description, "The truck's screen as CarPlay: shows the players around you on its map.");
        }
        let declared = declare_version(host.ctx, SDK_VERSION);
        if declared != RESULT_OK {
            log!("TruckersMP: its client refused SDK 1.1.0 ({declared}): not used");
            return false;
        }
        let table = |id: u64| get_api(host.ctx, id);
        let (core, players, handle, vehicle, network) = (
            (table(API_CORE) as *const Core).as_ref(),
            (table(API_PLAYERS) as *const Players).as_ref(),
            (table(API_PLAYER_HANDLE) as *const PlayerHandle).as_ref(),
            (table(API_VEHICLE) as *const Vehicle).as_ref(),
            (table(API_NETWORK) as *const Network).as_ref(),
        );
        let (Some(core), Some(players), Some(handle), Some(vehicle)) = (core, players, handle, vehicle) else {
            log!("TruckersMP: its client has no players or vehicles module: not used");
            return false;
        };
        let events = [EVT_PLAYER_STREAM_IN, EVT_PLAYER_STREAM_OUT, EVT_PRE_RENDER];
        for (index, &event) in events.iter().enumerate() {
            let added = (core.add_listener)(host.ctx, event, on_event, std::ptr::null_mut());
            if added != RESULT_OK {
                for &done in &events[..index] {
                    (core.remove_listener)(host.ctx, done, on_event, std::ptr::null_mut());
                }
                log!("TruckersMP: its client refused an event ({event:#x}: {added}): not used");
                return false;
            }
        }
        // The players streamed in already, once: the events follow the rest.
        let mut streamed = HashSet::new();
        let mut list = List { items: std::ptr::null(), count: 0, stride: 0 };
        if (players.get_all_players)(host.ctx, &mut list) && !list.items.is_null() {
            for index in 0..list.count as usize {
                streamed.insert(std::ptr::read_unaligned(
                    (list.items as *const u8).add(index * list.stride as usize) as *const u64
                ));
            }
            if let Some(free) = host.free {
                free(host.ctx, list.items as *mut c_void);
            }
        }
        streamed.remove(&0);
        let text = format!("WestPlay {}: the players around you go to its map", env!("CARGO_PKG_VERSION"));
        (core.log_message)(host.ctx, LOG_INFO, SdkString { ptr: text.as_ptr().cast(), size: text.len() as u32 });
        log!("TruckersMP: SDK session open, {} players streamed in", streamed.len());
        *session() = Some(Session { ctx: host.ctx, core, players, handle, vehicle, network, streamed });
        true
    }
}

/// The client unloading the plugin as one of TruckersMP's.
///
/// # Safety
/// Called by TruckersMP's client on the game's thread, after a `truckersmp_init` that returned true.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn truckersmp_shutdown() {
    hooks::contained("truckersmp_shutdown", (), || {
        if let Some(open) = session().take() {
            for event in [EVT_PLAYER_STREAM_IN, EVT_PLAYER_STREAM_OUT, EVT_PRE_RENDER] {
                // SAFETY: the client's own table and context, still alive during its shutdown call.
                unsafe { (open.core.remove_listener)(open.ctx, event, on_event, std::ptr::null_mut()) };
            }
        }
        // None around any more, as far as the app knows.
        if let Some(bridge) = bridge::get() {
            bridge.plugin.nearby.write(&Nearby::default());
        }
        log!("TruckersMP: SDK session closed");
    });
}

/// Every SDK event the plugin listens to, on the game's thread.
///
/// # Safety
/// Called by TruckersMP's client with the event's payload (`size` bytes) alive through the call.
unsafe extern "C" fn on_event(_user: *mut c_void, event: u64, payload: *mut c_void, size: u32) {
    hooks::contained("a TruckersMP event", (), || {
        let mut guard = session();
        let Some(open) = guard.as_mut() else { return };
        // A stream event's payload: the player's handle.
        let player = || {
            // SAFETY: the payload is the event's, alive through the call, at least a handle when `size` says so.
            (!payload.is_null() && size as usize >= size_of::<u64>())
                .then(|| unsafe { std::ptr::read_unaligned(payload as *const u64) })
                .filter(|&handle| handle != 0)
        };
        match event {
            EVT_PLAYER_STREAM_IN => {
                if let Some(handle) = player() {
                    open.streamed.insert(handle);
                }
            }
            EVT_PLAYER_STREAM_OUT => {
                if let Some(handle) = player() {
                    open.streamed.remove(&handle);
                }
            }
            // SAFETY: on the game's thread, inside the client's callback: where the SDK may be called.
            EVT_PRE_RENDER => unsafe { publish(open) },
            _ => {}
        }
    });
}

/// The players around the player, to the app: where each one's vehicle is now.
///
/// # Safety
/// On the game's thread, inside one of the client's callbacks.
unsafe fn publish(open: &Session) {
    let Some(bridge) = bridge::get() else { return };
    let ctx = open.ctx;
    // SAFETY: the client's tables and context, called as the SDK's header declares, each with its own handles.
    unsafe {
        let mut server = 0u64;
        if let Some(network) = open.network
            && !(network.get_game_server_id)(ctx, &mut server)
        {
            server = 0;
        }
        let mut local = -1i32;
        let mut me = 0u64;
        if (open.players.get_local_player)(ctx, &mut me) && me != 0 && !(open.handle.get_player_id)(ctx, me, &mut local)
        {
            local = -1;
        }
        let mut nearby =
            Nearby { frame: bridge.plugin.heartbeat.load(Ordering::Relaxed), server, local, ..Nearby::default() };
        for &player in &open.streamed {
            if nearby.count as usize == NEARBY_MAX {
                break;
            }
            let mut id = -1i32;
            let mut vehicle = 0u64;
            let mut placement = Placement::default();
            if !(open.handle.get_player_id)(ctx, player, &mut id)
                || id == local
                || !(open.handle.get_vehicle)(ctx, player, &mut vehicle)
                || vehicle == 0
                || !(open.vehicle.get_placement)(ctx, vehicle, &mut placement)
            {
                continue;
            }
            nearby.players[nearby.count as usize] =
                NearbyPlayer { id, heading: heading_of(&placement), x: placement.x, z: placement.z };
            nearby.count += 1;
        }
        bridge.plugin.nearby.write(&nearby);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lays_out_the_sdks_structures_as_its_header_asserts() {
        assert_eq!(size_of::<Host>(), 40);
        assert_eq!(size_of::<PluginDesc>(), 676);
        assert_eq!(size_of::<List>(), 16);
        assert_eq!(size_of::<SdkString>(), 16);
        assert_eq!(size_of::<Placement>(), 40);
        assert_eq!(std::mem::offset_of!(Placement, w), 24);
        // The player handle's table: GetVehicle is its fourteenth function.
        assert_eq!(std::mem::offset_of!(PlayerHandle, get_vehicle), 13 * 8);
        assert_eq!(std::mem::offset_of!(Core, log_message), 3 * 8);
        assert_eq!(std::mem::offset_of!(Players, get_all_players), 2 * 8);
        assert_eq!(std::mem::offset_of!(Vehicle, get_placement), 8);
        assert_eq!(std::mem::offset_of!(Network, get_game_server_id), 2 * 8);
    }

    #[test]
    fn turns_a_rotation_into_the_games_heading() {
        // A turn of `turns` about the vertical (y): w = cos(θ/2), y = sin(θ/2).
        let turned = |turns: f32| {
            let half = turns * std::f32::consts::PI;
            Placement { w: half.cos(), qy: half.sin(), ..Placement::default() }
        };
        for turns in [0.0, 0.25, 0.5, 0.75, 0.1] {
            assert!((heading_of(&turned(turns)) - turns).abs() < 1e-5, "{turns}");
        }
    }

    #[test]
    fn fills_a_descriptors_field_cut_and_terminated() {
        let mut field = [0xffu8; 8];
        fill(&mut field, "WestPlay plugin");
        assert_eq!(&field, b"WestPla\0");
    }
}
