//! SCS telemetry → [`westplay_shared::Telemetry`] in the bridge.
//!
//! Channel callbacks update a local snapshot; at the end of every game frame the snapshot is published to
//! the shared memory. The truck's identity comes from the "truck" configuration event and is published
//! when it changes. All SDK callbacks arrive on the game's main thread.

use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::Mutex;

use westplay_shared::{FixedStr, JobInfo, Telemetry, TruckInfo};

use crate::scs::{
    ChannelCallback, ConfigurationEvent, EventCallback, SCS_RESULT_OK, SCS_TELEMETRY_EVENT_CONFIGURATION,
    SCS_TELEMETRY_EVENT_FRAME_END, SCS_TELEMETRY_EVENT_PAUSED, SCS_TELEMETRY_EVENT_STARTED, SCS_U32_NIL,
    SCS_VALUE_TYPE_BOOL, SCS_VALUE_TYPE_DPLACEMENT, SCS_VALUE_TYPE_FLOAT, SCS_VALUE_TYPE_FPLACEMENT,
    SCS_VALUE_TYPE_S32, SCS_VALUE_TYPE_U32, ScsValue, TelemetryInitParamsV100,
};
use crate::{bridge, log};

static SNAPSHOT: Mutex<Telemetry> = Mutex::new(Telemetry {
    x: 0.0,
    y: 0.0,
    z: 0.0,
    heading: 0.0,
    pitch: 0.0,
    roll: 0.0,
    speed: 0.0,
    engine_rpm: 0.0,
    gear: 0,
    displayed_gear: 0,
    cruise_control_speed: 0.0,
    fuel: 0.0,
    fuel_range: 0.0,
    navigation_distance: 0.0,
    navigation_time: 0.0,
    navigation_speed_limit: 0.0,
    game_time: 0,
    local_scale: 0.0,
    dashboard_backlight: 0.0,
    electric_enabled: false,
    engine_enabled: false,
    parking_brake: false,
    low_beam: false,
    high_beam: false,
    left_blinker: false,
    right_blinker: false,
    paused: true,
    head_x: 0.0,
    head_y: 0.0,
    head_z: 0.0,
    head_heading: 0.0,
    head_pitch: 0.0,
    camera_known: false,
    camera_x: 0.0,
    camera_y: 0.0,
    camera_z: 0.0,
    cabin_x: 0.0,
    cabin_y: 0.0,
    cabin_z: 0.0,
    steering: 0.0,
});

/// Where the driver's head is by default in the truck's cab (the "truck" configuration's `head.position`).
/// The snapshot holds the head's offset from it (the `truck.head.offset` channel); they're added together as
/// the snapshot is published.
static HEAD_DEFAULT: Mutex<[f32; 3]> = Mutex::new([0.0; 3]);

/// Where the cab turns in the truck (the "truck" configuration's `cabin.position`), published with every
/// snapshot.
static CABIN: Mutex<[f32; 3]> = Mutex::new([0.0; 3]);

#[derive(Clone, Copy)]
enum Field {
    Placement,
    Speed,
    EngineRpm,
    Gear,
    DisplayedGear,
    CruiseControl,
    Fuel,
    FuelRange,
    NavigationDistance,
    NavigationTime,
    NavigationSpeedLimit,
    GameTime,
    LocalScale,
    DashboardBacklight,
    ElectricEnabled,
    EngineEnabled,
    ParkingBrake,
    LowBeam,
    HighBeam,
    LeftBlinker,
    RightBlinker,
    HeadOffset,
    Steering,
}

const CHANNELS: &[(&str, u32, Field)] = &[
    ("truck.world.placement", SCS_VALUE_TYPE_DPLACEMENT, Field::Placement),
    ("truck.speed", SCS_VALUE_TYPE_FLOAT, Field::Speed),
    ("truck.engine.rpm", SCS_VALUE_TYPE_FLOAT, Field::EngineRpm),
    ("truck.engine.gear", SCS_VALUE_TYPE_S32, Field::Gear),
    ("truck.displayed.gear", SCS_VALUE_TYPE_S32, Field::DisplayedGear),
    ("truck.cruise_control", SCS_VALUE_TYPE_FLOAT, Field::CruiseControl),
    ("truck.fuel.amount", SCS_VALUE_TYPE_FLOAT, Field::Fuel),
    ("truck.fuel.range", SCS_VALUE_TYPE_FLOAT, Field::FuelRange),
    ("truck.navigation.distance", SCS_VALUE_TYPE_FLOAT, Field::NavigationDistance),
    ("truck.navigation.time", SCS_VALUE_TYPE_FLOAT, Field::NavigationTime),
    ("truck.navigation.speed.limit", SCS_VALUE_TYPE_FLOAT, Field::NavigationSpeedLimit),
    ("game.time", SCS_VALUE_TYPE_U32, Field::GameTime),
    ("local.scale", SCS_VALUE_TYPE_FLOAT, Field::LocalScale),
    ("truck.dashboard.backlight", SCS_VALUE_TYPE_FLOAT, Field::DashboardBacklight),
    ("truck.electric.enabled", SCS_VALUE_TYPE_BOOL, Field::ElectricEnabled),
    ("truck.engine.enabled", SCS_VALUE_TYPE_BOOL, Field::EngineEnabled),
    ("truck.brake.parking", SCS_VALUE_TYPE_BOOL, Field::ParkingBrake),
    ("truck.light.beam.low", SCS_VALUE_TYPE_BOOL, Field::LowBeam),
    ("truck.light.beam.high", SCS_VALUE_TYPE_BOOL, Field::HighBeam),
    ("truck.light.lblinker", SCS_VALUE_TYPE_BOOL, Field::LeftBlinker),
    ("truck.light.rblinker", SCS_VALUE_TYPE_BOOL, Field::RightBlinker),
    ("truck.head.offset", SCS_VALUE_TYPE_FPLACEMENT, Field::HeadOffset),
    ("truck.effective.steering", SCS_VALUE_TYPE_FLOAT, Field::Steering),
];

