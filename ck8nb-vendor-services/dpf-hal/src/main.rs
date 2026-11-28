// IDisplayPanelFeature V1 HAL for Tecno CK8nB (Transsion NT37703 DSI video-mode panel, MTK).
// 100% Event-Driven & Deep-Sleep Friendly:
// - Primary FOD watcher blocks directly on the touchscreen /dev/input/event* node (EV_KEY 195),
//   meaning ZERO /dev/kmsg wakeups and 0.00% CPU when idle.
// - Automatic fallback to fast-filtered /dev/kmsg only if /dev/input/event* cannot be opened.
// - ID 217 (GetDisplayLongruiAOD) -> 15 (0b1111: Panoramic/Full-Screen + All-Day + Scheduled AOD).
// - ID 193 (GetDisplayPanelVideo) -> 1  (DSI video-mode panel).
// - Preserves full rsbinder bindings, gfpatch (in-memory libgf_hal.so patch), and lockscreen + screen-off FOD.
mod bindings;
mod fod_wake;
mod gfpatch;
mod logging;

use rsbinder::{hub, BinderResult, Interface, ProcessState};
use bindings::{BnDisplayPanelFeature, IDisplayPanelFeature};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

const HBM_NODE: &str = "/sys/kernel/tran_display/lcm_hbm_state";
const SERVICE_NAME: &str = "vendor.oplus.hardware.displaypanelfeature.IDisplayPanelFeature/default";

const ARM_MS: u64 = 30_000;
const EV_KEY: u16 = 1;
const FOD_KEY_CODE: u16 = 195;

static SCREEN_OFF: AtomicBool = AtomicBool::new(false);
static ARMED_UNTIL: AtomicU64 = AtomicU64::new(0);
static DOWN: AtomicBool = AtomicBool::new(false);
static LIT: AtomicBool = AtomicBool::new(false);
static WAKE_PENDING: AtomicBool = AtomicBool::new(false);

fn hbm(on: bool) {
    let v = if on { "1" } else { "0" };
    match std::fs::write(HBM_NODE, v) {
        Ok(_) => logging::info(&format!("hbm {v} written")),
        Err(e) => logging::error(&format!("hbm {v} write failed: {e}")),
    }
}

fn now_ms() -> u64 {
    static BASE: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    BASE.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64 + 1
}
fn arm() {
    ARMED_UNTIL.store(now_ms() + ARM_MS, Ordering::SeqCst);
}
fn disarm() {
    ARMED_UNTIL.store(0, Ordering::SeqCst);
}
fn armed() -> bool {
    now_ms() < ARMED_UNTIL.load(Ordering::SeqCst)
}

/// On-demand check (only called when FOD key 195 is pressed) to tell screen-off/AOD from bright active screen.
fn is_screen_off_or_aod() -> bool {
    const BL_PATHS: [&str; 3] = [
        "/sys/class/leds/lcd-backlight/brightness",
        "/sys/class/backlight/panel0-backlight/brightness",
        "/sys/class/backlight/mtk_disp_backlight/brightness",
    ];
    for p in BL_PATHS {
        if let Ok(s) = std::fs::read_to_string(p) {
            if let Ok(val) = s.trim().parse::<i32>() {
                return val <= 30;
            }
        }
    }
    SCREEN_OFF.load(Ordering::SeqCst)
}

struct Dpf;
impl Interface for Dpf {}

impl IDisplayPanelFeature for Dpf {
    fn getDisplayPanelFeatureValue(&self, id: i32, values: &mut Vec<i32>) -> BinderResult<i32> {
        let reply = match id {
            // GetDisplayLongruiAOD: 15 (0b1111) = Panoramic/Full-Screen + All-Day + Scheduled AOD
            217 => 15,
            // GetDisplayPanelVideo: 1 = DSI video-mode panel (nt37703_fhdp_dsi_vdo)
            193 => 1,
            _ => 0,
        };
        logging::info(&format!("get id={id} len={} -> [{reply}]", values.len()));
        if values.is_empty() {
            values.push(reply);
        } else {
            for v in values.iter_mut() {
                *v = 0;
            }
            values[0] = reply;
        }
        Ok(0)
    }

    fn setDisplayPanelFeatureValue(&self, id: i32, values: &[i32]) -> BinderResult<i32> {
        logging::info(&format!("set id={id} values={values:?}"));
        if id == 22 {
            if values.first().copied().unwrap_or(0) != 0 {
                arm();
                logging::info("fod armed");
            } else {
                disarm();
                logging::info("fod disarmed");
            }
        }
        Ok(0)
    }

    fn getDisplayPanelInfo(&self, id: i32, info: &mut Vec<String>) -> BinderResult<i32> {
        logging::info(&format!("info id={id}"));
        info.clear();
        Ok(0)
    }
}

