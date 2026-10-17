//! Minimal FFI declarations for the SCS Telemetry SDK 1.15 (`tools/scs_sdk_1_15/include`).
//! Only what the plugin uses is declared here; layouts follow the `scs_check_size` asserts in the headers.

use std::ffi::{c_char, c_void};

pub type ScsResult = i32;
pub const SCS_RESULT_OK: ScsResult = 0;
pub const SCS_RESULT_UNSUPPORTED: ScsResult = -1;

pub type LogType = i32;
pub const SCS_LOG_TYPE_MESSAGE: LogType = 0;

pub const fn make_version(major: u32, minor: u32) -> u32 {
    (major << 16) | minor
}

pub const fn major_version(version: u32) -> u32 {
    (version >> 16) & 0xffff
}

pub const fn minor_version(version: u32) -> u32 {
    version & 0xffff
}

pub const SCS_TELEMETRY_VERSION_1_00: u32 = make_version(1, 0);
pub const SCS_TELEMETRY_VERSION_1_01: u32 = make_version(1, 1);

pub type LogFn = unsafe extern "system" fn(LogType, *const c_char);

/// `scs_sdk_init_params_v100_t` (32 bytes on x64).
#[repr(C)]
pub struct SdkInitParamsV100 {
    pub game_name: *const c_char,
    pub game_id: *const c_char,
    pub game_version: u32,
    _padding: u32,
    pub log: Option<LogFn>,
}

/// `scs_telemetry_init_params_v100_t` / `_v101_t` (64 bytes on x64).
#[repr(C)]
pub struct TelemetryInitParamsV100 {
    pub common: SdkInitParamsV100,
    pub register_for_event: *const c_void,
    pub unregister_from_event: *const c_void,
    pub register_for_channel: *const c_void,
    pub unregister_from_channel: *const c_void,
}

const _: () = assert!(size_of::<SdkInitParamsV100>() == 32);
const _: () = assert!(size_of::<TelemetryInitParamsV100>() == 64);

pub const SCS_U32_NIL: u32 = u32::MAX;

pub const SCS_TELEMETRY_EVENT_FRAME_END: u32 = 2;
pub const SCS_TELEMETRY_EVENT_PAUSED: u32 = 3;
pub const SCS_TELEMETRY_EVENT_STARTED: u32 = 4;
pub const SCS_TELEMETRY_EVENT_CONFIGURATION: u32 = 5;

pub const SCS_VALUE_TYPE_BOOL: u32 = 1;
pub const SCS_VALUE_TYPE_S32: u32 = 2;
pub const SCS_VALUE_TYPE_U32: u32 = 3;
pub const SCS_VALUE_TYPE_FLOAT: u32 = 5;
pub const SCS_VALUE_TYPE_FVECTOR: u32 = 7;
pub const SCS_VALUE_TYPE_FPLACEMENT: u32 = 10;
pub const SCS_VALUE_TYPE_DPLACEMENT: u32 = 11;
pub const SCS_VALUE_TYPE_STRING: u32 = 12;

pub type EventCallback = unsafe extern "system" fn(event: u32, event_info: *const c_void, context: *mut c_void);
pub type ChannelCallback =
    unsafe extern "system" fn(name: *const c_char, index: u32, value: *const ScsValue, context: *mut c_void);

/// `scs_value_t`: a type tag followed by a 40-byte union (48 bytes total).
#[repr(C)]
pub struct ScsValue {
    pub value_type: u32,
    _padding: u32,
    data: [u64; 5],
}

const _: () = assert!(size_of::<ScsValue>() == 48);

/// `scs_value_dplacement_t`: position (dvector) + orientation (euler, in turns).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DPlacement {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub heading: f32,
    pub pitch: f32,
    pub roll: f32,
    _padding: u32,
}

/// `scs_value_fplacement_t`: position (fvector) + orientation (euler, in turns).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FPlacement {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub heading: f32,
    pub pitch: f32,
    pub roll: f32,
}