/// Registers the telemetry channels and events. Called from `scs_telemetry_init`.
///
/// # Safety
/// During `scs_telemetry_init`, with the parameters the game passed it.
pub unsafe fn register(params: &TelemetryInitParamsV100) {
    // SAFETY: the game's two registration functions, with the SDK's signatures (`EventRegisterFn`,
    // `ChannelRegisterFn`); the callbacks are ours, of the signatures the SDK calls, and the channel names are
    // copied by the SDK as they're registered.
    unsafe {
        let register_event: EventRegisterFn = std::mem::transmute(params.register_for_event);
        for event in [
            SCS_TELEMETRY_EVENT_FRAME_END,
            SCS_TELEMETRY_EVENT_PAUSED,
            SCS_TELEMETRY_EVENT_STARTED,
            SCS_TELEMETRY_EVENT_CONFIGURATION,
        ] {
            let result = register_event(event, on_event, std::ptr::null_mut());
            if result != SCS_RESULT_OK {
                log!("telemetry: event {event} registration failed ({result})");
            }
        }

        let register_channel: ChannelRegisterFn = std::mem::transmute(params.register_for_channel);
        let mut registered = 0;
        for (index, &(name, value_type, _)) in CHANNELS.iter().enumerate() {
            let Ok(c_name) = CString::new(name) else { continue };
            // The SDK copies the name during registration.
            let result =
                register_channel(c_name.as_ptr(), SCS_U32_NIL, value_type, 0, on_channel, index as *mut c_void);
            if result == SCS_RESULT_OK {
                registered += 1;
            } else {
                log!("telemetry: channel {name} registration failed ({result})");
            }
        }
        log!("telemetry: {registered}/{} channels registered", CHANNELS.len());
    }
}

type EventRegisterFn = unsafe extern "system" fn(u32, EventCallback, *mut c_void) -> i32;
type ChannelRegisterFn = unsafe extern "system" fn(*const c_char, u32, u32, u32, ChannelCallback, *mut c_void) -> i32;

/// # Safety
/// Called by the game, as the SDK says: on its main thread, `info` the event's (or null), alive through
/// the call.
unsafe extern "system" fn on_event(event: u32, info: *const c_void, _context: *mut c_void) {
    // SAFETY: as the SDK calls it (below), so `on_game_event` is called.
    unsafe {
        crate::hooks::contained("a telemetry event", (), || on_game_event(event, info));
    }
}

/// # Safety
/// As [`on_event`].
unsafe fn on_game_event(event: u32, info: *const c_void) {
    // SAFETY: a configuration event's `info` is the SDK's `ConfigurationEvent`, alive through the call; the
    // navigator is served on the game's main thread, where this is (`serve`'s contract).
    unsafe {
        if event == SCS_TELEMETRY_EVENT_CONFIGURATION {
            if let Some(configuration) = (info as *const ConfigurationEvent).as_ref() {
                on_configuration(configuration);
            }
            return;
        }
        let mut snapshot = SNAPSHOT.lock().unwrap_or_else(|e| e.into_inner());
        match event {
            SCS_TELEMETRY_EVENT_PAUSED => snapshot.paused = true,
            SCS_TELEMETRY_EVENT_STARTED => snapshot.paused = false,
            SCS_TELEMETRY_EVENT_FRAME_END => {
                if let Some(bridge) = bridge::get() {
                    let [x, y, z] = *HEAD_DEFAULT.lock().unwrap_or_else(|e| e.into_inner());
                    let [cabin_x, cabin_y, cabin_z] = *CABIN.lock().unwrap_or_else(|e| e.into_inner());
                    let camera = crate::camera::position();
                    let [camera_x, camera_y, camera_z] = camera.unwrap_or_default();
                    let published = Telemetry {
                        head_x: snapshot.head_x + x,
                        head_y: snapshot.head_y + y,
                        head_z: snapshot.head_z + z,
                        camera_known: camera.is_some(),
                        camera_x,
                        camera_y,
                        camera_z,
                        cabin_x,
                        cabin_y,
                        cabin_z,
                        ..*snapshot
                    };
                    bridge.plugin.telemetry.write(&published);
                }
                let (speed, paused) = (snapshot.speed, snapshot.paused);
                drop(snapshot);
                // Whether the player's keys type into WestPlay's field: they stop the moment the truck moves.
                crate::typing::update(speed, paused);
                // The end of a frame is on the game's main thread, where its navigator may be changed.
                crate::navigation::serve();
            }
            _ => {}
        }
    }
}

