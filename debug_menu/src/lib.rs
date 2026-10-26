//! Example mod: the debug menu we use to test the decomp in game.
//!
//! Insert opens / closes it (Escape closes it too). While it is open the game ignores the
//! keyboard and the mouse. The speedometer stays on screen when the menu is closed.

use b5_mod_sdk::overlay::Overlay;
mod debug_window;

use b5_mod_sdk::{
    export_mod, host, Frame, Host, Location, Mod, RaceCarInfo, TrafficSettings, Vec3, VehicleInfo, WindowMessage,
    BRN_CAPTURE_KEYBOARD, BRN_CAPTURE_MOUSE,
};
use debug_window::DebugWindow;
use egui::{Align2, Color32, Context, RichText};
use std::cell::Cell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

const WM_KEYDOWN: u32 = 0x0100;
const VK_ESCAPE: u64 = 0x1B;
const VK_INSERT: u64 = 0x2D;
const MPH_TO_KMH: f32 = 1.609_344;

// The original debug menu's car filters (ResetPlayerDebugComponent: label, CgsID prefix).
const CAR_GROUPS: [(&str, &str); 10] = [
    ("All cars", ""),
    ("Race cars", "P"),
    ("Traffic cars", "T"),
    ("US race cars", "PUS"),
    ("Euro race cars", "PEU"),
    ("Asian race cars", "PAS"),
    ("Special race cars", "PSP"),
    ("US Trophy race cars", "XUS"),
    ("Euro Trophy race cars", "XEU"),
    ("Asian Trophy race cars", "XAS"),
];

struct DebugMenu {
    overlay: Overlay<App>,
    open: Rc<Cell<bool>>,   // shared with the UI, whose close button clears it
    capturing: bool,        // what the host was last told
}

impl DebugMenu {
    fn new(host: &Host) -> Self {
        host.log("debug menu ready -- Insert to open");
        let open = Rc::new(Cell::new(false));
        let app = App::new(open.clone(), host.mod_directory());
        Self { overlay: Overlay::new(App::ui, app), open, capturing: false }
    }

    fn set_open(&mut self, host: &Host, open: bool) {
        self.open.set(open);
        self.capturing = open;
        self.overlay.set_cursor_visible(open);
        host.set_input_capture(if open { BRN_CAPTURE_KEYBOARD | BRN_CAPTURE_MOUSE } else { 0 });
    }
}

impl Mod for DebugMenu {
    fn on_message(&mut self, host: &Host, message: &WindowMessage) {
        let first_press = message.lparam & (1 << 30) == 0;
        if message.msg == WM_KEYDOWN && first_press {
            if message.wparam == VK_INSERT {
                let open = !self.open.get();
                self.set_open(host, open);
                return;
            }
            if message.wparam == VK_ESCAPE && self.open.get() {
                self.set_open(host, false);
                return;
            }
        }
        self.overlay.message(message);
    }

    fn on_frame(&mut self, host: &Host, frame: &Frame) {
        self.overlay.frame(frame);
        if frame.frame_index % 600 == 300 {
            host.log(&self.overlay.diagnostics());
        }
        if self.open.get() != self.capturing {
            let open = self.open.get();
            self.set_open(host, open);   // the window's close button was used
        }
    }
}

export_mod!(DebugMenu, DebugMenu::new);

// ---- the UI ----------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Vehicle,
    Teleport,
    Motion,
    Stats,
    Traffic,
    RaceCars,
    Mods,
}

struct SavedPosition {
    name: String,
    position: Vec3,
    direction: Vec3,
}

struct App {
    open: Rc<Cell<bool>>,
    tab: Tab,
    speedo: bool,
    infinite_boost: bool,
    invulnerable: bool,
    car_filter: String,
    car_group: usize,
    show_versions: bool,
    vehicles: Vec<VehicleInfo>,
    locations: Vec<Location>,
    location_filter: String,
    target: Vec3,
    saved: Vec<SavedPosition>,
    saved_name: String,
    saved_file: PathBuf,
    launch_mph: f32,
    status: String,
    debug_window: DebugWindow,
}

