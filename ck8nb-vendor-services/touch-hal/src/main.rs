// IOplusTouch V2 HAL for Transsion tran_touch / FocalTech (Tecno CK8nB).
// 100% Event-Driven (zero background threads, zero kernel log polling, zero 5s timer):
// - Reads fts_gesture_buf directly on-demand when ColorOS queries flag=2.
// - Writes /proc/gesture_function once at boot and on ColorOS touchWriteNodeFile calls.
// - Maps V (0x76/0x54) -> 2 (Flashlight), Λ (0x5e/0x55) -> 3, O/C (0x6f/0x30/0x63/0x34) -> 6,
//   M (0x6d/0x32) -> 12, W (0x77/0x31) -> 13, Double-Tap (0xcc/0x24) -> 1.
mod bindings;
mod logging;

use rsbinder::{hub, BinderResult, Interface, ProcessState, Strong};
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};

use bindings::{BnOplusTouch, IOplusTouch, IOplusTouchEventCallback, OplusTouchInfo, OplusTouchStatus};

const SERVICE_NAME: &str = "vendor.oplus.hardware.touch.IOplusTouch/default";
const GESTURE_FN: &str = "/proc/gesture_function";
const GESTURE_STATE: &str = "/proc/gesture_state";

const NODE_DOUBLE_TAP: i32 = 1;
const NODE_DOUBLE_TAP_INDEP: i32 = 21;
const NODE_85: i32 = 85;
const FTS_BUF: &str = "/sys/devices/platform/soc/11015000.i2c/i2c-0/0-0038/fts_gesture_buf";

static DT2W: AtomicBool = AtomicBool::new(true);

const NODE_GESTURE_INFO: i32 = 2;
const DOUBLE_TAP_INFO: &str = "1,0:0,0:0,0:0,0:0,0:0,0:0,0";

/// Maps kernel ASCII hex codes (g_gtp_gesture=0x..) and FocalTech Gesture IDs -> ColorOS nGesture:
/// 1 = Double-Tap, 2 = V (Flashlight), 3 = Λ/^ (Calculator), 4 = <, 5 = >,
/// 6 = O (Camera), 7 = || (Music Play/Pause), 12 = M, 13 = W.
fn gesture_type(code: u32) -> Option<u32> {
    match code {
        0xcc | 0x24 => Some(1),         // Double-Tap
        0x76 | 0x54 => Some(2),         // V -> UpVee (Flashlight)
        0x5e | 0x55 => Some(3),         // Λ / ^ -> DownVee (Calculator)
        0x3c => Some(4),                // < -> Previous track
        0x3e => Some(5),                // > -> Next track
        0x6f | 0x30 | 0x63 | 0x34 => Some(6), // O (and open-circle C) -> Circle (Camera)
        0x22 | 0x7c => Some(7),         // || -> Play/Pause
        0x6d | 0x32 => Some(12),        // M
        0x77 | 0x31 => Some(13),        // W
        _ => None,
    }
}

fn read_sysfs_libc(path: &str) -> String {
    use std::ffi::CString;
    let Ok(c) = CString::new(path) else { return String::new() };
    unsafe {
        let fd = libc::open(c.as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC);
        if fd < 0 {
            return String::new();
        }
        let mut buf = vec![0u8; 4096];
        let n = libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len());
        libc::close(fd);
        if n <= 0 {
            return String::new();
        }
        let n = (n as usize).min(buf.len());
        String::from_utf8_lossy(&buf[..n]).into_owned()
    }
}

fn parse_points_from_text(t: &str) -> (String, i32) {
    let mut pts: Vec<(i32, i32)> = Vec::new();
    for part in t.split('(').skip(1) {
        if let Some(inner) = part.split(')').next() {
            let mut it = inner.split(',');
            if let (Some(x), Some(y)) = (it.next(), it.next()) {
                if let (Ok(x), Ok(y)) = (x.trim().parse(), y.trim().parse()) {
                    pts.push((x, y));
                }
            }
        }
    }
    if pts.is_empty() {
        return ("540:800,540:800,340:1000,540:1200,740:1000,540:800".to_string(), 1);
    }
    let n = pts.len();
    let sample: Vec<String> = (0..6)
        .map(|i| {
            let (x, y) = pts[(i * (n - 1)) / 5];
            format!("{x}:{y}")
        })
        .collect();
    let mut area: i64 = 0;
    for i in 0..n {
        let (x1, y1) = pts[i];
        let (x2, y2) = pts[(i + 1) % n];
        area += x1 as i64 * y2 as i64 - x2 as i64 * y1 as i64;
    }
    (sample.join(","), if area >= 0 { 1 } else { 0 })
}

fn read_direct_fts_gesture() -> Option<String> {
    let t = read_sysfs_libc(FTS_BUF);
    for line in t.lines() {
        if let Some(val) = line.trim().strip_prefix("Gesture ID:") {
            let v = val.trim();
            let raw = if let Some(hex) = v.strip_prefix("0x").or_else(|| v.strip_prefix("0X")) {
                u32::from_str_radix(hex, 16).ok()?
            } else {
                v.parse::<u32>().ok()?
            };
            let Some(mapped) = gesture_type(raw) else {
                logging::info(&format!("fts_gesture_buf unknown ID={raw} (0x{raw:x})"));
                return None;
            };
            if mapped == 1 {
                return Some(DOUBLE_TAP_INFO.to_string());
            }
            let (pts, cw) = parse_points_from_text(&t);
            let info = format!("{mapped},{pts},{cw}");
            logging::info(&format!("fts_gesture_buf direct ID={raw} -> {info}"));
            return Some(info);
        }
    }
    None
}

