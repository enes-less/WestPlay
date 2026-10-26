//! WestPlay SCS SDK plugin (`westplay.dll`).
//!
//! Loaded by ETS2/ATS from `bin/win_x64/plugins`. The game calls `scs_telemetry_init` /
//! `scs_telemetry_shutdown` on its main thread, possibly several times per process (`sdk reload`). Under
//! TruckersMP its client loads it too, as one of its own plugins (`truckersmp_init`: `truckersmp`).

mod bridge;
mod camera;
mod d3d;
mod frame;
mod hooks;
mod input;
mod keybinds;
mod keyboard;
mod log;
mod mod_mount;
mod navigation;
mod screens;
mod scs;
mod telemetry;
mod truckersmp;
mod typing;

use std::ffi::{CStr, CString};

use scs::{SCS_LOG_TYPE_MESSAGE, SCS_RESULT_OK, SCS_RESULT_UNSUPPORTED, TelemetryInitParamsV100};

/// Entry point called by the game when it activates the telemetry API. A panic on the way leaves the game as
/// it was (no hooks) and declines.
///
/// # Safety
/// Called by the game with a valid `params` pointer for the negotiated `version`, on its main thread.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn scs_telemetry_init(version: u32, params: *const TelemetryInitParamsV100) -> i32 {
    // SAFETY: as the game calls this (the contract above), so `init` is called.
    let result = hooks::contained("scs_telemetry_init", None, || Some(unsafe { init(version, params) }));
    result.unwrap_or_else(|| {
        // SAFETY: on the game's main thread, no detour running: the hooks were made here, and never switched
        // on if `init` didn't get that far.
        unsafe { hooks::remove_all() };
        SCS_RESULT_UNSUPPORTED
    })
}

/// # Safety
/// As [`scs_telemetry_init`].
unsafe fn init(version: u32, params: *const TelemetryInitParamsV100) -> i32 {
    // SAFETY: the game passes `params` for the negotiated `version` (checked first), on its main thread, its
    // strings and log callback alive through the call (the SDK's contract); the plugin's own setup (its hooks,
    // the camera's search) is meant for exactly this moment.
    unsafe {
        if version != scs::SCS_TELEMETRY_VERSION_1_01 && version != scs::SCS_TELEMETRY_VERSION_1_00 {
            return SCS_RESULT_UNSUPPORTED;
        }
        let Some(params) = params.as_ref() else {
            return SCS_RESULT_UNSUPPORTED;
        };

        log::init();
        let game_name = cstr(params.common.game_name);
        let game_id = cstr(params.common.game_id);
        let game_version = params.common.game_version;
        log!(
            "westplay {} loaded: {game_name} ({game_id}), telemetry game version {}.{}, api {}.{}",
            env!("CARGO_PKG_VERSION"),
            scs::major_version(game_version),
            scs::minor_version(game_version),
            scs::major_version(version),
            scs::minor_version(version),
        );

        telemetry::register(params);
        camera::locate();
        // WestPlay's screen mod, before the game mounts the profile's mods.
        mod_mount::mount(&game_id);

        // Screen interaction is optional: without it the screens still work, they just can't be used.
        if let Err(e) = input::install() {
            log!("input hook install failed: {e}");
        }
        if let Err(e) = keyboard::install() {
            log!("keyboard hook install failed: {e}");
        }
        let status = match d3d::install().and_then(|()| hooks::enable_all()) {
            Ok(()) => {
                log!("hooks enabled");
                "hooks installed".to_string()
            }
            Err(e) => {
                log!("hook install failed: {e}");
                format!("hook install failed: {e}")
            }
        };

        if let Some(game_log) = params.common.log
            && let Ok(message) = CString::new(format!("[westplay] {status}; log: {}", log::log_dir().display()))
        {
            game_log(SCS_LOG_TYPE_MESSAGE, message.as_ptr());
        }

        SCS_RESULT_OK
    }
}

/// Called by the game when it deactivates the telemetry API (also on `sdk unload`).
///
/// # Safety
/// Called by the game on its main thread, after a successful `scs_telemetry_init`.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn scs_telemetry_shutdown() {
    // SAFETY: as the game calls this (the contract above), so `shutdown` is called.
    hooks::contained("scs_telemetry_shutdown", (), || unsafe { shutdown() });
}

/// # Safety
/// As [`scs_telemetry_shutdown`].
unsafe fn shutdown() {
    // SAFETY: on the game's main thread, after a successful init; a detour still running on the render
    // thread is given time to return below, before the plugin can be unloaded.
    unsafe {
        hooks::remove_all();
        d3d::on_unload();
    }
    log!("hooks removed");
    if let Some(bridge) = bridge::get() {
        // Tell the app the game is gone.
        bridge.plugin.pid.store(0, std::sync::atomic::Ordering::Release);
        bridge.plugin.screen_frame.store(0, std::sync::atomic::Ordering::Release);
        bridge.plugin.cab_frame.store(0, std::sync::atomic::Ordering::Release);
        bridge.plugin.truck_supported.store(0, std::sync::atomic::Ordering::Release);
        bridge.plugin.screen_layout.store(0, std::sync::atomic::Ordering::Release);
        bridge.plugin.pointer.active.store(0, std::sync::atomic::Ordering::Release);
    }
    // Give any render-thread call that is still inside a detour time to return before the DLL can be unloaded.
    std::thread::sleep(std::time::Duration::from_millis(100));
    // The log's thread runs the plugin's code: stopped before the plugin can be unloaded.
    log::shutdown();
    log!("shutdown");
}

/// A C string's text (empty for none).
///
/// # Safety
/// `ptr` is null or a nul-terminated string alive through the call.
unsafe fn cstr(ptr: *const std::ffi::c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    // SAFETY: a nul-terminated string, alive through the call (the caller's contract).
    unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned()
}
