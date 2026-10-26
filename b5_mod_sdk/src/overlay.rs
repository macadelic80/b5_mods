//! An egui overlay drawn with the game's own D3D9 device (feature `egui`).
//!
//! It draws into whatever render target the host bound for `on_frame` -- the frame the game
//! is about to present -- and leaves state restoring to the host. The game hides the Windows
//! cursor, so the overlay paints its own while [`Overlay::set_cursor_visible`] is on.
//!
//! ```ignore
//! fn on_message(&mut self, _: &Host, m: &WindowMessage) { self.overlay.message(m) }
//! fn on_frame(&mut self, _: &Host, f: &Frame) { self.overlay.frame(f) }
//! ```

use crate::ffi::{BRN_MODIFIER_ALT, BRN_MODIFIER_CTRL, BRN_MODIFIER_SHIFT};
use crate::{Frame, WindowMessage};
use egui::epaint::{ImageDelta, Primitive};
use egui::{Color32, Context, Event, Id, ImageData, Key, LayerId, Modifiers, Order, Pos2, RawInput, Rect, Shape, Stroke, TextureId, Vec2};
use std::collections::HashMap;
use std::ffi::c_void;
use std::time::Instant;
use windows::core::Interface;
use windows::Win32::Foundation::{HANDLE, HWND, RECT};
use windows::Win32::Graphics::Direct3D9::*;
use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

type UiFn<T> = Box<dyn FnMut(&Context, &mut T)>;

pub struct Overlay<T: 'static> {
    ui: UiFn<T>,
    state: T,
    ctx: Context,
    started: Instant,
    events: Vec<Event>,
    modifiers: Modifiers,
    scale: Vec2,              // window client pixels -> back buffer pixels
    cursor: bool,
    painter: Option<Painter>,
}

impl<T: 'static> Overlay<T> {
    /// `ui` runs once per frame with egui's context and your state.
    pub fn new(ui: impl FnMut(&Context, &mut T) + 'static, state: T) -> Self {
        Self {
            ui: Box::new(ui),
            state,
            ctx: Context::default(),
            started: Instant::now(),
            events: Vec::new(),
            modifiers: Modifiers::default(),
            scale: Vec2::splat(1.0),
            cursor: false,
            painter: None,
        }
    }

    /// One line for a log: textures, draws of the last frame, failed D3D calls.
    pub fn diagnostics(&self) -> String {
        match &self.painter {
            Some(p) => format!("overlay: {} textures, {} draws, {} D3D failures", p.textures.len(), p.last_draws, p.failures),
            None => "overlay: not started".into(),
        }
    }

    /// Paint a cursor (turn it on while your menu is open).
    pub fn set_cursor_visible(&mut self, visible: bool) {
        self.cursor = visible;
    }

    pub fn message(&mut self, message: &WindowMessage) {
        if let Some(event) = translate(message, self.scale, &mut self.modifiers) {
            self.events.push(event);
        }
    }

    pub fn frame(&mut self, frame: &Frame) {
        let raw: *mut c_void = frame.d3d9_device;
        // Borrowed: the game owns the device, no AddRef / Release here.
        let Some(device) = (unsafe { IDirect3DDevice9::from_raw_borrowed(&raw) }) else { return };
        let size = Vec2::new(frame.width as f32, frame.height as f32);
        self.scale = client_scale(HWND(frame.hwnd), size);

        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            time: Some(self.started.elapsed().as_secs_f64()),
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
                copy_to_clipboard(text);
            }
        }

        let painter = self.painter.get_or_insert_with(Painter::default);
        for (id, delta) in &output.textures_delta.set {
            painter.set_texture(device, *id, delta);
        }
        let primitives = self.ctx.tessellate(output.shapes, output.pixels_per_point);
        painter.paint(device, &primitives, size);
        for id in &output.textures_delta.free {
            painter.textures.remove(id);
        }
    }
}

// ---- input -------------------------------------------------------------------------------------

const WM_KEYDOWN: u32 = 0x0100;
const WM_KEYUP: u32 = 0x0101;
const WM_CHAR: u32 = 0x0102;
const WM_SYSKEYDOWN: u32 = 0x0104;
const WM_SYSKEYUP: u32 = 0x0105;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_LBUTTONDBLCLK: u32 = 0x0203;
const WM_RBUTTONDOWN: u32 = 0x0204;
const WM_RBUTTONUP: u32 = 0x0205;
const WM_MBUTTONDOWN: u32 = 0x0207;
const WM_MBUTTONUP: u32 = 0x0208;
const WM_MOUSEWHEEL: u32 = 0x020A;
const WM_MOUSEHWHEEL: u32 = 0x020E;
const WM_KILLFOCUS: u32 = 0x0008;

