#![cfg(feature = "opencv")]

use anyhow::{bail, Result};
use opencv::core::{self, Mat, Point, Rect, Size, Vec4i, Vector};
use opencv::imgcodecs;
use opencv::imgproc;
use opencv::prelude::*;

// OpenCV 5 moved contour geometry out of imgproc; keep OpenCV 4 builds working.
opencv::opencv_branch_5! {
    use opencv::geometry::bounding_rect;
}
opencv::not_opencv_branch_5! {
    use opencv::imgproc::bounding_rect;
}
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::hint::TargetArea;
use crate::wayland::{CapturedFrame, Monitor};
use wayland_client::protocol::wl_output;
use wayland_client::protocol::wl_shm;

static DEBUG_DUMP_DONE: AtomicBool = AtomicBool::new(false);

const MIN_RECT_HEIGHT: f64 = 3.0;
const MIN_RECT_WIDTH: f64 = 7.0;
const MAX_RECT_HEIGHT: f64 = 72.0;
const MAX_RECT_WIDTH: f64 = 500.0;
const CANNY_LOW_THRESHOLD: f64 = 50.0;
const CANNY_HIGH_THRESHOLD: f64 = 160.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contour_bounding_rect_uses_the_available_opencv_module() -> Result<()> {
        let contour = Vector::<Point>::from_iter([
            Point::new(10, 20),
            Point::new(30, 20),
            Point::new(30, 40),
            Point::new(10, 40),
        ]);
        assert_eq!(bounding_rect(&contour)?, Rect::new(10, 20, 21, 21));
        Ok(())
    }

    #[test]
    fn centered_child_is_kept_when_outer_container_is_filtered_by_size() {
        // Simulates a tall or wide outlined button whose outer border and inner hole
        // both exceed MAX_RECT_HEIGHT, with a single centered label inside.
        // Hierarchy format: [next, prev, first_child, parent]
        let rects = vec![
            LogicalRect {
                x: 100.0,
                y: 100.0,
                w: 160.0,
                h: 90.0, // outer border (filtered: h >= MAX_RECT_HEIGHT)
            },
            LogicalRect {
                x: 104.0,
                y: 104.0,
                w: 152.0,
                h: 82.0, // inner hole (filtered: h >= MAX_RECT_HEIGHT)
            },
            LogicalRect {
                x: 135.0,
                y: 138.0,
                w: 90.0,
                h: 14.0, // centered label (must be kept!)
            },
        ];
        let hierarchy = vec![
            Vec4i::from([-1, -1, 1, -1]),
            Vec4i::from([-1, -1, 2, 0]),
            Vec4i::from([-1, -1, -1, 1]),
        ];

        let filtered = filter_rects(&rects, &hierarchy);
        assert_eq!(filtered, vec![true, true, false]);
    }

    #[test]
    fn outlined_button_within_size_bounds_keeps_single_hint() {
        // Simulates a ~52px-tall outlined button (like Chrome's "Advanced" button at 1440p):
        // outer border is kept, inner border hole and centered text are deduplicated.
        let rects = vec![
            LogicalRect {
                x: 100.0,
                y: 100.0,
                w: 140.0,
                h: 52.0, // outer border (kept)
            },
            LogicalRect {
                x: 104.0,
                y: 104.0,
                w: 132.0,
                h: 44.0, // inner hole (deduplicated against outer)
            },
            LogicalRect {
                x: 125.0,
                y: 119.0,
                w: 90.0,
                h: 14.0, // centered label (deduplicated against outer)
            },
        ];
        let hierarchy = vec![
            Vec4i::from([-1, -1, 1, -1]),
            Vec4i::from([-1, -1, 2, 0]),
            Vec4i::from([-1, -1, -1, 1]),
        ];

        let filtered = filter_rects(&rects, &hierarchy);
        assert_eq!(filtered, vec![false, true, true]);
    }

    #[test]
    fn small_square_icon_deduplicates_nested_details() {
        let rects = vec![
            LogicalRect {
                x: 10.0,
                y: 10.0,
                w: 28.0,
                h: 28.0, // outer icon (kept)
            },
            LogicalRect {
                x: 13.0,
                y: 13.0,
                w: 22.0,
                h: 22.0, // inner hole (filtered)
            },
            LogicalRect {
                x: 22.0,
                y: 15.0,
                w: 10.0,
                h: 10.0, // off-center detail inside small square icon (filtered)
            },
        ];
        let hierarchy = vec![
            Vec4i::from([-1, -1, 1, -1]),
            Vec4i::from([-1, -1, 2, 0]),
            Vec4i::from([-1, -1, -1, 1]),
        ];

        let filtered = filter_rects(&rects, &hierarchy);
        assert_eq!(filtered, vec![false, true, true]);
    }

    #[test]
    fn off_center_children_inside_kept_bar_are_preserved() {
        let rects = vec![
            LogicalRect {
                x: 0.0,
                y: 0.0,
                w: 400.0,
                h: 40.0, // bar (kept)
            },
            LogicalRect {
                x: 10.0,
                y: 10.0,
                w: 60.0,
                h: 20.0, // left button (kept)
            },
            LogicalRect {
                x: 320.0,
                y: 10.0,
                w: 60.0,
                h: 20.0, // right button (kept)
            },
        ];
        let hierarchy = vec![
            Vec4i::from([-1, -1, 1, -1]),
            Vec4i::from([2, -1, -1, 0]),
            Vec4i::from([-1, 1, -1, 0]),
        ];

        let filtered = filter_rects(&rects, &hierarchy);
        assert_eq!(filtered, vec![false, false, false]);
    }

    #[test]
    fn argb8888_preserves_blue_channel_in_grayscale() -> Result<()> {
        // Pure blue pixel in little-endian ARGB8888: [B=255, G=0, R=0, A=255]
        let frame = CapturedFrame {
            width: 1,
            height: 1,
            stride: 4,
            format: wl_shm::Format::Argb8888,
            y_invert: false,
            data: vec![255, 0, 0, 255],
        };
        let gray = grayscale_from_frame(&frame)?;
        let val = *gray.at_2d::<u8>(0, 0)?;
        // (29 * 255) >> 8 == 28; previously returned 0 because px[1] (green) was used twice.
        assert_eq!(val, 28);
        Ok(())
    }
}