fn gesture_info() -> String {
    if let Some(direct) = read_direct_fts_gesture() {
        logging::info(&format!("flag2 (direct fts_buf) -> {direct}"));
        return direct;
    }
    DOUBLE_TAP_INFO.to_string()
}

// Enable ONLY the gestures ColorOS actually uses; explicitly disable e0/c0/z0/s0
// so FocalTech doesn't misclassify slightly open O circles as C (0x63/52).
const GESTURES_ON: [&str; 9] = ["al", "cc", "fp", "5e", "3e", "o0", "v0", "w0", "m0"];
const GESTURES_OFF: [&str; 4] = ["e0", "c0", "z0", "s0"];

fn enable_gestures(first: bool) {
    for g in GESTURES_OFF {
        let _ = fs::write(GESTURE_FN, format!("{g}0"));
    }
    for g in GESTURES_ON {
        let cmd = format!("{g}1");
        match fs::write(GESTURE_FN, &cmd) {
            Ok(_) => {
                if first {
                    logging::info(&format!("enabled gesture {g}"));
                }
            }
            Err(e) => {
                if first {
                    logging::error(&format!("enable gesture {g} failed: {e}"));
                }
            }
        }
    }
}

fn read_node(path: &str) -> String {
    let raw = read_sysfs_libc(path);
    if raw.is_empty() {
        String::new()
    } else {
        raw.trim().replace('\n', " | ")
    }
}

fn state() -> String {
    read_node(GESTURE_STATE)
}

fn write_node(path: &str, v: &str) -> bool {
    match fs::write(path, v) {
        Ok(_) => true,
        Err(e) => {
            logging::error(&format!("write {path} <- {v:?} failed: {e}"));
            false
        }
    }
}

fn set_double_tap(on: bool) -> bool {
    if !on {
        logging::info("double tap OFF requested -> ignored, keeping gestures armed");
    }
    DT2W.store(true, Ordering::Relaxed);
    enable_gestures(false);
    write_node(GESTURE_FN, "cc1")
}

struct TouchHal;
impl Interface for TouchHal {}

impl IOplusTouch for TouchHal {
    fn initialize(&self) -> BinderResult<OplusTouchStatus> {
        logging::info("initialize");
        Ok(OplusTouchStatus::OK)
    }

    fn isTouchNodeSupport(&self, d: i32, flag: i32) -> BinderResult<i32> {
        let s = (d == 0 && matches!(flag, NODE_DOUBLE_TAP | NODE_DOUBLE_TAP_INDEP | NODE_85 | NODE_GESTURE_INFO)) as i32;
        Ok(s)
    }

    fn touchReadNodeFile(&self, d: i32, flag: i32) -> BinderResult<String> {
        let v = match flag {
            NODE_DOUBLE_TAP | NODE_DOUBLE_TAP_INDEP => {
                if DT2W.load(Ordering::Relaxed) { "1" } else { "0" }.to_string()
            }
            NODE_GESTURE_INFO => gesture_info(),
            _ => String::from("0"),
        };
        logging::info(&format!("read dev={d} flag={flag} -> {v}"));
        Ok(v)
    }

    fn touchWriteNodeFile(&self, d: i32, flag: i32, info: &str) -> BinderResult<i32> {
        match flag {
            NODE_DOUBLE_TAP => {
                let on = info.trim() != "0";
                Ok(if set_double_tap(on) { 0 } else { -1 })
            }
            NODE_DOUBLE_TAP_INDEP => {
                let mask: i64 = info.trim().parse().unwrap_or(0);
                let ok = set_double_tap(mask & 1 != 0);
                logging::info(&format!("flag21 mask={mask:#x} -> dt2w={} ok={ok} state=[{}]", mask & 1 != 0, state()));
                Ok(0)
            }
            _ => Ok(0),
        }
    }

    fn touchWriteBtInfo(&self, _d: i32, _flag: i32, _info: &str) -> BinderResult<i32> {
        Ok(0)
    }

    fn touchWriteNodeFileOneWay(&self, _d: i32, flag: i32, info: &str) -> BinderResult<()> {
        if flag == NODE_DOUBLE_TAP {
            set_double_tap(info.trim() != "0");
        }
        Ok(())
    }

    fn touchNotifyClient(&self, _c: i32, _info: &OplusTouchInfo) -> BinderResult<OplusTouchStatus> {
        Ok(OplusTouchStatus::OK)
    }

    fn registerEventCallback(&self, _cb: &Strong<dyn IOplusTouchEventCallback>) -> BinderResult<OplusTouchStatus> {
        Ok(OplusTouchStatus::OK)
    }

    fn unregisterEventCallback(&self, _cb: &Strong<dyn IOplusTouchEventCallback>) -> BinderResult<OplusTouchStatus> {
        Ok(OplusTouchStatus::OK)
    }
}

fn main() {
    logging::init();
    logging::init_rsbinder_logger();
    logging::info(concat!("touch-hal build ", env!("CARGO_PKG_VERSION")));
    logging::info(&format!("gesture_state at start: [{}]", state()));
    if let Err(e) = ProcessState::init_default() {
        logging::error(&format!("binder init failed: {e:?}"));
        std::process::exit(1);
    }
    enable_gestures(true);
    ProcessState::start_thread_pool();
    let binder = BnOplusTouch::new_binder(TouchHal);
    match hub::add_service(SERVICE_NAME, binder.as_binder()) {
        Ok(()) => logging::info(&format!("registered {SERVICE_NAME}")),
        Err(e) => {
            logging::error(&format!("register failed: {e:?}"));
            std::process::exit(1);
        }
    }
    let _ = ProcessState::join_thread_pool();
}