impl App {
    fn new(open: Rc<Cell<bool>>, directory: PathBuf) -> Self {
        let saved_file = directory.join("positions.txt");
        let debug_window = DebugWindow::new(directory.clone());
        Self {
            open,
            tab: Tab::Vehicle,
            speedo: true,
            infinite_boost: false,
            invulnerable: false,
            car_filter: String::new(),
            car_group: 1,   // race cars: traffic cars lack engine audio data in ARTIST and assert
            show_versions: false,
            vehicles: Vec::new(),
            locations: Vec::new(),
            location_filter: String::new(),
            target: Vec3::default(),
            saved: load_positions(&saved_file),
            saved_name: String::new(),
            saved_file,
            launch_mph: 100.0,
            status: String::new(),
            debug_window,
        }
    }

    fn ui(ctx: &Context, app: &mut App) {
        let host = host();
        let player = host.player.and_then(|p| p.state());
        if app.speedo {
            speedometer(ctx, player.as_ref());
        }
        if !app.open.get() {
            return;
        }
        let mut open = true;
        egui::Window::new("B5 debug menu")
            .open(&mut open)
            .default_size([460.0, 520.0])
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (tab, label) in [
                        (Tab::Vehicle, "Vehicle"),
                        (Tab::Teleport, "Teleport"),
                        (Tab::Motion, "Motion"),
                        (Tab::Stats, "Stats"),
                        (Tab::Traffic, "Traffic"),
                        (Tab::RaceCars, "Race cars"),
                        (Tab::Mods, "Mods"),
                    ] {
                        ui.selectable_value(&mut app.tab, tab, label);
                    }
                    ui.separator();
                    if host.debug_vars.is_some() {
                        ui.toggle_value(&mut app.debug_window.open, "Debug menu")
                            .on_hover_text("The game's own debug menu (Ctrl+Space), in its own window");
                    }
                });
                ui.separator();
                match app.tab {
                    Tab::Vehicle => app.vehicle_tab(ui, host, player.as_ref()),
                    Tab::Teleport => app.teleport_tab(ui, host, player.as_ref()),
                    Tab::Motion => app.motion_tab(ui, host, player.as_ref()),
                    Tab::Stats => stats_tab(ui, player.as_ref()),
                    Tab::Traffic => traffic_tab(ui, host),
                    Tab::RaceCars => app.race_cars_tab(ui, host, player.as_ref()),
                    Tab::Mods => mods_tab(ui, host),
                }
                if !app.status.is_empty() {
                    ui.separator();
                    ui.label(RichText::new(&app.status).weak());
                }
            });
        if !open {
            app.open.set(false);
        }
        app.debug_window.show(ctx, host);
    }

    fn vehicle_tab(&mut self, ui: &mut egui::Ui, host: &Host, player: Option<&b5_mod_sdk::PlayerState>) {
        let Some(service) = host.player else { return no_service(ui, "brn.player.v1") };
        if ui.checkbox(&mut self.infinite_boost, "Infinite boost").changed() {
            service.set_infinite_boost(self.infinite_boost);
        }
        if ui.checkbox(&mut self.invulnerable, "Invulnerable (no crashes)").changed() {
            service.set_invulnerable(self.invulnerable);
        }
        ui.checkbox(&mut self.speedo, "Speedometer");
        ui.separator();

        if let Some(state) = player {
            ui.label(format!("Current car: {} ({})", state.car_name(), state.car_id_text()));
            if state.in_game_mode != 0 {
                ui.colored_label(Color32::YELLOW, "An event is running: the game refuses car changes.");
            }
        }
        if self.vehicles.is_empty() {
            if let Some(list) = host.vehicles {
                self.vehicles = list.all();
            }
        }
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("car_group")
                .selected_text(CAR_GROUPS[self.car_group].0)
                .show_ui(ui, |ui| {
                    for (index, (label, _)) in CAR_GROUPS.iter().enumerate() {
                        ui.selectable_value(&mut self.car_group, index, *label);
                    }
                });
            ui.label("Search");
            ui.text_edit_singleline(&mut self.car_filter);
            ui.checkbox(&mut self.show_versions, "Versions");
        });
        let filter = self.car_filter.to_lowercase();
        let prefix = CAR_GROUPS[self.car_group].1;
        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
            for vehicle in &self.vehicles {
                if (vehicle.parent_id != 0 && !self.show_versions) || !vehicle.id_text().starts_with(prefix) {
                    continue;
                }
                let label = format!("{}  ({})", vehicle.name(), vehicle.id_text());
                if !filter.is_empty() && !label.to_lowercase().contains(&filter) {
                    continue;
                }
                if ui.button(label).clicked() {
                    self.status = if service.change_car(vehicle.id, 0) {
                        format!("Changing to {}", vehicle.name())
                    } else {
                        "Car change refused (an event is running)".into()
                    };
                }
            }
        });
    }

    fn teleport_tab(&mut self, ui: &mut egui::Ui, host: &Host, player: Option<&b5_mod_sdk::PlayerState>) {
        let Some(service) = host.player else { return no_service(ui, "brn.player.v1") };
        if let Some(state) = player {
            let p = state.position;
            ui.horizontal(|ui| {
                ui.label(format!("Position  {:.1}  {:.1}  {:.1}", p.x, p.y, p.z));
                if ui.small_button("Copy").clicked() {
                    ui.ctx().copy_text(format!("{:.2} {:.2} {:.2}", p.x, p.y, p.z));
                }
                if ui.small_button("Use as target").clicked() {
                    self.target = p;
                }
            });
        }
        ui.horizontal(|ui| {
            ui.label("Target");
            ui.add(egui::DragValue::new(&mut self.target.x).speed(1.0).prefix("x "));
            ui.add(egui::DragValue::new(&mut self.target.y).speed(1.0).prefix("y "));
            ui.add(egui::DragValue::new(&mut self.target.z).speed(1.0).prefix("z "));
            if ui.button("Go").clicked() {
                service.teleport(self.target, Vec3::default());
            }
        });

        ui.separator();
        ui.label("Saved positions");
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.saved_name);
            if ui.button("Save here").clicked() {
                if let Some(state) = player {
                    let name = if self.saved_name.trim().is_empty() {
                        format!("Position {}", self.saved.len() + 1)
                    } else {
                        self.saved_name.trim().replace('|', "/")
                    };
                    self.saved.push(SavedPosition { name, position: state.position, direction: state.direction });
                    self.saved_name.clear();
                    save_positions(&self.saved_file, &self.saved);
                }
            }
        });
        let mut remove = None;
        for (index, saved) in self.saved.iter().enumerate() {
            ui.horizontal(|ui| {
                if ui.button("Go").clicked() {
                    service.teleport(saved.position, saved.direction);
                }
                if ui.small_button("x").clicked() {
                    remove = Some(index);
                }
                ui.label(&saved.name);
            });
        }
        if let Some(index) = remove {
            self.saved.remove(index);
            save_positions(&self.saved_file, &self.saved);
        }

        ui.separator();
        if self.locations.is_empty() {
            if let Some(list) = host.locations {
                self.locations = list.all();
            }
        }
        ui.horizontal(|ui| {
            ui.label(format!("Places ({})", self.locations.len()));
            ui.text_edit_singleline(&mut self.location_filter);
        });
        let filter = self.location_filter.to_lowercase();
        egui::ScrollArea::vertical().id_salt("places").max_height(220.0).show(ui, |ui| {
            for location in &self.locations {
                let label = location.label();
                if !filter.is_empty() && !label.to_lowercase().contains(&filter) {
                    continue;
                }
                if ui.button(label).clicked() {
                    service.teleport(location.position, location.direction);
                }
            }
        });
    }

    fn motion_tab(&mut self, ui: &mut egui::Ui, host: &Host, player: Option<&b5_mod_sdk::PlayerState>) {
        let Some(service) = host.player else { return no_service(ui, "brn.player.v1") };
        let Some(state) = player else {
            ui.label("No player car yet.");
            return;
        };
        let v = state.velocity;
        ui.label(format!("Velocity  {:.1}  {:.1}  {:.1} m/s", v.x, v.y, v.z));
        ui.add(egui::Slider::new(&mut self.launch_mph, 0.0..=250.0).text("mph"));
        ui.horizontal(|ui| {
            if ui.button("Launch forward").clicked() {
                service.place_on_track(state.position, state.direction, self.launch_mph);
            }
            if ui.button("Stop").clicked() {
                service.place_on_track(state.position, state.direction, 0.0);
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Reset on track").clicked() {
                service.reset_on_track(40.0);
            }
            if ui.button("Re-seat here").clicked() {
                service.teleport(state.position, state.direction);
            }
        });
        ui.label(RichText::new("Launch / Stop drop the car on the road at its position, moving forward.").weak());
    }

    fn race_cars_tab(&mut self, ui: &mut egui::Ui, host: &Host, player: Option<&b5_mod_sdk::PlayerState>) {
        let Some(cars) = host.race_cars else { return no_service(ui, "brn.racecars.v1") };
        let here = player.map(|p| p.position);
        egui::Grid::new("race_cars").striped(true).show(ui, |ui| {
            ui.strong("Slot");
            ui.strong("Car");
            ui.strong("mph");
            ui.strong("Distance");
            ui.end_row();
            for (slot, car) in cars.slots().into_iter().enumerate() {
                let Some(car) = car else { continue };
                ui.label(format!("{slot}{}", if car.is_player != 0 { " (you)" } else { "" }));
                ui.label(car.car_id_text());
                ui.label(format!("{:.0}", car.speed_mph));
                ui.label(here.map(|h| format!("{:.0} m", distance(h, car.position))).unwrap_or_default());
                if car.is_player == 0 && ui.small_button("Go").clicked() {
                    go_behind(host, &car);
                }
                ui.end_row();
            }
        });
        if ui.button("Remove all rivals").clicked() {
            cars.remove_all_rivals();
        }
    }
}

