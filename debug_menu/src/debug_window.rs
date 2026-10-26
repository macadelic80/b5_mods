//! The "Debug menu" window: the game's own CgsDev debug menu (Ctrl+Space) as an egui tree.
//!
//! * Expanding a folder opens the component sections inside it, so their controls appear
//!   (the host marks sections apart from actions: opening one never runs anything else).
//! * Favourites (the star) are pinned on top and saved in favourites.txt next to the mod.
//! * Reset puts a variable back to the value it had when this window first saw it.
//! * Export writes every changed variable to config.ini [Debug], which the game applies at
//!   start-up (pc/debug/DebugIni.h), so a test set-up survives a restart.

use b5_mod_sdk::{DebugVar, DebugVarKind, DebugVars, Host};
use egui::{Color32, RichText, Ui};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

pub struct DebugWindow {
    pub open: bool,
    vars: Vec<DebugVar>,
    generation: u32,
    search: String,
    opened_sections: HashSet<String>,             // sections this window already opened
    favourites: Vec<String>,
    favourites_file: PathBuf,
    originals: HashMap<String, (f64, String)>,    // first value seen, and its text for set()
    pending: HashMap<String, (f64, Instant)>,     // edits not yet seen back from the game
    config_file: PathBuf,
    status: String,
}

/// One folder level of the flat listing, rebuilt from the paths.
struct Node {
    index: usize,
    children: Vec<Node>,
}

