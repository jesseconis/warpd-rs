///! warpd-rs — a modal keyboard-driven virtual pointer for Wayland.
///!
///! Usage:
///!   warpd-rs --hint      Launch directly into hint mode
///!   warpd-rs --grid      Launch directly into grid mode
///!   warpd-rs --normal    Launch directly into normal (discrete) mode
///!
///! Designed for wlroots-based compositors (Sway, Hyprland, etc.).
///! Bind one of these commands to a compositor hotkey, e.g. in Hyprland:
///!
///!   bind = SUPER ALT, x, exec, warpd-rs --hint
///!   bind = SUPER ALT, g, exec, warpd-rs --grid
///!   bind = SUPER ALT, c, exec, warpd-rs --normal

mod config;
mod hint;
mod input;
mod wayland;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use wayland::KeyState;

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(name = "warpd-rs", about = "Modal keyboard-driven pointer for Wayland")]
struct Cli {
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
    let mut overlay = wayland::create_overlay(state, queue, &monitor)?;

    // Generate hints
    let hints = hint::generate_hints(&monitor, cfg);
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
                    wayland::warp_pointer(state, abs_x, abs_y);
                    queue.flush()?;
                    queue.roundtrip(state)?;

                    // Now safe to tear down the overlay
                    overlay.layer_surface.destroy();
                    overlay.surface.destroy();
                    wayland::click_button(state, 1);
                    queue.flush()?;

                    log::info!("hint selected → warp to ({abs_x}, {abs_y})");
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

    let mut sel = hint::GridSelection::new(&monitor);

    hint::draw_grid(&mut overlay, &sel, cfg)?;
    queue.flush()?;

    loop {
        queue.blocking_dispatch(state)?;

        while let Ok(event) = state.key_rx.try_recv() {
            if event.state != KeyState::Pressed {
                continue;
            }
            match event.sym {
                input::keysyms::ESCAPE => {
                    overlay.layer_surface.destroy();
                    overlay.surface.destroy();
                    queue.flush()?;
                    return Ok(());
                }
                // Quadrant selection
                input::keysyms::U | input::keysyms::I
                | input::keysyms::J | input::keysyms::K => {
                    let ch = match event.sym {
                        input::keysyms::U => 'u',
                        input::keysyms::I => 'i',
                        input::keysyms::J => 'j',
                        input::keysyms::K => 'k',
                        _ => unreachable!(),
                    };
                    sel.subdivide(ch);

                    // If the grid is small enough, warp
                    if sel.w < cfg.grid_min_size && sel.h < cfg.grid_min_size {
                        let (cx, cy) = sel.centre();
                        let abs_x = monitor.x as f64 + cx;
                        let abs_y = monitor.y as f64 + cy;
                        wayland::warp_pointer(state, abs_x, abs_y);
                        queue.flush()?;
                        queue.roundtrip(state)?;
                        overlay.layer_surface.destroy();
                        overlay.surface.destroy();
                        queue.flush()?;
                        log::info!("grid selected → warp to ({abs_x}, {abs_y})");
                        return Ok(());
                    }

                    hint::draw_grid(&mut overlay, &sel, cfg)?;
                    queue.flush()?;
                }
                // Mouse buttons
                input::keysyms::M => {
                    let (cx, cy) = sel.centre();
                    let abs_x = monitor.x as f64 + cx;
                    let abs_y = monitor.y as f64 + cy;
                    wayland::warp_pointer(state, abs_x, abs_y);
                    wayland::click_button(state, 1);
                    queue.flush()?;
                    queue.roundtrip(state)?;
                    overlay.layer_surface.destroy();
                    overlay.surface.destroy();
                    queue.flush()?;
                    return Ok(());
                }
                _ => {}
            }
        }
    }
}