fn client_scale(hwnd: HWND, back_buffer: Vec2) -> Vec2 {
    let mut rect = RECT::default();
    if unsafe { GetClientRect(hwnd, &mut rect) }.is_err() || rect.right <= rect.left || rect.bottom <= rect.top {
        return Vec2::splat(1.0);
    }
    Vec2::new(back_buffer.x / (rect.right - rect.left) as f32, back_buffer.y / (rect.bottom - rect.top) as f32)
}

fn translate(message: &WindowMessage, scale: Vec2, modifiers: &mut Modifiers) -> Option<Event> {
    let held = message.modifiers;
    *modifiers = Modifiers {
        alt: held & BRN_MODIFIER_ALT != 0,
        ctrl: held & BRN_MODIFIER_CTRL != 0,
        shift: held & BRN_MODIFIER_SHIFT != 0,
        mac_cmd: false,
        command: held & BRN_MODIFIER_CTRL != 0,
    };
    let point = || {
        let x = (message.lparam & 0xFFFF) as i16 as f32;
        let y = ((message.lparam >> 16) & 0xFFFF) as i16 as f32;
        Pos2::new(x * scale.x, y * scale.y)
    };
    let button = |button, pressed| Event::PointerButton { pos: point(), button, pressed, modifiers: *modifiers };
    match message.msg {
        WM_MOUSEMOVE => Some(Event::PointerMoved(point())),
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => Some(button(egui::PointerButton::Primary, true)),
        WM_LBUTTONUP => Some(button(egui::PointerButton::Primary, false)),
        WM_RBUTTONDOWN => Some(button(egui::PointerButton::Secondary, true)),
        WM_RBUTTONUP => Some(button(egui::PointerButton::Secondary, false)),
        WM_MBUTTONDOWN => Some(button(egui::PointerButton::Middle, true)),
        WM_MBUTTONUP => Some(button(egui::PointerButton::Middle, false)),
        WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
            let notches = ((message.wparam >> 16) & 0xFFFF) as i16 as f32 / 120.0;
            let delta = if message.msg == WM_MOUSEWHEEL { Vec2::new(0.0, notches) } else { Vec2::new(notches, 0.0) };
            Some(Event::MouseWheel { unit: egui::MouseWheelUnit::Line, delta, modifiers: *modifiers })
        }
        WM_CHAR => {
            let c = char::from_u32(message.wparam as u32)?;
            (!c.is_control()).then(|| Event::Text(c.to_string()))
        }
        WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP => {
            let pressed = matches!(message.msg, WM_KEYDOWN | WM_SYSKEYDOWN);
            let key = key(message.wparam)?;
            if pressed && modifiers.ctrl {
                match key {
                    Key::C => return Some(Event::Copy),
                    Key::X => return Some(Event::Cut),
                    _ => {}
                }
            }
            Some(Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: pressed && message.lparam & (1 << 30) != 0,
                modifiers: *modifiers,
            })
        }
        WM_KILLFOCUS => Some(Event::WindowFocused(false)),
        _ => None,
    }
}

fn key(virtual_key: u64) -> Option<Key> {
    Some(match virtual_key {
        0x08 => Key::Backspace,
        0x09 => Key::Tab,
        0x0D => Key::Enter,
        0x1B => Key::Escape,
        0x20 => Key::Space,
        0x21 => Key::PageUp,
        0x22 => Key::PageDown,
        0x23 => Key::End,
        0x24 => Key::Home,
        0x25 => Key::ArrowLeft,
        0x26 => Key::ArrowUp,
        0x27 => Key::ArrowRight,
        0x28 => Key::ArrowDown,
        0x2E => Key::Delete,
        0x41 => Key::A,
        0x43 => Key::C,
        0x56 => Key::V,
        0x58 => Key::X,
        0x5A => Key::Z,
        _ => return None,
    })
}