fn speedometer(ctx: &Context, player: Option<&b5_mod_sdk::PlayerState>) {
    let Some(state) = player else { return };
    egui::Area::new(egui::Id::new("speedo"))
        .anchor(Align2::RIGHT_BOTTOM, [-24.0, -24.0])
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.label(RichText::new(format!("{:.0} mph", state.speed_mph)).size(26.0).strong());
                ui.label(format!(
                    "{:.0} km/h   gear {}   {:.0} rpm",
                    state.speed_mph * MPH_TO_KMH,
                    state.gear,
                    state.rpm
                ));
            });
        });
}

fn stats_tab(ui: &mut egui::Ui, player: Option<&b5_mod_sdk::PlayerState>) {
    let Some(s) = player else {
        ui.label("No player car yet.");
        return;
    };
    egui::Grid::new("stats").striped(true).show(ui, |ui| {
        let mut row = |name: &str, value: String| {
            ui.label(name);
            ui.label(value);
            ui.end_row();
        };
        row("Car", format!("{} ({}), wheel {:016X}", s.car_name(), s.car_id_text(), s.wheel_id));
        row("Speed", format!("{:.1} mph / {:.1} km/h", s.speed_mph, s.speed_mph * MPH_TO_KMH));
        row("Top speed", format!("{:.0} mph, boosting {:.0} mph", s.max_speed_mph, s.max_boost_speed_mph));
        row("Engine", format!("gear {}, {:.0} rpm", s.gear, s.rpm));
        row("Time in air", format!("{:.2} s", s.time_in_air));
        row("Drifting", format!("{:.2} s", s.time_drifting));
        row("Boosting", format!("{:.2} s", s.time_boosting));
        row("Crashing", format!("{} ({:.2} s)", yes(s.crashing), s.time_crashing));
        row("Driveable", yes(s.driveable).into());
        row("Event running", format!("{} (mode type {})", yes(s.in_game_mode), s.game_mode_type));
        row("Simulation paused", yes(s.sim_paused).into());
    });
}

