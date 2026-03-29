/// warpd-rs — a modal keyboard-driven virtual pointer for Wayland.
///
/// Usage:
///   warpd-rs --hint      Launch directly into hint mode
///   warpd-rs --grid      Launch directly into grid mode
///   warpd-rs --normal    Launch directly into normal (discrete) mode
///
/// Designed for wlroots-based compositors (Sway, Hyprland, etc.).
/// Bind one of these commands to a compositor hotkey, e.g. in Hyprland:
///
///   bind = SUPER ALT, x, exec, warpd-rs --hint
///   bind = SUPER ALT, g, exec, warpd-rs --grid
///   bind = SUPER ALT, c, exec, warpd-rs --normal
mod config;
mod hint;
mod input;
mod pointer;
#[cfg(feature = "opencv")]
mod target_detection;
mod wayland;

use anyhow::Result;
use clap::Parser;
use pointer::{click_button, resolve_initial_pointer_position, surface_pointer_pos, warp_pointer};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use wayland::KeyState;
use wayland_client::protocol::wl_output::Transform;

// see pump_wayland_nonblocking , seems necessary but not entirely sure why 
// it fixed laggy/unresponsive behavior of pointer in normal mode 
use rustix::event::{self, PollFd};
use rustix::io::Errno;

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(
    name = "warpd-rs",
    about = "Modal keyboard-driven pointer for Wayland",
    disable_version_flag = true
)]
struct Cli {
    /// Print version and compiled runtime features.
    #[arg(short = 'V', long = "version")]
    version: bool,

    /// Activate hint mode (show labelled hints, type to warp).
    #[arg(long)]
    hint: bool,

    /// Activate grid mode (recursive quadrant subdivision).
    #[arg(long)]
    grid: bool,

    /// Activate normal mode (hjkl cursor movement).
    #[arg(long)]
    normal: bool,

    /// Path to config TOML file. If set, default config search locations are skipped.
    #[arg(long)]
    config: Option<PathBuf>,

    /// Generate a default config file at the specified path, then exit.
    /// If the file already exists, it will be overwritten.
    #[arg(long)]
    generate_config: bool,
}

// ---------------------------------------------------------------------------
// Mode implementations
// ---------------------------------------------------------------------------

