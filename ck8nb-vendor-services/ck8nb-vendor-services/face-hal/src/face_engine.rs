//! Pure-Rust, license-free on-device 2D Face Detection & Recognition engine.
//! - Upright rotation from camera sensor orientation (0/90/180/270).
//! - YCbCr skin-tone + local-contrast face localization (rejects dark/blank/non-face frames).
//! - Illumination-normalized 64x64 crop -> 8x8 spatial HOG + LBP descriptor (1024-D L2-normalized).
//! - Binary template storage in `/data/vendor_de/<userId>/facedata/ck8nb_face_v2.bin`.

use std::fs;
use std::path::PathBuf;

use crate::camera_ndk::YuvFrame;

pub const DESC_DIM: usize = 1024;
pub const FACE_ID_DEFAULT: i32 = 1;
pub const ENROLL_SAMPLES: usize = 10;
const MAGIC: &[u8; 4] = b"CK8F";
const VERSION: u32 = 2;
const DEFAULT_THRESHOLD: f32 = 0.78;

#[derive(Copy, Clone, Debug)]
pub struct FaceBox {
    pub x0: usize,
    pub y0: usize,
    pub x1: usize,
    pub y1: usize,
}

#[derive(Debug)]
pub enum DetectResult {
    Good([f32; DESC_DIM]),
    TooDark,
    TooBright,
    NoFace,
    TooSmall,
}

/// Sample (Y, Cb, Cr) in upright portrait coordinates (w_up x h_up).
#[inline]
fn sample_upright(frame: &YuvFrame, ux: usize, uy: usize) -> (u8, u8, u8) {
    let (sx, sy) = match frame.sensor_orientation {
        90 => (uy, frame.height - 1 - ux),
        270 => (frame.width - 1 - uy, ux),
        180 => (frame.width - 1 - ux, frame.height - 1 - uy),
        _ => (ux, uy),
    };
    let yi = sy * frame.y_row_stride + sx;
    let y = frame.y.get(yi).copied().unwrap_or(0);
    let cx = sx >> 1;
    let cy = sy >> 1;
    let ui = cy * frame.u_row_stride + cx * frame.u_pixel_stride;
    let vi = cy * frame.v_row_stride + cx * frame.v_pixel_stride;
    let cb = frame.u.get(ui).copied().unwrap_or(128);
    let cr = frame.v.get(vi).copied().unwrap_or(128);
    (y, cb, cr)
}

#[inline]
fn upright_dims(frame: &YuvFrame) -> (usize, usize) {
    match frame.sensor_orientation {
        90 | 270 => (frame.height, frame.width),
        _ => (frame.width, frame.height),
    }
}

