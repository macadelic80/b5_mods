//! Field-for-field mirror of `b5-decomp/src/pc/mods/api/brn_mod_api.h` (API version 1).
//! Keep the two in step: the host checks only the version number.

#![allow(non_camel_case_types)]

use std::ffi::{c_char, c_void};

pub const BRN_MOD_API_VERSION: u32 = 1;

pub const BRN_MODIFIER_CTRL: u32 = 1;
pub const BRN_MODIFIER_SHIFT: u32 = 2;
pub const BRN_MODIFIER_ALT: u32 = 4;
pub const BRN_MODIFIER_SUPER: u32 = 8;

pub const BRN_CAPTURE_KEYBOARD: u32 = 1;
pub const BRN_CAPTURE_MOUSE: u32 = 2;
pub const BRN_CAPTURE_GAMEPAD: u32 = 4;

#[repr(C)]
pub struct BrnMod {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BrnVec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BrnFrameContext {
    pub d3d9_device: *mut c_void,
    pub hwnd: *mut c_void,
    pub width: u32,
    pub height: u32,
    pub delta_seconds: f32,
    pub frame_index: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BrnWindowMessage {
    pub hwnd: *mut c_void,
    pub msg: u32,
    pub wparam: u64,
    pub lparam: i64,
    pub modifiers: u32,
}

#[repr(C)]
pub struct BrnModCallbacks {
    pub size: u32,
    pub user: *mut c_void,
    pub on_message: Option<unsafe extern "C" fn(*mut c_void, *const BrnWindowMessage)>,
    pub on_frame: Option<unsafe extern "C" fn(*mut c_void, *const BrnFrameContext)>,
    pub on_game_tick: Option<unsafe extern "C" fn(*mut c_void)>,
    pub on_shutdown: Option<unsafe extern "C" fn(*mut c_void)>,
}

#[repr(C)]
pub struct BrnHostApi {
    pub size: u32,
    pub api_version: u32,
    pub self_: *mut BrnMod,
    pub get_service: unsafe extern "C" fn(*const c_char) -> *const c_void,
}

// ---- brn.core.v1 -----------------------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BrnModInfo {
    pub name: [u8; 64],
    pub folder: [u8; 64],
    pub version: [u8; 32],
    pub loaded: i32,
    pub enabled: i32,
    pub error: [u8; 128],
}

#[repr(C)]
pub struct BrnCoreV1 {
    pub size: u32,
    pub log: unsafe extern "C" fn(*mut BrnMod, *const c_char),
    pub mod_directory: unsafe extern "C" fn(*mut BrnMod) -> *const c_char,
    pub set_input_capture: unsafe extern "C" fn(*mut BrnMod, u32),
    pub mod_count: unsafe extern "C" fn() -> i32,
    pub get_mod_info: unsafe extern "C" fn(i32, *mut BrnModInfo) -> i32,
    pub set_mod_enabled: unsafe extern "C" fn(i32, i32),
}

// ---- brn.player.v1 ---------------------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BrnPlayerState {
    pub size: u32,
    pub valid: i32,
    pub position: BrnVec3,
    pub direction: BrnVec3,
    pub velocity: BrnVec3,
    pub speed_mph: f32,
    pub max_speed_mph: f32,
    pub max_boost_speed_mph: f32,
    pub rpm: f32,
    pub gear: i32,
    pub time_in_air: f32,
    pub time_drifting: f32,
    pub time_boosting: f32,
    pub time_crashing: f32,
    pub crashing: i32,
    pub driveable: i32,
    pub car_id: u64,
    pub car_id_text: [u8; 16],
    pub car_name: [u8; 64],
    pub wheel_id: u64,
    pub in_game_mode: i32,
    pub game_mode_type: i32,
    pub sim_paused: i32,
}

#[repr(C)]
pub struct BrnPlayerV1 {
    pub size: u32,
    pub get_state: unsafe extern "C" fn(*mut BrnPlayerState) -> i32,
    pub teleport: unsafe extern "C" fn(BrnVec3, BrnVec3),
    pub reset_on_track: unsafe extern "C" fn(f32),
    pub place_on_track: unsafe extern "C" fn(BrnVec3, BrnVec3, f32),
    pub set_infinite_boost: unsafe extern "C" fn(i32),
    pub set_invulnerable: unsafe extern "C" fn(i32),
    pub change_car: unsafe extern "C" fn(u64, u64) -> i32,
}