fn grayscale_from_frame(frame: &CapturedFrame) -> Result<Mat> {
    let width = frame.width.max(0) as usize;
    let height = frame.height.max(0) as usize;
    let stride = frame.stride.max(0) as usize;

    if width == 0 || height == 0 || stride < width * 4 {
        bail!("invalid capture dimensions for target detection");
    }

    let mut gray = vec![0u8; width * height];

    for y in 0..height {
        let row = y * stride;
        for x in 0..width {
            let i = row + x * 4;
            let px = &frame.data[i..i + 4];

            // Convert 32-bit wl_shm pixel formats to (B, G, R) for grayscale luminance.
            // On little-endian, ARGB8888/XRGB8888 bytes are laid out as [B, G, R, A/X].
            let (b, g, r) = match frame.format {
                wl_shm::Format::Argb8888 | wl_shm::Format::Xrgb8888 => (px[0], px[1], px[2]),
                wl_shm::Format::Abgr8888 | wl_shm::Format::Xbgr8888 => (px[2], px[1], px[0]),
                wl_shm::Format::Rgba8888 | wl_shm::Format::Rgbx8888 => (px[2], px[1], px[0]),
                wl_shm::Format::Bgra8888 | wl_shm::Format::Bgrx8888 => (px[0], px[1], px[2]),
                _ => bail!(
                    "unsupported wl_shm format for detection: {:?}",
                    frame.format
                ),
            };

            // OpenCV COLOR_BGR2GRAY coefficients.
            let lum = (29u16 * b as u16 + 150u16 * g as u16 + 77u16 * r as u16) >> 8;
            gray[y * width + x] = lum as u8;
        }
    }

    let raw = Mat::from_slice(&gray)?;
    let reshaped = raw.reshape(1, frame.height)?;
    let mut out = Mat::default();
    reshaped.copy_to(&mut out)?;

    if frame.y_invert {
        let mut flipped = Mat::default();
        core::flip(&out, &mut flipped, 0)?;
        out = flipped;
    }

    Ok(out)
}