fn run_hint_mode(
    state: &mut wayland::WaylandState,
    queue: &mut wayland_client::EventQueue<wayland::WaylandState>,
    cfg: &config::Config,
) -> Result<()> {
    let monitor = wayland::find_focused_monitor(state, queue)?;
    let hint_source = cfg.hint_source.trim().to_ascii_lowercase();
    let hints = match hint_source.as_str() {
        "stdin" => {
            let areas = hint::read_target_areas_from_stdin()?;
            let areas = hint::normalize_areas_for_monitor(&monitor, &areas);
            if areas.is_empty() {
                log::warn!("hint_source=stdin but no valid areas received, falling back to grid");
                hint::generate_hints(&monitor, cfg)
            } else {
                hint::generate_hints_from_areas(cfg, &areas)
            }
        }
        "detect" => {
            #[cfg(feature = "opencv")]
            {
                if !wayland::supports_screencopy(state) {
                    log::warn!(
                        "hint_source=detect requires zwlr_screencopy_manager_v1; falling back to grid"
                    );
                    hint::generate_hints(&monitor, cfg)
                } else {
                    match wayland::capture_output_frame(state, queue, &monitor)
                        .and_then(|f| target_detection::detect_target_areas(&f, &monitor))
                    {
                        Ok(areas) if !areas.is_empty() => {
                            let areas = hint::normalize_areas_for_monitor(&monitor, &areas);
                            if areas.is_empty() {
                                log::warn!("target detection returned off-monitor areas; falling back to grid");
                                hint::generate_hints(&monitor, cfg)
                            } else {
                                hint::generate_hints_from_areas(cfg, &areas)
                            }
                        }
                        Ok(_) => {
                            log::warn!("target detection found no targets; falling back to grid");
                            hint::generate_hints(&monitor, cfg)
                        }
                        Err(e) => {
                            log::warn!("target detection failed ({e}), falling back to grid");
                            hint::generate_hints(&monitor, cfg)
                        }
                    }
                }
            }
            #[cfg(not(feature = "opencv"))]
            {
                log::warn!(
                    "hint_source=detect requested but binary was built without --features opencv; falling back to grid"
                );
                hint::generate_hints(&monitor, cfg)
            }
        }
        "static" => hint::generate_hints(&monitor, cfg),
        _ => {
            log::warn!(
                "unknown hint_source '{}', falling back to grid",
                cfg.hint_source
            );
            hint::generate_hints(&monitor, cfg)
        }
    };

    let mut overlay = wayland::create_overlay(state, queue, &monitor)?;
    let mut typed = String::new();

    // Initial draw
    hint::draw_hints(&mut overlay, &hints, cfg, &typed)?;
    queue.flush()?;

    // Event loop: blocking_dispatch reads from the Wayland socket and dispatches
    // events (including keyboard events which get sent to key_tx channel).
    loop {
        // This blocks until at least one event arrives from the compositor,
        // dispatches all pending events (triggering our wl_keyboard handler
        // which pushes KeyEvents into key_tx), then returns.
        queue.blocking_dispatch(state)?;

        // Process all key events that were dispatched
        while let Ok(event) = state.key_rx.try_recv() {
            match hint::process_key(&event, &hints, &mut typed) {
                hint::HintResult::Selected { x, y } => {
                    // Warp the pointer to the selected hint (offset by monitor position)
                    let abs_x = monitor.x as f64 + x;
                    let abs_y = monitor.y as f64 + y;

                    // CRITICAL: Send the pointer motion BEFORE destroying the overlay.
                    // Then flush + roundtrip to guarantee the compositor processes it
                    // before we tear down our surfaces and exit.
                    warp_pointer(state, abs_x, abs_y);
                    queue.flush()?;
                    queue.roundtrip(state)?;

                    // Now safe to tear down the overlay
                    overlay.layer_surface.destroy();
                    overlay.surface.destroy();
                    click_button(state, 1);
                    queue.flush()?;

                    // log::info!("hint selected → warp to ({abs_x}, {abs_y})");
                    return Ok(());
                }
                hint::HintResult::Cancel => {
                    overlay.layer_surface.destroy();
                    overlay.surface.destroy();
                    queue.flush()?;
                    log::info!("hint mode cancelled");
                    return Ok(());
                }
                hint::HintResult::Continue => {
                    // Redraw with updated filter
                    hint::draw_hints(&mut overlay, &hints, cfg, &typed)?;
                    queue.flush()?;
                }
            }
        }
    }
}

fn run_grid_mode(
    state: &mut wayland::WaylandState,
    queue: &mut wayland_client::EventQueue<wayland::WaylandState>,
    cfg: &config::Config,
) -> Result<()> {
    let monitor = wayland::find_focused_monitor(state, queue)?;
    let mut overlay = wayland::create_overlay(state, queue, &monitor)?;

    let grid_keysyms = input::bindings_to_keysyms(
        &cfg.grid_quadrant_keys,
        &input::DEFAULT_GRID_QUADRANT_KEYSYMS,
        "grid_quadrant_keys",
    );
    let button_keysyms =
        input::bindings_to_keysyms(&cfg.buttons, &input::DEFAULT_BUTTON_KEYSYMS, "buttons");

    let mut sel = hint::GridSelection::new(&monitor);

    hint::draw_grid(&mut overlay, &sel, cfg)?;
    queue.flush()?;

    loop {
        queue.blocking_dispatch(state)?;

        while let Ok(event) = state.key_rx.try_recv() {
            if event.state != KeyState::Pressed {
                continue;
            }

            if event.sym == input::keysyms::ESCAPE {
                overlay.layer_surface.destroy();
                overlay.surface.destroy();
                queue.flush()?;
                return Ok(());
            }

            if let Some(idx) = grid_keysyms.iter().position(|&sym| sym == event.sym) {
                sel.subdivide(idx);

                /*
                If the grid is small enough, warp
                */
                if sel.w < cfg.grid_min_size && sel.h < cfg.grid_min_size {
                    let (cx, cy) = sel.centre();
                    let abs_x = monitor.x as f64 + cx;
                    let abs_y = monitor.y as f64 + cy;
                    warp_pointer(state, abs_x, abs_y);
                    queue.flush()?;
                    queue.roundtrip(state)?;
                    overlay.layer_surface.destroy();
                    overlay.surface.destroy();
                    click_button(state, 1);
                    queue.flush()?;
                    return Ok(());
                }

                hint::draw_grid(&mut overlay, &sel, cfg)?;
                queue.flush()?;
                continue;
            }

            if event.sym == button_keysyms[0] {
                let (cx, cy) = sel.centre();
                let abs_x = monitor.x as f64 + cx;
                let abs_y = monitor.y as f64 + cy;
                warp_pointer(state, abs_x, abs_y);
                click_button(state, 1);
                queue.flush()?;
                queue.roundtrip(state)?;
                overlay.layer_surface.destroy();
                overlay.surface.destroy();
                queue.flush()?;
                return Ok(());
            }
        }
    }
}

