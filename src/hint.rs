///! Hint mode: generates a grid of labelled hint boxes across the screen,
///! draws them using Cairo onto the overlay's shared-memory buffer, and
///! progressively filters them as the user types characters.
use crate::config::{self, Config};
use crate::wayland::{KeyEvent, KeyState, Monitor, Overlay};

use anyhow::Result;
use std::io::{self, BufRead, IsTerminal};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A single hint on the screen.
#[derive(Debug, Clone)]
pub struct Hint {
    /// Pixel position relative to the monitor's top-left.
    pub x: i32,
    pub y: i32,
    /// Pixel dimensions of the hint box.
    pub w: i32,
    pub h: i32,
    /// The label the user must type to select this hint (e.g. "aj").
    pub label: String,
}

/// Arbitrary target area in monitor-local coordinates.
#[derive(Debug, Clone)]
pub struct TargetArea {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Result of processing a key press during hint mode.
pub enum HintResult {
    /// A hint was uniquely selected – warp to its centre.
    Selected { x: f64, y: f64 },
    /// The user pressed Escape or Backspace – cancel or undo.
    Cancel,
    /// Hints were filtered but not yet unique – keep going.
    Continue,
}

// ---------------------------------------------------------------------------
// Hint generation
// ---------------------------------------------------------------------------

/// Generate hint labels from a set of characters.
/// With N chars we can produce N single-char labels and N*N two-char labels.
/// We generate enough labels to cover `count` grid cells, preferring short labels.
pub fn generate_labels(chars: &str, count: usize) -> Vec<String> {
    let chars: Vec<char> = chars.chars().collect();
    let mut labels = Vec::with_capacity(count);

    // Single-character labels first
    for &c in &chars {
        labels.push(c.to_string());
        if labels.len() >= count {
            return labels;
        }
    }

    // Two-character labels
    for &a in &chars {
        for &b in &chars {
            labels.push(format!("{a}{b}"));
            if labels.len() >= count {
                return labels;
            }
        }
    }

    labels
}

/// Generate the grid of hints that covers the monitor.
pub fn generate_hints(monitor: &Monitor, config: &Config) -> Vec<Hint> {
    let gap = config.hint_grid_gap.max(20) as i32;
    let cols = (monitor.width / gap).max(1);
    let rows = (monitor.height / gap).max(1);
    let total = (cols * rows) as usize;

    let labels = generate_labels(&config.hint_chars, total);

    let box_w = (config.hint_font_size * 2.5) as i32;
    let box_h = (config.hint_font_size * 1.8) as i32;

    let mut hints = Vec::with_capacity(total);
    for row in 0..rows {
        for col in 0..cols {
            let idx = (row * cols + col) as usize;
            if idx >= labels.len() {
                break;
            }
            let cx = col * gap + gap / 2;
            let cy = row * gap + gap / 2;

            hints.push(Hint {
                x: cx - box_w / 2,
                y: cy - box_h / 2,
                w: box_w,
                h: box_h,
                label: labels[idx].clone(),
            });
        }
    }
    hints
}

/// Parse one `wxh+x+y` line.
fn parse_area_line(line: &str) -> Option<TargetArea> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let x_pos = trimmed.find('x')?;
    let plus1 = trimmed[x_pos + 1..].find('+')? + x_pos + 1;
    let plus2 = trimmed[plus1 + 1..].find('+')? + plus1 + 1;

    let w = trimmed[..x_pos].parse::<i32>().ok()?;
    let h = trimmed[x_pos + 1..plus1].parse::<i32>().ok()?;
    let x = trimmed[plus1 + 1..plus2].parse::<i32>().ok()?;
    let y = trimmed[plus2 + 1..].parse::<i32>().ok()?;

    if w <= 0 || h <= 0 {
        return None;
    }

    Some(TargetArea { x, y, w, h })
}

