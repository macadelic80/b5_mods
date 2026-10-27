//! An egui overlay over the game (feature `egui`).
//!
//! The mod never sees the graphics device: egui's tessellated meshes go to the host as draw
//! lists (`brn.ui.v1` submit32) and its textures as RGBA8 pixels; the host draws them over the
//! finished frame. The game hides the Windows cursor, so the overlay paints its own while
//! [`Overlay::set_cursor_visible`] is on.
//!
//! ```ignore
//! fn on_input(&mut self, _: &Host, e: &InputEvent) { self.overlay.input(e) }
//! fn on_frame(&mut self, _: &Host, f: &Frame) { self.overlay.frame(f) }
//! ```

use crate::ffi::{self, BRN_MODIFIER_ALT, BRN_MODIFIER_CTRL, BRN_MODIFIER_SHIFT};
use crate::{Frame, InputEvent};
use egui::epaint::{ImageDelta, Primitive, Vertex};
use egui::{Color32, Context, Event, Id, ImageData, Key, LayerId, Modifiers, Order, Pos2, RawInput, Rect, Shape, Stroke, TextureId, Vec2};
use std::collections::HashMap;

type UiFn<T> = Box<dyn FnMut(&Context, &mut T)>;

// egui's vertex is exactly the host's brn_vertex: pos, uv, colour bytes r g b a.
const _: () = assert!(std::mem::size_of::<Vertex>() == std::mem::size_of::<ffi::brn_vertex>());

pub struct Overlay<T: 'static> {
    ui: UiFn<T>,
    state: T,
    ctx: Context,
    time: f64,
    events: Vec<Event>,
    modifiers: Modifiers,
    cursor: bool,
    textures: HashMap<TextureId, (u32, [usize; 2])>,   // egui texture -> host handle, size
    last_draws: usize,
}

impl<T: 'static> Overlay<T> {
    /// `ui` runs once per frame with egui's context and your state.
    pub fn new(ui: impl FnMut(&Context, &mut T) + 'static, state: T) -> Self {
        Self {
            ui: Box::new(ui),
            state,
            ctx: Context::default(),
            time: 0.0,
            events: Vec::new(),
            modifiers: Modifiers::default(),
            cursor: false,
            textures: HashMap::new(),
            last_draws: 0,
        }
    }

    /// One line for a log: textures, draws of the last frame.
    pub fn diagnostics(&self) -> String {
        format!("overlay: {} textures, {} draws", self.textures.len(), self.last_draws)
    }

    /// Paint a cursor (turn it on while your menu is open).
    pub fn set_cursor_visible(&mut self, visible: bool) {
        self.cursor = visible;
    }

    pub fn input(&mut self, event: &InputEvent) {
        if let Some(event) = translate(event, &mut self.modifiers) {
            self.events.push(event);
        }
    }

    pub fn frame(&mut self, frame: &Frame) {
        let size = Vec2::new(frame.width as f32, frame.height as f32);
        self.time += f64::from(frame.delta_seconds);
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            time: Some(self.time),
            predicted_dt: frame.delta_seconds.max(1.0 / 240.0),
            modifiers: self.modifiers,
            events: std::mem::take(&mut self.events),
            focused: true,
            ..Default::default()
        };
        let (ui, state, cursor) = (&mut self.ui, &mut self.state, self.cursor);
        let output = self.ctx.run(input, |ctx| {
            ui(ctx, state);
            if cursor {
                paint_cursor(ctx);
            }
        });
        for command in &output.platform_output.commands {
            if let egui::OutputCommand::CopyText(text) = command {
                crate::set_clipboard_text(text);
            }
        }
        for (id, delta) in &output.textures_delta.set {
            self.set_texture(*id, delta);
        }
        let primitives = self.ctx.tessellate(output.shapes, output.pixels_per_point);
        self.last_draws = 0;
        for primitive in &primitives {
            let Primitive::Mesh(mesh) = &primitive.primitive else { continue };
            let Some(&(handle, _)) = self.textures.get(&mesh.texture_id) else { continue };
            if mesh.indices.is_empty() {
                continue;
            }
            let clip = primitive.clip_rect;
            unsafe {
                ffi::ui_submit32(
                    mesh.vertices.as_ptr() as *const ffi::brn_vertex,
                    mesh.vertices.len() as u32,
                    mesh.indices.as_ptr(),
                    mesh.indices.len() as u32,
                    handle,
                    clip.min.x,
                    clip.min.y,
                    clip.max.x,
                    clip.max.y,
                    ffi::BRN_UI_PREMULTIPLIED,
                );
            }
            self.last_draws += 1;
        }
        for id in &output.textures_delta.free {
            if let Some((handle, _)) = self.textures.remove(id) {
                unsafe { ffi::ui_texture_destroy(handle) };
            }
        }
    }

    fn set_texture(&mut self, id: TextureId, delta: &ImageDelta) {
        let (size, pixels): ([usize; 2], Vec<Color32>) = match &delta.image {
            ImageData::Color(image) => (image.size, image.pixels.clone()),
            ImageData::Font(font) => (font.size, font.srgba_pixels(None).collect()),
        };
        let bytes: Vec<u8> = pixels.iter().flat_map(|c| c.to_array()).collect();
        match delta.pos {
            None => {
                if let Some((old, _)) = self.textures.remove(&id) {
                    unsafe { ffi::ui_texture_destroy(old) };
                }
                let handle = unsafe { ffi::ui_texture_create(bytes.as_ptr(), size[0] as i32, size[1] as i32) };
                if handle != 0 {
                    self.textures.insert(id, (handle, size));
                }
            }
            Some([x, y]) => {
                let Some(&(handle, full)) = self.textures.get(&id) else { return };
                if x + size[0] > full[0] || y + size[1] > full[1] {
                    return;
                }
                unsafe { ffi::ui_texture_update(handle, x as i32, y as i32, size[0] as i32, size[1] as i32, bytes.as_ptr()) };
            }
        }
    }
}