fn copy_to_clipboard(text: &str) {
    const CF_UNICODETEXT: u32 = 13;
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        if OpenClipboard(None).is_err() {
            return;
        }
        let _ = EmptyClipboard();
        if let Ok(memory) = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2) {
            let target = GlobalLock(memory) as *mut u16;
            if !target.is_null() {
                std::ptr::copy_nonoverlapping(wide.as_ptr(), target, wide.len());
                let _ = GlobalUnlock(memory);
                let _ = SetClipboardData(CF_UNICODETEXT, Some(HANDLE(memory.0)));
            }
        }
        let _ = CloseClipboard();
    }
}

fn paint_cursor(ctx: &Context) {
    let Some(tip) = ctx.input(|input| input.pointer.latest_pos()) else { return };
    let painter = ctx.layer_painter(LayerId::new(Order::Debug, Id::new("b5_mod_sdk_cursor")));
    // A plain arrow head: convex, so one filled polygon draws it.
    let points = [(0.0, 0.0), (0.0, 17.0), (12.0, 12.0)].map(|(x, y)| Pos2::new(tip.x + x, tip.y + y)).to_vec();
    painter.add(Shape::convex_polygon(points.clone(), Color32::WHITE, Stroke::NONE));
    painter.add(Shape::closed_line(points, Stroke::new(1.0, Color32::BLACK)));
}

// ---- painting ---------------------------------------------------------------------------------

/// Pre-transformed vertex: screen pixels, packed colour, uv.
#[repr(C)]
#[derive(Clone, Copy)]
struct Vertex {
    x: f32,
    y: f32,
    z: f32,
    rhw: f32,
    color: u32,
    u: f32,
    v: f32,
}

const FVF: u32 = D3DFVF_XYZRHW | D3DFVF_DIFFUSE | D3DFVF_TEX1;

struct Texture {
    handle: IDirect3DTexture9,
    size: [usize; 2],
    pixels: Vec<Color32>,   // CPU copy: partial deltas patch it, then the whole texture is re-sent
}

#[derive(Default)]
struct Painter {
    textures: HashMap<TextureId, Texture>,
    failures: u32,       // D3D calls that failed (reported by Overlay::diagnostics)
    last_draws: usize,
    vertices: Option<(IDirect3DVertexBuffer9, usize)>,
    indices: Option<(IDirect3DIndexBuffer9, usize)>,
}

fn argb(color: Color32) -> u32 {
    // egui colours are premultiplied RGBA; D3DCOLOR is ARGB.
    let [r, g, b, a] = color.to_array();
    (a as u32) << 24 | (r as u32) << 16 | (g as u32) << 8 | b as u32
}

impl Painter {
    fn set_texture(&mut self, device: &IDirect3DDevice9, id: TextureId, delta: &ImageDelta) {
        let (size, pixels): ([usize; 2], Vec<Color32>) = match &delta.image {
            ImageData::Color(image) => (image.size, image.pixels.clone()),
            ImageData::Font(font) => (font.size, font.srgba_pixels(None).collect()),
        };
        match delta.pos {
            None => {
                let mut handle = None;
                let created = unsafe {
                    device.CreateTexture(
                        size[0] as u32,
                        size[1] as u32,
                        1,
                        D3DUSAGE_DYNAMIC as u32,
                        D3DFMT_A8R8G8B8,
                        D3DPOOL_DEFAULT,
                        &mut handle,
                        std::ptr::null_mut(),
                    )
                };
                let (Ok(()), Some(handle)) = (created, handle) else {
                    self.failures += 1;
                    return;
                };
                self.textures.insert(id, Texture { handle, size, pixels });
            }
            Some(at) => {
                let Some(texture) = self.textures.get_mut(&id) else { return };
                if at[0] + size[0] > texture.size[0] || at[1] + size[1] > texture.size[1] {
                    return;
                }
                for row in 0..size[1] {
                    let target = (at[1] + row) * texture.size[0] + at[0];
                    texture.pixels[target..target + size[0]].copy_from_slice(&pixels[row * size[0]..(row + 1) * size[0]]);
                }
            }
        }
        let Some(texture) = self.textures.get(&id) else { return };
        let mut locked = D3DLOCKED_RECT::default();
        if unsafe { texture.handle.LockRect(0, &mut locked, std::ptr::null(), D3DLOCK_DISCARD as u32) }.is_err() {
            self.failures += 1;
            return;
        }
        let [width, height] = texture.size;
        for row in 0..height {
            let target = unsafe { (locked.pBits as *mut u8).add(row * locked.Pitch as usize) as *mut u32 };
            for column in 0..width {
                unsafe { *target.add(column) = argb(texture.pixels[row * width + column]) };
            }
        }
        let _ = unsafe { texture.handle.UnlockRect(0) };
    }

