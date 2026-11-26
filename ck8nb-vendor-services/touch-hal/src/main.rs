// IOplusTouch V2 HAL for Transsion tran_touch / FocalTech (Tecno CK8nB).
// The framework (ColorTouchNodeManagerService) writes numbered "node flags" through this
// interface. Flag 1 = double-tap-to-wake enable. Every call is logged under tag OplusTouchHal
// so unknown flags (draw gestures etc.) can be mapped from a logcat.
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
/// Asked by ColorTouchNodeManagerService (seen in logcat); the stock-style Qui HAL answered "supported" for dev 0.
const NODE_85: i32 = 85;
const FTS_BUF: &str = "/sys/devices/platform/soc/11015000.i2c/i2c-0/0-0038/fts_gesture_buf";

static DT2W: AtomicBool = AtomicBool::new(true);

/// Node flag the framework reads after a screen-off gesture key: "type,x:y,x:y,x:y,x:y,x:y,x:y,clockwise"
/// (OplusScreenOffGestureUtil.updateGestureInfo, confirmed in oplus-services.jar).
const NODE_GESTURE_INFO: i32 = 2;
const DOUBLE_TAP_INFO: &str = "1,0:0,0:0,0:0,0:0,0:0,0:0,0";

/// The last drawn gesture: (when, "type,points,clockwise"). The tran_touch driver reports every
/// gesture to Android as the same KEY_F13, so the letter is taken from its kernel log line
/// (g_gtp_gesture=0x..) and handed to the framework through the flag-2 read.
static LAST_GESTURE: Mutex<Option<(Instant, String)>> = Mutex::new(None);

/// FocalTech/tran_touch letter code -> ColorOS gesture type (gestureTosString in oplus-services.jar:
/// 1 double tap, 2 up-V, 3 down-V (the normal V), 4 <, 5 >, 6 circle (O), 7 II, 12 M, 13 W).
fn gesture_type(code: u32) -> Option<u32> {
    match code {
        0x6f => Some(6),  // o
        0x76 => Some(3),  // v
        0x77 => Some(13), // w
        0x6d => Some(12), // m
        0xcc | 0x24 => Some(1), // double tap
        _ => None,
    }
}

/// Parse the "n( x,y)" pairs of fts_gesture_buf and sample six points + the turning direction.
fn gesture_points() -> (String, i32) {
    let t = read_node_raw(FTS_BUF);
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
        return ("0:0,0:0,0:0,0:0,0:0,0:0".to_string(), 0);
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
    (sample.join(","), if area > 0 { 1 } else { 0 })
}