/// Detects a centered human face and extracts a 1024-D illumination-invariant descriptor.
pub fn analyze_frame(frame: &YuvFrame) -> DetectResult {
    let (uw, uh) = upright_dims(frame);
    if uw < 120 || uh < 120 {
        return DetectResult::NoFace;
    }

    // Subsample central 80% region at stride 4 for fast skin-cluster localization.
    let step = 4usize;
    let x_start = uw / 10;
    let x_end = uw - uw / 10;
    let y_start = uh / 10;
    let y_end = uh - uh / 10;

    let mut y_sum: u64 = 0;
    let mut total_pts: u64 = 0;
    let mut skin_pts: u64 = 0;
    let mut sum_x: u64 = 0;
    let mut sum_y: u64 = 0;
    let mut sum_x2: u64 = 0;
    let mut sum_y2: u64 = 0;

    let mut py = y_start;
    while py < y_end {
        let mut px = x_start;
        while px < x_end {
            let (y, cb, cr) = sample_upright(frame, px, py);
            y_sum += y as u64;
            total_pts += 1;
            // Chai & Ngan YCbCr skin-tone range (broad enough for warm/cool indoor lighting).
            if y >= 35 && y <= 240 && cb >= 75 && cb <= 132 && cr >= 130 && cr <= 178 {
                skin_pts += 1;
                sum_x += px as u64;
                sum_y += py as u64;
                sum_x2 += (px as u64) * (px as u64);
                sum_y2 += (py as u64) * (py as u64);
            }
            px += step;
        }
        py += step;
    }

    if total_pts == 0 {
        return DetectResult::NoFace;
    }
    let mean_y = (y_sum / total_pts) as u32;
    if mean_y < 22 {
        return DetectResult::TooDark;
    }
    if mean_y > 242 {
        return DetectResult::TooBright;
    }

    // Require at least ~6% skin pixels in the central frame to count as a face candidate.
    if skin_pts * 100 < total_pts * 6 {
        return DetectResult::NoFace;
    }

    let cx = (sum_x as f32) / (skin_pts as f32);
    let cy = (sum_y as f32) / (skin_pts as f32);
    let var_x = ((sum_x2 as f32) / (skin_pts as f32) - cx * cx).max(1.0);
    let var_y = ((sum_y2 as f32) / (skin_pts as f32) - cy * cy).max(1.0);
    let half_w = (var_x.sqrt() * 1.85).clamp(uw as f32 * 0.14, uw as f32 * 0.42);
    let half_h = (var_y.sqrt() * 1.95).clamp(uh as f32 * 0.14, uh as f32 * 0.42);

    if half_w * 2.0 < (uw as f32) * 0.20 || half_h * 2.0 < (uh as f32) * 0.18 {
        return DetectResult::TooSmall;
    }

    let x0 = (cx - half_w).max(0.0) as usize;
    let y0 = (cy - half_h).max(0.0) as usize;
    let x1 = (cx + half_w).min((uw - 1) as f32) as usize;
    let y1 = (cy + half_h).min((uh - 1) as f32) as usize;
    if x1 <= x0 + 32 || y1 <= y0 + 32 {
        return DetectResult::TooSmall;
    }

    let fbox = FaceBox { x0, y0, x1, y1 };
    match extract_descriptor(frame, fbox) {
        Some(desc) => DetectResult::Good(desc),
        None => DetectResult::NoFace,
    }
}

/// Crops `fbox` to 64x64 grayscale, checks facial texture contrast (rejects flat walls/hands),
/// applies 4x4 local contrast normalization, and extracts an 8x8 spatial HOG+LBP descriptor.
fn extract_descriptor(frame: &YuvFrame, b: FaceBox) -> Option<[f32; DESC_DIM]> {
    const N: usize = 64;
    let mut patch = [0f32; N * N];
    let bw = (b.x1 - b.x0) as f32;
    let bh = (b.y1 - b.y0) as f32;

    let mut sum = 0.0f32;
    let mut sum2 = 0.0f32;
    for j in 0..N {
        let uy = b.y0 + (((j as f32 + 0.5) * bh) / N as f32) as usize;
        for i in 0..N {
            let ux = b.x0 + (((i as f32 + 0.5) * bw) / N as f32) as usize;
            let (y, _, _) = sample_upright(frame, ux, uy);
            let v = y as f32;
            patch[j * N + i] = v;
            sum += v;
            sum2 += v * v;
        }
    }
    let cnt = (N * N) as f32;
    let mean = sum / cnt;
    let std = (sum2 / cnt - mean * mean).max(0.0).sqrt();
    // Reject flat/blank surfaces (walls, covered lens, uniform skin patch without facial features).
    if std < 12.0 {
        return None;
    }

    // 4x4 block local mean/variance normalization for strong lighting invariance.
    let mut norm = [0f32; N * N];
    let blk = 16usize;
    for by in 0..4 {
        for bx in 0..4 {
            let mut bs = 0.0f32;
            let mut bs2 = 0.0f32;
            for j in (by * blk)..((by + 1) * blk) {
                for i in (bx * blk)..((bx + 1) * blk) {
                    let v = patch[j * N + i];
                    bs += v;
                    bs2 += v * v;
                }
            }
            let bmean = bs / 256.0;
            let bstd = (bs2 / 256.0 - bmean * bmean).max(0.0).sqrt().max(6.0);
            for j in (by * blk)..((by + 1) * blk) {
                for i in (bx * blk)..((bx + 1) * blk) {
                    norm[j * N + i] = ((patch[j * N + i] - bmean) / bstd).clamp(-3.0, 3.0);
                }
            }
        }
    }

    // 8x8 spatial cells (each 8x8 px), 16 features per cell -> 1024-D descriptor:
    //   bins 0..8:  8-direction oriented gradient (HOG)
    //   bins 8..16: 8-neighbor center-symmetric LBP / directional sign histogram
    let mut desc = [0f32; DESC_DIM];
    let offsets: [(isize, isize); 8] = [
        (-1, -1),
        (0, -1),
        (1, -1),
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
    ];

    for cy in 0..8usize {
        for cx in 0..8usize {
            let base = (cy * 8 + cx) * 16;
            for j in (cy * 8)..((cy + 1) * 8) {
                if j == 0 || j + 1 >= N {
                    continue;
                }
                for i in (cx * 8)..((cx + 1) * 8) {
                    if i == 0 || i + 1 >= N {
                        continue;
                    }
                    let c = norm[j * N + i];
                    let gx = norm[j * N + (i + 1)] - norm[j * N + (i - 1)];
                    let gy = norm[(j + 1) * N + i] - norm[(j - 1) * N + i];
                    let mag = (gx * gx + gy * gy).sqrt();
                    let angle = gy.atan2(gx) + core::f32::consts::PI; // [0, 2pi]
                    let bin = ((angle * 8.0 / (2.0 * core::f32::consts::PI)) as usize).min(7);
                    desc[base + bin] += mag;

                    for (k, &(dx, dy)) in offsets.iter().enumerate() {
                        let nb = norm[(j as isize + dy) as usize * N + (i as isize + dx) as usize];
                        if nb >= c {
                            desc[base + 8 + k] += 1.0;
                        }
                    }
                }
            }
        }
    }

    // L2 normalize entire descriptor
    let mut l2 = 0.0f32;
    for &v in &desc {
        l2 += v * v;
    }
    let inv = 1.0 / l2.sqrt().max(1e-6);
    for v in &mut desc {
        *v *= inv;
    }
    Some(desc)
}