fn traffic_tab(ui: &mut egui::Ui, host: &Host) {
    let Some(traffic) = host.traffic else { return no_service(ui, "brn.traffic.v1") };
    let Some(mut settings) = traffic.get() else {
        ui.label("No traffic yet.");
        return;
    };
    let before: TrafficSettings = settings;
    let flag = |ui: &mut egui::Ui, value: &mut i32, label: &str| {
        let mut on = *value != 0;
        ui.checkbox(&mut on, label);
        *value = on as i32;
    };
    flag(ui, &mut settings.turn_off, "Turn traffic off");
    flag(ui, &mut settings.stop_moving, "Stop traffic moving");
    // "Allow ANARCHY!" is left out: ARTIST only initialises that switch, nothing reads it.
    flag(ui, &mut settings.enable_avoidance, "Avoidance");
    ui.add(egui::Slider::new(&mut settings.speed_multiplier, 0.0..=50.0).text("Speed multiplier"));
    ui.add(egui::Slider::new(&mut settings.density_scale, 0.0..=16.0).text("Density scale"));
    ui.add(egui::Slider::new(&mut settings.big_vehicle_amount, 0..=100).text("Big vehicles"));
    ui.label(format!("Final density: {:.2}", settings.final_density));
    if ui.button("Kill traffic near me").clicked() {
        traffic.kill_near_player();
    }
    let changed = settings.turn_off != before.turn_off
        || settings.stop_moving != before.stop_moving
        || settings.allow_anarchy != before.allow_anarchy
        || settings.enable_avoidance != before.enable_avoidance
        || settings.speed_multiplier != before.speed_multiplier
        || settings.density_scale != before.density_scale
        || settings.big_vehicle_amount != before.big_vehicle_amount;
    if changed {
        traffic.set(&settings);
    }
}

