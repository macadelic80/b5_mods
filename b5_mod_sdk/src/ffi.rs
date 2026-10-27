//! Field-for-field mirror of `b5-decomp/src/pc/mods/api/brn_mod_api.h` (API version 2, the
//! WebAssembly tier). Keep the two in step: the structs are copied byte for byte between the game
//! and the mod's memory, and the host checks only the version number.

#![allow(non_camel_case_types)]
// Several modules export a `get` / `count` / `set`: same import name, different import module.
// The wasm imports stay distinct (module.name, each with its own signature); the lint only
// compares the bare names.
#![allow(clashing_extern_declarations)]

pub const BRN_MOD_API_VERSION: u32 = 2;

pub const BRN_MODIFIER_CTRL: u32 = 1;
pub const BRN_MODIFIER_SHIFT: u32 = 2;
pub const BRN_MODIFIER_ALT: u32 = 4;
pub const BRN_MODIFIER_SUPER: u32 = 8;

pub const BRN_CAPTURE_KEYBOARD: u32 = 1;
pub const BRN_CAPTURE_MOUSE: u32 = 2;
pub const BRN_CAPTURE_GAMEPAD: u32 = 4;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BrnVec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

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
#[derive(Clone, Copy)]
pub struct BrnLocation {
    pub label: [u8; 96],
    pub type_: i32,
    pub type_name: [u8; 32],
    pub position: BrnVec3,
    pub direction: BrnVec3,
}

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

// ---- input --------------------------------------------------------------------------------

pub const BRN_INPUT_MOUSE_MOVE: u32 = 1;
pub const BRN_INPUT_MOUSE_BUTTON: u32 = 2;
pub const BRN_INPUT_MOUSE_WHEEL: u32 = 3;
pub const BRN_INPUT_MOUSE_LEAVE: u32 = 4;
pub const BRN_INPUT_KEY: u32 = 5;
pub const BRN_INPUT_TEXT: u32 = 6;
pub const BRN_INPUT_FOCUS: u32 = 7;
pub const BRN_INPUT_RESIZE: u32 = 8;

pub const BRN_MOUSE_LEFT: i32 = 0;
pub const BRN_MOUSE_RIGHT: i32 = 1;
pub const BRN_MOUSE_MIDDLE: i32 = 2;
pub const BRN_MOUSE_X1: i32 = 3;
pub const BRN_MOUSE_X2: i32 = 4;

pub const BRN_KEY_ESCAPE: i32 = 256;
pub const BRN_KEY_ENTER: i32 = 257;
pub const BRN_KEY_TAB: i32 = 258;
pub const BRN_KEY_BACKSPACE: i32 = 259;
pub const BRN_KEY_INSERT: i32 = 260;
pub const BRN_KEY_DELETE: i32 = 261;
pub const BRN_KEY_RIGHT: i32 = 262;
pub const BRN_KEY_LEFT: i32 = 263;
pub const BRN_KEY_DOWN: i32 = 264;
pub const BRN_KEY_UP: i32 = 265;
pub const BRN_KEY_PAGE_UP: i32 = 266;
pub const BRN_KEY_PAGE_DOWN: i32 = 267;
pub const BRN_KEY_HOME: i32 = 268;
pub const BRN_KEY_END: i32 = 269;
pub const BRN_KEY_F1: i32 = 290;
pub const BRN_KEY_KP_0: i32 = 320;
pub const BRN_KEY_KP_ENTER: i32 = 335;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct brn_input_event {
    pub type_: u32,
    pub modifiers: u32,
    pub code: i32,
    pub pressed: i32,
    pub repeat: i32,
    pub x: f32,
    pub y: f32,
}

pub const BRN_INPUT_BUFFER_CAPACITY: usize = 64;

// ---- UI -----------------------------------------------------------------------------------

/// pos, uv, colour bytes r g b a: the layout of `egui::epaint::Vertex`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct brn_vertex {
    pub x: f32,
    pub y: f32,
    pub u: f32,
    pub v: f32,
    pub rgba: u32,
}

pub const BRN_UI_PREMULTIPLIED: u32 = 1;

// ---- host imports -------------------------------------------------------------------------

#[link(wasm_import_module = "brn.core.v1")]
extern "C" {
    #[link_name = "log"]
    pub fn core_log(text: *const u8, len: u32);
    #[link_name = "has_module"]
    pub fn core_has_module(name: *const u8, len: u32) -> i32;
    #[link_name = "time"]
    pub fn core_time() -> f64;
    #[link_name = "set_input_capture"]
    pub fn core_set_input_capture(flags: u32);
    #[link_name = "mod_count"]
    pub fn core_mod_count() -> i32;
    #[link_name = "get_mod_info"]
    pub fn core_get_mod_info(index: i32, out: *mut BrnModInfo) -> i32;
    #[link_name = "set_mod_enabled"]
    pub fn core_set_mod_enabled(index: i32, enabled: i32);
}

