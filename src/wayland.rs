///! Wayland backend: connection setup, output (monitor) discovery, shared-memory
///! surface allocation, layer-shell overlay management, and virtual pointer control.
///!
///! We deliberately depend on wlroots-specific protocols:
///!   • zwlr_layer_shell_v1          – overlay surfaces above everything
///!   • zwlr_virtual_pointer_manager_v1 – synthetic mouse events
///!   • zxdg_output_manager_v1       – logical output geometry
///! These are available on Sway, Hyprland, and other wlroots compositors.

use std::os::fd::{AsFd, OwnedFd};

use anyhow::{bail, Context, Result};
use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_keyboard, wl_output, wl_pointer, wl_region, wl_registry, wl_seat, wl_shm,
    wl_shm_pool, wl_surface,
};
use wayland_client::{
    globals::{registry_queue_init, GlobalList, GlobalListContents},
    Connection, Dispatch, EventQueue, QueueHandle,
};
use wayland_protocols::xdg::xdg_output::zv1::client::{
    zxdg_output_manager_v1, zxdg_output_v1,
};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1, zwlr_layer_surface_v1,
};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1, zwlr_virtual_pointer_v1,
};
use wayland_protocols::wp::relative_pointer::zv1::client::{
    zwp_relative_pointer_manager_v1, zwp_relative_pointer_v1,   
};

// Re-exports for use by other modules
pub use wayland_client::protocol::wl_keyboard::KeyState;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Describes a single monitor / output.
#[derive(Debug, Clone)]
pub struct Monitor {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale: i32,
    pub transform: wl_output::Transform,
    /// The wl_output for this monitor (needed for layer-shell targeting).
    pub wl_output: wl_output::WlOutput,
}

/// A shared-memory backed drawing buffer that can be attached to a wl_surface.
pub struct ShmBuffer {
    pub width: i32,
    pub height: i32,
    pub stride: i32,
    pub fd: OwnedFd,
    pub pool: wl_shm_pool::WlShmPool,
    pub buffer: wl_buffer::WlBuffer,
    /// Raw mmap'd pointer. Safety: lives as long as the fd.
    pub data: memmap2::MmapMut,
}

/// An overlay surface created via the layer-shell protocol.
pub struct Overlay {
    pub surface: wl_surface::WlSurface,
    pub layer_surface: zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
    pub shm_buffer: ShmBuffer,
    pub configured: bool,
}

/// A keyboard event delivered from Wayland.
#[derive(Debug, Clone)]
pub struct KeyEvent {
    pub key: u32,       // Linux keycode (evdev)
    pub sym: u32,       // XKB keysym
    pub state: KeyState, // pressed / released
    pub utf8: Option<String>,
}

// ---------------------------------------------------------------------------
// Central Wayland state
// ---------------------------------------------------------------------------

pub struct WaylandState {
    // globals we bind during registry
    pub compositor: Option<wl_compositor::WlCompositor>,
    pub shm: Option<wl_shm::WlShm>,
    pub seat: Option<wl_seat::WlSeat>,
    pub layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    pub vptr_mgr: Option<zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1>,
    pub xdg_output_mgr: Option<zxdg_output_manager_v1::ZxdgOutputManagerV1>,

    // collected monitors
    pub monitors: Vec<Monitor>,
    // in-progress output being filled in by events
    pending_outputs: Vec<PendingOutput>,

    // keyboard state
    pub keyboard: Option<wl_keyboard::WlKeyboard>,
    pub pointer: Option<wl_pointer::WlPointer>,
    pub xkb_context: Option<xkbcommon::xkb::Context>,
    pub xkb_keymap: Option<xkbcommon::xkb::Keymap>,
    pub xkb_state: Option<xkbcommon::xkb::State>,

    // Pointer focus and position relative to the focused surface.
    pub pointer_focus_surface: Option<wl_surface::WlSurface>,
    pub pointer_surface_pos: Option<(f64, f64)>,

    // Output that the compositor assigned to our surface (from wl_surface.enter).
    pub surface_entered_output: Option<wl_output::WlOutput>,

    // channel for key events → mode loop
    pub key_tx: std::sync::mpsc::Sender<KeyEvent>,
    pub key_rx: std::sync::mpsc::Receiver<KeyEvent>,

    // virtual pointer
    pub vptr: Option<zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1>,

