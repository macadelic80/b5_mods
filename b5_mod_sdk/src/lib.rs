//! Write Burnout_PC.exe mods in Rust, as sandboxed WebAssembly.
//!
//! A mod is a `cdylib` built for `wasm32-unknown-unknown` that implements [`Mod`] and calls
//! [`export_mod!`] once. The game loads `mods/<folder>/<name>.wasm` (see `mod.ini`) and calls it:
//!   * [`Mod::on_input`] then [`Mod::on_frame`] on the render thread;
//!   * [`Mod::on_game_tick`] on the update thread.
//! The host never runs two of them at once, so they all get `&mut self`.
//! Game state comes from the services on [`Host`]; every getter returns a copy made at the last
//! game tick and every command is applied at the next one. The mod sees nothing but its own
//! memory and these calls: no file system (use [`Storage`]), no network, no game pointer.
//! A panic is logged to BrnGame.log and traps: the host switches the mod off, the game keeps
//! running. Switching it back on from the mods list reloads the .wasm.

pub mod ffi;
#[cfg(feature = "egui")]
pub mod overlay;

use std::cell::UnsafeCell;
use std::sync::OnceLock;

pub use ffi::{
    brn_input_event as InputEvent, BrnLocation as Location, BrnModInfo, BrnPlayerState as PlayerState,
    BrnRaceCarInfo as RaceCarInfo, BrnTrafficSettings as TrafficSettings, BrnVec3 as Vec3, BrnVehicleInfo as VehicleInfo,
    BRN_CAPTURE_GAMEPAD, BRN_CAPTURE_KEYBOARD, BRN_CAPTURE_MOUSE,
};

/// The NUL-terminated text of a fixed `char[N]` field.
pub fn text(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// One frame, as `brn_mod_render` receives it.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    /// Back buffer size in pixels: the coordinate space of the UI and of mouse events.
    pub width: u32,
    pub height: u32,
    /// The window's DPI / 96.
    pub dpi_scale: f32,
    pub delta_seconds: f32,
    pub frame_index: u64,
}

fn has_module(name: &str) -> bool {
    unsafe { ffi::core_has_module(name.as_ptr(), name.len() as u32) != 0 }
}

/// What the game offers this mod. `None` when this build lacks a service.
pub struct Host {
    pub player: Option<Player>,
    pub vehicles: Option<Vehicles>,
    pub locations: Option<Locations>,
    pub race_cars: Option<RaceCars>,
    pub traffic: Option<Traffic>,
    pub debug_vars: Option<DebugVars>,
    pub debug_ini: Option<DebugIni>,
    pub storage: Option<Storage>,
}

static HOST: OnceLock<Host> = OnceLock::new();

/// The host (valid from `brn_mod_init` on).
pub fn host() -> &'static Host {
    HOST.get_or_init(|| Host {
        player: has_module("brn.player.v1").then_some(Player(())),
        vehicles: has_module("brn.vehicles.v1").then_some(Vehicles(())),
        locations: has_module("brn.locations.v1").then_some(Locations(())),
        race_cars: has_module("brn.racecars.v1").then_some(RaceCars(())),
        traffic: has_module("brn.traffic.v1").then_some(Traffic(())),
        debug_vars: has_module("brn.debugvars.v1").then_some(DebugVars(())),
        debug_ini: has_module("brn.debugini.v1").then_some(DebugIni(())),
        storage: has_module("brn.storage.v1").then_some(Storage(())),
    })
}

impl Host {
    pub fn log(&self, message: &str) {
        unsafe { ffi::core_log(message.as_ptr(), message.len() as u32) }
    }

    /// Seconds since the game started (std::time::Instant is not available in the sandbox).
    pub fn time(&self) -> f64 {
        unsafe { ffi::core_time() }
    }

    /// `BRN_CAPTURE_*`: while set, the game stops reading that input (your menu is open).
    pub fn set_input_capture(&self, flags: u32) {
        unsafe { ffi::core_set_input_capture(flags) }
    }