// ---- brn.vehicles.v1 -------------------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BrnVehicleInfo {
    pub id: u64,
    pub parent_id: u64,
    pub id_text: [u8; 16],
    pub name: [u8; 64],
    pub default_wheel: [u8; 64],
    pub car_type: i32,
    pub speed_stat: i32,
    pub boost_stat: i32,
    pub strength_stat: i32,
}

#[repr(C)]
pub struct BrnVehiclesV1 {
    pub size: u32,
    pub count: unsafe extern "C" fn() -> i32,
    pub get: unsafe extern "C" fn(i32, *mut BrnVehicleInfo) -> i32,
}

// ---- brn.locations.v1 ------------------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BrnLocation {
    pub label: [u8; 96],
    pub type_: i32,
    pub type_name: [u8; 32],
    pub position: BrnVec3,
    pub direction: BrnVec3,
}

#[repr(C)]
pub struct BrnLocationsV1 {
    pub size: u32,
    pub count: unsafe extern "C" fn() -> i32,
    pub get: unsafe extern "C" fn(i32, *mut BrnLocation) -> i32,
}

// ---- brn.racecars.v1 -------------------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BrnRaceCarInfo {
    pub active: i32,
    pub is_player: i32,
    pub position: BrnVec3,
    pub direction: BrnVec3,
    pub velocity: BrnVec3,
    pub speed_mph: f32,
    pub crashing: i32,
    pub car_id: u64,
    pub car_id_text: [u8; 16],
}

#[repr(C)]
pub struct BrnRaceCarsV1 {
    pub size: u32,
    pub count: unsafe extern "C" fn() -> i32,
    pub get: unsafe extern "C" fn(i32, *mut BrnRaceCarInfo) -> i32,
    pub remove_all_rivals: unsafe extern "C" fn(),
}

// ---- brn.traffic.v1 --------------------------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BrnTrafficSettings {
    pub size: u32,
    pub turn_off: i32,
    pub stop_moving: i32,
    pub allow_anarchy: i32,
    pub enable_avoidance: i32,
    pub speed_multiplier: f32,
    pub density_scale: f32,
    pub big_vehicle_amount: i32,
    pub final_density: f32,
}

#[repr(C)]
pub struct BrnTrafficV1 {
    pub size: u32,
    pub get: unsafe extern "C" fn(*mut BrnTrafficSettings) -> i32,
    pub set: unsafe extern "C" fn(*const BrnTrafficSettings),
    pub kill_near_player: unsafe extern "C" fn(),
}

// ---- brn.debugvars.v1 ------------------------------------------------------------------

pub const BRN_DEBUGVAR_MENU: i32 = 0;
pub const BRN_DEBUGVAR_FUNCTION: i32 = 1;
pub const BRN_DEBUGVAR_BOOL: i32 = 2;
pub const BRN_DEBUGVAR_INT: i32 = 3;
pub const BRN_DEBUGVAR_UINT: i32 = 4;
pub const BRN_DEBUGVAR_FLOAT: i32 = 5;
pub const BRN_DEBUGVAR_CHOICE: i32 = 6;
pub const BRN_DEBUGVAR_SECTION: i32 = 7;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BrnDebugVar {
    pub path: [u8; 192],
    pub name: [u8; 64],
    pub depth: i32,
    pub kind: i32,
    pub read_only: i32,
    pub has_range: i32,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub text: [u8; 64],
}

#[repr(C)]
pub struct BrnDebugVarsV1 {
    pub size: u32,
    pub refresh: unsafe extern "C" fn() -> u32,
    pub count: unsafe extern "C" fn() -> i32,
    pub get: unsafe extern "C" fn(i32, *mut BrnDebugVar) -> i32,
    pub option_count: unsafe extern "C" fn(i32) -> i32,
    pub get_option: unsafe extern "C" fn(i32, i32, *mut c_char, i32, *mut i32) -> i32,
    pub set: unsafe extern "C" fn(*const c_char, *const c_char),
    pub call: unsafe extern "C" fn(*const c_char),
}