/// Follows the kernel log for "g_gtp_gesture=0x.." and keeps the matching ColorOS gesture string.
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
        let mut seen = 0u32;
        loop {
            match f.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    seen += 1;
                    if seen <= 3 || seen % 500 == 0 {
                        logging::info(&format!("gesture watch alive, {seen} kmsg records read"));
                    }
                    let rec = String::from_utf8_lossy(&buf[..n]).into_owned();
                    if rec.contains("GestureHandle") || rec.contains("gesture has been disabled") || rec.contains("GestureIdTranslate") {
                        if let Some(i) = rec.find(';') {
                            logging::info(&format!("kernel: {}", rec[i + 1..].trim()));
                        }
                    }
                    if let Some(i) = rec.find("g_gtp_gesture=0x") {
                        let hex: String = rec[i + 16..].chars().take_while(|c| c.is_ascii_hexdigit()).collect();
                        let code = u32::from_str_radix(&hex, 16).unwrap_or(0);
                        match gesture_type(code) {
                            Some(t) => {
                                let (pts, cw) = gesture_points();
                                let info = format!("{t},{pts},{cw}");
                                logging::info(&format!("gesture 0x{code:x} -> type {t} info={info}"));
                                *LAST_GESTURE.lock().unwrap() = Some((Instant::now(), info));
                            }
                            None => {
                                logging::info(&format!("gesture 0x{code:x} has no ColorOS type, ignored"));
                                *LAST_GESTURE.lock().unwrap() = Some((Instant::now(), "0".to_string()));
                            }
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
    if let Some((t, s)) = LAST_GESTURE.lock().unwrap().as_ref() {
        if t.elapsed() < Duration::from_secs(3) {
            return s.clone();
        }
    }
    DOUBLE_TAP_INFO.to_string()
}

/// Gesture codes the tran_touch driver accepts, written as "<code><1>": cc = double tap,
/// o0/v0/w0/m0/e0/c0/z0/s0 = drawn letters. The kernel rejects gestures that were never enabled
/// ("this gesture has been disabled"), and the framework did not enable them on this ROM.
const GESTURES: [&str; 11] = ["al", "cc", "fp", "o0", "v0", "w0", "m0", "e0", "c0", "z0", "s0"];

fn enable_gestures(first: bool) {
    // Always re-write every code: the driver reports the state as on but drops the screen-off
    // configuration at suspend, so skipping codes that "read as on" broke double-tap.
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

/// Log which touch/FOD related nodes exist, so unknown framework flags can be mapped from one logcat.
fn inventory() {
    for dir in ["/proc", "/sys/devices/platform/touch", "/sys/devices/platform/soc/11015000.i2c/i2c-0/0-0038", "/sys/kernel/tran_display"] {
        if let Ok(rd) = fs::read_dir(dir) {
            let mut names: Vec<String> = rd
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| dir != "/proc" || ["gesture", "fod", "tp", "touch", "fts", "tran", "lcm", "hbm"].iter().any(|k| n.contains(k)))
                .collect();
            names.sort();
            logging::info(&format!("nodes {dir}: {}", names.join(" ")));
        }
    }
}

/// Read a proc/sysfs node with a raw read(). std::fs::read_to_string probes with a tiny buffer
/// and panics when this vendor driver reports more bytes than were requested
/// (observed: "range end index 119 out of range for slice of length 32").
fn read_node(path: &str) -> String {
    use std::ffi::CString;
    let Ok(c) = CString::new(path) else { return String::new() };
    unsafe {
        let fd = libc::open(c.as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC);
        if fd < 0 {
            return format!("err open errno={}", std::io::Error::last_os_error().raw_os_error().unwrap_or(-1));
        }
        let mut buf = vec![0u8; 4096];
        let n = libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len());
        libc::close(fd);
        if n < 0 {
            return format!("err read errno={}", std::io::Error::last_os_error().raw_os_error().unwrap_or(-1));
        }
        let n = (n as usize).min(buf.len());
        String::from_utf8_lossy(&buf[..n]).trim().replace('\n', " | ")
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

/// Kernel format is "<2-char code><digit>"; "cc1" enables double click (confirmed on device).
/// The disable digit is not documented, so try 0 then 2 and verify through gesture_state.
fn set_double_tap(on: bool) -> bool {
    // v5: never switch double tap off. On this ROM the framework writes a "0" at screen-off
    // and the driver then answers every 0xcc with "this gesture has been disabled".
    if !on {
        logging::info("double tap OFF requested -> ignored, keeping cc1");
    }
    DT2W.store(true, Ordering::Relaxed);
    write_node(GESTURE_FN, "cc1")
}

/// Find the /dev/input/eventN of the touch panel from /proc/bus/input/devices.
fn find_touch_event() -> Option<String> {
    let t = read_node_raw("/proc/bus/input/devices");
    let mut name_ok = false;
    for line in t.lines() {
        if line.starts_with("N: ") {
            let l = line.to_lowercase();
            name_ok = l.contains("mtk-tpd") || l.contains("fts") || l.contains("tran_touch") || l.contains("touch");
        } else if line.starts_with("H: ") && name_ok {
            if let Some(h) = line.split_whitespace().find(|w| w.starts_with("event")) {
                return Some(format!("/dev/input/{h}"));
            }
        }
    }
    None
}

fn read_node_raw(path: &str) -> String {
    fs::read(path).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default()
}

/// Gesture trace: logs every key event of the touch panel together with the chip's gesture buffer,
/// so the next logcat shows what a drawn letter really produces (key code, ID, points).
fn gesture_trace() {
    use std::io::Read;
    loop {
        let Some(path) = find_touch_event() else {
            std::thread::sleep(std::time::Duration::from_secs(10));
            continue;
        };
        let Ok(mut f) = fs::File::open(&path) else {
            std::thread::sleep(std::time::Duration::from_secs(10));
            continue;
        };
        logging::info(&format!("gesture trace on {path}"));
        let mut buf = [0u8; 24];
        while f.read_exact(&mut buf).is_ok() {
            let typ = u16::from_ne_bytes([buf[16], buf[17]]);
            let code = u16::from_ne_bytes([buf[18], buf[19]]);
            let val = i32::from_ne_bytes([buf[20], buf[21], buf[22], buf[23]]);
            if typ == 1 && val == 1 {
                // EV_KEY press
                logging::info(&format!("touch key code={code} buf=[{}]", read_node(FTS_BUF)));
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}

struct TouchHal;
impl Interface for TouchHal {}

impl IOplusTouch for TouchHal {
    fn initialize(&self) -> BinderResult<OplusTouchStatus> {
        logging::info("initialize");
        Ok(OplusTouchStatus::OK)
    }

    fn isTouchNodeSupport(&self, d: i32, flag: i32) -> BinderResult<i32> {
        // only the main touch device (0) is supported; flags 8, 171 and others are logged here
        let s = (d == 0 && matches!(flag, NODE_DOUBLE_TAP | NODE_DOUBLE_TAP_INDEP | NODE_85 | NODE_GESTURE_INFO)) as i32;
        logging::info(&format!("isTouchNodeSupport dev={d} flag={flag} -> {s}"));
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
        logging::info(&format!("write dev={d} flag={flag} value={info:?}"));
        match flag {
            NODE_DOUBLE_TAP => {
                let on = info.trim() != "0";
                Ok(if set_double_tap(on) { 0 } else { -1 })
            }
            NODE_DOUBLE_TAP_INDEP => {
                // value looks like a bitmask (65537 = 0x10001); bit 0 = double tap enable
                let mask: i64 = info.trim().parse().unwrap_or(0);
                let ok = set_double_tap(mask & 1 != 0);
                logging::info(&format!("flag21 mask={mask:#x} -> dt2w={} ok={ok} state=[{}]", mask & 1 != 0, state()));
                Ok(0)
            }
            _ => Ok(0),
        }
    }

    fn touchWriteBtInfo(&self, d: i32, flag: i32, info: &str) -> BinderResult<i32> {
        logging::info(&format!("btinfo dev={d} flag={flag} value={info:?}"));
        Ok(0)
    }

    fn touchWriteNodeFileOneWay(&self, d: i32, flag: i32, info: &str) -> BinderResult<()> {
        logging::info(&format!("write-oneway dev={d} flag={flag} value={info:?}"));
        if flag == NODE_DOUBLE_TAP {
            set_double_tap(info.trim() != "0");
        }
        Ok(())
    }

    fn touchNotifyClient(&self, c: i32, info: &OplusTouchInfo) -> BinderResult<OplusTouchStatus> {
        logging::info(&format!("notifyClient {c} dev={} flag={} data={} info={:?}", info.deviceId, info.nodeFlag, info.data, info.info));
        Ok(OplusTouchStatus::OK)
    }

    fn registerEventCallback(&self, _cb: &Strong<dyn IOplusTouchEventCallback>) -> BinderResult<OplusTouchStatus> {
        logging::info("registerEventCallback");
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
    inventory();
    logging::info(&format!("gesture_state at start: [{}]", state()));
    if let Err(e) = ProcessState::init_default() {
        logging::error(&format!("binder init failed: {e:?}"));
        std::process::exit(1);
    }
    enable_gestures(true);
    std::thread::spawn(gesture_trace);
    std::thread::spawn(gesture_watch);
    std::thread::spawn(|| loop {
        // the driver drops its gesture mask on suspend/resume cycles, so keep it asserted
        std::thread::sleep(std::time::Duration::from_secs(5));
        if DT2W.load(Ordering::Relaxed) {
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