/// Publishes what the game (re)configures: the truck's identity and the player's job. An empty attribute set
/// means the player has none right now.
///
/// # Safety
/// `configuration` is the game's, alive through the call (an event's `info`).
unsafe fn on_configuration(configuration: &ConfigurationEvent) {
    // SAFETY: the SDK's strings, nul-terminated and alive through the call; checked for none first.
    unsafe {
        if configuration.id.is_null() {
            return;
        }
        match CStr::from_ptr(configuration.id).to_bytes() {
            b"truck" => on_truck(configuration),
            b"job" => on_job(configuration),
            _ => {}
        }
    }
}

/// # Safety
/// As [`on_configuration`].
unsafe fn on_truck(configuration: &ConfigurationEvent) {
    // SAFETY: the attributes' names are the SDK's strings, nul-terminated and alive through the call.
    unsafe {
        let mut truck = TruckInfo::default();
        let mut head = [0.0; 3];
        let mut cabin = [0.0; 3];
        for attribute in configuration.attributes() {
            match CStr::from_ptr(attribute.name).to_bytes() {
                b"head.position" => {
                    head = attribute.value.fvector().unwrap_or_default();
                    continue;
                }
                b"cabin.position" => {
                    cabin = attribute.value.fvector().unwrap_or_default();
                    continue;
                }
                _ => {}
            }
            let Some(value) = attribute.value.string() else { continue };
            match CStr::from_ptr(attribute.name).to_bytes() {
                b"brand_id" => truck.brand_id = FixedStr::new(&value),
                b"brand" => truck.brand = FixedStr::new(&value),
                b"id" => truck.id = FixedStr::new(&value),
                b"name" => truck.name = FixedStr::new(&value),
                _ => {}
            }
        }
        log!(
            "truck: {} {} ({}, {}), head at {head:?}, cab turning at {cabin:?}",
            truck.brand.as_str(),
            truck.name.as_str(),
            truck.brand_id.as_str(),
            truck.id.as_str()
        );
        *HEAD_DEFAULT.lock().unwrap_or_else(|e| e.into_inner()) = head;
        *CABIN.lock().unwrap_or_else(|e| e.into_inner()) = cabin;
        if let Some(bridge) = bridge::get() {
            bridge.plugin.truck.write(&truck);
        }
    }
}

/// # Safety
/// As [`on_configuration`].
unsafe fn on_job(configuration: &ConfigurationEvent) {
    // SAFETY: the attributes' names are the SDK's strings, nul-terminated and alive through the call.
    unsafe {
        let mut job = JobInfo::default();
        for attribute in configuration.attributes() {
            job.active = true;
            let value = &attribute.value;
            let text = || FixedStr::new(&value.string().unwrap_or_default());
            match CStr::from_ptr(attribute.name).to_bytes() {
                b"cargo" => job.cargo = text(),
                b"cargo.mass" => job.cargo_mass = value.float(),
                b"cargo.loaded" => job.cargo_loaded = value.bool(),
                b"is.special.job" => job.special = value.bool(),
                b"source.city" => job.source_city = text(),
                b"source.company" => job.source_company = text(),
                b"destination.city" => job.destination_city = text(),
                b"destination.company" => job.destination_company = text(),
                b"delivery.time" => job.delivery_time = value.u32(),
                b"planned_distance.km" => job.planned_distance_km = value.u32(),
                b"income" => job.income = value.u64(),
                _ => {}
            }
        }
        log!(
            "job: {} ({} kg) from {}, {} to {}, {}",
            job.cargo.as_str(),
            job.cargo_mass,
            job.source_company.as_str(),
            job.source_city.as_str(),
            job.destination_company.as_str(),
            job.destination_city.as_str()
        );
        if let Some(bridge) = bridge::get() {
            bridge.plugin.job.write(&job);
        }
    }
}