fn run_normal_mode(
    state: &mut wayland::WaylandState,
    queue: &mut wayland_client::EventQueue<wayland::WaylandState>,
    cfg: &config::Config,
) -> Result<()> {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum HeldKeyRole {
        MoveLeft,
        MoveDown,
        MoveUp,
        MoveRight,
        SpeedModifier,
    }

    let normalize_keysym = |sym: u32| -> u32 {
        if (b'A' as u32..=b'Z' as u32).contains(&sym) {
            sym + 32
        } else {
            sym
        }
    };

    let mut monitor = wayland::find_focused_monitor(state, queue)?;
    let mut overlay = wayland::create_overlay(state, queue, &monitor)?;

    let move_keysyms = input::bindings_to_keysyms(
        &cfg.normal_move_keys,
        &input::DEFAULT_NORMAL_MOVE_KEYSYMS,
        "normal_move_keys",
    );
    let [left_key, down_key, up_key, right_key] = move_keysyms.map(normalize_keysym);
    let button_keysyms =
        input::bindings_to_keysyms(&cfg.buttons, &input::DEFAULT_BUTTON_KEYSYMS, "buttons")
            .map(normalize_keysym);
    let speed_modifier_syms: Vec<u32> = {
        let binding = cfg.speed_modifier_key.trim();
        if binding.eq_ignore_ascii_case("shift") {
            input::SHIFT_KEYS
                .iter()
                .copied()
                .map(normalize_keysym)
                .collect()
        } else if let Some(sym) = input::binding_to_keysym(binding) {
            vec![normalize_keysym(sym)]
        } else {
            log::warn!(
                "invalid speed_modifier_key '{}'; defaulting to both Shift keys",
                cfg.speed_modifier_key
            );
            input::SHIFT_KEYS
                .iter()
                .copied()
                .map(normalize_keysym)
                .collect()
        }
    };
    let speed_modifier_multiplier = if cfg.speed_modifier_multiplier > 0.0 {
        cfg.speed_modifier_multiplier
    } else {
        log::warn!(
            "speed_modifier_multiplier must be > 0 (got {}); using 2.0",
            cfg.speed_modifier_multiplier
        );
        2.0
    };

    let point_in_monitor = |x: f64, y: f64, m: &wayland::Monitor, tol: f64| {
        x >= (m.x as f64 - tol)
            && x < ((m.x + m.width) as f64 + tol)
            && y >= (m.y as f64 - tol)
            && y < ((m.y + m.height) as f64 + tol)
    };

    // Current cursor position (fallback to monitor centre until pointer focus is known)
    let mut cx = monitor.width as f64 / 2.0;
    let mut cy = monitor.height as f64 / 2.0;
    let mut last_pointer_motion_sample: Option<(f64, f64)> = None;
    let base_speed = cfg.speed as f64 / 60.0; // pixels per frame at 60fps
    let mut last_frame = Instant::now();
    let target_frame = Duration::from_micros(8_333); // ~120fps
    let mut held_keys: HashMap<u32, HeldKeyRole> = HashMap::new();

    // very unsure about what this is doing (🤖) but it works 🤷... 
    let pump_wayland_nonblocking =
        |state: &mut wayland::WaylandState,
         queue: &mut wayland_client::EventQueue<wayland::WaylandState>|
         -> Result<()> {
            if let Some(guard) = queue.prepare_read() {
                let fd = guard.connection_fd();
                let mut fds = [PollFd::new(
                    &fd,
                    event::PollFlags::IN | event::PollFlags::ERR,
                )];

                match event::poll(&mut fds, 0) {
                    Ok(ready) if ready > 0 => {
                        let _ = guard.read();
                    }
                    Ok(_) => {
                        // No compositor events ready right now; drop guard to cancel read.
                    }
                    Err(Errno::INTR) => {
                        // Interrupted syscall: try again next frame.
                    }
                    Err(err) => {
                        return Err(anyhow::anyhow!(
                            "failed polling Wayland socket in normal mode: {err}"
                        ));
                    }
                }
            }

            queue.dispatch_pending(state)?;
            queue.flush()?;
            Ok(())
        };

    // Draw a small cursor indicator
    let draw_cursor = |overlay: &mut wayland::Overlay, cx: f64, cy: f64| -> Result<()> {
        let width: i32 = overlay.shm_buffer.width;
        let height = overlay.shm_buffer.height;
        let stride = overlay.shm_buffer.stride;
        let data: &mut [u8] = &mut overlay.shm_buffer.data;
        let surface = unsafe {
            cairo::ImageSurface::create_for_data_unsafe(
                data.as_mut_ptr(),
                cairo::Format::ARgb32,
                width,
                height,
                stride,
            )?
        };
        let cr = cairo::Context::new(&surface)?;

        // Mostly transparent
        cr.set_operator(cairo::Operator::Source);
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.05);
        cr.paint()?;

        // Cursor dot
        let (r, g, b, _) = config::parse_hex_color(&cfg.cursor_color);
        cr.set_operator(cairo::Operator::Over);
        cr.set_source_rgba(r, g, b, 0.9);
        cr.arc(
            cx,
            cy,
            cfg.cursor_size as f64,
            0.0,
            2.0 * std::f64::consts::PI,
        );
        cr.fill()?;

        // Crosshair lines
        cr.set_source_rgba(r, g, b, 0.3);
        cr.set_line_width(cfg.crosshair_line_width as f64);
        cr.move_to(cx, 0.0);
        cr.line_to(cx, height as f64);
        cr.stroke()?;
        cr.move_to(0.0, cy);
        cr.line_to(width as f64, cy);
        cr.stroke()?;

        drop(cr);
        surface.flush();

        overlay
            .surface
            .attach(Some(&overlay.shm_buffer.buffer), 0, 0);
        overlay.surface.damage_buffer(0, 0, width, height);
        overlay.surface.commit();
        Ok(())
    };

    draw_cursor(&mut overlay, cx, cy)?;
    queue.flush()?;

    // Give Wayland a chance to deliver pointer enter/motion for the new overlay.
    queue.roundtrip(state)?;
    let startup_deadline = Instant::now() + Duration::from_millis(25);
    while Instant::now() < startup_deadline {
        if let Some((_, _, source)) = pointer::surface_pointer_pos(state, &overlay.surface) {
            if source == wayland::PointerPosSource::Motion {
                break;
            }
        }

        pump_wayland_nonblocking(state, queue)?;
        std::thread::sleep(Duration::from_millis(1));
    }

    if let Some((px, py, source)) = pointer::surface_pointer_pos(state, &overlay.surface) {
        if source == wayland::PointerPosSource::Motion {
            last_pointer_motion_sample = Some((px, py));
        }
        let mut init_x = px;
        let mut init_y = py;

        // wl_pointer coordinates are expected to be surface-local, but some
        // compositor/output arrangements can initially report values that map
        // to a different output. Infer an absolute position from the current
        // monitor and retarget the overlay if needed.
        let inferred_abs_x = monitor.x as f64 + px;
        let inferred_abs_y = monitor.y as f64 + py;

        if source == wayland::PointerPosSource::Motion {
            if let Some(pointer_monitor) = state
                .monitors
                .iter()
                .find(|m| point_in_monitor(inferred_abs_x, inferred_abs_y, m, 1.0))
                .cloned()
            {
                if pointer_monitor.wl_output != monitor.wl_output {
                    log::info!(
                        "normal mode retargeting overlay: focused={} -> pointer={}",
                        monitor.name,
                        pointer_monitor.name
                    );

                    overlay.layer_surface.destroy();
                    overlay.surface.destroy();
                    queue.flush()?;

                    monitor = pointer_monitor;
                    overlay = wayland::create_overlay(state, queue, &monitor)?;
                }

                init_x = inferred_abs_x - monitor.x as f64;
                init_y = inferred_abs_y - monitor.y as f64;
            }
        } else {
            log::debug!(
                "normal init using Enter-only pointer sample; skipping monitor retarget: raw=({:.2},{:.2}) monitor={}",
                px,
                py,
                monitor.name
            );
        }

        // Some compositors report pointer coords here as surface-local, others
        // effectively as global logical coords. Pick the value that fits.
        let allow_global_map = source == wayland::PointerPosSource::Motion;
        if let Some((mapping, mapped_x, mapped_y)) = resolve_initial_pointer_position(
            init_x,
            init_y,
            monitor.x as f64,
            monitor.y as f64,
            monitor.width as f64,
            monitor.height as f64,
            allow_global_map,
        ) {
            cx = mapped_x;
            cy = mapped_y;
            log::debug!(
                "normal init pointer mapping: {} raw=({:.2},{:.2}) mapped=({:.2},{:.2}) monitor={}",
                mapping,
                init_x,
                init_y,
                cx,
                cy,
                monitor.name
            );
        } else {
            log::debug!(
                "normal init pointer coords unusable, keeping fallback position: raw=({:.2},{:.2}) monitor={}",
                init_x,
                init_y,
                monitor.name
            );
        }

        let max_x = monitor.width as f64 - 1.0;
        let max_y = monitor.height as f64 - 1.0;

        // Normalize for outputs that advertise transforms (e.g. horizontal flip).
        let tx = monitor.transform;
        let (transform_case, nx, ny) = match tx {
            Transform::Normal => ("Normal", cx, cy),
            Transform::_90 => ("_90", cy, max_x - cx),
            Transform::_180 => ("_180", max_x - cx, max_y - cy),
            Transform::_270 => ("_270", max_y - cy, cx),
            Transform::Flipped => ("Flipped", max_x - cx, cy),
            Transform::Flipped90 => ("Flipped90", max_y - cy, max_x - cx),
            Transform::Flipped180 => ("Flipped180", cx, max_y - cy),
            Transform::Flipped270 => ("Flipped270", cy, cx),
            _ => ("Unknown/Fallback", cx, cy),
        };
        log::debug!(
            "normal init transform match: {:?} -> {} (raw=({:.2},{:.2}) mapped=({:.2},{:.2}))",
            tx,
            transform_case,
            cx,
            cy,
            nx,
            ny
        );
        cx = cx.clamp(0.0, max_x);
        cy = cy.clamp(0.0, max_y);

        draw_cursor(&mut overlay, cx, cy)?;
        queue.flush()?;
    }

    loop {
        let frame_start = Instant::now();

        // We use prepare_read + read_events + dispatch_pending so we don't
        // block forever (we need to process held-key movement each frame).
        pump_wayland_nonblocking(state, queue)?;

        let mut moved = false;
        let keyboard_steering_active = held_keys.values().any(|role| {
            matches!(
                role,
                HeldKeyRole::MoveLeft
                    | HeldKeyRole::MoveRight
                    | HeldKeyRole::MoveUp
                    | HeldKeyRole::MoveDown
            )
        });

        // Keep crosshair bound to physical pointer movement while in normal mode.
        if !keyboard_steering_active {
            if let Some((px, py, source)) = surface_pointer_pos(state, &overlay.surface) {
                if source == wayland::PointerPosSource::Motion {
                    let changed = last_pointer_motion_sample
                        .map(|(lx, ly)| (lx - px).abs() > 0.01 || (ly - py).abs() > 0.01)
                        .unwrap_or(true);

                    if changed {
                        let max_x = monitor.width as f64 - 1.0;
                        let max_y = monitor.height as f64 - 1.0;
                        if (0.0..=max_x).contains(&px) && (0.0..=max_y).contains(&py) {
                            cx = px;
                            cy = py;
                            moved = true;
                        }
                        last_pointer_motion_sample = Some((px, py));
                    }
                }
            }
        }

        // Collect all pending key events
        while let Ok(event) = state.key_rx.try_recv() {
            let event_sym = normalize_keysym(event.sym);

            let key_role = if event_sym == left_key {
                Some(HeldKeyRole::MoveLeft)
            } else if event_sym == down_key {
                Some(HeldKeyRole::MoveDown)
            } else if event_sym == up_key {
                Some(HeldKeyRole::MoveUp)
            } else if event_sym == right_key {
                Some(HeldKeyRole::MoveRight)
            } else if speed_modifier_syms.contains(&event_sym) {
                Some(HeldKeyRole::SpeedModifier)
            } else {
                None
            };

            match event.state {
                KeyState::Pressed => {
                    if let Some(role) = key_role {
                        held_keys.entry(event.key).or_insert(role);
                    }
                }
                KeyState::Released => {
                    held_keys.remove(&event.key);
                }
                _ => {}
            }

            if event.state == KeyState::Pressed {
                if event_sym == input::keysyms::ESCAPE {
                    overlay.layer_surface.destroy();
                    overlay.surface.destroy();
                    queue.flush()?;
                    return Ok(());
                }

                if event_sym == button_keysyms[0] {
                    let abs_x = monitor.x as f64 + cx;
                    let abs_y = monitor.y as f64 + cy;
                    warp_pointer(state, abs_x, abs_y);
                    queue.flush()?;
                    queue.roundtrip(state)?;
                    overlay.layer_surface.destroy();
                    overlay.surface.destroy();
                    click_button(state, 1);
                    queue.flush()?;
                    return Ok(());
                }

                if event_sym == button_keysyms[1] {
                    let abs_x = monitor.x as f64 + cx;
                    let abs_y = monitor.y as f64 + cy;
                    warp_pointer(state, abs_x, abs_y);
                    click_button(state, 2);
                    queue.flush()?;
                    queue.roundtrip(state)?;
                    overlay.layer_surface.destroy();
                    overlay.surface.destroy();
                    queue.flush()?;
                    return Ok(());
                }

                if event_sym == button_keysyms[2] {
                    let abs_x = monitor.x as f64 + cx;
                    let abs_y = monitor.y as f64 + cy;
                    warp_pointer(state, abs_x, abs_y);
                    queue.flush()?;
                    queue.roundtrip(state)?;
                    overlay.layer_surface.destroy();
                    overlay.surface.destroy();
                    click_button(state, 3);
                    queue.flush()?;
                    return Ok(());
                }

                if event_sym == input::keysyms::X {
                    overlay.layer_surface.destroy();
                    overlay.surface.destroy();
                    queue.flush()?;
                    return run_hint_mode(state, queue, cfg);
                }

                if event_sym == input::keysyms::G {
                    overlay.layer_surface.destroy();
                    overlay.surface.destroy();
                    queue.flush()?;
                    return run_grid_mode(state, queue, cfg);
                }
            }
        }

        // Continuous movement for held directional keys
        let now = Instant::now();
        let dt = now.duration_since(last_frame).as_secs_f64().min(1.0 / 20.0);
        last_frame = now;

        let mut frame_speed = base_speed * dt * 60.0; // normalise to ~60fps
        let speed_modifier_active = held_keys
            .values()
            .any(|role| *role == HeldKeyRole::SpeedModifier);
        if speed_modifier_active {
            frame_speed *= speed_modifier_multiplier;
        }

        let moving_left = held_keys.values().any(|role| *role == HeldKeyRole::MoveLeft);
        let moving_right = held_keys
            .values()
            .any(|role| *role == HeldKeyRole::MoveRight);
        let moving_up = held_keys.values().any(|role| *role == HeldKeyRole::MoveUp);
        let moving_down = held_keys.values().any(|role| *role == HeldKeyRole::MoveDown);

        if moving_left {
            cx = (cx - frame_speed).max(0.0);
            moved = true;
        }
        if moving_right {
            cx = (cx + frame_speed).min(monitor.width as f64 - 1.0);
            moved = true;
        }
        if moving_up {
            cy = (cy - frame_speed).max(0.0);
            moved = true;
        }
        if moving_down {
            cy = (cy + frame_speed).min(monitor.height as f64 - 1.0);
            moved = true;
        }

        if moved {
            draw_cursor(&mut overlay, cx, cy)?;
            queue.flush()?;
        }

        if let Some(remaining) = target_frame.checked_sub(frame_start.elapsed()) {
            std::thread::sleep(remaining);
        }
    }
}

fn runtime_features_string() -> &'static str {
    #[cfg(feature = "opencv")]
    {
        "opencv"
    }

    #[cfg(not(feature = "opencv"))]
    {
        "none"
    }
}

/// ---------------------------------------------------------------------------
/// Entry point
/// ---------------------------------------------------------------------------
fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.version {
        println!(
            "{} {} ({})",
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION"),
            runtime_features_string()
        );
        return Ok(());
    }

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .init();

    let cfg = config::Config::load(cli.config.as_deref());
    let (mut state, mut queue) = wayland::connect()?;

    if cli.hint {
        run_hint_mode(&mut state, &mut queue, &cfg)?;
    } else if cli.grid {
        run_grid_mode(&mut state, &mut queue, &cfg)?;
    } else if cli.normal {
        run_normal_mode(&mut state, &mut queue, &cfg)?;
    }
    else if cli.generate_config {
        let pwd = std::env::current_dir()?;
        let path =  pwd.join("config.toml");
        config::Config::create_config(&path)?;
        log::info!("Default config written to {}", pwd.display());
    }

    Ok(())
}