fn mods_tab(ui: &mut egui::Ui, host: &Host) {
    for (index, info) in host.mods().iter().enumerate() {
        ui.horizontal(|ui| {
            let mut enabled = info.enabled != 0;
            let label = format!("{} {}", info.name(), info.version());
            if ui.checkbox(&mut enabled, label).changed() {
                host.set_mod_enabled(index, enabled);
            }
            if info.loaded == 0 {
                ui.colored_label(Color32::LIGHT_RED, info.error());
            }
        });
    }
    ui.label(RichText::new("A mod switched on here that was not loaded starts with the next run.").weak());
}

fn no_service(ui: &mut egui::Ui, name: &str) {
    ui.colored_label(Color32::LIGHT_RED, format!("This build has no {name}."));
}

fn yes(value: i32) -> &'static str {
    if value != 0 { "yes" } else { "no" }
}

fn distance(a: Vec3, b: Vec3) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt()
}

// Land 10 m behind the car, facing the same way.
fn go_behind(host: &Host, car: &RaceCarInfo) {
    let Some(player) = host.player else { return };
    let d = car.direction;
    let target = Vec3 { x: car.position.x - d.x * 10.0, y: car.position.y, z: car.position.z - d.z * 10.0 };
    player.teleport(target, d);
}

// ---- positions.txt: "name|x|y|z|dx|dy|dz" per line ----------------------------------------

fn load_positions(path: &PathBuf) -> Vec<SavedPosition> {
    let Ok(text) = fs::read_to_string(path) else { return Vec::new() };
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split('|');
            let name = parts.next()?.to_string();
            let numbers: Vec<f32> = parts.filter_map(|p| p.trim().parse().ok()).collect();
            let [x, y, z, dx, dy, dz] = numbers.as_slice() else { return None };
            Some(SavedPosition {
                name,
                position: Vec3 { x: *x, y: *y, z: *z },
                direction: Vec3 { x: *dx, y: *dy, z: *dz },
            })
        })
        .collect()
}

fn save_positions(path: &PathBuf, saved: &[SavedPosition]) {
    let text: String = saved
        .iter()
        .map(|s| {
            let (p, d) = (s.position, s.direction);
            format!("{}|{}|{}|{}|{}|{}|{}\n", s.name, p.x, p.y, p.z, d.x, d.y, d.z)
        })
        .collect();
    if let Err(error) = fs::write(path, text) {
        host().log(&format!("could not save {}: {error}", path.display()));
    }
}