fn apply_transform(mut m: Mat, transform: wl_output::Transform) -> Result<Mat> {
    let mut tmp = Mat::default();

    match transform {
        wl_output::Transform::Normal => {}
        wl_output::Transform::_90 => {
            core::rotate(&m, &mut tmp, core::RotateFlags::ROTATE_90_CLOCKWISE as i32)?;
            m = tmp;
        }
        wl_output::Transform::_270 => {
            core::rotate(
                &m,
                &mut tmp,
                core::RotateFlags::ROTATE_90_COUNTERCLOCKWISE as i32,
            )?;
            m = tmp;
        }
        wl_output::Transform::_180 => {
            core::rotate(&m, &mut tmp, core::RotateFlags::ROTATE_180 as i32)?;
            m = tmp;
        }
        wl_output::Transform::Flipped => {
            core::flip(&m, &mut tmp, 1)?;
            m = tmp;
        }
        wl_output::Transform::Flipped90 => {
            core::rotate(&m, &mut tmp, core::RotateFlags::ROTATE_90_CLOCKWISE as i32)?;
            core::flip(&tmp, &mut m, 1)?;
        }
        wl_output::Transform::Flipped180 => {
            core::rotate(&m, &mut tmp, core::RotateFlags::ROTATE_180 as i32)?;
            core::flip(&tmp, &mut m, 1)?;
        }
        wl_output::Transform::Flipped270 => {
            core::rotate(
                &m,
                &mut tmp,
                core::RotateFlags::ROTATE_90_COUNTERCLOCKWISE as i32,
            )?;
            core::flip(&tmp, &mut m, 1)?;
        }
        _ => {}
    }

    Ok(m)
}

#[derive(Debug, Clone, Copy)]
struct LogicalRect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

fn is_outside_size_bounds(rect: LogicalRect) -> bool {
    rect.h >= MAX_RECT_HEIGHT
        || rect.w >= MAX_RECT_WIDTH
        || rect.h <= MIN_RECT_HEIGHT
        || rect.w <= MIN_RECT_WIDTH
}

fn filter_rects(rects: &[LogicalRect], hierarchy: &[Vec4i]) -> Vec<bool> {
    let mut filtered: Vec<bool> = rects.iter().copied().map(is_outside_size_bounds).collect();
    let mut visited = vec![false; rects.len()];

    // Walk OpenCV's RETR_TREE hierarchy top-down ([next, prev, first_child, parent]).
    // Deduplicate children only against the nearest *kept* ancestor so that when an
    // outer container (e.g. a tall button or wide input) is filtered out by size,
    // its centered label or icon is preserved instead of disappearing too.
    let mut stack: Vec<(usize, Option<usize>)> = hierarchy
        .iter()
        .enumerate()
        .take(rects.len())
        .filter_map(|(i, h)| if h[3] < 0 { Some((i, None)) } else { None })
        .collect();

    while let Some((i, kept_ancestor)) = stack.pop() {
        if visited[i] {
            continue;
        }
        visited[i] = true;

        let has_parent = hierarchy[i][3] >= 0;
        if !filtered[i] && has_parent {
            let rect = rects[i];
            if rect.h <= 6.0 {
                filtered[i] = true;
            } else if let Some(ancestor_i) = kept_ancestor {
                let ancestor = rects[ancestor_i];
                let center_x = rect.x + rect.w / 2.0;
                let center_y = rect.y + rect.h / 2.0;
                let ancestor_center_x = ancestor.x + ancestor.w / 2.0;
                let ancestor_center_y = ancestor.y + ancestor.h / 2.0;

                if (center_x - ancestor_center_x).abs() < 8.0
                    && (center_y - ancestor_center_y).abs() < 8.0
                {
                    filtered[i] = true;
                } else if (ancestor.h - ancestor.w).abs() < 5.0
                    && ancestor.h < 40.0
                    && ancestor.w < 40.0
                {
                    filtered[i] = true;
                }
            }
        }

        let next_kept_ancestor = if !filtered[i] { Some(i) } else { kept_ancestor };
        let mut child = hierarchy[i][2];
        while child >= 0 {
            let child_idx = child as usize;
            if child_idx < rects.len() && !visited[child_idx] {
                stack.push((child_idx, next_kept_ancestor));
            }
            child = hierarchy[child_idx][0];
        }
    }

    filtered
}