fn send_full_hbm_uevent() {
    unsafe {
        let fd = libc::socket(libc::AF_NETLINK, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, 15);
        if fd < 0 {
            logging::error(&format!("uevent socket failed: {}", std::io::Error::last_os_error()));
            return;
        }
        let mut addr: libc::sockaddr_nl = std::mem::zeroed();
        addr.nl_family = libc::AF_NETLINK as u16;
        addr.nl_pid = 0;
        addr.nl_groups = 1;
        let msg: &[u8] = b"change@/devices/virtual/ck8nb/hbm\0ACTION=change\0DEVPATH=/devices/virtual/ck8nb/hbm\0SUBSYSTEM=ck8nb\0TRAN_FULL_HBM_EVENT=FULL_HBM_SET\0";
        let r = libc::sendto(
            fd,
            msg.as_ptr() as *const libc::c_void,
            msg.len(),
            0,
            &addr as *const libc::sockaddr_nl as *const libc::sockaddr,
            std::mem::size_of::<libc::sockaddr_nl>() as u32,
        );
        if r < 0 {
            logging::error(&format!("uevent sendto failed: {}", std::io::Error::last_os_error()));
        } else {
            logging::info(&format!("sent TRAN_FULL_HBM_EVENT=FULL_HBM_SET uevent ({r} bytes)"));
        }
        libc::close(fd);
    }
}

fn announce_hbm() {
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(80));
        if DOWN.load(Ordering::SeqCst) {
            send_full_hbm_uevent();
        }
        std::thread::sleep(std::time::Duration::from_millis(180));
        if DOWN.load(Ordering::SeqCst) {
            send_full_hbm_uevent();
        }
    });
}

fn handle_fod_down_event_driven() {
    if armed() && !is_screen_off_or_aod() {
        DOWN.store(true, Ordering::SeqCst);
        arm();
        if !LIT.swap(true, Ordering::SeqCst) {
            hbm(true);
            announce_hbm();
        }
    } else if is_screen_off_or_aod() {
        DOWN.store(true, Ordering::SeqCst);
        arm();
        if !WAKE_PENDING.swap(true, Ordering::SeqCst) {
            logging::info("fod key 195 down while screen off/AOD -> waking panel via uinput");
            std::thread::spawn(|| {
                fod_wake::inject_wakeup();
                // Wait for nt37703 panel to enter active state, then write HBM + retry once if still held.
                std::thread::sleep(std::time::Duration::from_millis(110));
                if DOWN.load(Ordering::SeqCst) {
                    LIT.store(true, Ordering::SeqCst);
                    hbm(true);
                    announce_hbm();
                    std::thread::sleep(std::time::Duration::from_millis(80));
                    if DOWN.load(Ordering::SeqCst) {
                        hbm(true);
                    }
                }
                WAKE_PENDING.store(false, Ordering::SeqCst);
            });
        }
    } else {
        logging::info("fod key 195 down ignored (screen brightly on, FOD not armed)");
    }
}

fn handle_fod_up() {
    DOWN.store(false, Ordering::SeqCst);
    if LIT.swap(false, Ordering::SeqCst) {
        hbm(false);
    }
}

#[repr(C)]
struct InputEvent {
    tv_sec: i64,
    tv_usec: i64,
    type_: u16,
    code: u16,
    value: i32,
}

fn find_touch_event_device() -> Option<String> {
    for n in 0..32 {
        let name_path = format!("/sys/class/input/event{n}/device/name");
        if let Ok(name) = std::fs::read_to_string(&name_path) {
            let low = name.trim().to_ascii_lowercase();
            if low.contains("tpd") || low.contains("touch") || low.contains("fts") || low.contains("focaltech") {
                let dev = format!("/dev/input/event{n}");
                logging::info(&format!("found touch input device: {dev} ({})", name.trim()));
                return Some(dev);
            }
        }
    }
    None
}

fn try_input_event_watch() -> bool {
    use std::io::Read;
    let Some(dev_path) = find_touch_event_device() else {
        logging::error("no touch /dev/input/event* device found; falling back to kmsg");
        return false;
    };
    let mut f = match std::fs::File::open(&dev_path) {
        Ok(f) => f,
        Err(e) => {
            logging::error(&format!("open {dev_path} failed ({e}); falling back to kmsg"));
            return false;
        }
    };
    logging::info(&format!("event-driven FOD watch active on {dev_path} (0 idle wakeups)"));
    let mut raw = [0u8; std::mem::size_of::<InputEvent>()];
    loop {
        match f.read_exact(&mut raw) {
            Ok(()) => {
                let ev: InputEvent = unsafe { std::ptr::read_unaligned(raw.as_ptr() as *const InputEvent) };
                if ev.type_ == EV_KEY && ev.code == FOD_KEY_CODE {
                    if ev.value == 1 {
                        handle_fod_down_event_driven();
                    } else if ev.value == 0 {
                        handle_fod_up();
                    }
                }
            }
            Err(e) => {
                logging::error(&format!("input read error on {dev_path}: {e}"));
                return false;
            }
        }
    }
}