pub fn cosine_similarity(a: &[f32; DESC_DIM], b: &[f32; DESC_DIM]) -> f32 {
    let mut s = 0.0f32;
    for i in 0..DESC_DIM {
        s += a[i] * b[i];
    }
    s
}

pub fn template_path(user_id: i32) -> PathBuf {
    PathBuf::from(format!("/data/vendor_de/{user_id}/facedata/ck8nb_face_v2.bin"))
}

pub fn match_threshold(user_id: i32) -> f32 {
    let p = format!("/data/vendor_de/{user_id}/facedata/threshold.txt");
    if let Ok(s) = fs::read_to_string(p) {
        if let Ok(v) = s.trim().parse::<f32>() {
            if (0.50..=0.95).contains(&v) {
                return v;
            }
        }
    }
    DEFAULT_THRESHOLD
}

pub fn save_templates(user_id: i32, templates: &[[f32; DESC_DIM]]) -> std::io::Result<()> {
    let dir = format!("/data/vendor_de/{user_id}/facedata");
    fs::create_dir_all(&dir)?;
    let mut buf = Vec::with_capacity(16 + templates.len() * DESC_DIM * 4);
    buf.extend_from_slice(MAGIC);
    buf.extend_from_slice(&VERSION.to_le_bytes());
    buf.extend_from_slice(&(FACE_ID_DEFAULT as u32).to_le_bytes());
    buf.extend_from_slice(&(templates.len() as u32).to_le_bytes());
    for t in templates {
        for &v in t {
            buf.extend_from_slice(&v.to_le_bytes());
        }
    }
    fs::write(template_path(user_id), &buf)
}

pub fn load_templates(user_id: i32) -> Vec<[f32; DESC_DIM]> {
    let Ok(bytes) = fs::read(template_path(user_id)) else {
        return Vec::new();
    };
    if bytes.len() < 16 || &bytes[0..4] != MAGIC {
        return Vec::new();
    }
    let ver = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
    let count = u32::from_le_bytes(bytes[12..16].try_into().unwrap_or([0; 4])) as usize;
    if ver != VERSION || count == 0 || count > 32 {
        return Vec::new();
    }
    let expected = 16 + count * DESC_DIM * 4;
    if bytes.len() < expected {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(count);
    let mut off = 16;
    for _ in 0..count {
        let mut v = [0f32; DESC_DIM];
        for slot in &mut v {
            *slot = f32::from_le_bytes(bytes[off..off + 4].try_into().unwrap_or([0; 4]));
            off += 4;
        }
        out.push(v);
    }
    out
}

pub fn has_enrollment(user_id: i32) -> bool {
    !load_templates(user_id).is_empty()
}

pub fn remove_enrollment(user_id: i32) {
    let _ = fs::remove_file(template_path(user_id));
}