    fn paint(&mut self, device: &IDirect3DDevice9, primitives: &[egui::ClippedPrimitive], screen: Vec2) {
        let mut vertices: Vec<Vertex> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        let mut draws: Vec<(TextureId, RECT, usize, usize, usize, usize)> = Vec::new();
        for primitive in primitives {
            let Primitive::Mesh(mesh) = &primitive.primitive else { continue };
            if mesh.indices.is_empty() {
                continue;
            }
            let clip = primitive.clip_rect.intersect(Rect::from_min_size(Pos2::ZERO, screen));
            if clip.width() < 1.0 || clip.height() < 1.0 {
                continue;
            }
            let scissor = RECT {
                left: clip.min.x.round() as i32,
                top: clip.min.y.round() as i32,
                right: clip.max.x.round() as i32,
                bottom: clip.max.y.round() as i32,
            };
            draws.push((mesh.texture_id, scissor, vertices.len(), mesh.vertices.len(), indices.len(), mesh.indices.len()));
            // -0.5: D3D9 samples pixel centres at integer coordinates.
            vertices.extend(mesh.vertices.iter().map(|v| Vertex {
                x: v.pos.x - 0.5,
                y: v.pos.y - 0.5,
                z: 0.0,
                rhw: 1.0,
                color: argb(v.color),
                u: v.uv.x,
                v: v.uv.y,
            }));
            indices.extend_from_slice(&mesh.indices);
        }
        self.last_draws = draws.len();
        if draws.is_empty() {
            return;
        }
        if !self.upload(device, &vertices, &indices) {
            self.failures += 1;
            return;
        }
        let (Some((vb, _)), Some((ib, _))) = (&self.vertices, &self.indices) else { return };
        unsafe {
            if setup_state(device, vb, ib).is_err() {
                self.failures += 1;
                return;
            }
            for (texture, scissor, first_vertex, vertex_count, first_index, index_count) in draws {
                let Some(texture) = self.textures.get(&texture) else { continue };
                let _ = device.SetScissorRect(&scissor);
                let _ = device.SetTexture(0, &texture.handle);
                let _ = device.DrawIndexedPrimitive(
                    D3DPT_TRIANGLELIST,
                    first_vertex as i32,
                    0,
                    vertex_count as u32,
                    first_index as u32,
                    (index_count / 3) as u32,
                );
            }
        }
    }

    fn upload(&mut self, device: &IDirect3DDevice9, vertices: &[Vertex], indices: &[u32]) -> bool {
        unsafe {
            if self.vertices.as_ref().map_or(true, |(_, capacity)| *capacity < vertices.len()) {
                let capacity = vertices.len().next_power_of_two().max(4096);
                let mut buffer = None;
                let bytes = (capacity * std::mem::size_of::<Vertex>()) as u32;
                let usage = (D3DUSAGE_DYNAMIC | D3DUSAGE_WRITEONLY) as u32;
                if device.CreateVertexBuffer(bytes, usage, FVF, D3DPOOL_DEFAULT, &mut buffer, std::ptr::null_mut()).is_err() {
                    return false;
                }
                self.vertices = buffer.map(|b| (b, capacity));
            }
            if self.indices.as_ref().map_or(true, |(_, capacity)| *capacity < indices.len()) {
                let capacity = indices.len().next_power_of_two().max(8192);
                let mut buffer = None;
                let usage = (D3DUSAGE_DYNAMIC | D3DUSAGE_WRITEONLY) as u32;
                if device
                    .CreateIndexBuffer((capacity * 4) as u32, usage, D3DFMT_INDEX32, D3DPOOL_DEFAULT, &mut buffer, std::ptr::null_mut())
                    .is_err()
                {
                    return false;
                }
                self.indices = buffer.map(|b| (b, capacity));
            }
            let (Some((vb, _)), Some((ib, _))) = (&self.vertices, &self.indices) else { return false };
            let mut target: *mut c_void = std::ptr::null_mut();
            let bytes = std::mem::size_of_val(vertices) as u32;
            if vb.Lock(0, bytes, &mut target, D3DLOCK_DISCARD as u32).is_err() {
                return false;
            }
            std::ptr::copy_nonoverlapping(vertices.as_ptr(), target as *mut Vertex, vertices.len());
            let _ = vb.Unlock();
            let bytes = std::mem::size_of_val(indices) as u32;
            if ib.Lock(0, bytes, &mut target, D3DLOCK_DISCARD as u32).is_err() {
                return false;
            }
            std::ptr::copy_nonoverlapping(indices.as_ptr(), target as *mut u32, indices.len());
            let _ = ib.Unlock();
        }
        true
    }
}