#[inline]
fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn fod_watch_kmsg_fallback() {
    use std::io::Read;
    loop {
        let mut f = match std::fs::File::open("/dev/kmsg") {
            Ok(f) => f,
            Err(e) => {
                logging::error(&format!("fod watch: open /dev/kmsg failed: {e}"));
                std::thread::sleep(std::time::Duration::from_secs(5));
                continue;
            }
        };
        unsafe { lseek_end(&f) };
        logging::info("fod watch running (kmsg fallback)");
        let mut buf = vec![0u8; 8192];
        loop {
            match f.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let rec = &buf[..n];
                    let msg = match rec.iter().position(|&b| b == b';') {
                        Some(i) => &rec[i + 1..],
                        None => rec,
                    };
                    if msg.len() < 12 || contains_bytes(msg, b"gesture_data_write") {
                        continue;
                    }
                    let is_hbm = contains_bytes(msg, b"hbm");
                    let is_fod = contains_bytes(msg, b"fod finger ");
                    let is_mode = contains_bytes(msg, b"core mode change");
                    if !is_hbm && !is_fod && !is_mode {
                        continue;
                    }
                    if is_hbm && contains_bytes(msg, b"skip set hbm node") {
                        LIT.store(false, Ordering::SeqCst);
                        if DOWN.load(Ordering::SeqCst) {
                            std::thread::spawn(|| {
                                std::thread::sleep(std::time::Duration::from_millis(60));
                                if DOWN.load(Ordering::SeqCst) && !LIT.swap(true, Ordering::SeqCst) {
                                    hbm(true);
                                }
                            });
                        }
                    } else if is_hbm && contains_bytes(msg, b"tran_lcm_hbm_store value= 1") {
                        announce_hbm();
                        if !DOWN.load(Ordering::SeqCst) {
                            std::thread::spawn(|| {
                                std::thread::sleep(std::time::Duration::from_millis(30));
                                if !DOWN.load(Ordering::SeqCst) {
                                    hbm(false);
                                }
                            });
                        }
                    }
                    if is_mode {
                        let text = String::from_utf8_lossy(msg);
                        if let Some(i) = text.find("--> ") {
                            let new_mode = text[i + 4..].trim().to_string();
                            let off = !new_mode.contains("TRAN_ACTIVE_MODE");
                            SCREEN_OFF.store(off, Ordering::SeqCst);
                            if !off {
                                arm();
                            }
                        }
                    }
                    if is_fod && contains_bytes(msg, b"fod finger down") {
                        if SCREEN_OFF.load(Ordering::SeqCst) {
                            DOWN.store(true, Ordering::SeqCst);
                            arm();
                            if !WAKE_PENDING.swap(true, Ordering::SeqCst) {
                                std::thread::spawn(|| {
                                    fod_wake::inject_wakeup();
                                    for _ in 0..40 {
                                        if !SCREEN_OFF.load(Ordering::SeqCst) {
                                            break;
                                        }
                                        std::thread::sleep(std::time::Duration::from_millis(10));
                                    }
                                    std::thread::sleep(std::time::Duration::from_millis(50));
                                    WAKE_PENDING.store(false, Ordering::SeqCst);
                                    if DOWN.load(Ordering::SeqCst) && !LIT.swap(true, Ordering::SeqCst) {
                                        hbm(true);
                                    }
                                });
                            }
                        } else if armed() {
                            DOWN.store(true, Ordering::SeqCst);
                            arm();
                            if !LIT.swap(true, Ordering::SeqCst) {
                                hbm(true);
                            }
                        }
                    } else if is_fod && contains_bytes(msg, b"fod finger up") {
                        handle_fod_up();
                    }
                }
                Err(e) if e.raw_os_error() == Some(32) => continue,
                Err(_) => break,
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}

fn fod_watch() {
    if !try_input_event_watch() {
        fod_watch_kmsg_fallback();
    }
}

extern "C" {
    fn lseek(fd: i32, off: i64, whence: i32) -> i64;
}
unsafe fn lseek_end(f: &std::fs::File) {
    use std::os::fd::AsRawFd;
    lseek(f.as_raw_fd(), 0, 2);
}

fn main() {
    logging::init();
    logging::init_rsbinder_logger();
    if let Err(e) = ProcessState::init_default() {
        logging::error(&format!("binder init failed: {e:?}"));
        std::process::exit(1);
    }
    std::thread::spawn(gfpatch::run);
    std::thread::spawn(fod_watch);
    ProcessState::start_thread_pool();
    let binder = BnDisplayPanelFeature::new_binder(Dpf);
    match hub::add_service(SERVICE_NAME, binder.as_binder()) {
        Ok(()) => logging::info(&format!("registered {SERVICE_NAME}")),
        Err(e) => {
            logging::error(&format!("register failed: {e:?}; continuing without the binder service"));
            loop {
                std::thread::sleep(std::time::Duration::from_secs(3600));
            }
        }
    }
    let _ = ProcessState::join_thread_pool();
}