fn filter_rects_size_only(rects: &[LogicalRect]) -> Vec<bool> {
    rects.iter().copied().map(is_outside_size_bounds).collect()
}

fn kept_split_counts(rects: &[LogicalRect], filtered: &[bool], split_x: f64) -> (usize, usize) {
    let mut left = 0usize;
    let mut right = 0usize;

    for (rect, is_filtered) in rects.iter().zip(filtered.iter()) {
        if *is_filtered {
            continue;
        }
        let center_x = rect.x + rect.w / 2.0;
        if center_x < split_x {
            left += 1;
        } else {
            right += 1;
        }
    }

    (left, right)
}

fn balance_ratio(a: usize, b: usize) -> f64 {
    if a == 0 && b == 0 {
        return 1.0;
    }
    let min_v = a.min(b) as f64;
    let max_v = a.max(b) as f64;
    if max_v == 0.0 {
        1.0
    } else {
        min_v / max_v
    }
}

fn detect_debug_enabled() -> bool {
    std::env::var("WARPDRS_DETECT_DEBUG")
        .map(|v| {
            let val = v.trim().to_ascii_lowercase();
            val == "1" || val == "true" || val == "yes"
        })
        .unwrap_or(false)
}

fn debug_dump_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("WARPDRS_DETECT_DEBUG_DIR") {
        if !dir.trim().is_empty() {
            return PathBuf::from(dir);
        }
    }

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    PathBuf::from(format!("/tmp/warpd-rs-detect-{ts}-{}", std::process::id()))
}