/// Read `wxh+x+y` areas from stdin.
pub fn read_target_areas_from_stdin() -> Result<Vec<TargetArea>> {
    if io::stdin().is_terminal() {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    for line in io::stdin().lock().lines() {
        let line = line?;
        if let Some(area) = parse_area_line(&line) {
            out.push(area);
        }
    }
    Ok(out)
}

/// Convert global areas into monitor-local, clipped areas.
pub fn normalize_areas_for_monitor(monitor: &Monitor, areas: &[TargetArea]) -> Vec<TargetArea> {
    let mut out = Vec::new();
    let mon_l = monitor.x;
    let mon_t = monitor.y;
    let mon_r = monitor.x + monitor.width;
    let mon_b = monitor.y + monitor.height;

    for area in areas {
        let l = area.x.max(mon_l);
        let t = area.y.max(mon_t);
        let r = (area.x + area.w).min(mon_r);
        let b = (area.y + area.h).min(mon_b);

        let w = r - l;
        let h = b - t;
        if w <= 0 || h <= 0 {
            continue;
        }

        out.push(TargetArea {
            x: l - mon_l,
            y: t - mon_t,
            w,
            h,
        });
    }

    out
}

/// Build hint labels for arbitrary rectangles.
pub fn generate_hints_from_areas(config: &Config, areas: &[TargetArea]) -> Vec<Hint> {
    let labels = generate_labels(&config.hint_chars, areas.len());
    let min_w = (config.hint_font_size * 2.2).round() as i32;
    let min_h = (config.hint_font_size * 1.6).round() as i32;

    areas
        .iter()
        .zip(labels)
        .map(|(area, label)| {
            let cx = area.x + area.w / 2;
            let cy = area.y + area.h / 2;
            let w = area.w.max(min_w);
            let h = area.h.max(min_h);

            Hint {
                x: cx - w / 2,
                y: cy - h / 2,
                w,
                h,
                label,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------

/// Draw the semi-transparent background scrim and all visible hints onto the
/// overlay's shared-memory buffer using Cairo.
pub fn draw_hints(
    overlay: &mut Overlay,
    hints: &[Hint],
    config: &Config,
    typed: &str,
) -> Result<()> {
    let width = overlay.shm_buffer.width;
    let height = overlay.shm_buffer.height;
    let stride = overlay.shm_buffer.stride;

    // Create a Cairo ImageSurface that writes directly into the shm mmap
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

    // Clear to semi-transparent black (scrim)
    cr.set_operator(cairo::Operator::Source);
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.35);
    cr.paint()?;

    // Parse colours
    let (bg_r, bg_g, bg_b, bg_a) = config::parse_hex_color(&config.hint_bgcolor);
    let (fg_r, fg_g, fg_b, _) = config::parse_hex_color(&config.hint_fgcolor);
    let bg_alpha = (bg_a * config.hint_bg_opacity).clamp(0.0, 1.0);

    cr.set_operator(cairo::Operator::Over);

    // Select font
    cr.select_font_face(
        &config.hint_font,
        cairo::FontSlant::Normal,
        cairo::FontWeight::Bold,
    );
    cr.set_font_size(config.hint_font_size);

    let radius = config.hint_border_radius;

    for hint in hints {
        // Skip hints that don't match the typed prefix
        if !typed.is_empty() && !hint.label.starts_with(typed) {
            continue;
        }

        let x = hint.x as f64;
        let y = hint.y as f64;
        let w = hint.w as f64;
        let h = hint.h as f64;

        // --- Rounded rectangle background ---
        cr.new_sub_path();
        cr.arc(
            x + w - radius,
            y + radius,
            radius,
            -std::f64::consts::FRAC_PI_2,
            0.0,
        );
        cr.arc(
            x + w - radius,
            y + h - radius,
            radius,
            0.0,
            std::f64::consts::FRAC_PI_2,
        );
        cr.arc(
            x + radius,
            y + h - radius,
            radius,
            std::f64::consts::FRAC_PI_2,
            std::f64::consts::PI,
        );
        cr.arc(
            x + radius,
            y + radius,
            radius,
            std::f64::consts::PI,
            3.0 * std::f64::consts::FRAC_PI_2,
        );
        cr.close_path();
        cr.set_source_rgba(bg_r, bg_g, bg_b, bg_alpha);
        cr.fill()?;

        // --- Label text ---
        let extents = cr.text_extents(&hint.label)?;
        let tx = x + (w - extents.width()) / 2.0 - extents.x_bearing();
        let ty = y + (h - extents.height()) / 2.0 - extents.y_bearing();

        // Highlight the already-typed prefix in a slightly different style
        if !typed.is_empty() && hint.label.starts_with(typed) {
            let remaining = &hint.label[typed.len()..];

            // Draw typed portion dimmed
            cr.set_source_rgba(fg_r, fg_g, fg_b, 0.4);
            cr.move_to(tx, ty);
            cr.show_text(typed)?;

            // Draw remaining portion bright
            let prefix_ext = cr.text_extents(typed)?;
            cr.set_source_rgba(fg_r, fg_g, fg_b, 1.0);
            cr.move_to(tx + prefix_ext.x_advance(), ty);
            cr.show_text(remaining)?;
        } else {
            cr.set_source_rgba(fg_r, fg_g, fg_b, 1.0);
            cr.move_to(tx, ty);
            cr.show_text(&hint.label)?;
        }
    }

    drop(cr);
    surface.flush();

    // Attach the buffer to the wl_surface and commit
    overlay
        .surface
        .attach(Some(&overlay.shm_buffer.buffer), 0, 0);
    overlay.surface.damage_buffer(0, 0, width, height);
    overlay.surface.commit();

    Ok(())
}

// ---------------------------------------------------------------------------
// Input handling
// ---------------------------------------------------------------------------

/// Map an XKB keysym to the lowercase ASCII character it represents, if any.
fn keysym_to_char(sym: u32) -> Option<char> {
    // xkbcommon keysyms for ASCII a-z are 0x61..0x7a, A-Z are 0x41..0x5a
    if (0x61..=0x7a).contains(&sym) {
        return Some(sym as u8 as char);
    }
    if (0x41..=0x5a).contains(&sym) {
        return Some((sym as u8 + 32) as char); // lowercase
    }
    None
}

/// Process a key event during hint mode. Returns what the mode loop should do.
pub fn process_key(event: &KeyEvent, hints: &[Hint], typed: &mut String) -> HintResult {
    if event.state != KeyState::Pressed {
        return HintResult::Continue;
    }

    // Escape → cancel
    if event.sym == 0xff1b {
        return HintResult::Cancel;
    }

    // Backspace → remove last typed character
    if event.sym == 0xff08 {
        typed.pop();
        return HintResult::Continue;
    }

    // Append character
    if let Some(ch) = keysym_to_char(event.sym) {
        typed.push(ch);
    } else {
        return HintResult::Continue;
    }

    // Filter hints
    let matching: Vec<&Hint> = hints
        .iter()
        .filter(|h| h.label.starts_with(typed.as_str()))
        .collect();

    match matching.len() {
        0 => {
            // No match – undo last character
            typed.pop();
            HintResult::Continue
        }
        1 => {
            let h = matching[0];
            HintResult::Selected {
                x: h.x as f64 + h.w as f64 / 2.0,
                y: h.y as f64 + h.h as f64 / 2.0,
            }
        }
        _ => HintResult::Continue,
    }
}

// ---------------------------------------------------------------------------
// Grid mode (bonus) – simple 2x2 recursive subdivision
// ---------------------------------------------------------------------------

/// Represents the current grid selection area.
#[derive(Debug, Clone)]
pub struct GridSelection {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl GridSelection {
    pub fn new(monitor: &Monitor) -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            w: monitor.width as f64,
            h: monitor.height as f64,
        }
    }

    /// Subdivide into quadrant index: 0=TL, 1=TR, 2=BL, 3=BR
    pub fn subdivide(&mut self, quadrant: usize) {
        let hw = self.w / 2.0;
        let hh = self.h / 2.0;
        match quadrant {
            0 => { /* top-left, x/y stay */ }
            1 => {
                self.x += hw;
            }
            2 => {
                self.y += hh;
            }
            3 => {
                self.x += hw;
                self.y += hh;
            }
            _ => return,
        }
        self.w = hw;
        self.h = hh;
    }

    pub fn centre(&self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
}

/// Draw the grid overlay: a crosshair dividing the current selection.
pub fn draw_grid(overlay: &mut Overlay, sel: &GridSelection, config: &Config) -> Result<()> {
    let width = overlay.shm_buffer.width;
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

    /*
    Clear to semi-transparent
    */
    cr.set_operator(cairo::Operator::Source);
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.25);
    cr.paint()?;

    /*
    Highlight selected region
    */
    cr.set_operator(cairo::Operator::Over);
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.05);
    cr.rectangle(sel.x, sel.y, sel.w, sel.h);
    cr.fill()?;

    /*
    Draw crosshair
    */
    let (gr, gg, gb, _) = config::parse_hex_color(&config.grid_color);
    let lw = config.grid_border_size as f64;
    cr.set_source_rgba(gr, gg, gb, 0.8);
    cr.set_line_width(lw);

    let mid_x = sel.x + sel.w / 2.0;
    let mid_y = sel.y + sel.h / 2.0;

    cr.move_to(mid_x, sel.y);
    cr.line_to(mid_x, sel.y + sel.h);
    cr.stroke()?;

    cr.move_to(sel.x, mid_y);
    cr.line_to(sel.x + sel.w, mid_y);
    cr.stroke()?;

    // Labels in each quadrant
    cr.select_font_face(
        "monospace",
        cairo::FontSlant::Normal,
        cairo::FontWeight::Bold,
    );
    cr.set_font_size(config.grid_font_size);
    cr.set_source_rgba(gr, gg, gb, 0.6);

    let label_positions = [
        (sel.x + sel.w * 0.25, sel.y + sel.h * 0.25),
        (sel.x + sel.w * 0.75, sel.y + sel.h * 0.25),
        (sel.x + sel.w * 0.25, sel.y + sel.h * 0.75),
        (sel.x + sel.w * 0.75, sel.y + sel.h * 0.75),
    ];
    for (idx, (lx, ly)) in label_positions.iter().enumerate() {
        let label = config.grid_quadrant_keys[idx].as_str();
        let label = if label.is_empty() { "?" } else { label };
        let ext = cr.text_extents(label)?;
        cr.move_to(lx - ext.width() / 2.0, ly + ext.height() / 2.0);
        cr.show_text(label)?;
    }

    drop(cr);
    surface.flush();

    overlay
        .surface
        .attach(Some(&overlay.shm_buffer.buffer), 0, 0);
    overlay.surface.damage_buffer(0, 0, width, height);
    overlay.surface.commit();

    Ok(())
}