    pub fn mods(&self) -> Vec<BrnModInfo> {
        let count = unsafe { ffi::core_mod_count() };
        (0..count)
            .filter_map(|i| {
                let mut info: BrnModInfo = unsafe { std::mem::zeroed() };
                (unsafe { ffi::core_get_mod_info(i, &mut info) } != 0).then_some(info)
            })
            .collect()
    }

    /// Saved in mods/mods.ini, applied before the next frame. Switching a WebAssembly mod on
    /// (re)loads its .wasm; a native mod that is not loaded starts with the next run.
    pub fn set_mod_enabled(&self, index: usize, enabled: bool) {
        unsafe { ffi::core_set_mod_enabled(index as i32, enabled as i32) }
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
pub struct Player(());

impl Player {
    pub fn state(&self) -> Option<PlayerState> {
        let mut state: PlayerState = unsafe { std::mem::zeroed() };
        (unsafe { ffi::player_get_state(&mut state) } != 0).then_some(state)
    }
    /// Same car, new pose. `Vec3::default()` as direction keeps the heading.
    pub fn teleport(&self, p: Vec3, d: Vec3) { unsafe { ffi::player_teleport(p.x, p.y, p.z, d.x, d.y, d.z) } }
    pub fn reset_on_track(&self, speed_mph: f32) { unsafe { ffi::player_reset_on_track(speed_mph) } }
    pub fn place_on_track(&self, p: Vec3, d: Vec3, speed_mph: f32) {
        unsafe { ffi::player_place_on_track(p.x, p.y, p.z, d.x, d.y, d.z, speed_mph) }
    }
    pub fn set_infinite_boost(&self, enabled: bool) { unsafe { ffi::player_set_infinite_boost(enabled as i32) } }
    pub fn set_invulnerable(&self, enabled: bool) { unsafe { ffi::player_set_invulnerable(enabled as i32) } }
    /// `wheel_id` 0 = the car's default wheels. False while an event is running.
    pub fn change_car(&self, car_id: u64, wheel_id: u64) -> bool { unsafe { ffi::player_change_car(car_id, wheel_id) != 0 } }
}

/// `brn.vehicles.v1`: the game's vehicle list.
#[derive(Clone, Copy)]
pub struct Vehicles(());

impl Vehicles {
    pub fn all(&self) -> Vec<VehicleInfo> {
        let count = unsafe { ffi::vehicles_count() };
        (0..count)
            .filter_map(|i| {
                let mut info: VehicleInfo = unsafe { std::mem::zeroed() };
                (unsafe { ffi::vehicles_get(i, &mut info) } != 0).then_some(info)
            })
            .collect()
    }
}

/// `brn.locations.v1`: junkyards, gas stations, shops and car parks.
#[derive(Clone, Copy)]
pub struct Locations(());

impl Locations {
    pub fn all(&self) -> Vec<Location> {
        let count = unsafe { ffi::locations_count() };
        (0..count)
            .filter_map(|i| {
                let mut info: Location = unsafe { std::mem::zeroed() };
                (unsafe { ffi::locations_get(i, &mut info) } != 0).then_some(info)
            })
            .collect()
    }
}

/// `brn.racecars.v1`: the active race car slots.
#[derive(Clone, Copy)]
pub struct RaceCars(());

impl RaceCars {
    /// One entry per slot; `None` for an empty slot.
    pub fn slots(&self) -> Vec<Option<RaceCarInfo>> {
        let count = unsafe { ffi::racecars_count() };
        (0..count)
            .map(|i| {
                let mut info: RaceCarInfo = unsafe { std::mem::zeroed() };
                (unsafe { ffi::racecars_get(i, &mut info) } != 0).then_some(info)
            })
            .collect()
    }
    pub fn remove_all_rivals(&self) { unsafe { ffi::racecars_remove_all_rivals() } }
}

/// `brn.traffic.v1`: the traffic module's debug switches.
#[derive(Clone, Copy)]
pub struct Traffic(());

impl Traffic {
    pub fn get(&self) -> Option<TrafficSettings> {
        let mut settings = TrafficSettings::default();
        (unsafe { ffi::traffic_get(&mut settings) } != 0).then_some(settings)
    }
    pub fn set(&self, settings: &TrafficSettings) { unsafe { ffi::traffic_set(settings) } }
    pub fn kill_near_player(&self) { unsafe { ffi::traffic_kill_near_player() } }
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
pub struct DebugVars(());

impl DebugVars {
    /// Call every frame while you show the list; returns its generation (0 = not built yet).
    pub fn refresh(&self) -> u32 { unsafe { ffi::debugvars_refresh() } }

    pub fn all(&self) -> Vec<DebugVar> {
        let count = unsafe { ffi::debugvars_count() };
        (0..count)
            .filter_map(|index| {
                let mut raw: ffi::BrnDebugVar = unsafe { std::mem::zeroed() };
                if unsafe { ffi::debugvars_get(index, &mut raw) } == 0 {
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
        let count = unsafe { ffi::debugvars_option_count(index) };
        (0..count)
            .filter_map(|option| {
                let mut name = [0u8; 96];
                let mut value = 0;
                (unsafe { ffi::debugvars_get_option(index, option, name.as_mut_ptr(), name.len() as i32, &mut value) } != 0)
                    .then(|| (text(&name), value))
            })
            .collect()
    }

    /// As typed in the game's console: "true"/"false" (a bool is true only for "TRUE"), "12", "0.5",
    /// an option name. Applied at the next tick.
    pub fn set(&self, path: &str, value: &str) {
        unsafe { ffi::debugvars_set(path.as_ptr(), path.len() as u32, value.as_ptr(), value.len() as u32) }
    }

    /// Run an action, or open a component section (its controls then appear in the list).
    pub fn call(&self, path: &str) {
        unsafe { ffi::debugvars_call(path.as_ptr(), path.len() as u32) }
    }
}

/// `brn.debugini.v1`: config.ini [Debug], which the game applies at start-up -- the only part of
/// config.ini a mod can write.
#[derive(Clone, Copy)]
pub struct DebugIni(());

impl DebugIni {
    /// `path=value` in [Debug]; an empty value removes the line. False when refused.
    pub fn set(&self, path: &str, value: &str) -> bool {
        unsafe { ffi::debugini_set(path.as_ptr(), path.len() as u32, value.as_ptr(), value.len() as u32) != 0 }
    }
}

/// `brn.storage.v1`: this mod's own files, `mods/<folder>/data/<file>`. A file name is 1..64
/// characters of `[A-Za-z0-9._-]`, not starting with '.'; 16 MB a file.
#[derive(Clone, Copy)]
pub struct Storage(());

impl Storage {
    pub fn read(&self, file: &str) -> Option<Vec<u8>> {
        let size = unsafe { ffi::storage_read(file.as_ptr(), file.len() as u32, std::ptr::null_mut(), 0) };
        if size < 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        let read = unsafe { ffi::storage_read(file.as_ptr(), file.len() as u32, data.as_mut_ptr(), data.len() as u32) };
        (read == size).then_some(data)
    }
    pub fn read_string(&self, file: &str) -> Option<String> {
        self.read(file).map(|data| String::from_utf8_lossy(&data).into_owned())
    }
    pub fn write(&self, file: &str, data: &[u8]) -> bool {
        unsafe { ffi::storage_write(file.as_ptr(), file.len() as u32, data.as_ptr(), data.len() as u32) != 0 }
    }
    pub fn remove(&self, file: &str) -> bool {
        unsafe { ffi::storage_remove(file.as_ptr(), file.len() as u32) != 0 }
    }
}

/// Copy `text` to the Windows clipboard (write only: a mod cannot read it).
pub fn set_clipboard_text(text: &str) {
    unsafe { ffi::ui_set_clipboard_text(text.as_ptr(), text.len() as u32) }
}

// ---- the mod -----------------------------------------------------------------------------

pub trait Mod: 'static {
    /// Render thread, before `on_frame`, once per input event.
    fn on_input(&mut self, _host: &Host, _event: &InputEvent) {}
    /// Render thread. Submit the UI here ([`overlay`] does it for egui).
    fn on_frame(&mut self, _host: &Host, _frame: &Frame) {}
    /// Update thread, once per game update.
    fn on_game_tick(&mut self, _host: &Host) {}
    /// Switched off from the mods list (not after a trap): save what you need.
    fn on_shutdown(&mut self, _host: &Host) {}
}

/// Exports the `brn_mod_*` functions for `$ty`, built by `$new(&Host)`.
#[macro_export]
macro_rules! export_mod {
    ($ty:ty, $new:expr) => {
        static __BRN_MOD: $crate::__private::Slot<$ty> = $crate::__private::Slot::new();

        #[no_mangle]
        pub extern "C" fn brn_mod_api_version() -> u32 {
            $crate::ffi::BRN_MOD_API_VERSION
        }
        #[no_mangle]
        pub extern "C" fn brn_mod_init() -> i32 {
            $crate::__private::init(&__BRN_MOD, $new)
        }
        #[no_mangle]
        pub extern "C" fn brn_mod_tick() {
            __BRN_MOD.with(|m| $crate::Mod::on_game_tick(m, $crate::host()));
        }
        #[no_mangle]
        pub extern "C" fn brn_mod_render(width: u32, height: u32, dpi_scale: f32, delta_seconds: f32, frame_index: u64) {
            let frame = $crate::Frame { width, height, dpi_scale, delta_seconds, frame_index };
            __BRN_MOD.with(|m| $crate::Mod::on_frame(m, $crate::host(), &frame));
        }
        #[no_mangle]
        pub extern "C" fn brn_mod_input_buffer() -> *mut $crate::InputEvent {
            $crate::__private::input_buffer()
        }
        #[no_mangle]
        pub extern "C" fn brn_mod_on_input(count: u32) {
            __BRN_MOD.with(|m| $crate::__private::deliver_input(m, count));
        }
        #[no_mangle]
        pub extern "C" fn brn_mod_shutdown() {
            __BRN_MOD.with(|m| $crate::Mod::on_shutdown(m, $crate::host()));
        }
    };
}

#[doc(hidden)]
pub mod __private {
    use super::*;

    /// The mod's state. A wasm instance runs one call at a time on one stack, so a plain cell is
    /// enough; `Sync` only satisfies `static`.
    pub struct Slot<T>(UnsafeCell<Option<T>>);
    unsafe impl<T> Sync for Slot<T> {}

    impl<T> Slot<T> {
        pub const fn new() -> Self {
            Self(UnsafeCell::new(None))
        }
        pub fn with(&self, body: impl FnOnce(&mut T)) {
            // SAFETY: the host never calls two exports of one instance at the same time, and no
            // export calls another, so this is the only live reference.
            if let Some(state) = unsafe { &mut *self.0.get() } {
                body(state);
            }
        }
    }

    struct InputBuffer(UnsafeCell<[InputEvent; ffi::BRN_INPUT_BUFFER_CAPACITY]>);
    unsafe impl Sync for InputBuffer {}
    static INPUT: InputBuffer = InputBuffer(UnsafeCell::new(
        [InputEvent { type_: 0, modifiers: 0, code: 0, pressed: 0, repeat: 0, x: 0.0, y: 0.0 }; ffi::BRN_INPUT_BUFFER_CAPACITY],
    ));

    pub fn input_buffer() -> *mut InputEvent {
        INPUT.0.get() as *mut InputEvent
    }

    pub fn deliver_input<T: Mod>(state: &mut T, count: u32) {
        let events = unsafe { &*INPUT.0.get() };
        for event in &events[..(count as usize).min(events.len())] {
            state.on_input(host(), event);
        }
    }

    pub fn init<T: Mod>(slot: &Slot<T>, new: impl FnOnce(&Host) -> T) -> i32 {
        // Name the panic in BrnGame.log; the abort that follows traps and switches the mod off.
        std::panic::set_hook(Box::new(|info| {
            let message = info
                .payload()
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_default();
            let place = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
            host().log(&format!("panic at {place}: {message}"));
        }));
        let state = new(host());
        // SAFETY: init runs once, before any other export.
        unsafe { *slot.0.get() = Some(state) };
        1
    }
}
