//! Write Burnout_PC.exe mods in Rust.
//!
//! A mod is a `cdylib` that implements [`Mod`] and calls [`export_mod!`] once. The game
//! loads it from `mods/<folder>/` (see `mod.ini`) and calls it:
//!   * `on_message` / `on_frame` on the render thread (messages first, then the frame);
//!   * [`Mod::on_game_tick`] on the update thread, without `self` -- keep game-thread work
//!     stateless or behind your own lock.
//! Game state comes from the services on [`Host`]; every getter returns a copy made at the
//! last game tick and every command is applied at the next one, so no call can race the game.
//! A panic in a callback is caught at the C boundary: the mod is switched off and named in
//! BrnGame.log, the game keeps running.

pub mod ffi;
#[cfg(feature = "egui")]
pub mod overlay;

use std::ffi::{c_char, c_void, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

pub use ffi::{
    BrnLocation as Location, BrnModInfo, BrnPlayerState as PlayerState, BrnRaceCarInfo as RaceCarInfo,
    BrnTrafficSettings as TrafficSettings, BrnVec3 as Vec3, BrnVehicleInfo as VehicleInfo,
    BRN_CAPTURE_GAMEPAD, BRN_CAPTURE_KEYBOARD, BRN_CAPTURE_MOUSE,
};
pub type Frame = ffi::BrnFrameContext;
pub type WindowMessage = ffi::BrnWindowMessage;

/// The NUL-terminated text of a fixed `char[N]` field.
pub fn text(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// What the game offers this mod. Fetched once at load; `None` when this build lacks a service.
pub struct Host {
    self_: *mut ffi::BrnMod,
    core: &'static ffi::BrnCoreV1,
    pub player: Option<Player>,
    pub vehicles: Option<Vehicles>,
    pub locations: Option<Locations>,
    pub race_cars: Option<RaceCars>,
    pub traffic: Option<Traffic>,
    pub debug_vars: Option<DebugVars>,
}

// The tables are static in the exe and every entry is thread-safe (see brn_mod_api.h).
unsafe impl Send for Host {}
unsafe impl Sync for Host {}

static HOST: OnceLock<Host> = OnceLock::new();

/// The host, once `brn_mod_init` has run (always true inside a callback).
pub fn host() -> &'static Host {
    HOST.get().expect("b5_mod_sdk::host() before brn_mod_init")
}

unsafe fn service<T>(get: unsafe extern "C" fn(*const c_char) -> *const c_void, name: &CStr) -> Option<&'static T> {
    (get(name.as_ptr()) as *const T).as_ref()
}

impl Host {
    pub fn log(&self, message: &str) {
        let line = CString::new(message.replace('\0', " ")).unwrap_or_default();
        unsafe { (self.core.log)(self.self_, line.as_ptr()) }
    }

    /// `...\mods\<folder>\`, for the mod's own files.
    pub fn mod_directory(&self) -> PathBuf {
        unsafe { PathBuf::from(CStr::from_ptr((self.core.mod_directory)(self.self_)).to_string_lossy().into_owned()) }
    }

    /// `BRN_CAPTURE_*`: while set, the game stops reading that input (your menu is open).
    pub fn set_input_capture(&self, flags: u32) {
        unsafe { (self.core.set_input_capture)(self.self_, flags) }
    }

    pub fn mods(&self) -> Vec<BrnModInfo> {
        let count = unsafe { (self.core.mod_count)() };
        (0..count)
            .filter_map(|i| {
                let mut info: BrnModInfo = unsafe { std::mem::zeroed() };
                (unsafe { (self.core.get_mod_info)(i, &mut info) } != 0).then_some(info)
            })
            .collect()
    }

    /// Saved in mods/mods.ini; a mod that is not loaded starts with the next run.
    pub fn set_mod_enabled(&self, index: usize, enabled: bool) {
        unsafe { (self.core.set_mod_enabled)(index as i32, enabled as i32) }
    }
}

impl BrnModInfo {
    pub fn name(&self) -> String { text(&self.name) }
    pub fn folder(&self) -> String { text(&self.folder) }
    pub fn version(&self) -> String { text(&self.version) }
    pub fn error(&self) -> String { text(&self.error) }
}

impl PlayerState {
    pub fn car_name(&self) -> String { text(&self.car_name) }
    pub fn car_id_text(&self) -> String { text(&self.car_id_text) }
}