fn maybe_dump_debug(
    frame: &CapturedFrame,
    monitor: &Monitor,
    gray: &Mat,
    transformed: &Mat,
    edges: &Mat,
    dilated: &Mat,
    raw_rects: &[Rect],
    filtered_primary: &[bool],
    filtered_size_only: &[bool],
    filtered_final: &[bool],
    used_fallback: bool,
    scale: f64,
    kx: i32,
    ky: i32,
) {
    if !detect_debug_enabled() {
        return;
    }
    if DEBUG_DUMP_DONE.swap(true, Ordering::SeqCst) {
        return;
    }

    let dir = debug_dump_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        log::warn!("target-detect debug: cannot create {}: {e}", dir.display());
        return;
    }

    let params = Vector::<i32>::new();
    let gray_path = dir.join("01_gray.png");
    let transformed_path = dir.join("02_transformed.png");
    let edges_path = dir.join("03_edges.png");
    let dilated_path = dir.join("04_dilated.png");
    let rects_path = dir.join("05_rects_overlay.png");
    let stats_path = dir.join("stats.txt");

    let _ = imgcodecs::imwrite(&gray_path, gray, &params);
    let _ = imgcodecs::imwrite(&transformed_path, transformed, &params);
    let _ = imgcodecs::imwrite(&edges_path, edges, &params);
    let _ = imgcodecs::imwrite(&dilated_path, dilated, &params);

    let mut overlay = Mat::default();
    if imgproc::cvt_color(
        transformed,
        &mut overlay,
        imgproc::COLOR_GRAY2BGR,
        0,
        core::AlgorithmHint::ALGO_HINT_DEFAULT,
    )
    .is_ok()
    {
        for (i, r) in raw_rects.iter().enumerate() {
            let color = if filtered_final.get(i).copied().unwrap_or(true) {
                core::Scalar::new(0.0, 0.0, 255.0, 0.0)
            } else {
                core::Scalar::new(0.0, 255.0, 0.0, 0.0)
            };
            let _ = imgproc::rectangle(&mut overlay, *r, color, 1, imgproc::LINE_8, 0);
        }
        let _ = imgcodecs::imwrite(&rects_path, &overlay, &params);
    }

    let primary_kept = filtered_primary.iter().filter(|f| !**f).count();
    let size_only_kept = filtered_size_only.iter().filter(|f| !**f).count();
    let final_kept = filtered_final.iter().filter(|f| !**f).count();

    let mut stats = String::new();
    let _ = writeln!(
        &mut stats,
        "frame: {}x{} stride={} format={:?} y_invert={}",
        frame.width, frame.height, frame.stride, frame.format, frame.y_invert
    );
    let _ = writeln!(
        &mut stats,
        "monitor: name={} pos=({}, {}) size={}x{} transform={:?}",
        monitor.name, monitor.x, monitor.y, monitor.width, monitor.height, monitor.transform
    );
    let _ = writeln!(
        &mut stats,
        "pipeline: scale={scale:.4} kernel={}x{}",
        kx, ky
    );
    let _ = writeln!(
        &mut stats,
        "rects: total={} kept_primary={} kept_size_only={} kept_final={} used_fallback={}",
        raw_rects.len(),
        primary_kept,
        size_only_kept,
        final_kept,
        used_fallback
    );
    let _ = writeln!(&mut stats, "files:");
    let _ = writeln!(&mut stats, "- {}", gray_path.display());
    let _ = writeln!(&mut stats, "- {}", transformed_path.display());
    let _ = writeln!(&mut stats, "- {}", edges_path.display());
    let _ = writeln!(&mut stats, "- {}", dilated_path.display());
    let _ = writeln!(&mut stats, "- {}", rects_path.display());

    if let Err(e) = std::fs::write(&stats_path, stats) {
        log::warn!(
            "target-detect debug: cannot write {}: {e}",
            stats_path.display()
        );
    }

    log::debug!("target detection debug dump written to {}", dir.display());
}