// ---- input -------------------------------------------------------------------------------------

fn translate(event: &InputEvent, modifiers: &mut Modifiers) -> Option<Event> {
    let held = event.modifiers;
    *modifiers = Modifiers {
        alt: held & BRN_MODIFIER_ALT != 0,
        ctrl: held & BRN_MODIFIER_CTRL != 0,
        shift: held & BRN_MODIFIER_SHIFT != 0,
        mac_cmd: false,
        command: held & BRN_MODIFIER_CTRL != 0,
    };
    let pos = Pos2::new(event.x, event.y);
    match event.type_ {
        ffi::BRN_INPUT_MOUSE_MOVE => Some(Event::PointerMoved(pos)),
        ffi::BRN_INPUT_MOUSE_BUTTON => {
            let button = match event.code {
                ffi::BRN_MOUSE_LEFT => egui::PointerButton::Primary,
                ffi::BRN_MOUSE_RIGHT => egui::PointerButton::Secondary,
                ffi::BRN_MOUSE_MIDDLE => egui::PointerButton::Middle,
                ffi::BRN_MOUSE_X1 => egui::PointerButton::Extra1,
                _ => egui::PointerButton::Extra2,
            };
            Some(Event::PointerButton { pos, button, pressed: event.pressed != 0, modifiers: *modifiers })
        }
        ffi::BRN_INPUT_MOUSE_WHEEL => Some(Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: Vec2::new(event.x, event.y),
            modifiers: *modifiers,
        }),
        ffi::BRN_INPUT_MOUSE_LEAVE => Some(Event::PointerGone),
        ffi::BRN_INPUT_TEXT => {
            let c = char::from_u32(event.code as u32)?;
            Some(Event::Text(c.to_string()))
        }
        ffi::BRN_INPUT_KEY => {
            let pressed = event.pressed != 0;
            let key = key(event.code)?;
            if pressed && modifiers.ctrl {
                match key {
                    Key::C => return Some(Event::Copy),
                    Key::X => return Some(Event::Cut),
                    _ => {}
                }
            }
            Some(Event::Key { key, physical_key: None, pressed, repeat: event.repeat != 0, modifiers: *modifiers })
        }
        ffi::BRN_INPUT_FOCUS => Some(Event::WindowFocused(event.pressed != 0)),
        _ => None,
    }
}

fn key(code: i32) -> Option<Key> {
    const LETTERS: [Key; 26] = [
        Key::A, Key::B, Key::C, Key::D, Key::E, Key::F, Key::G, Key::H, Key::I, Key::J, Key::K, Key::L, Key::M,
        Key::N, Key::O, Key::P, Key::Q, Key::R, Key::S, Key::T, Key::U, Key::V, Key::W, Key::X, Key::Y, Key::Z,
    ];
    const DIGITS: [Key; 10] =
        [Key::Num0, Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9];
    const FUNCTION: [Key; 12] =
        [Key::F1, Key::F2, Key::F3, Key::F4, Key::F5, Key::F6, Key::F7, Key::F8, Key::F9, Key::F10, Key::F11, Key::F12];
    Some(match code {
        0x41..=0x5A => LETTERS[(code - 0x41) as usize],
        0x30..=0x39 => DIGITS[(code - 0x30) as usize],
        c if (ffi::BRN_KEY_F1..ffi::BRN_KEY_F1 + 12).contains(&c) => FUNCTION[(c - ffi::BRN_KEY_F1) as usize],
        c if (ffi::BRN_KEY_KP_0..ffi::BRN_KEY_KP_0 + 10).contains(&c) => DIGITS[(c - ffi::BRN_KEY_KP_0) as usize],
        0x20 => Key::Space,
        0x2D => Key::Minus,
        0x3D => Key::Equals,
        ffi::BRN_KEY_ESCAPE => Key::Escape,
        ffi::BRN_KEY_ENTER | ffi::BRN_KEY_KP_ENTER => Key::Enter,
        ffi::BRN_KEY_TAB => Key::Tab,
        ffi::BRN_KEY_BACKSPACE => Key::Backspace,
        ffi::BRN_KEY_INSERT => Key::Insert,
        ffi::BRN_KEY_DELETE => Key::Delete,
        ffi::BRN_KEY_RIGHT => Key::ArrowRight,
        ffi::BRN_KEY_LEFT => Key::ArrowLeft,
        ffi::BRN_KEY_DOWN => Key::ArrowDown,
        ffi::BRN_KEY_UP => Key::ArrowUp,
        ffi::BRN_KEY_PAGE_UP => Key::PageUp,
        ffi::BRN_KEY_PAGE_DOWN => Key::PageDown,
        ffi::BRN_KEY_HOME => Key::Home,
        ffi::BRN_KEY_END => Key::End,
        _ => return None,
    })
}

fn paint_cursor(ctx: &Context) {
    let Some(tip) = ctx.input(|input| input.pointer.latest_pos()) else { return };
    let painter = ctx.layer_painter(LayerId::new(Order::Debug, Id::new("b5_mod_sdk_cursor")));
    // A plain arrow head: convex, so one filled polygon draws it.
    let points = [(0.0, 0.0), (0.0, 17.0), (12.0, 12.0)].map(|(x, y)| Pos2::new(tip.x + x, tip.y + y)).to_vec();
    painter.add(Shape::convex_polygon(points.clone(), Color32::WHITE, Stroke::NONE));
    painter.add(Shape::closed_line(points, Stroke::new(1.0, Color32::BLACK)));
}