/// Fixed-function, premultiplied alpha, no depth. The host restores everything afterwards.
unsafe fn setup_state(device: &IDirect3DDevice9, vb: &IDirect3DVertexBuffer9, ib: &IDirect3DIndexBuffer9) -> windows::core::Result<()> {
    device.SetVertexShader(None)?;
    device.SetPixelShader(None)?;
    device.SetFVF(FVF)?;
    // The renderer instances: make sure stream 0 is plain per-vertex data.
    device.SetStreamSourceFreq(0, 1)?;
    device.SetStreamSourceFreq(1, 1)?;
    device.SetStreamSource(0, vb, 0, std::mem::size_of::<Vertex>() as u32)?;
    device.SetIndices(ib)?;
    for (state, value) in [
        (D3DRS_FILLMODE, D3DFILL_SOLID.0 as u32),
        (D3DRS_ZENABLE, 0),
        (D3DRS_ZWRITEENABLE, 0),
        (D3DRS_ALPHATESTENABLE, 0),
        (D3DRS_CULLMODE, D3DCULL_NONE.0 as u32),
        (D3DRS_ALPHABLENDENABLE, 1),
        (D3DRS_BLENDOP, D3DBLENDOP_ADD.0 as u32),
        (D3DRS_SRCBLEND, D3DBLEND_ONE.0 as u32),
        (D3DRS_DESTBLEND, D3DBLEND_INVSRCALPHA.0 as u32),
        (D3DRS_SEPARATEALPHABLENDENABLE, 1),
        (D3DRS_BLENDOPALPHA, D3DBLENDOP_ADD.0 as u32),
        (D3DRS_SRCBLENDALPHA, D3DBLEND_ONE.0 as u32),
        (D3DRS_DESTBLENDALPHA, D3DBLEND_INVSRCALPHA.0 as u32),
        (D3DRS_SCISSORTESTENABLE, 1),
        (D3DRS_STENCILENABLE, 0),
        (D3DRS_FOGENABLE, 0),
        (D3DRS_LIGHTING, 0),
        (D3DRS_COLORWRITEENABLE, 0xF),
        (D3DRS_SRGBWRITEENABLE, 0),
        (D3DRS_MULTISAMPLEANTIALIAS, 0),
    ] {
        device.SetRenderState(state, value)?;
    }
    device.SetTextureStageState(0, D3DTSS_COLOROP, D3DTOP_MODULATE.0 as u32)?;
    device.SetTextureStageState(0, D3DTSS_COLORARG1, D3DTA_TEXTURE)?;
    device.SetTextureStageState(0, D3DTSS_COLORARG2, D3DTA_DIFFUSE)?;
    device.SetTextureStageState(0, D3DTSS_ALPHAOP, D3DTOP_MODULATE.0 as u32)?;
    device.SetTextureStageState(0, D3DTSS_ALPHAARG1, D3DTA_TEXTURE)?;
    device.SetTextureStageState(0, D3DTSS_ALPHAARG2, D3DTA_DIFFUSE)?;
    device.SetTextureStageState(0, D3DTSS_TEXCOORDINDEX, 0)?;
    device.SetTextureStageState(0, D3DTSS_TEXTURETRANSFORMFLAGS, D3DTTFF_DISABLE.0 as u32)?;
    device.SetTextureStageState(1, D3DTSS_COLOROP, D3DTOP_DISABLE.0 as u32)?;
    device.SetTextureStageState(1, D3DTSS_ALPHAOP, D3DTOP_DISABLE.0 as u32)?;
    for (state, value) in [
        (D3DSAMP_MINFILTER, D3DTEXF_LINEAR.0 as u32),
        (D3DSAMP_MAGFILTER, D3DTEXF_LINEAR.0 as u32),
        (D3DSAMP_MIPFILTER, D3DTEXF_NONE.0 as u32),
        (D3DSAMP_ADDRESSU, D3DTADDRESS_CLAMP.0 as u32),
        (D3DSAMP_ADDRESSV, D3DTADDRESS_CLAMP.0 as u32),
        (D3DSAMP_SRGBTEXTURE, 0),
    ] {
        device.SetSamplerState(0, state, value)?;
    }
    Ok(())
}