    // Set to true when the layer surface receives its configure event
    pub layer_surface_configured: bool,
}

impl std::fmt::Debug for WaylandState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WaylandState")
            .field("compositor", &self.compositor)
            .field("shm", &self.shm)
            .field("seat", &self.seat)
            .field("layer_shell", &self.layer_shell)
            .field("vptr_mgr", &self.vptr_mgr)
            .field("xdg_output_mgr", &self.xdg_output_mgr)
            .field("monitors", &self.monitors)
            .field("pending_outputs", &self.pending_outputs)
            .field("keyboard", &self.keyboard)
            .field("pointer", &self.pointer)
            .field("xkb_context", &self.xkb_context.as_ref().map(|_| "<xkb::Context>"))
            .field("xkb_keymap", &self.xkb_keymap.as_ref().map(|_| "<xkb::Keymap>"))
            .field("xkb_state", &self.xkb_state.as_ref().map(|_| "<xkb::State>"))
            .field("pointer_focus_surface", &self.pointer_focus_surface)
            .field("pointer_surface_pos", &self.pointer_surface_pos)
            .field("surface_entered_output", &self.surface_entered_output)
            .field("vptr", &self.vptr)
            .field("layer_surface_configured", &self.layer_surface_configured)
            .finish()
    }
}

#[derive(Default, Debug)]
struct PendingOutput {
    wl_output: Option<wl_output::WlOutput>,
    name: String,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    scale: i32,
    transform: Option<wl_output::Transform>,
    done: bool,
}

