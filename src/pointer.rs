use crate::wayland::{PointerPosSource, WaylandState};
use wayland_client::protocol::{wl_pointer, wl_surface};

/// Return the current pointer position for the given surface, if that surface
/// currently has pointer focus.
pub fn surface_pointer_pos(
    state: &WaylandState,
    surface: &wl_surface::WlSurface,
) -> Option<(f64, f64, PointerPosSource)> {
    if state.pointer_focus_surface.as_ref() == Some(surface) {
        if let (Some((x, y)), Some(source)) = (state.pointer_surface_pos, state.pointer_surface_pos_source) {
            return Some((x, y, source));
        }
    }
    None
}

/// Move the virtual pointer to absolute coordinates within the compositor's
/// logical coordinate space.
///
/// The zwlr_virtual_pointer_v1.motion_absolute protocol works as follows:
///   position = (x / x_extent, y / y_extent)  →  normalised 0,1 range
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

        log::info!(
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



pub fn resolve_initial_pointer_position(
    raw_x: f64,
    raw_y: f64,
    monitor_x: f64,
    monitor_y: f64,
    monitor_width: f64,
    monitor_height: f64,
    allow_global_map: bool,
) -> Option<(&'static str, f64, f64)> {
    let max_x = monitor_width - 1.0;
    let max_y = monitor_height - 1.0;
    if max_x < 0.0 || max_y < 0.0 {
        return None;
    }

    let in_bounds = |x: f64, y: f64| {
        (0.0..=max_x).contains(&x) && (0.0..=max_y).contains(&y)
    };

    let mut candidates = vec![("surface-local", raw_x, raw_y)];
    if allow_global_map {
        candidates.push(("global-logical", raw_x - monitor_x, raw_y - monitor_y));
    }
    candidates.push(("offset-adjusted-enter", raw_x + monitor_x, raw_y + monitor_y));

    candidates
        .into_iter()
        .find(|(_, x, y)| in_bounds(*x, *y))
}

#[cfg(test)]
mod tests {
    use super::resolve_initial_pointer_position;

    #[test]
    fn resolves_surface_local_pointer_coords() {
        assert_eq!(
            resolve_initial_pointer_position(831.11328125, 558.5, 1920.0, -1080.0, 2560.0, 1080.0, true),
            Some(("surface-local", 831.11328125, 558.5))
        );
    }

    #[test]
    fn resolves_global_pointer_coords() {
        assert_eq!(
            resolve_initial_pointer_position(2752.0, -522.0, 1920.0, -1080.0, 2560.0, 1080.0, true),
            Some(("global-logical", 832.0, 558.0))
        );
    }

    #[test]
    fn resolves_hyprland_enter_pointer_coords() {
        assert_eq!(
            resolve_initial_pointer_position(-1088.0, 1638.5, 1920.0, -1080.0, 2560.0, 1080.0, false),
            Some(("offset-adjusted-enter", 832.0, 558.5))
        );
    }
}