pub fn detect_target_areas(frame: &CapturedFrame, monitor: &Monitor) -> Result<Vec<TargetArea>> {
    let gray = grayscale_from_frame(frame)?;
    let transformed = apply_transform(gray.clone(), monitor.transform)?;

    let scale = transformed.rows() as f64 / monitor.height.max(1) as f64;
    // Match wl-kbptr's kernel sizing: rows=2.5*scale, cols=3.5*scale.
    let ky = (2.5 * scale).round().max(1.0) as i32;
    let kx = (3.5 * scale).round().max(1.0) as i32;

    let mut edges = Mat::default();
    imgproc::canny(
        &transformed,
        &mut edges,
        CANNY_LOW_THRESHOLD,
        CANNY_HIGH_THRESHOLD,
        3,
        false,
    )?;

    let kernel = imgproc::get_structuring_element(
        imgproc::MORPH_RECT,
        Size::new(kx, ky),
        Point::new(-1, -1),
    )?;

    let mut dilated = Mat::default();
    imgproc::dilate(
        &edges,
        &mut dilated,
        &kernel,
        Point::new(-1, -1),
        1,
        core::BORDER_CONSTANT,
        imgproc::morphology_default_border_value()?,
    )?;

    let mut contours: Vector<Vector<Point>> = Vector::new();
    let mut hierarchy = Mat::default();
    imgproc::find_contours_with_hierarchy(
        &dilated,
        &mut contours,
        &mut hierarchy,
        imgproc::RETR_TREE,
        imgproc::CHAIN_APPROX_SIMPLE,
        Point::new(0, 0),
    )?;

    let mut raw_rects = Vec::with_capacity(contours.len());
    for i in 0..contours.len() {
        let c = contours.get(i)?;
        raw_rects.push(bounding_rect(&c)?);
    }

    let hierarchy_vec = hierarchy.data_typed::<Vec4i>()?.to_vec();
    if hierarchy_vec.len() < raw_rects.len() {
        bail!("opencv hierarchy/contours size mismatch");
    }

    // wl-kbptr computes and filters rects in logical coordinates.
    let rects: Vec<LogicalRect> = raw_rects
        .iter()
        .map(|r: &Rect| LogicalRect {
            x: r.x as f64 / scale,
            y: r.y as f64 / scale,
            w: r.width as f64 / scale,
            h: r.height as f64 / scale,
        })
        .collect();

    let filtered_primary = filter_rects(&rects, &hierarchy_vec);
    let filtered_size_only = filter_rects_size_only(&rects);
    let kept_primary = filtered_primary.iter().filter(|f| !**f).count();
    let kept_size_only = filtered_size_only.iter().filter(|f| !**f).count();
    let total_rects = rects.len();
    let primary_ratio = if total_rects > 0 {
        kept_primary as f64 / total_rects as f64
    } else {
        0.0
    };

    let split_x = monitor.width.max(1) as f64 / 2.0;
    let (primary_left, primary_right) = kept_split_counts(&rects, &filtered_primary, split_x);
    let (size_left, size_right) = kept_split_counts(&rects, &filtered_size_only, split_x);
    let primary_balance = balance_ratio(primary_left, primary_right);
    let size_balance = balance_ratio(size_left, size_right);

    // Some compositors/themes produce hierarchy trees that over-prune targets
    // (especially inside browser/content areas). Fall back to size-only filter
    // when primary output is sparse relative to total candidates.
    let sparse_fallback = total_rects > 120 && (kept_primary < 80 || primary_ratio < 0.08);

    // Focused/inactive styling can make hierarchy filtering hide one side of
    // a split view. If size-only candidates are well balanced but primary is
    // heavily one-sided, prefer size-only output.
    let skew_fallback = total_rects > 200
        && size_left >= 80
        && size_right >= 80
        && size_balance >= 0.55
        && primary_balance <= 0.35
        && kept_primary + 120 <= kept_size_only;

    let used_fallback = sparse_fallback || skew_fallback;
    let filtered = if used_fallback {
        if sparse_fallback {
            log::debug!(
                "target detection fallback (sparse): primary kept {kept_primary}/{total_rects} ({:.2}%), size-only would keep {kept_size_only}; retrying with size-only filter",
                primary_ratio * 100.0
            );
        }
        if skew_fallback {
            log::debug!(
                "target detection fallback (skew): primary L/R={primary_left}/{primary_right} (balance {:.2}), size-only L/R={size_left}/{size_right} (balance {:.2}); retrying with size-only filter",
                primary_balance,
                size_balance
            );
        }
        log::debug!(
            "target detection: keeping size-only filter with {} candidates left",
            kept_size_only
        );
        filtered_size_only.clone()
    } else {
        log::debug!(
            "target detection: primary kept {kept_primary}/{total_rects} ({:.2}%), size-only would keep {kept_size_only}; keeping primary filter",
            primary_ratio * 100.0
        );
        filtered_primary.clone()
    };

    maybe_dump_debug(
        frame,
        monitor,
        &gray,
        &transformed,
        &edges,
        &dilated,
        &raw_rects,
        &filtered_primary,
        &filtered_size_only,
        &filtered,
        used_fallback,
        scale,
        kx,
        ky,
    );

    let mut out = Vec::new();
    for (i, rect) in rects.into_iter().enumerate() {
        if filtered[i] {
            continue;
        }
        let x = (rect.x + monitor.x as f64).round() as i32;
        let y = (rect.y + monitor.y as f64).round() as i32;
        let w = rect.w.round() as i32;
        let h = rect.h.round() as i32;
        if w > 0 && h > 0 {
            out.push(TargetArea { x, y, w, h });
        }
    }

    Ok(out)
}