impl ScsValue {
    fn data(&self) -> *const u8 {
        self.data.as_ptr() as *const u8
    }

    pub fn bool(&self) -> bool {
        // SAFETY: the union's first byte, always there.
        unsafe { *self.data() != 0 }
    }

    pub fn s32(&self) -> i32 {
        // SAFETY: the union is 40 bytes, 8-aligned (`[u64; 5]`); this plain type fits it and is valid for any bits.
        unsafe { (self.data() as *const i32).read() }
    }

    pub fn u32(&self) -> u32 {
        // SAFETY: the union is 40 bytes, 8-aligned (`[u64; 5]`); this plain type fits it and is valid for any bits.
        unsafe { (self.data() as *const u32).read() }
    }

    pub fn u64(&self) -> u64 {
        // SAFETY: the union is 40 bytes, 8-aligned (`[u64; 5]`); this plain type fits it and is valid for any bits.
        unsafe { (self.data() as *const u64).read() }
    }

    pub fn float(&self) -> f32 {
        // SAFETY: the union is 40 bytes, 8-aligned (`[u64; 5]`); this plain type fits it and is valid for any bits.
        unsafe { (self.data() as *const f32).read() }
    }

    pub fn dplacement(&self) -> DPlacement {
        // SAFETY: the union is 40 bytes, 8-aligned (`[u64; 5]`): exactly a `DPlacement`, valid for any bits.
        unsafe { (self.data() as *const DPlacement).read() }
    }

    pub fn fplacement(&self) -> FPlacement {
        // SAFETY: the union is 40 bytes, 8-aligned (`[u64; 5]`); this plain type fits it and is valid for any bits.
        unsafe { (self.data() as *const FPlacement).read() }
    }

    /// The vector, if this value is one (`scs_value_fvector_t`).
    pub fn fvector(&self) -> Option<[f32; 3]> {
        // SAFETY: a vector (its type checked): three floats, within the union.
        (self.value_type == SCS_VALUE_TYPE_FVECTOR).then(|| unsafe { (self.data() as *const [f32; 3]).read() })
    }

    /// The string value, if this value is a string. Only valid during the callback that delivered it.
    pub fn string(&self) -> Option<String> {
        if self.value_type != SCS_VALUE_TYPE_STRING {
            return None;
        }
        // SAFETY: a string (its type checked): the union holds its pointer.
        let pointer = unsafe { (self.data() as *const *const c_char).read() };
        if pointer.is_null() {
            return None;
        }
        // SAFETY: the SDK's string, nul-terminated and alive while its callback runs, which is when values exist
        // (they're only ever borrowed from the callback's argument).
        Some(unsafe { std::ffi::CStr::from_ptr(pointer) }.to_string_lossy().into_owned())
    }
}

/// `scs_named_value_t` (64 bytes on x64).
#[repr(C)]
pub struct ScsNamedValue {
    pub name: *const c_char,
    pub index: u32,
    _padding: u32,
    pub value: ScsValue,
}

const _: () = assert!(size_of::<ScsNamedValue>() == 64);

/// `scs_telemetry_configuration_t`: a configuration id and its attributes, terminated by a null name.
#[repr(C)]
pub struct ConfigurationEvent {
    pub id: *const c_char,
    pub attributes: *const ScsNamedValue,
}

impl ConfigurationEvent {
    /// Iterates the attributes until the terminating entry.
    ///
    /// # Safety
    /// Only while the callback that delivered the configuration runs (the SDK frees it after).
    pub unsafe fn attributes(&self) -> impl Iterator<Item = &ScsNamedValue> {
        let mut next = self.attributes;
        std::iter::from_fn(move || {
            if next.is_null() {
                return None;
            }
            // SAFETY: `next` is within the attribute array, which ends with an entry of null name (the SDK's contract,
            // checked below before going on).
            let attribute = unsafe { &*next };
            if attribute.name.is_null() {
                return None;
            }
            // SAFETY: the entry wasn't the terminating one, so the array goes on past it.
            next = unsafe { next.add(1) };
            Some(attribute)
        })
    }
}