impl WaylandState {
    pub fn new() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        Self {
            compositor: None,
            shm: None,
            seat: None,
            layer_shell: None,
            vptr_mgr: None,
            xdg_output_mgr: None,
            monitors: Vec::new(),
            pending_outputs: Vec::new(),
            keyboard: None,
            pointer: None,
            xkb_context: Some(xkbcommon::xkb::Context::new(xkbcommon::xkb::CONTEXT_NO_FLAGS)),
            xkb_keymap: None,
            xkb_state: None,
            pointer_focus_surface: None,
            pointer_surface_pos: None,
            surface_entered_output: None,
            key_tx: tx,
            key_rx: rx,
            vptr: None,
            layer_surface_configured: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Registry – discover and bind globals
// ---------------------------------------------------------------------------

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for WaylandState {
    fn event(
        _state: &mut Self,
        _registry: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // Globals are bound explicitly in connect() via GlobalList::bind().
        // This impl is required by registry_queue_init's trait bounds but is a no-op.
    }
}

// ---------------------------------------------------------------------------
// wl_output – physical display events
// ---------------------------------------------------------------------------

impl Dispatch<wl_output::WlOutput, ()> for WaylandState {
    fn event(
        state: &mut Self,
        proxy: &wl_output::WlOutput,
        event: wl_output::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let pending = state
            .pending_outputs
            .iter_mut()
            .find(|p| p.wl_output.as_ref() == Some(proxy));
        let Some(pending) = pending else { return };

        match event {
            wl_output::Event::Name { name } => {
                pending.name = name;
            }
            wl_output::Event::Geometry { x, y, transform, .. } => {
                pending.x = x;
                pending.y = y;
                pending.transform = match transform {
                    wayland_client::WEnum::Value(t) => Some(t),
                    wayland_client::WEnum::Unknown(_) => Some(wl_output::Transform::Normal),
                };
            }
            wl_output::Event::Mode { width, height, .. } => {
                pending.width = width;
                pending.height = height;
            }
            wl_output::Event::Scale { factor } => {
                pending.scale = factor;
            }
            wl_output::Event::Done => {
                pending.done = true;
            }
            _ => {            log::debug!("WlOutputEvent::{:?}",event);}
        }
    }
}

// ---------------------------------------------------------------------------
// xdg_output – logical geometry (accounts for scaling / transforms)
// ---------------------------------------------------------------------------

impl Dispatch<zxdg_output_manager_v1::ZxdgOutputManagerV1, ()> for WaylandState {
    fn event(
        _state: &mut Self,
        _proxy: &zxdg_output_manager_v1::ZxdgOutputManagerV1,
        _event: zxdg_output_manager_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // no events on the manager itself
    }
}

impl Dispatch<zxdg_output_v1::ZxdgOutputV1, usize> for WaylandState {
    fn event(
        state: &mut Self,
        _proxy: &zxdg_output_v1::ZxdgOutputV1,
        event: zxdg_output_v1::Event,
        idx: &usize,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let Some(pending) = state.pending_outputs.get_mut(*idx) else { return };
        match event {
            zxdg_output_v1::Event::LogicalPosition { x, y } => {
                pending.x = x;
                pending.y = y;
            }
            zxdg_output_v1::Event::LogicalSize { width, height } => {
                pending.width = width;
                pending.height = height;
            }
            zxdg_output_v1::Event::Name { name } => {
                pending.name = name;
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// wl_seat / wl_keyboard – input
// ---------------------------------------------------------------------------

impl Dispatch<wl_seat::WlSeat, ()> for WaylandState {
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities { capabilities } = event {
            let has_keyboard = capabilities
                .into_result()
                .map(|c| c.contains(wl_seat::Capability::Keyboard))
                .unwrap_or(false);
            let has_pointer = capabilities
                .into_result()
                .map(|c| c.contains(wl_seat::Capability::Pointer))
                .unwrap_or(false);
            log::debug!(
                "seat capabilities changed: keyboard={}, pointer={}",
                has_keyboard,
                has_pointer
            );
            if has_keyboard && state.keyboard.is_none() {
                state.keyboard = Some(seat.get_keyboard(qh, ()));
            }
            if has_pointer && state.pointer.is_none() {
                state.pointer = Some(seat.get_pointer(qh, ()));
                log::debug!("pointer={:?}", state.pointer);
            }
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for WaylandState {
    fn event(
        state: &mut Self,
        _proxy: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        log::debug!("WLPointerEvent::{:?}", &event);
        match event {
            wl_pointer::Event::Enter {
                serial, 
                surface,
                surface_x,
                surface_y,
                ..
            } => {
                log::debug!(
                    "WLPointerEvent::Enter: serial={}, surface={:?}, x={}, y={}",
                    serial,
                    surface,
                    surface_x,
                    surface_y
                );
                state.pointer_focus_surface = Some(surface);
                state.pointer_surface_pos = Some((surface_x, surface_y));
            }
            wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => {
                state.pointer_surface_pos = Some((surface_x, surface_y));
            }
            wl_pointer::Event::Leave { surface, .. } => {
                log::debug!("pointer leave: surface={:?}", surface);
            }
            _ => {log::debug!("WLPointerEvent::{:?}", event);}
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for WaylandState {
    fn event(
        state: &mut Self,
        _proxy: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        log::debug!("WLKeyboardEvent::{:?}", event);
        match event {
            wl_keyboard::Event::Keymap { format, fd, size } => {
                if format != wayland_client::WEnum::Value(wl_keyboard::KeymapFormat::XkbV1) {
                    return;
                }
                let ctx = match &state.xkb_context {
                    Some(c) => c,
                    None => return,
                };
                // mmap the keymap fd
                unsafe {
                    let map = memmap2::MmapOptions::new()
                        .len(size as usize)
                        .map(&fd)
                        .ok();
                    if let Some(map) = map {
                        let keymap_str = std::str::from_utf8(&map[..size as usize - 1])
                            .unwrap_or("");
                        let keymap = xkbcommon::xkb::Keymap::new_from_string(
                            ctx,
                            keymap_str.to_string(),
                            xkbcommon::xkb::KEYMAP_FORMAT_TEXT_V1,
                            xkbcommon::xkb::KEYMAP_COMPILE_NO_FLAGS,
                        );
                        if let Some(km) = keymap {
                            let xkb_state = xkbcommon::xkb::State::new(&km);
                            state.xkb_keymap = Some(km);
                            state.xkb_state = Some(xkb_state);
                        }
                    }
                }
            }
            wl_keyboard::Event::Key { key, state: key_state, .. } => {
                let key_state = match key_state {
                    wayland_client::WEnum::Value(s) => s,
                    _ => return,
                };
                // XKB keycodes are evdev + 8
                let xkb_keycode: xkbcommon::xkb::Keycode = (key + 8).into();
                let sym: u32 = state
                    .xkb_state
                    .as_ref()
                    .map(|s| s.key_get_one_sym(xkb_keycode).raw())
                    .unwrap_or(0);
                let utf8 = state
                    .xkb_state
                    .as_ref()
                    .and_then(|s| {
                        let u = s.key_get_utf8(xkb_keycode);
                        if u.is_empty() { None } else { Some(u) }
                    });
                let _ = state.key_tx.send(KeyEvent {
                    key,
                    sym,
                    state: key_state,
                    utf8,
                });
            }
            wl_keyboard::Event::Modifiers {
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
                ..
            } => {
                if let Some(xkb_state) = state.xkb_state.as_mut() {
                    xkb_state.update_mask(
                        mods_depressed,
                        mods_latched,
                        mods_locked,
                        0,
                        0,
                        group,
                    );
                }
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Layer shell surface
// ---------------------------------------------------------------------------

impl Dispatch<zwlr_layer_shell_v1::ZwlrLayerShellV1, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &zwlr_layer_shell_v1::ZwlrLayerShellV1,
        _: zwlr_layer_shell_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for WaylandState {
    fn event(
        state: &mut Self,
        surface: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            _ => {log::debug!("WlLayerSurfaceEvent::{:?}",event);}
        };
        if let zwlr_layer_surface_v1::Event::Configure { serial, width, height } = event {
            surface.ack_configure(serial);
            state.layer_surface_configured = true;
            log::info!("layer surface configured (serial={serial}, width={width}, height={height})");
        };
    }
}

// ---------------------------------------------------------------------------
// Misc dispatches (buffer, surface, shm, pool, compositor, vptr)
// ---------------------------------------------------------------------------

impl Dispatch<wl_compositor::WlCompositor, ()> for WaylandState {
    fn event(_: &mut Self, _: &wl_compositor::WlCompositor, _: wl_compositor::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<wl_shm::WlShm, ()> for WaylandState {
    fn event(_: &mut Self, _: &wl_shm::WlShm, _: wl_shm::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<wl_shm_pool::WlShmPool, ()> for WaylandState {
    fn event(_: &mut Self, _: &wl_shm_pool::WlShmPool, _: wl_shm_pool::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<wl_buffer::WlBuffer, ()> for WaylandState {
    fn event(_: &mut Self, _: &wl_buffer::WlBuffer, _: wl_buffer::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<wl_region::WlRegion, ()> for WaylandState {
    fn event(_: &mut Self, _: &wl_region::WlRegion, _: wl_region::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<wl_surface::WlSurface, ()> for WaylandState {
    fn event(
        state: &mut Self,
        _proxy: &wl_surface::WlSurface,
        event: wl_surface::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let wl_surface::Event::Enter { output } = event {
            log::debug!("wl_surface.enter: output={:?}", output);
            state.surface_entered_output = Some(output);
        }
    }
}
impl Dispatch<zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1, ()> for WaylandState {
    fn event(_: &mut Self, _: &zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1, _: zwlr_virtual_pointer_manager_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1, ()> for WaylandState {
    fn event(_: &mut Self, _: &zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1, _: zwlr_virtual_pointer_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

// ---------------------------------------------------------------------------
// Public API – connect, discover monitors, create overlays, warp pointer
// ---------------------------------------------------------------------------

/// Connect to the Wayland compositor, bind all required globals, discover monitors.
pub fn connect() -> Result<(WaylandState, EventQueue<WaylandState>)> {
    let conn = Connection::connect_to_env().context("cannot connect to Wayland display")?;
    let (globals, mut queue) = registry_queue_init::<WaylandState>(&conn)
        .context("failed to initialise registry")?;
    let mut state = WaylandState::new();
    let qh = queue.handle();

    // Bind globals from the GlobalList (registry_queue_init already did the roundtrip)
    log::debug!("compositor registry: {:?}", globals.contents().clone_list());
    state.compositor = globals.bind::<wl_compositor::WlCompositor, _, _>(&qh, 1..=6, ()).ok();
    state.shm = globals.bind::<wl_shm::WlShm, _, _>(&qh, 1..=1, ()).ok();
    state.seat = globals.bind::<wl_seat::WlSeat, _, _>(&qh, 1..=9, ()).ok();
    state.layer_shell = globals.bind::<zwlr_layer_shell_v1::ZwlrLayerShellV1, _, _>(&qh, 1..=5, ()).ok();
    state.vptr_mgr = globals.bind::<zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1, _, _>(&qh, 1..=2, ()).ok();
    state.xdg_output_mgr = globals.bind::<zxdg_output_manager_v1::ZxdgOutputManagerV1, _, _>(&qh, 1..=3, ()).ok();

    // Bind all wl_output instances
    let global_list = globals.contents().clone_list();
    // log::debug!("{:#?}", global_list);
    for g in &global_list {
        if g.interface == "wl_output" {
            let output: wl_output::WlOutput = globals
                .registry()
                .bind(g.name, g.version.min(4), &qh, ());
            state.pending_outputs.push(PendingOutput {
                wl_output: Some(output),
                ..Default::default()
            });
        }
    }

    // Validate required globals
    if state.compositor.is_none() { bail!("wl_compositor not available"); }
    if state.shm.is_none() { bail!("wl_shm not available"); }
    if state.layer_shell.is_none() { bail!("zwlr_layer_shell_v1 not available – is this a wlroots compositor?"); }
    if state.vptr_mgr.is_none() { bail!("zwlr_virtual_pointer_manager_v1 not available"); }

    // Roundtrip to receive output geometry events
    queue.roundtrip(&mut state)?;

    // Request xdg_output for logical geometry if available
    if let Some(ref mgr) = state.xdg_output_mgr {
        let qh = queue.handle();
        for (idx, po) in state.pending_outputs.iter().enumerate() {
            if let Some(ref wl_out) = po.wl_output {
                mgr.get_xdg_output(wl_out, &qh, idx);
            }
        }
    }

    // Second roundtrip to collect xdg output info
    queue.roundtrip(&mut state)?;

    // Materialise monitors from pending_outputs
    state.monitors = state
        .pending_outputs
        .iter()
        .filter(|p| p.done || p.width > 0)
        .map(|p| Monitor {
            name: if p.name.is_empty() { "unknown".into() } else { p.name.clone() },
            x: p.x,
            y: p.y,
            width: p.width,
            height: p.height,
            scale: if p.scale > 0 { p.scale } else { 1 },
            transform: p.transform.unwrap_or(wl_output::Transform::Normal),
            wl_output: p.wl_output.clone().unwrap(),
        })
        .collect();

    if state.monitors.is_empty() {
        bail!("no monitors detected");
    }

    // Create virtual pointer
    if let (Some(ref mgr), Some(ref seat)) = (&state.vptr_mgr, &state.seat) {
        let qh = queue.handle();
        state.vptr = Some(mgr.create_virtual_pointer(Some(seat), &qh, ()));
    }

    // Request keyboard
    if let Some(ref seat) = state.seat {
        let qh = queue.handle();
        if state.keyboard.is_none() {
            state.keyboard = Some(seat.get_keyboard(&qh, ()));
        }
    }
    queue.roundtrip(&mut state)?;

    log::info!(
        "connected – {} monitor(s): {}",
        state.monitors.len(),
        state
            .monitors
            .iter()
            .map(|m| format!(
                "{} ({}x{} @ {},{} transform={:?})",
                m.name, m.width, m.height, m.x, m.y, m.transform
            ))
            .collect::<Vec<_>>()
            .join(", ")
    );
    // log::debug!("{:#?}", &state);

    Ok((state, queue))
}

/// Allocate an shm buffer of the given size.
pub fn create_shm_buffer(
    shm: &wl_shm::WlShm,
    qh: &QueueHandle<WaylandState>,
    width: i32,
    height: i32,
) -> Result<ShmBuffer> {
    let stride = width * 4; // ARGB8888
    let size = (stride * height) as usize;

    // Create an anonymous shared memory file
    let name = format!("warpd-rs-{}", std::process::id());
    let fd = rustix::shm::shm_open(
        &*name,
        rustix::shm::ShmOFlags::CREATE | rustix::shm::ShmOFlags::EXCL | rustix::shm::ShmOFlags::RDWR,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )?;
    rustix::shm::shm_unlink(&*name)?;
    rustix::fs::ftruncate(&fd, size as u64)?;

    let data = unsafe { memmap2::MmapOptions::new().len(size).map_mut(&fd)? };

    let pool = shm.create_pool(fd.as_fd(), size as i32, qh, ());
    let buffer = pool.create_buffer(
        0,
        width,
        height,
        stride,
        wl_shm::Format::Argb8888,
        qh,
        (),
    );

    Ok(ShmBuffer {
        width,
        height,
        stride,
        fd,
        pool,
        buffer,
        data,
    })
}

/// Determine which monitor currently has focus by creating a layer-shell
/// surface with no target output (NULL). The compositor places the surface on
/// the focused output and fires a `wl_surface.enter` event with the chosen
/// `wl_output`. This works even when the pointer has not been moved recently
/// (e.g. invoked via a keyboard shortcut).
pub fn find_focused_monitor(
    state: &mut WaylandState,
    queue: &mut EventQueue<WaylandState>,
) -> Result<Monitor> {
    if state.monitors.len() <= 1 {
        return Ok(state.monitors[0].clone());
    }

    let qh = queue.handle();

    // Create a layer surface with NO target output — the compositor will
    // assign it to the currently focused output.
    let surface = state.compositor.as_ref().unwrap().create_surface(&qh, ());
    let layer_surface = state.layer_shell.as_ref().unwrap().get_layer_surface(
        &surface,
        None, // NULL output → compositor picks the focused one
        zwlr_layer_shell_v1::Layer::Overlay,
        "warpd-rs-probe".to_string(),
        &qh,
        (),
    );
    layer_surface.set_anchor(
        zwlr_layer_surface_v1::Anchor::Top
            | zwlr_layer_surface_v1::Anchor::Bottom
            | zwlr_layer_surface_v1::Anchor::Left
            | zwlr_layer_surface_v1::Anchor::Right,
    );
    layer_surface.set_exclusive_zone(-1);
    layer_surface.set_keyboard_interactivity(
        zwlr_layer_surface_v1::KeyboardInteractivity::None,
    );

    // Set an empty input region so the probe doesn't steal pointer events.
    let region = state.compositor.as_ref().unwrap().create_region(&qh, ());
    region.add(0, 0, 0, 0);
    surface.set_input_region(Some(&region));
    surface.commit();

    // Wait for the configure event so we can map the surface.
    state.layer_surface_configured = false;
    queue.roundtrip(state)?;

    // Attach a tiny 1×1 transparent buffer so the compositor considers the
    // surface "mapped" and sends the wl_surface.enter event.
    let qh = queue.handle();
    let probe_buf = create_shm_buffer(state.shm.as_ref().unwrap(), &qh, 1, 1)?;
    // Clear to transparent.
    probe_buf.data[..4].iter().for_each(|_| {});
    surface.attach(Some(&probe_buf.buffer), 0, 0);
    surface.damage(0, 0, 1, 1);
    surface.commit();

    // Wait for the wl_surface.enter event (tells us which output).
    state.surface_entered_output = None;
    queue.flush()?;

    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(200);
    while state.surface_entered_output.is_none() {
        if std::time::Instant::now() >= deadline {
            break;
        }
        if let Some(guard) = queue.prepare_read() {
            let _ = guard.read();
        }
        queue.dispatch_pending(state)?;
        queue.flush()?;
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    // Match the entered wl_output to our known monitors.
    let focused_idx = state
        .surface_entered_output
        .as_ref()
        .and_then(|entered| {
            state.monitors.iter().position(|m| m.wl_output == *entered)
        });

    // Tear down the probe surface.
    state.surface_entered_output = None;
    drop(probe_buf);
    region.destroy();
    layer_surface.destroy();
    surface.destroy();
    queue.flush()?;

    let idx = focused_idx.unwrap_or(0);
    log::info!("focused monitor: {}", state.monitors[idx].name);
    Ok(state.monitors[idx].clone())
}

/// Create a full-screen overlay on the given monitor, returning the Overlay.
/// The overlay is an "overlay" layer surface that captures keyboard input.
/// This function blocks until the compositor sends a `configure` event.
pub fn create_overlay(
    state: &mut WaylandState,
    queue: &mut EventQueue<WaylandState>,
    monitor: &Monitor,
) -> Result<Overlay> {
    let qh = queue.handle();
    let compositor = state.compositor.as_ref().unwrap();
    let layer_shell = state.layer_shell.as_ref().unwrap();
    let shm = state.shm.as_ref().unwrap();

    let surface = compositor.create_surface(&qh, ());
    let layer_surface = layer_shell.get_layer_surface(
        &surface,
        Some(&monitor.wl_output),
        zwlr_layer_shell_v1::Layer::Overlay,
        "warpd-rs".to_string(),
        &qh,
        (),
    );

    // Configure: anchor to all edges, exclusive zone -1 (don't push other surfaces),
    // request keyboard interactivity.
    layer_surface.set_anchor(
        zwlr_layer_surface_v1::Anchor::Top
            | zwlr_layer_surface_v1::Anchor::Bottom
            | zwlr_layer_surface_v1::Anchor::Left
            | zwlr_layer_surface_v1::Anchor::Right,
    );
    layer_surface.set_exclusive_zone(-1);
    layer_surface.set_keyboard_interactivity(
        zwlr_layer_surface_v1::KeyboardInteractivity::Exclusive,
    );
    layer_surface.set_size(monitor.width as u32, monitor.height as u32);
    surface.commit();

    // Wait for the compositor to send a configure event before we attach a buffer.
    // Without this, the compositor will destroy the surface as "not mapped".
    state.layer_surface_configured = false;
    while !state.layer_surface_configured {
        queue.blocking_dispatch(state)?;
    }
    log::info!("overlay configured, creating shm buffer {}x{}", monitor.width, monitor.height);

    let shm = state.shm.as_ref().unwrap();
    let qh = queue.handle();
    let shm_buffer = create_shm_buffer(shm, &qh, monitor.width, monitor.height)?;

    Ok(Overlay {
        surface,
        layer_surface,
        shm_buffer,
        configured: true,
    })
}

/// Move the virtual pointer to absolute coordinates within the compositor's
/// logical coordinate space.
///
/// The zwlr_virtual_pointer_v1.motion_absolute protocol works as follows:
///   position = (x / x_extent, y / y_extent)  →  normalised [0,1] range
/// So we pass pixel coordinates relative to the bounding-box origin,
/// with the bounding-box dimensions as extents.
pub fn warp_pointer(
    state: &WaylandState,
    x: f64,
    y: f64,
) {
    if let Some(ref vptr) = state.vptr {
        // Compute the bounding box of all monitors (may have negative offsets)
        let min_x = state.monitors.iter().map(|m| m.x).min().unwrap_or(0);
        let min_y = state.monitors.iter().map(|m| m.y).min().unwrap_or(0);
        let max_x = state.monitors.iter().map(|m| m.x + m.width).max().unwrap_or(1);
        let max_y = state.monitors.iter().map(|m| m.y + m.height).max().unwrap_or(1);

        let extent_w = (max_x - min_x) as u32;
        let extent_h = (max_y - min_y) as u32;

        // Translate absolute compositor coordinates to bounding-box-relative
        let rel_x = (x - min_x as f64) as u32;
        let rel_y = (y - min_y as f64) as u32;

        let now = {
            let dur = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            dur.as_millis() as u32
        };

        log::debug!(
            "warp_pointer: abs=({x},{y}) rel=({rel_x},{rel_y}) extent=({extent_w},{extent_h})"
        );

        vptr.motion_absolute(now, rel_x, rel_y, extent_w, extent_h);
        vptr.frame();
    }
}

/// Simulate a mouse button click (press + release).
/// button: 1=left, 2=middle, 3=right  (mapped to Linux BTN_LEFT etc.)
pub fn click_button(state: &WaylandState, button: u32) {
    let code = match button {
        1 => 0x110, // BTN_LEFT
        2 => 0x112, // BTN_MIDDLE
        3 => 0x111, // BTN_RIGHT
        _ => return,
    };
    if let Some(ref vptr) = state.vptr {
        let now = {
            let dur = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            dur.as_millis() as u32
        };
        vptr.button(now, code, wl_pointer::ButtonState::Pressed);
        vptr.frame();
        vptr.button(now + 1, code, wl_pointer::ButtonState::Released);
        vptr.frame();
    }
}

/// Return the current pointer position for the given surface, if that surface
/// currently has pointer focus.
pub fn pointer_position_on_surface(
    state: &WaylandState,
    surface: &wl_surface::WlSurface,
) -> Option<(f64, f64)> {
    if state.pointer_focus_surface.as_ref() == Some(surface) {
        log::debug!("pointer position on surface: {:?}", state.pointer_surface_pos);
        return state.pointer_surface_pos;
    }
    None
}
