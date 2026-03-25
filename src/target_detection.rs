#![cfg(feature = "opencv")]

use anyhow::{bail, Result};
use opencv::core::{self, Mat, Point, Rect, Size, Vec4i, Vector};
use opencv::imgcodecs;
use opencv::imgproc;
use opencv::prelude::*;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::hint::TargetArea;
use crate::wayland::{CapturedFrame, Monitor};
use wayland_client::protocol::wl_output;
use wayland_client::protocol::wl_shm;

static DEBUG_DUMP_DONE: AtomicBool = AtomicBool::new(false);

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

            // Mirror wl-kbptr's channel selection before BGR->gray conversion:
            //   ARGB/XRGB: channels [1,1,2]
            //   ABGR/XBGR: channels [2,1,0]
            // For other 32-bit formats, fall back to natural BGR order.
            let (b, g, r) = match frame.format {
                wl_shm::Format::Argb8888 | wl_shm::Format::Xrgb8888 => (px[1], px[1], px[2]),
                wl_shm::Format::Abgr8888 | wl_shm::Format::Xbgr8888 => (px[2], px[1], px[0]),
                wl_shm::Format::Rgba8888 | wl_shm::Format::Rgbx8888 => (px[2], px[1], px[0]),
                wl_shm::Format::Bgra8888 | wl_shm::Format::Bgrx8888 => (px[0], px[1], px[2]),
                _ => bail!("unsupported wl_shm format for detection: {:?}", frame.format),
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

fn filter_rects(rects: &[LogicalRect], hierarchy: &[Vec4i]) -> Vec<bool> {
    let mut filtered = vec![false; rects.len()];

    for (i, rect) in rects.iter().enumerate() {
        if rect.h >= 50.0 || rect.w >= 500.0 || rect.h <= 3.0 || rect.w <= 7.0 {
            filtered[i] = true;
        }
    }

    let mut stack: Vec<usize> = hierarchy
        .iter()
        .enumerate()
        .filter_map(|(i, h)| if h[3] >= 0 { Some(i) } else { None })
        .collect();

    while let Some(i) = stack.pop() {
        if !filtered[i] {
            let parent_i = hierarchy[i][3] as usize;
            if filtered[parent_i] {
                filtered[i] = true;
            } else {
                let rect = rects[i];
                if rect.h <= 6.0 {
                    filtered[i] = true;
                } else {
                    let parent = rects[parent_i];

                    let center_x = rect.x + rect.w / 2.0;
                    let center_y = rect.y + rect.h / 2.0;
                    let parent_center_x = parent.x + parent.w / 2.0;
                    let parent_center_y = parent.y + parent.h / 2.0;

                    if (center_x - parent_center_x).abs() < 8.0
                        && (center_y - parent_center_y).abs() < 8.0
                    {
                        filtered[i] = true;
                    } else if (parent.h - parent.w).abs() < 5.0
                        && parent.h < 40.0
                        && parent.w < 40.0
                    {
                        filtered[i] = true;
                    }
                }
            }
        }

        let mut child = hierarchy[i][0];
        while child >= 0 {
            stack.push(child as usize);
            child = hierarchy[child as usize][2];
        }
    }

    filtered
}

fn filter_rects_size_only(rects: &[LogicalRect]) -> Vec<bool> {
    rects
        .iter()
        .map(|r| r.h >= 50.0 || r.w >= 500.0 || r.h <= 3.0 || r.w <= 7.0)
        .collect()
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

    let _ = imgcodecs::imwrite(&gray_path.to_string_lossy(), gray, &params);
    let _ = imgcodecs::imwrite(&transformed_path.to_string_lossy(), transformed, &params);
    let _ = imgcodecs::imwrite(&edges_path.to_string_lossy(), edges, &params);
    let _ = imgcodecs::imwrite(&dilated_path.to_string_lossy(), dilated, &params);

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
        let _ = imgcodecs::imwrite(&rects_path.to_string_lossy(), &overlay, &params);
    }

    let primary_kept = filtered_primary.iter().filter(|f| !**f).count();
    let size_only_kept = filtered_size_only.iter().filter(|f| !**f).count();
    let final_kept = filtered_final.iter().filter(|f| !**f).count();

    let mut stats = String::new();
    let _ = writeln!(&mut stats, "frame: {}x{} stride={} format={:?} y_invert={}", frame.width, frame.height, frame.stride, frame.format, frame.y_invert);
    let _ = writeln!(&mut stats, "monitor: name={} pos=({}, {}) size={}x{} transform={:?}", monitor.name, monitor.x, monitor.y, monitor.width, monitor.height, monitor.transform);
    let _ = writeln!(&mut stats, "pipeline: scale={scale:.4} kernel={}x{}", kx, ky);
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
        log::warn!("target-detect debug: cannot write {}: {e}", stats_path.display());
    }

    log::warn!(
        "target detection debug dump written to {}",
        dir.display()
    );
}

pub fn detect_target_areas(frame: &CapturedFrame, monitor: &Monitor) -> Result<Vec<TargetArea>> {
    let gray = grayscale_from_frame(frame)?;
    let transformed = apply_transform(gray.clone(), monitor.transform)?;

    let scale = transformed.rows() as f64 / monitor.height.max(1) as f64;
    // Match wl-kbptr's kernel sizing: rows=2.5*scale, cols=3.5*scale.
    let ky = (2.5 * scale).round().max(1.0) as i32;
    let kx = (3.5 * scale).round().max(1.0) as i32;

    let mut edges = Mat::default();
    imgproc::canny(&transformed, &mut edges, 70.0, 220.0, 3, false)?;

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
        raw_rects.push(imgproc::bounding_rect(&c)?);
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

    // Some compositors/themes produce hierarchy trees that over-prune targets
    // (especially inside browser/content areas). Fall back to size-only filter
    // when primary output is sparse relative to total candidates.
    let used_fallback = total_rects > 120 && (kept_primary < 80 || primary_ratio < 0.08);
    let filtered = if used_fallback {
        log::debug!(
            "target detection fallback: primary kept {kept_primary}/{total_rects} ({:.2}%), size-only would keep {kept_size_only}; retrying with size-only filter",
            primary_ratio * 100.0
        );
        filtered_size_only.clone()
    } else {
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
