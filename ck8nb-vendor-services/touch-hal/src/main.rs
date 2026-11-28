// IOplusTouch V2 HAL for Transsion tran_touch / FocalTech (Tecno CK8nB).
// Fixes:
// 1. V (0x76 / 0x54) -> ColorOS 2 (UpVee = Flashlight), Λ/^ (0x5e) -> ColorOS 3 (DownVee = Calculator).
// 2. Adds "5e" (^) to GESTURES so Λ/^ is actually enabled in /proc/gesture_function!
// 3. Stops hammering /proc/gesture_function every 5s while screen is in TRAN_GESTURE_SUSPEND_MODE
//    (only refreshes while screen is active or on flag=21) so drawn letters aren't interrupted mid-stroke.
// 4. Ignores non-F4 events (0xbe) without poisoning LAST_GESTURE / GESTURE_SEQ and reads fts_gesture_buf directly.
mod bindings;
mod logging;

use rsbinder::{hub, BinderResult, Interface, ProcessState, Strong};
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use bindings::{BnOplusTouch, IOplusTouch, IOplusTouchEventCallback, OplusTouchInfo, OplusTouchStatus};

const SERVICE_NAME: &str = "vendor.oplus.hardware.touch.IOplusTouch/default";
const GESTURE_FN: &str = "/proc/gesture_function";
const GESTURE_STATE: &str = "/proc/gesture_state";

const NODE_DOUBLE_TAP: i32 = 1;
const NODE_DOUBLE_TAP_INDEP: i32 = 21;
const NODE_85: i32 = 85;
const FTS_BUF: &str = "/sys/devices/platform/soc/11015000.i2c/i2c-0/0-0038/fts_gesture_buf";

static DT2W: AtomicBool = AtomicBool::new(true);
static SCREEN_OFF: AtomicBool = AtomicBool::new(false);

const NODE_GESTURE_INFO: i32 = 2;
const DOUBLE_TAP_INFO: &str = "1,0:0,0:0,0:0,0:0,0:0,0:0,0";

static LAST_GESTURE: Mutex<Option<(Instant, String)>> = Mutex::new(None);
static GESTURE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static CONSUMED_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Maps kernel ASCII hex codes (g_gtp_gesture=0x..) and FocalTech Gesture IDs -> ColorOS nGesture:
/// 1 = Double-Tap, 2 = V (UpVee / Flashlight), 3 = Λ/^ (DownVee / Calculator),
/// 4 = <, 5 = >, 6 = O (Circle / Camera), 12 = M, 13 = W.
fn gesture_type(code: u32) -> Option<u32> {
    match code {
        0xcc | 0x24 => Some(1),  // Double-Tap
        0x76 | 0x54 => Some(2),  // V -> UpVee (Flashlight)
        0x5e | 0x55 => Some(3),  // Λ / ^ -> DownVee (Calculator)
        0x3c => Some(4),         // < -> LeftVee
        0x3e => Some(5),         // > -> RightVee
        0x6f | 0x30 => Some(6),  // O -> Circle (Camera)
        0x6d | 0x32 => Some(12), // M
        0x77 | 0x31 => Some(13), // W
        _ => None,
    }
}

/// Raw libc::read helper (keeps newlines intact and avoids std::fs::read_to_string panics on vendor sysfs).
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

/// Parse the "n( x,y)" pairs of fts_gesture_buf and sample six points + turning direction.
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

fn gesture_points() -> (String, i32) {
    parse_points_from_text(&read_sysfs_libc(FTS_BUF))
}

/// Direct zero-race read of Gesture ID + points straight from fts_gesture_buf.
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

