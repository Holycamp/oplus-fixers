//! Zero-race screen-off gesture reader for FocalTech (CK8nB / MT6891).
//! Drop this file into: ck8nb-vendor-services/touch-hal/src/gesture_direct.rs
//!
//! Reads `/sys/devices/platform/soc/11015000.i2c/i2c-0/0-0038/fts_gesture_buf`
//! directly when ColorOS calls `touchReadNodeFile(dev=0, flag=2)` instead of
//! racing `/dev/kmsg`. Also fixes Draw V (`UpVee = 2`).

use std::fs;

const FTS_BUF_PATH: &str =
    "/sys/devices/platform/soc/11015000.i2c/i2c-0/0-0038/fts_gesture_buf";

/// Maps FocalTech `Gesture ID` (decimal or ASCII byte) to ColorOS `nGesture` type.
/// - 1  = Double-Tap (`DouTap`)
/// - 2  = Draw V (`UpVee` — Flashlight)
/// - 6  = Draw O (`Circle` — Camera)
/// - 12 = Draw M (`Mgestrue`)
/// - 13 = Draw W (`Wgestrue`)
fn map_gesture_id(raw_id: i32) -> i32 {
    match raw_id {
        36 | 0x24 | 0xcc => 1,  // Double-Tap
        84 | 0x54 | 0x76 => 2,  // V (UpVee in ColorOS)
        48 | 0x30 | 0x6f => 6,  // O (Circle)
        50 | 0x32 | 0x6d => 12, // M
        49 | 0x31 | 0x77 => 13, // W
        _ => 1,                 // Fallback to Double-Tap so screen still wakes
    }
}

/// Parses `(x, y)` tuples from FocalTech `Gesture Points Buffer:` lines like:
/// `  0( 727,1157)   1( 721,1149)   2( 716,1143)`
fn parse_points(text: &str) -> Vec<(i32, i32)> {
    let mut pts = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('(') {
        let after_open = &rest[open + 1..];
        let Some(close) = after_open.find(')') else {
            break;
        };
        let inside = &after_open[..close];
        if let Some((xs, ys)) = inside.split_once(',') {
            if let (Ok(x), Ok(y)) = (xs.trim().parse::<i32>(), ys.trim().parse::<i32>()) {
                pts.push((x, y));
            }
        }
        rest = &after_open[close + 1..];
    }
    pts
}

/// Computes clockwise direction (1 = clockwise, 0 = counter-clockwise) using signed area.
fn is_clockwise(pts: &[(i32, i32)]) -> i32 {
    if pts.len() < 3 {
        return 1;
    }
    let mut area: i64 = 0;
    for i in 0..pts.len() {
        let (x1, y1) = pts[i];
        let (x2, y2) = pts[(i + 1) % pts.len()];
        area += (x2 as i64 - x1 as i64) * (y2 as i64 + y1 as i64);
    }
    if area >= 0 { 1 } else { 0 }
}

/// Reads `fts_gesture_buf` directly and returns ColorOS's 8-field `flag=2` string:
/// `<type>,<x0>:<y0>,<x1>:<y1>,<x2>:<y2>,<x3>:<y3>,<x4>:<y4>,<x5>:<y5>,<clockwise>`
pub fn read_flag2_coordinate() -> String {
    let Ok(buf) = fs::read_to_string(FTS_BUF_PATH) else {
        return "1,540:1200,540:1200,540:1200,540:1200,540:1200,540:1200,1".to_string();
    };

    let mut raw_id: i32 = 36;
    for line in buf.lines() {
        if let Some(val) = line.trim().strip_prefix("Gesture ID:") {
            let v = val.trim();
            raw_id = if let Some(hex) = v.strip_prefix("0x").or_else(|| v.strip_prefix("0X")) {
                i32::from_str_radix(hex, 16).unwrap_or(36)
            } else {
                v.parse::<i32>().unwrap_or(36)
            };
            break;
        }
    }

    let oplus_type = map_gesture_id(raw_id);
    if oplus_type == 1 {
        return "1,540:1200,540:1200,540:1200,540:1200,540:1200,540:1200,1".to_string();
    }

    let pts = parse_points(&buf);
    if pts.is_empty() {
        return format!(
            "{},540:800,540:800,340:1000,540:1200,740:1000,540:800,1",
            oplus_type
        );
    }

    let last = pts.len() - 1;
    let sample = |idx: usize| -> (i32, i32) { pts[(idx * last) / 5] };
    let p0 = sample(0);
    let p1 = sample(1);
    let p2 = sample(2);
    let p3 = sample(3);
    let p4 = sample(4);
    let p5 = sample(5);
    let cw = is_clockwise(&pts);

    format!(
        "{},{}:{},{}:{},{}:{},{}:{},{}:{},{}:{},{}",
        oplus_type, p0.0, p0.1, p1.0, p1.1, p2.0, p2.1, p3.0, p3.1, p4.0, p4.1, p5.0, p5.1, cw
    )
}
