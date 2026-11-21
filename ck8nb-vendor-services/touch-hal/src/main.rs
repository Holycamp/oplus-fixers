// IOplusTouch V2 HAL for Transsion tran_touch / FocalTech (Tecno CK8nB).
// The framework (ColorTouchNodeManagerService) writes numbered "node flags" through this
// interface. Flag 1 = double-tap-to-wake enable. Every call is logged under tag OplusTouchHal
// so unknown flags (draw gestures etc.) can be mapped from a logcat.
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

static DT2W: AtomicBool = AtomicBool::new(false);

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
    DT2W.store(on, Ordering::Relaxed);
    if on {
        return write_node(GESTURE_FN, "cc1");
    }
    let ok = write_node(GESTURE_FN, "cc0") || write_node(GESTURE_FN, "cc2");
    logging::info(&format!("double tap off ok={ok}"));
    ok
}

struct TouchHal;
impl Interface for TouchHal {}

impl IOplusTouch for TouchHal {
    fn initialize(&self) -> BinderResult<OplusTouchStatus> {
        logging::info("initialize");
        Ok(OplusTouchStatus::OK)
    }

    fn isTouchNodeSupport(&self, d: i32, flag: i32) -> BinderResult<i32> {
        let s = matches!(flag, NODE_DOUBLE_TAP | NODE_DOUBLE_TAP_INDEP) as i32; // flags 8, 171 and others are logged here
        logging::info(&format!("isTouchNodeSupport dev={d} flag={flag} -> {s}"));
        Ok(s)
    }

    fn touchReadNodeFile(&self, d: i32, flag: i32) -> BinderResult<String> {
        let v = match flag {
            NODE_DOUBLE_TAP | NODE_DOUBLE_TAP_INDEP => {
                if DT2W.load(Ordering::Relaxed) { "1" } else { "0" }.to_string()
            }
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
    inventory();
    logging::info(&format!("gesture_state at start: [{}]", state()));
    if let Err(e) = ProcessState::init_default() {
        logging::error(&format!("binder init failed: {e:?}"));
        std::process::exit(1);
    }
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