impl VehicleInfo {
    pub fn name(&self) -> String { text(&self.name) }
    pub fn id_text(&self) -> String { text(&self.id_text) }
}

impl Location {
    pub fn label(&self) -> String { text(&self.label) }
    pub fn type_name(&self) -> String { text(&self.type_name) }
}

impl RaceCarInfo {
    pub fn car_id_text(&self) -> String { text(&self.car_id_text) }
}

// ---- services --------------------------------------------------------------------------

/// `brn.player.v1`: the local player's car.
#[derive(Clone, Copy)]
pub struct Player(&'static ffi::BrnPlayerV1);

impl Player {
    pub fn state(&self) -> Option<PlayerState> {
        let mut state: PlayerState = unsafe { std::mem::zeroed() };
        state.size = std::mem::size_of::<PlayerState>() as u32;
        (unsafe { (self.0.get_state)(&mut state) } != 0).then_some(state)
    }
    /// Same car, new pose. `Vec3::default()` as direction keeps the heading.
    pub fn teleport(&self, position: Vec3, direction: Vec3) { unsafe { (self.0.teleport)(position, direction) } }
    pub fn reset_on_track(&self, speed_mph: f32) { unsafe { (self.0.reset_on_track)(speed_mph) } }
    pub fn place_on_track(&self, position: Vec3, direction: Vec3, speed_mph: f32) {
        unsafe { (self.0.place_on_track)(position, direction, speed_mph) }
    }
    pub fn set_infinite_boost(&self, enabled: bool) { unsafe { (self.0.set_infinite_boost)(enabled as i32) } }
    pub fn set_invulnerable(&self, enabled: bool) { unsafe { (self.0.set_invulnerable)(enabled as i32) } }
    /// `wheel_id` 0 = the car's default wheels. False while an event is running.
    pub fn change_car(&self, car_id: u64, wheel_id: u64) -> bool { unsafe { (self.0.change_car)(car_id, wheel_id) != 0 } }
}

/// `brn.vehicles.v1`: the game's vehicle list.
#[derive(Clone, Copy)]
pub struct Vehicles(&'static ffi::BrnVehiclesV1);

impl Vehicles {
    pub fn all(&self) -> Vec<VehicleInfo> {
        let count = unsafe { (self.0.count)() };
        (0..count)
            .filter_map(|i| {
                let mut info: VehicleInfo = unsafe { std::mem::zeroed() };
                (unsafe { (self.0.get)(i, &mut info) } != 0).then_some(info)
            })
            .collect()
    }
}

/// `brn.locations.v1`: junkyards, gas stations, shops and car parks.
#[derive(Clone, Copy)]
pub struct Locations(&'static ffi::BrnLocationsV1);

impl Locations {
    pub fn all(&self) -> Vec<Location> {
        let count = unsafe { (self.0.count)() };
        (0..count)
            .filter_map(|i| {
                let mut info: Location = unsafe { std::mem::zeroed() };
                (unsafe { (self.0.get)(i, &mut info) } != 0).then_some(info)
            })
            .collect()
    }
}

/// `brn.racecars.v1`: the active race car slots.
#[derive(Clone, Copy)]
pub struct RaceCars(&'static ffi::BrnRaceCarsV1);

impl RaceCars {
    /// One entry per slot; `None` for an empty slot.
    pub fn slots(&self) -> Vec<Option<RaceCarInfo>> {
        let count = unsafe { (self.0.count)() };
        (0..count)
            .map(|i| {
                let mut info: RaceCarInfo = unsafe { std::mem::zeroed() };
                (unsafe { (self.0.get)(i, &mut info) } != 0).then_some(info)
            })
            .collect()
    }
    pub fn remove_all_rivals(&self) { unsafe { (self.0.remove_all_rivals)() } }
}

/// `brn.traffic.v1`: the traffic module's debug switches.
#[derive(Clone, Copy)]
pub struct Traffic(&'static ffi::BrnTrafficV1);

impl Traffic {
    pub fn get(&self) -> Option<TrafficSettings> {
        let mut settings = TrafficSettings { size: std::mem::size_of::<TrafficSettings>() as u32, ..Default::default() };
        (unsafe { (self.0.get)(&mut settings) } != 0).then_some(settings)
    }
    pub fn set(&self, settings: &TrafficSettings) { unsafe { (self.0.set)(settings) } }
    pub fn kill_near_player(&self) { unsafe { (self.0.kill_near_player)() } }
}

/// One row of the game's debug menu (`brn.debugvars.v1`).
#[derive(Clone, Debug)]
pub struct DebugVar {
    pub index: i32,
    pub path: String,
    pub name: String,
    pub depth: i32,
    pub kind: DebugVarKind,
    pub read_only: bool,
    pub range: Option<(f64, f64)>,
    pub value: f64,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugVarKind {
    Menu,
    Function,
    Bool,
    Int,
    UInt,
    Float,
    Choice,
    /// A component's section: `call()` opens it (no side effect); `value` is 1 once open.
    Section,
}

/// `brn.debugvars.v1`: the game's CgsDev debug menu tree.
#[derive(Clone, Copy)]
pub struct DebugVars(&'static ffi::BrnDebugVarsV1);

impl DebugVars {
    /// Call every frame while you show the list; returns its generation (0 = not built yet).
    pub fn refresh(&self) -> u32 { unsafe { (self.0.refresh)() } }

    pub fn all(&self) -> Vec<DebugVar> {
        let count = unsafe { (self.0.count)() };
        (0..count)
            .filter_map(|index| {
                let mut raw: ffi::BrnDebugVar = unsafe { std::mem::zeroed() };
                if unsafe { (self.0.get)(index, &mut raw) } == 0 {
                    return None;
                }
                let kind = match raw.kind {
                    ffi::BRN_DEBUGVAR_MENU => DebugVarKind::Menu,
                    ffi::BRN_DEBUGVAR_BOOL => DebugVarKind::Bool,
                    ffi::BRN_DEBUGVAR_INT => DebugVarKind::Int,
                    ffi::BRN_DEBUGVAR_UINT => DebugVarKind::UInt,
                    ffi::BRN_DEBUGVAR_FLOAT => DebugVarKind::Float,
                    ffi::BRN_DEBUGVAR_CHOICE => DebugVarKind::Choice,
                    ffi::BRN_DEBUGVAR_SECTION => DebugVarKind::Section,
                    _ => DebugVarKind::Function,
                };
                Some(DebugVar {
                    index,
                    path: text(&raw.path),
                    name: text(&raw.name),
                    depth: raw.depth,
                    kind,
                    read_only: raw.read_only != 0,
                    range: (raw.has_range != 0).then_some((raw.min, raw.max)),
                    value: raw.value,
                    text: text(&raw.text),
                })
            })
            .collect()
    }

    /// The named options of a `Choice` row: (name, value).
    pub fn options(&self, index: i32) -> Vec<(String, i32)> {
        let count = unsafe { (self.0.option_count)(index) };
        (0..count)
            .filter_map(|option| {
                let mut name = [0u8; 96];
                let mut value = 0;
                (unsafe { (self.0.get_option)(index, option, name.as_mut_ptr() as *mut c_char, name.len() as i32, &mut value) } != 0)
                    .then(|| (text(&name), value))
            })
            .collect()
    }

    /// As typed in the game's console: "true"/"false" (a bool is true only for "TRUE"), "12", "0.5",
    /// an option name. Applied at the next tick.
    pub fn set(&self, path: &str, value: &str) {
        let (Ok(path), Ok(value)) = (CString::new(path), CString::new(value)) else { return };
        unsafe { (self.0.set)(path.as_ptr(), value.as_ptr()) }
    }

    /// Run an action, or open a component section (its controls then appear in the list).
    pub fn call(&self, path: &str) {
        let Ok(path) = CString::new(path) else { return };
        unsafe { (self.0.call)(path.as_ptr()) }
    }
}

// ---- the mod -----------------------------------------------------------------------------

pub trait Mod: 'static {
    /// Render thread, before `on_frame`, once per queued window message.
    fn on_message(&mut self, _host: &Host, _message: &WindowMessage) {}
    /// Render thread, inside the scene, back buffer bound. Draw here.
    fn on_frame(&mut self, _host: &Host, _frame: &Frame) {}
    /// Update thread, once per game update. No `self`: the UI state lives on the render thread.
    fn on_game_tick(_host: &Host) where Self: Sized {}
}

/// Exports `brn_mod_api_version` and `brn_mod_init` for `$ty`, built by `$new(&Host)`.
#[macro_export]
macro_rules! export_mod {
    ($ty:ty, $new:expr) => {
        #[no_mangle]
        pub extern "C" fn brn_mod_api_version() -> u32 {
            $crate::ffi::BRN_MOD_API_VERSION
        }

        #[no_mangle]
        pub unsafe extern "C" fn brn_mod_init(
            host: *const $crate::ffi::BrnHostApi,
            callbacks: *mut $crate::ffi::BrnModCallbacks,
        ) -> i32 {
            $crate::__private::init::<$ty>(host, callbacks, $new)
        }
    };
}

#[doc(hidden)]
pub mod __private {
    use super::*;

    struct Cell<T> {
        state: Mutex<T>,
        failed: AtomicBool,
    }

    fn fail(what: &str) {
        let host = host();
        host.set_input_capture(0);
        host.log(&format!("panicked in {what}; the mod is off until the next start"));
    }

    fn guarded<T>(cell: &Cell<T>, what: &str, body: impl FnOnce(&mut T)) {
        if cell.failed.load(Ordering::Relaxed) {
            return;
        }
        let mut state = cell.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if catch_unwind(AssertUnwindSafe(|| body(&mut state))).is_err() {
            cell.failed.store(true, Ordering::Relaxed);
            drop(state);
            fail(what);
        }
    }

    unsafe extern "C" fn on_message<T: Mod>(user: *mut c_void, message: *const ffi::BrnWindowMessage) {
        let cell = &*(user as *const Cell<T>);
        guarded(cell, "on_message", |state| state.on_message(host(), &*message));
    }

    unsafe extern "C" fn on_frame<T: Mod>(user: *mut c_void, frame: *const ffi::BrnFrameContext) {
        let cell = &*(user as *const Cell<T>);
        guarded(cell, "on_frame", |state| state.on_frame(host(), &*frame));
    }

    unsafe extern "C" fn on_game_tick<T: Mod>(user: *mut c_void) {
        let cell = &*(user as *const Cell<T>);
        if cell.failed.load(Ordering::Relaxed) {
            return;
        }
        if catch_unwind(|| T::on_game_tick(host())).is_err() {
            cell.failed.store(true, Ordering::Relaxed);
            fail("on_game_tick");
        }
    }

    pub unsafe fn init<T: Mod>(
        host_api: *const ffi::BrnHostApi,
        callbacks: *mut ffi::BrnModCallbacks,
        new: impl FnOnce(&Host) -> T,
    ) -> i32 {
        let (Some(api), Some(callbacks)) = (host_api.as_ref(), callbacks.as_mut()) else { return 0 };
        if api.api_version != ffi::BRN_MOD_API_VERSION {
            return 0;
        }
        // The BrnHostApi lives only for this call: keep what it points at, not it.
        let get = api.get_service;
        let Some(core) = service::<ffi::BrnCoreV1>(get, c"brn.core.v1") else { return 0 };
        let host = Host {
            self_: api.self_,
            core,
            player: service(get, c"brn.player.v1").map(Player),
            vehicles: service(get, c"brn.vehicles.v1").map(Vehicles),
            locations: service(get, c"brn.locations.v1").map(Locations),
            race_cars: service(get, c"brn.racecars.v1").map(RaceCars),
            traffic: service(get, c"brn.traffic.v1").map(Traffic),
            debug_vars: service(get, c"brn.debugvars.v1").map(DebugVars),
        };
        let host = HOST.get_or_init(|| host);
        // Name the panic in BrnGame.log before catch_unwind swallows it.
        std::panic::set_hook(Box::new(|info| {
            let message = info
                .payload()
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_default();
            let place = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
            if let Some(host) = HOST.get() {
                host.log(&format!("panic at {place}: {message}"));
            }
        }));
        let Ok(state) = catch_unwind(AssertUnwindSafe(|| new(host))) else {
            host.log("panicked while starting");
            return 0;
        };
        let cell = Box::new(Cell { state: Mutex::new(state), failed: AtomicBool::new(false) });
        callbacks.user = Box::into_raw(cell) as *mut c_void;   // lives until the process exits
        callbacks.on_message = Some(on_message::<T>);
        callbacks.on_frame = Some(on_frame::<T>);
        callbacks.on_game_tick = Some(on_game_tick::<T>);
        callbacks.on_shutdown = None;
        1
    }
}