impl DebugWindow {
    pub fn new(mod_directory: PathBuf) -> Self {
        let favourites_file = mod_directory.join("favourites.txt");
        let favourites = fs::read_to_string(&favourites_file)
            .map(|text| text.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect())
            .unwrap_or_default();
        // mods\<folder>\ -> the exe's folder, where config.ini lives.
        let config_file = mod_directory
            .parent()
            .and_then(|mods| mods.parent())
            .map(|game| game.join("config.ini"))
            .unwrap_or_else(|| PathBuf::from("config.ini"));
        Self {
            open: false,
            vars: Vec::new(),
            generation: 0,
            search: String::new(),
            opened_sections: HashSet::new(),
            favourites,
            favourites_file,
            originals: HashMap::new(),
            pending: HashMap::new(),
            config_file,
            status: String::new(),
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, host: &Host) {
        if !self.open {
            return;
        }
        let Some(service) = host.debug_vars else { return };
        let generation = service.refresh();
        if generation != self.generation {
            self.vars = service.all();
            self.generation = generation;
            for var in &self.vars {
                if is_value(var) && !self.originals.contains_key(&var.path) {
                    self.originals.insert(var.path.clone(), (var.value, value_text(var, var.value, &service)));
                }
            }
        }
        self.pending.retain(|_, (_, at)| at.elapsed().as_secs_f32() < 0.6);

        let mut open = self.open;
        egui::Window::new("Debug menu")
            .open(&mut open)
            .default_size([520.0, 640.0])
            .default_pos([520.0, 40.0])
            .show(ctx, |ui| {
                if generation == 0 {
                    ui.label("Waiting for the game's debug menu (it is listed once in game)...");
                    return;
                }
                self.toolbar(ui, service);
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    self.favourites_block(ui, service);
                    if self.search.trim().is_empty() {
                        let tree = build_tree(&self.vars);
                        for node in &tree {
                            self.node(ui, service, node);
                        }
                    } else {
                        self.search_results(ui, service);
                    }
                });
            });
        self.open = open;
    }

    fn toolbar(&mut self, ui: &mut Ui, service: DebugVars) {
        ui.horizontal(|ui| {
            ui.label("Search");
            ui.add(egui::TextEdit::singleline(&mut self.search).desired_width(200.0));
            if ui.button("Export to config.ini").on_hover_text("Changed values become config.ini [Debug] lines").clicked() {
                self.status = self.export(service);
            }
        });
        let changed = self.vars.iter().filter(|v| self.is_changed(v)).count();
        ui.label(RichText::new(format!("{} entries, {changed} changed. {}", self.vars.len(), self.status)).weak());
    }

    fn favourites_block(&mut self, ui: &mut Ui, service: DebugVars) {
        if self.favourites.is_empty() {
            return;
        }
        egui::CollapsingHeader::new(RichText::new("Favourites").strong())
            .default_open(true)
            .show(ui, |ui| {
                for path in self.favourites.clone() {
                    match self.vars.iter().position(|v| v.path == path) {
                        Some(index) => self.row(ui, service, index, true),
                        None => {
                            ui.horizontal(|ui| {
                                self.star(ui, &path);
                                ui.label(RichText::new(format!("{path}  (open its section)")).weak());
                            });
                        }
                    }
                }
            });
        ui.separator();
    }

    fn search_results(&mut self, ui: &mut Ui, service: DebugVars) {
        let needle = self.search.to_lowercase();
        let matches: Vec<usize> = (0..self.vars.len())
            .filter(|&i| self.vars[i].kind != DebugVarKind::Menu && self.vars[i].path.to_lowercase().contains(&needle))
            .take(400)
            .collect();
        if matches.is_empty() {
            ui.label("Nothing found. Controls of a closed section are not listed: expand its folder first.");
        }
        for index in matches {
            self.row(ui, service, index, true);
        }
    }

    fn node(&mut self, ui: &mut Ui, service: DebugVars, node: &Node) {
        let var = &self.vars[node.index];
        if var.kind != DebugVarKind::Menu {
            self.row(ui, service, node.index, false);
            return;
        }
        let path = var.path.clone();
        let response = egui::CollapsingHeader::new(&var.name).id_salt(&path).show(ui, |ui| {
            // Expanded: open the sections in this folder, once each, so their controls appear.
            for child in &node.children {
                let child_var = &self.vars[child.index];
                if child_var.kind == DebugVarKind::Section
                    && child_var.value == 0.0
                    && self.opened_sections.insert(child_var.path.clone())
                {
                    service.call(&child_var.path);
                }
            }
            for child in &node.children {
                self.node(ui, service, child);
            }
        });
        let _ = response;
    }

    fn row(&mut self, ui: &mut Ui, service: DebugVars, index: usize, full_path: bool) {
        let var = self.vars[index].clone();
        let label = if full_path { var.path.clone() } else { var.name.clone() };
        ui.horizontal(|ui| {
            if is_value(&var) {
                self.star(ui, &var.path);
            }
            let shown = self.pending.get(&var.path).map(|(v, _)| *v).unwrap_or(var.value);
            match var.kind {
                DebugVarKind::Menu => {}
                DebugVarKind::Section => {
                    if var.value != 0.0 {
                        ui.label(RichText::new(format!("{label} (open)")).weak());
                    } else if ui.button(format!("Open {label}")).clicked() {
                        self.opened_sections.insert(var.path.clone());
                        service.call(&var.path);
                    }
                }
                DebugVarKind::Function => {
                    if ui.button(&label).on_hover_text("Runs this action in the game").clicked() {
                        service.call(&var.path);
                        self.status = format!("Ran {}", var.path);
                    }
                }
                _ if var.read_only => {
                    ui.label(&label);
                    ui.label(RichText::new(&var.text).color(Color32::LIGHT_GRAY));
                }
                DebugVarKind::Bool => {
                    let mut on = shown != 0.0;
                    if ui.add(toggle(&mut on)).changed() {
                        // The game reads a bool as true only for "TRUE" (Variant::ConvertFromString).
                        self.send(service, &var, on as i32 as f64, if on { "true".into() } else { "false".into() });
                    }
                    ui.label(&label);
                }
                DebugVarKind::Choice => {
                    let options = service.options(var.index);
                    let current = options
                        .iter()
                        .find(|(_, v)| *v as f64 == shown)
                        .map(|(n, _)| n.clone())
                        .unwrap_or_else(|| var.text.clone());
                    egui::ComboBox::from_id_salt(&var.path).selected_text(current).show_ui(ui, |ui| {
                        for (name, value) in &options {
                            if ui.selectable_label(*value as f64 == shown, name).clicked() {
                                self.send(service, &var, *value as f64, name.clone());
                            }
                        }
                    });
                    ui.label(&label);
                }
                DebugVarKind::Int | DebugVarKind::UInt => {
                    let mut value = shown as i64;
                    let changed = match var.range {
                        Some((min, max)) if max - min <= 1000.0 => {
                            ui.add(egui::Slider::new(&mut value, min as i64..=max as i64)).changed()
                        }
                        range => {
                            let low = if var.kind == DebugVarKind::UInt { 0 } else { i64::from(i32::MIN) };
                            let (min, max) = range.map(|(a, b)| (a as i64, b as i64)).unwrap_or((low, i64::from(u32::MAX)));
                            ui.add(egui::DragValue::new(&mut value).speed(0.25).range(min..=max)).changed()
                        }
                    };
                    if changed {
                        self.send(service, &var, value as f64, value.to_string());
                    }
                    ui.label(&label);
                }
                DebugVarKind::Float => {
                    let mut value = shown;
                    let changed = match var.range {
                        Some((min, max)) => ui.add(egui::Slider::new(&mut value, min..=max)).changed(),
                        None => {
                            let speed = (value.abs() * 0.01).max(0.001);
                            ui.add(egui::DragValue::new(&mut value).speed(speed)).changed()
                        }
                    };
                    if changed {
                        self.send(service, &var, value, format!("{value}"));
                    }
                    ui.label(&label);
                }
            }
            if self.is_changed(&var) {
                if let Some((original, text)) = self.originals.get(&var.path).cloned() {
                    if ui.small_button("↺").on_hover_text(format!("Reset to {text}")).clicked() {
                        self.send(service, &var, original, text);
                    }
                }
            }
        });
    }

    fn star(&mut self, ui: &mut Ui, path: &str) {
        let on = self.favourites.iter().any(|f| f == path);
        let icon = if on { RichText::new("★").color(Color32::GOLD) } else { RichText::new("☆").weak() };
        if ui.add(egui::Button::new(icon).frame(false)).on_hover_text("Favourite").clicked() {
            if on {
                self.favourites.retain(|f| f != path);
            } else {
                self.favourites.push(path.to_string());
            }
            let _ = fs::write(&self.favourites_file, self.favourites.join("\r\n"));
        }
    }

    fn send(&mut self, service: DebugVars, var: &DebugVar, value: f64, text: String) {
        self.pending.insert(var.path.clone(), (value, Instant::now()));
        service.set(&var.path, &text);
    }

    fn is_changed(&self, var: &DebugVar) -> bool {
        is_value(var)
            && !var.read_only
            && self.originals.get(&var.path).is_some_and(|(original, _)| (original - var.value).abs() > 1e-6)
    }

    /// Changed values become `path=value` lines of config.ini [Debug]; values back at their
    /// original drop their line. Lines for variables this window has not seen are kept.
    fn export(&self, service: DebugVars) -> String {
        let mut wanted: HashMap<String, Option<String>> = HashMap::new();
        for var in self.vars.iter().filter(|v| is_value(v) && !v.read_only) {
            let key = ini_key(&var.path);
            if self.is_changed(var) {
                wanted.insert(key, Some(format!("{}={}", var.path, value_text(var, var.value, &service))));
            } else if self.originals.contains_key(&var.path) {
                wanted.insert(key, None);
            }
        }
        let text = fs::read_to_string(&self.config_file).unwrap_or_default();
        let mut lines: Vec<String> = text.lines().map(String::from).collect();
        let section = lines.iter().position(|l| l.trim().eq_ignore_ascii_case("[debug]"));
        let start = match section {
            Some(at) => at + 1,
            None => {
                lines.push(String::new());
                lines.push("[Debug]".into());
                lines.len()
            }
        };
        let end = lines[start..].iter().position(|l| l.trim_start().starts_with('[')).map_or(lines.len(), |p| start + p);
        let mut kept = Vec::new();
        let mut written = 0;
        for line in &lines[start..end] {
            let key = line.split('=').next().map(ini_key).unwrap_or_default();
            let is_entry = line.contains('=') && !line.trim_start().starts_with(';');
            match wanted.remove(&key) {
                Some(Some(new_line)) if is_entry => {
                    kept.push(new_line);
                    written += 1;
                }
                Some(None) if is_entry => {}
                _ => kept.push(line.clone()),
            }
        }
        // Insert the new lines before the section's trailing blank lines.
        let mut tail = Vec::new();
        while kept.last().is_some_and(|l| l.trim().is_empty()) {
            tail.push(kept.pop().unwrap());
        }
        for new_line in wanted.into_values().flatten() {
            kept.push(new_line);
            written += 1;
        }
        kept.extend(tail);
        lines.splice(start..end, kept);
        match fs::write(&self.config_file, lines.join("\r\n") + "\r\n") {
            Ok(()) => format!("Exported {written} values to {}.", self.config_file.display()),
            Err(error) => format!("Export failed: {error}"),
        }
    }
}