/// Follows the kernel log for "g_gtp_gesture=0x.." and touch suspend/active mode transitions.
fn gesture_watch() {
    use std::io::Read;
    loop {
        let mut f = match fs::File::open("/dev/kmsg") {
            Ok(f) => f,
            Err(e) => {
                logging::error(&format!("gesture watch: open /dev/kmsg failed: {e}"));
                std::thread::sleep(Duration::from_secs(5));
                continue;
            }
        };
        unsafe { libc::lseek(std::os::fd::AsRawFd::as_raw_fd(&f), 0, libc::SEEK_END) };
        logging::info("gesture watch running (kmsg)");
        let mut buf = vec![0u8; 8192];
        loop {
            match f.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let rec = String::from_utf8_lossy(&buf[..n]).into_owned();
                    if rec.contains("core mode change") {
                        if let Some(i) = rec.find("--> ") {
                            let mode = rec[i + 4..].trim();
                            let off = !mode.contains("TRAN_ACTIVE_MODE");
                            SCREEN_OFF.store(off, Ordering::SeqCst);
                        }
                    }
                    if let Some(i) = rec.find("g_gtp_gesture=0x") {
                        let hex: String = rec[i + 16..].chars().take_while(|c| c.is_ascii_hexdigit()).collect();
                        let code = u32::from_str_radix(&hex, 16).unwrap_or(0);
                        if let Some(t) = gesture_type(code) {
                            let (pts, cw) = gesture_points();
                            let info = if t == 1 { DOUBLE_TAP_INFO.to_string() } else { format!("{t},{pts},{cw}") };
                            logging::info(&format!("gesture 0x{code:x} -> type {t} info={info}"));
                            *LAST_GESTURE.lock().unwrap() = Some((Instant::now(), info));
                            GESTURE_SEQ.fetch_add(1, Ordering::SeqCst);
                        } else {
                            logging::info(&format!("gesture 0x{code:x} ignored (non-F4 event)"));
                        }
                    }
                }
                Err(e) if e.raw_os_error() == Some(32) => continue,
                Err(e) => {
                    logging::error(&format!("gesture watch: kmsg read error: {e}"));
                    break;
                }
            }
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

fn gesture_info() -> String {
    // 1. Primary path: read Gesture ID directly from fts_gesture_buf (zero race!)
    if let Some(direct) = read_direct_fts_gesture() {
        CONSUMED_SEQ.store(GESTURE_SEQ.load(Ordering::SeqCst), Ordering::SeqCst);
        logging::info(&format!("flag2 (direct fts_buf) -> {direct}"));
        return direct;
    }

    // 2. Fallback: wait briefly for kmsg watcher
    let start = Instant::now();
    loop {
        let seq = GESTURE_SEQ.load(Ordering::SeqCst);
        if seq != CONSUMED_SEQ.load(Ordering::SeqCst) {
            CONSUMED_SEQ.store(seq, Ordering::SeqCst);
            if let Some((t, s)) = LAST_GESTURE.lock().unwrap().as_ref() {
                if t.elapsed() < Duration::from_millis(800) && s != "0" {
                    logging::info(&format!("flag2 (kmsg) -> {s} (waited {} ms)", start.elapsed().as_millis()));
                    return s.clone();
                }
            }
            break;
        }
        if start.elapsed() > Duration::from_millis(120) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    DOUBLE_TAP_INFO.to_string()
}

// Includes "5e" (Λ / ^) and "3e" (>) so all ColorOS screen-off shapes are enabled in the touch IC!
const GESTURES: [&str; 13] = ["al", "cc", "fp", "5e", "3e", "o0", "v0", "w0", "m0", "e0", "c0", "z0", "s0"];

fn enable_gestures(first: bool) {
    for g in GESTURES {
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
    std::thread::spawn(gesture_watch);
    std::thread::spawn(|| loop {
        std::thread::sleep(std::time::Duration::from_secs(5));
        // Only refresh while the screen is awake so we never interrupt gesture IC mid-draw!
        if DT2W.load(Ordering::Relaxed) && !SCREEN_OFF.load(Ordering::SeqCst) {
            enable_gestures(false);
        }
    });
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