fn run_normal_mode(
    state: &mut wayland::WaylandState,
    queue: &mut wayland_client::EventQueue<wayland::WaylandState>,
    cfg: &config::Config,
) -> Result<()> {
    let monitor = wayland::find_focused_monitor(state, queue)?;
    let mut overlay = wayland::create_overlay(state, queue, &monitor)?;

    // Current cursor position (fallback to monitor centre until pointer focus is known)
    let mut cx = monitor.width as f64 / 2.0;
    let mut cy = monitor.height as f64 / 2.0;
    let base_speed = cfg.speed as f64 / 60.0; // pixels per frame at 60fps
    let mut last_frame = Instant::now();
    let mut held_keys: std::collections::HashSet<u32> = std::collections::HashSet::new();

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
        cr.arc(cx, cy, cfg.cursor_size as f64, 0.0, 2.0 * std::f64::consts::PI);
        cr.fill()?;

        // Crosshair lines
        cr.set_source_rgba(r, g, b, 0.3);
        cr.set_line_width(1.0);
        cr.move_to(cx, 0.0);
        cr.line_to(cx, height as f64);
        cr.stroke()?;
        cr.move_to(0.0, cy);
        cr.line_to(width as f64, cy);
        cr.stroke()?;

        drop(cr);
        surface.flush();

        overlay.surface.attach(Some(&overlay.shm_buffer.buffer), 0, 0);
        overlay.surface.damage_buffer(0, 0, width, height);
        overlay.surface.commit();
        Ok(())
    };

    draw_cursor(&mut overlay, cx, cy)?;
    queue.flush()?;

    // Give Wayland a chance to deliver pointer enter/motion for the new overlay.
    queue.roundtrip(state)?;
    if let Some((px, py)) = wayland::pointer_position_on_surface(state, &overlay.surface) {
        // Some compositors report pointer coords here as surface-local, others
        // effectively as global logical coords. Pick the value that fits.
        let max_x = monitor.width as f64 - 1.0;
        let max_y = monitor.height as f64 - 1.0;

        let local_x = px;
        let global_to_local_x = px - monitor.x as f64;
        cx = if (0.0..=max_x).contains(&local_x) {
            local_x
        } else if (0.0..=max_x).contains(&global_to_local_x) {
            global_to_local_x
        } else {
            local_x.clamp(0.0, max_x)
        };

        let local_y = py;
        let global_to_local_y = py - monitor.y as f64;
        cy = if (0.0..=max_y).contains(&local_y) {
            local_y
        } else if (0.0..=max_y).contains(&global_to_local_y) {
            global_to_local_y
        } else {
            local_y.clamp(0.0, max_y)
        };

        // Normalize for outputs that advertise transforms (e.g. horizontal flip).
        // let tx = monitor.transform;
        // let (transform_case, nx, ny) = match tx {
        //     // wl_output::Transform::Normal => ("Normal", cx, cy),
        //     wl_output::Transform::_90 => ("_90", cy, max_x - cx),
        //     wl_output::Transform::_180 => ("_180", max_x - cx, max_y - cy),
        //     wl_output::Transform::_270 => ("_270", max_y - cy, cx),
        //     wl_output::Transform::Flipped => ("Flipped", max_x - cx, cy),
        //     wl_output::Transform::Flipped90 => ("Flipped90", max_y - cy, max_x - cx),
        //     wl_output::Transform::Flipped180 => ("Flipped180", cx, max_y - cy),
        //     wl_output::Transform::Flipped270 => ("Flipped270", cy, cx),
        //     _ => ("Unknown/Fallback", cx, cy),
        // };
        // log::debug!(
        //     "normal init transform match: {:?} -> {} (raw=({:.2},{:.2}) mapped=({:.2},{:.2}))",
        //     tx,
        //     transform_case,
        //     cx,
        //     cy,
        //     nx,
        //     ny
        // );
        let _ = cx.clamp(0.0, max_x);
        let _ = cy.clamp(0.0, max_y);

        draw_cursor(&mut overlay, cx, cy)?;
        queue.flush()?;
    }

    loop {
        // Non-blocking: try to read events from the Wayland fd, then dispatch.
        // We use prepare_read + read_events + dispatch_pending so we don't
        // block forever (we need to process held-key movement each frame).
        if let Some(guard) = queue.prepare_read() {
            // Non-blocking read: if nothing available, that's fine
            let _ = guard.read();
        }
        queue.dispatch_pending(state)?;
        queue.flush()?;

        // Collect all pending key events
        while let Ok(event) = state.key_rx.try_recv() {
            match event.state {
                KeyState::Pressed => { held_keys.insert(event.sym); }
                KeyState::Released => { held_keys.remove(&event.sym); }
                _ => {}
            }

            if event.state == KeyState::Pressed {
                match event.sym {
                    input::keysyms::ESCAPE => {
                        overlay.layer_surface.destroy();
                        overlay.surface.destroy();
                        queue.flush()?;
                        return Ok(());
                    }
                    // Mouse buttons
                    input::keysyms::M => {
                        let abs_x = monitor.x as f64 + cx;
                        let abs_y = monitor.y as f64 + cy;
                        wayland::warp_pointer(state, abs_x, abs_y);
                        queue.flush()?;
                        queue.roundtrip(state)?;
                        overlay.layer_surface.destroy();
                        overlay.surface.destroy();
                        wayland::click_button(state, 1);
                        queue.flush()?;
                        return Ok(());
                    }
                    input::keysyms::COMMA => {
                        let abs_x = monitor.x as f64 + cx;
                        let abs_y = monitor.y as f64 + cy;
                        wayland::warp_pointer(state, abs_x, abs_y);
                        wayland::click_button(state, 2);
                        queue.flush()?;
                        queue.roundtrip(state)?;
                        overlay.layer_surface.destroy();
                        overlay.surface.destroy();
                        queue.flush()?;
                        return Ok(());
                    }
                    input::keysyms::PERIOD => {
                        let abs_x = monitor.x as f64 + cx;
                        let abs_y = monitor.y as f64 + cy;
                        wayland::warp_pointer(state, abs_x, abs_y);
                        // wayland::click_button(state, 3);
                        queue.flush()?;
                        queue.roundtrip(state)?;
                        overlay.layer_surface.destroy();
                        overlay.surface.destroy();
                        wayland::click_button(state, 3);
                        queue.flush()?;
                        return Ok(());
                    }
                    // Switch to hint mode
                    input::keysyms::X => {
                        overlay.layer_surface.destroy();
                        overlay.surface.destroy();
                        queue.flush()?;
                        return run_hint_mode(state, queue, cfg);
                    }
                    // Switch to grid mode
                    input::keysyms::G => {
                        overlay.layer_surface.destroy();
                        overlay.surface.destroy();
                        queue.flush()?;
                        return run_grid_mode(state, queue, cfg);
                    }
                    _ => {}
                }
            }
        }

        // Continuous movement for held directional keys
        let now = Instant::now();
        let dt = now.duration_since(last_frame).as_secs_f64();
        last_frame = now;

        let speed = base_speed * dt * 60.0; // normalise to ~60fps
        let mut moved = false;

        if held_keys.contains(&input::keysyms::H) {
            cx = (cx - speed).max(0.0);
            moved = true;
        }
        if held_keys.contains(&input::keysyms::L) {
            cx = (cx + speed).min(monitor.width as f64 - 1.0);
            moved = true;
        }
        if held_keys.contains(&input::keysyms::K) {
            cy = (cy - speed).max(0.0);
            moved = true;
        }
        if held_keys.contains(&input::keysyms::J) {
            cy = (cy + speed).min(monitor.height as f64 - 1.0);
            moved = true;
        }

        if moved {
            draw_cursor(&mut overlay, cx, cy)?;
            queue.flush()?;
        }

        // ~120fps frame rate cap
        std::thread::sleep(Duration::from_millis(8));
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .init();

    let cli = Cli::parse();

    if !cli.hint && !cli.grid && !cli.normal {
        eprintln!("warpd-rs: specify a mode: --hint, --grid, or --normal");
        eprintln!("Bind in your compositor, e.g. for Hyprland:");
        eprintln!("  bind = SUPER ALT, x, exec, warpd-rs --hint");
        eprintln!("  bind = SUPER ALT, g, exec, warpd-rs --grid");
        eprintln!("  bind = SUPER ALT, c, exec, warpd-rs --normal");
        std::process::exit(1);
    }

    let cfg = config::Config::load(cli.config.as_deref());
    let (mut state, mut queue) = wayland::connect()?;

    //log::info!("monitors:");
    
    for m in &state.monitors {
        log::info!(
            "  {} — {}×{} @ ({}, {}) scale={}",
            m.name, m.width, m.height, m.x, m.y, m.scale
        );
    }

    if cli.hint {
        run_hint_mode(&mut state, &mut queue, &cfg)?;
    } else if cli.grid {
        run_grid_mode(&mut state, &mut queue, &cfg)?;
    } else if cli.normal {
        run_normal_mode(&mut state, &mut queue, &cfg)?;
    }

    Ok(())
}