fn is_value(var: &DebugVar) -> bool {
    matches!(var.kind, DebugVarKind::Bool | DebugVarKind::Int | DebugVarKind::UInt | DebugVarKind::Float | DebugVarKind::Choice)
}

/// The text the game's console and config.ini accept for this value.
fn value_text(var: &DebugVar, value: f64, service: &DebugVars) -> String {
    match var.kind {
        DebugVarKind::Bool => if value != 0.0 { "true".into() } else { "false".into() },
        DebugVarKind::Float => format!("{value}"),
        DebugVarKind::Choice => service
            .options(var.index)
            .into_iter()
            .find(|(_, v)| *v as f64 == value)
            .map(|(name, _)| name)
            .unwrap_or_else(|| format!("{}", value as i64)),
        _ => format!("{}", value as i64),
    }
}

/// config.ini keys compare as the game does (DebugIniPathPC): trimmed, '/' only, lower case.
fn ini_key(path: &str) -> String {
    path.trim()
        .replace('\\', "/")
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("/")
        .to_lowercase()
}

/// The flat, depth-first listing back into folders.
fn build_tree(vars: &[DebugVar]) -> Vec<Node> {
    fn take(vars: &[DebugVar], at: &mut usize, depth: i32) -> Vec<Node> {
        let mut nodes = Vec::new();
        while *at < vars.len() && vars[*at].depth >= depth {
            if vars[*at].depth > depth {
                // A child without its folder row (should not happen): attach it here.
                let index = *at;
                *at += 1;
                nodes.push(Node { index, children: Vec::new() });
                continue;
            }
            let index = *at;
            *at += 1;
            let children = if vars[index].kind == DebugVarKind::Menu { take(vars, at, depth + 1) } else { Vec::new() };
            nodes.push(Node { index, children });
        }
        nodes
    }
    let mut at = 0;
    take(vars, &mut at, 0)
}

/// An iOS-style switch for booleans.
fn toggle(on: &mut bool) -> impl egui::Widget + '_ {
    move |ui: &mut Ui| {
        let size = ui.spacing().interact_size.y * egui::vec2(1.8, 0.9);
        let (rect, mut response) = ui.allocate_exact_size(size, egui::Sense::click());
        if response.clicked() {
            *on = !*on;
            response.mark_changed();
        }
        if ui.is_rect_visible(rect) {
            let t = ui.ctx().animate_bool_responsive(response.id, *on);
            let visuals = ui.style().interact_selectable(&response, *on);
            let radius = 0.5 * rect.height();
            ui.painter().rect(rect, radius, visuals.bg_fill, visuals.bg_stroke, egui::StrokeKind::Inside);
            let x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), t);
            ui.painter().circle(egui::pos2(x, rect.center().y), 0.75 * radius, visuals.bg_fill, visuals.fg_stroke);
        }
        response
    }
}