/// # Safety
/// Called by the game, as the SDK says: `value` the channel's (or null, no value), alive through the
/// call; `context` the index it was registered with.
unsafe extern "system" fn on_channel(_name: *const c_char, _index: u32, value: *const ScsValue, context: *mut c_void) {
    // SAFETY: as the SDK calls it (below), so `on_channel_value` is called.
    unsafe {
        crate::hooks::contained("a telemetry channel", (), || on_channel_value(value, context));
    }
}

/// # Safety
/// As [`on_channel`].
unsafe fn on_channel_value(value: *const ScsValue, context: *mut c_void) {
    // SAFETY: `value` is the SDK's, of the type the channel was registered with (each `Field` reads its own),
    // or null.
    unsafe {
        let Some(&(_, _, field)) = CHANNELS.get(context as usize) else { return };
        let mut s = SNAPSHOT.lock().unwrap_or_else(|e| e.into_inner());
        // A null value means the channel currently has no value (e.g. no truck); reset to the default.
        let Some(value) = value.as_ref() else {
            reset(&mut s, field);
            return;
        };
        match field {
            Field::Placement => {
                let p = value.dplacement();
                (s.x, s.y, s.z) = (p.x, p.y, p.z);
                (s.heading, s.pitch, s.roll) = (p.heading, p.pitch, p.roll);
            }
            Field::Speed => s.speed = value.float(),
            Field::EngineRpm => s.engine_rpm = value.float(),
            Field::Gear => s.gear = value.s32(),
            Field::DisplayedGear => s.displayed_gear = value.s32(),
            Field::CruiseControl => s.cruise_control_speed = value.float(),
            Field::Fuel => s.fuel = value.float(),
            Field::FuelRange => s.fuel_range = value.float(),
            Field::NavigationDistance => s.navigation_distance = value.float(),
            Field::NavigationTime => s.navigation_time = value.float(),
            Field::NavigationSpeedLimit => s.navigation_speed_limit = value.float(),
            Field::GameTime => s.game_time = value.u32(),
            Field::LocalScale => s.local_scale = value.float(),
            Field::DashboardBacklight => s.dashboard_backlight = value.float(),
            Field::ElectricEnabled => s.electric_enabled = value.bool(),
            Field::EngineEnabled => s.engine_enabled = value.bool(),
            Field::ParkingBrake => s.parking_brake = value.bool(),
            Field::LowBeam => s.low_beam = value.bool(),
            Field::HighBeam => s.high_beam = value.bool(),
            Field::LeftBlinker => s.left_blinker = value.bool(),
            Field::RightBlinker => s.right_blinker = value.bool(),
            Field::HeadOffset => {
                let p = value.fplacement();
                (s.head_x, s.head_y, s.head_z) = (p.x, p.y, p.z);
                (s.head_heading, s.head_pitch) = (p.heading, p.pitch);
            }
            Field::Steering => s.steering = value.float(),
        }
    }
}

fn reset(s: &mut Telemetry, field: Field) {
    let d = Telemetry::default();
    match field {
        Field::Placement => {
            (s.x, s.y, s.z) = (d.x, d.y, d.z);
            (s.heading, s.pitch, s.roll) = (d.heading, d.pitch, d.roll);
        }
        Field::Speed => s.speed = d.speed,
        Field::EngineRpm => s.engine_rpm = d.engine_rpm,
        Field::Gear => s.gear = d.gear,
        Field::DisplayedGear => s.displayed_gear = d.displayed_gear,
        Field::CruiseControl => s.cruise_control_speed = d.cruise_control_speed,
        Field::Fuel => s.fuel = d.fuel,
        Field::FuelRange => s.fuel_range = d.fuel_range,
        Field::NavigationDistance => s.navigation_distance = d.navigation_distance,
        Field::NavigationTime => s.navigation_time = d.navigation_time,
        Field::NavigationSpeedLimit => s.navigation_speed_limit = d.navigation_speed_limit,
        Field::GameTime => s.game_time = d.game_time,
        Field::LocalScale => s.local_scale = d.local_scale,
        Field::DashboardBacklight => s.dashboard_backlight = d.dashboard_backlight,
        Field::ElectricEnabled => s.electric_enabled = d.electric_enabled,
        Field::EngineEnabled => s.engine_enabled = d.engine_enabled,
        Field::ParkingBrake => s.parking_brake = d.parking_brake,
        Field::LowBeam => s.low_beam = d.low_beam,
        Field::HighBeam => s.high_beam = d.high_beam,
        Field::LeftBlinker => s.left_blinker = d.left_blinker,
        Field::RightBlinker => s.right_blinker = d.right_blinker,
        Field::HeadOffset => {
            (s.head_x, s.head_y, s.head_z) = (d.head_x, d.head_y, d.head_z);
            (s.head_heading, s.head_pitch) = (d.head_heading, d.head_pitch);
        }
        Field::Steering => s.steering = d.steering,
    }
}