#[link(wasm_import_module = "brn.player.v1")]
extern "C" {
    #[link_name = "get_state"]
    pub fn player_get_state(out: *mut BrnPlayerState) -> i32;
    #[link_name = "teleport"]
    pub fn player_teleport(x: f32, y: f32, z: f32, dx: f32, dy: f32, dz: f32);
    #[link_name = "reset_on_track"]
    pub fn player_reset_on_track(speed_mph: f32);
    #[link_name = "place_on_track"]
    pub fn player_place_on_track(x: f32, y: f32, z: f32, dx: f32, dy: f32, dz: f32, speed_mph: f32);
    #[link_name = "set_infinite_boost"]
    pub fn player_set_infinite_boost(enabled: i32);
    #[link_name = "set_invulnerable"]
    pub fn player_set_invulnerable(enabled: i32);
    #[link_name = "change_car"]
    pub fn player_change_car(car_id: u64, wheel_id: u64) -> i32;
}

#[link(wasm_import_module = "brn.vehicles.v1")]
extern "C" {
    #[link_name = "count"]
    pub fn vehicles_count() -> i32;
    #[link_name = "get"]
    pub fn vehicles_get(index: i32, out: *mut BrnVehicleInfo) -> i32;
}

#[link(wasm_import_module = "brn.locations.v1")]
extern "C" {
    #[link_name = "count"]
    pub fn locations_count() -> i32;
    #[link_name = "get"]
    pub fn locations_get(index: i32, out: *mut BrnLocation) -> i32;
}

#[link(wasm_import_module = "brn.racecars.v1")]
extern "C" {
    #[link_name = "count"]
    pub fn racecars_count() -> i32;
    #[link_name = "get"]
    pub fn racecars_get(index: i32, out: *mut BrnRaceCarInfo) -> i32;
    #[link_name = "remove_all_rivals"]
    pub fn racecars_remove_all_rivals();
}

#[link(wasm_import_module = "brn.traffic.v1")]
extern "C" {
    #[link_name = "get"]
    pub fn traffic_get(out: *mut BrnTrafficSettings) -> i32;
    #[link_name = "set"]
    pub fn traffic_set(settings: *const BrnTrafficSettings);
    #[link_name = "kill_near_player"]
    pub fn traffic_kill_near_player();
}

#[link(wasm_import_module = "brn.debugvars.v1")]
extern "C" {
    #[link_name = "refresh"]
    pub fn debugvars_refresh() -> u32;
    #[link_name = "count"]
    pub fn debugvars_count() -> i32;
    #[link_name = "get"]
    pub fn debugvars_get(index: i32, out: *mut BrnDebugVar) -> i32;
    #[link_name = "option_count"]
    pub fn debugvars_option_count(index: i32) -> i32;
    #[link_name = "get_option"]
    pub fn debugvars_get_option(index: i32, option: i32, name: *mut u8, name_size: i32, value: *mut i32) -> i32;
    #[link_name = "set"]
    pub fn debugvars_set(path: *const u8, path_len: u32, value: *const u8, value_len: u32);
    #[link_name = "call"]
    pub fn debugvars_call(path: *const u8, path_len: u32);
}

#[link(wasm_import_module = "brn.debugini.v1")]
extern "C" {
    #[link_name = "set"]
    pub fn debugini_set(path: *const u8, path_len: u32, value: *const u8, value_len: u32) -> i32;
}

#[link(wasm_import_module = "brn.storage.v1")]
extern "C" {
    #[link_name = "read"]
    pub fn storage_read(file: *const u8, file_len: u32, out: *mut u8, capacity: u32) -> i32;
    #[link_name = "write"]
    pub fn storage_write(file: *const u8, file_len: u32, data: *const u8, size: u32) -> i32;
    #[link_name = "remove"]
    pub fn storage_remove(file: *const u8, file_len: u32) -> i32;
}

#[link(wasm_import_module = "brn.ui.v1")]
extern "C" {
    #[link_name = "texture_create"]
    pub fn ui_texture_create(rgba: *const u8, w: i32, h: i32) -> u32;
    #[link_name = "texture_update"]
    pub fn ui_texture_update(tex: u32, x: i32, y: i32, w: i32, h: i32, rgba: *const u8);
    #[link_name = "texture_destroy"]
    pub fn ui_texture_destroy(tex: u32);
    #[link_name = "submit32"]
    pub fn ui_submit32(
        v: *const brn_vertex,
        nv: u32,
        idx: *const u32,
        ni: u32,
        tex: u32,
        clip_x0: f32,
        clip_y0: f32,
        clip_x1: f32,
        clip_y1: f32,
        flags: u32,
    );
    #[link_name = "set_clipboard_text"]
    pub fn ui_set_clipboard_text(text: *const u8, len: u32);
}
