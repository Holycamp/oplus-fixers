// IDisplayPanelFeature V1 HAL for Tecno CK8nB (Transsion NT37703 DSI video-mode panel, MTK).
// Preserves full rsbinder bindings, gfpatch (in-memory libgf_hal.so patch), Panoramic/Full-Screen AOD
// (ID 217=15, ID 193=1), and ultra-fast screen-on + screen-off/AOD FOD timing:
//   - Pre-creates /dev/uinput wakeup device at HAL boot (0ms first-tap penalty).
//   - Triggers HBM immediately on kernel `panel_doze_disable end` / `lcm_setbacklight_cmdq level =`
//     instead of waiting for touch TRAN_ACTIVE_MODE + 20ms (~45-50ms faster screen-off FOD).
//   - Tightens `FULL_HBM_SET` uevent to 10ms (1+ frame at 120Hz) with 45ms backup retry.
//   - Fast 12ms HBM retry if `skip set hbm node` is hit 1 frame early (with uinput wake fallback).
mod bindings;
mod fod_wake;
mod gfpatch;
mod logging;

use rsbinder::{hub, BinderResult, Interface, ProcessState};
use bindings::{BnDisplayPanelFeature, IDisplayPanelFeature};

const HBM_NODE: &str = "/sys/kernel/tran_display/lcm_hbm_state";

fn hbm(on: bool) {
    let v = if on { "1" } else { "0" };
    match std::fs::write(HBM_NODE, v) {
        Ok(_) => logging::info(&format!("hbm {v} written")),
        Err(e) => logging::error(&format!("hbm {v} write failed: {e}")),
    }
}

const ARM_MS: u64 = 60_000;
static SCREEN_OFF: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static FOD_SHOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static ARMED_UNTIL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn now_ms() -> u64 {
    static BASE: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    BASE.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64 + 1
}
fn arm() {
    ARMED_UNTIL.store(now_ms() + ARM_MS, std::sync::atomic::Ordering::SeqCst);
}
fn disarm() {
    ARMED_UNTIL.store(0, std::sync::atomic::Ordering::SeqCst);
}
fn armed() -> bool {
    FOD_SHOWN.load(std::sync::atomic::Ordering::SeqCst)
        || now_ms() < ARMED_UNTIL.load(std::sync::atomic::Ordering::SeqCst)
}

const SERVICE_NAME: &str = "vendor.oplus.hardware.displaypanelfeature.IDisplayPanelFeature/default";

struct Dpf;
impl Interface for Dpf {}

impl IDisplayPanelFeature for Dpf {
    fn getDisplayPanelFeatureValue(&self, id: i32, values: &mut Vec<i32>) -> BinderResult<i32> {
        let reply = match id {
            // GetDisplayLongruiAOD: 15 (0b1111) = Panoramic/Full-Screen + All-Day + Scheduled + Keep-on-entry.
            217 => 15,
            // GetDisplayPanelVideo: 1 = DSI video-mode panel (nt37703_fhdp_dsi_vdo).
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
        let v0 = values.first().copied().unwrap_or(0);
        match id {
            // 22 = DimlayHBM, 23 = DimlayerBLEN: ColorOS FOD overlay shown/hidden
            22 | 23 => {
                if v0 != 0 {
                    FOD_SHOWN.store(true, std::sync::atomic::Ordering::SeqCst);
                    arm();
                    logging::info(&format!("fod armed (id={id} on)"));
                } else {
                    FOD_SHOWN.store(false, std::sync::atomic::Ordering::SeqCst);
                    disarm();
                    logging::info(&format!("fod disarmed (id={id} off)"));
                }
            }
            // 28 = FPPress: direct FOD finger press/release hint from ColorOS
            28 => {
                if v0 != 0 {
                    FOD_SHOWN.store(true, std::sync::atomic::Ordering::SeqCst);
                    arm();
                }
            }
            _ => {}
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
        // 10ms (>1 frame at 120Hz) after synchronous DSI HBM cmdq is enough for peak brightness
        std::thread::sleep(std::time::Duration::from_millis(10));
        if DOWN.load(std::sync::atomic::Ordering::SeqCst) {
            send_full_hbm_uevent();
        }
        // Backup pulse if Goodix wasn't ready on the first frame
        std::thread::sleep(std::time::Duration::from_millis(45));
        if DOWN.load(std::sync::atomic::Ordering::SeqCst) {
            send_full_hbm_uevent();
        }
    });
}

static DOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static LIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static WAKE_PENDING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RETRY_PENDING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static LAST_WAKE_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[inline]
fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn trigger_hbm_now() {
    use std::sync::atomic::Ordering;
    if DOWN.load(Ordering::SeqCst) && !LIT.swap(true, Ordering::SeqCst) {
        hbm(true);
    }
}

fn start_panel_wake(reason: &'static str) {
    use std::sync::atomic::Ordering;
    if !WAKE_PENDING.swap(true, Ordering::SeqCst) {
        LAST_WAKE_MS.store(now_ms(), Ordering::SeqCst);
        logging::info(reason);
        std::thread::spawn(|| {
            fod_wake::inject_wakeup();
            // Safety fallback if neither panel_doze_disable end nor TRAN_ACTIVE_MODE fires
            std::thread::sleep(std::time::Duration::from_millis(180));
            if WAKE_PENDING.swap(false, Ordering::SeqCst) {
                trigger_hbm_now();
            }
        });
    }
}

fn fod_watch() {
    use std::sync::atomic::Ordering;
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
        logging::info("fod watch running (kmsg, ultra-fast)");
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
                    // Fast reject: ignore the high-rate gesture_data_write flood and unrelated lines.
                    if msg.len() < 12 || contains_bytes(msg, b"gesture_data_write") {
                        continue;
                    }
                    let is_hbm = contains_bytes(msg, b"hbm");
                    let is_fod = contains_bytes(msg, b"fod finger ");
                    let is_mode = contains_bytes(msg, b"core mode change");
                    let is_panel_ready = contains_bytes(msg, b"panel_doze_disable end")
                        || contains_bytes(msg, b"lcm_setbacklight_cmdq level =");
                    if !is_hbm && !is_fod && !is_mode && !is_panel_ready {
                        continue;
                    }

                    // Instant HBM as soon as panel exits DOZE or sets active backlight during a wake.
                    if is_panel_ready && WAKE_PENDING.swap(false, Ordering::SeqCst) {
                        logging::info("panel active -> firing immediate HBM");
                        std::thread::spawn(|| {
                            std::thread::sleep(std::time::Duration::from_millis(4));
                            trigger_hbm_now();
                        });
                    }

                    if is_hbm && contains_bytes(msg, b"skip set hbm node") {
                        LIT.store(false, Ordering::SeqCst);
                        if DOWN.load(Ordering::SeqCst) {
                            let since_wake = now_ms().saturating_sub(LAST_WAKE_MS.load(Ordering::SeqCst));
                            if since_wake > 250 && !WAKE_PENDING.load(Ordering::SeqCst) {
                                start_panel_wake("hbm skipped by sleeping/AOD panel -> waking panel via uinput");
                            } else if !WAKE_PENDING.load(Ordering::SeqCst)
                                && !RETRY_PENDING.swap(true, Ordering::SeqCst)
                            {
                                std::thread::spawn(|| {
                                    std::thread::sleep(std::time::Duration::from_millis(12));
                                    RETRY_PENDING.store(false, Ordering::SeqCst);
                                    trigger_hbm_now();
                                });
                            }
                        }
                    } else if is_hbm && contains_bytes(msg, b"tran_lcm_hbm_store value= 1") {
                        announce_hbm();
                        if !DOWN.load(Ordering::SeqCst) {
                            std::thread::spawn(|| {
                                std::thread::sleep(std::time::Duration::from_millis(20));
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
                                // Fallback: if panel_doze_disable end wasn't logged, fire HBM right on TRAN_ACTIVE_MODE
                                if WAKE_PENDING.swap(false, Ordering::SeqCst) {
                                    std::thread::spawn(|| {
                                        std::thread::sleep(std::time::Duration::from_millis(4));
                                        trigger_hbm_now();
                                    });
                                }
                            }
                            logging::info(&format!("touch mode -> {new_mode} (screen_off={off})"));
                        }
                    }

                    if is_fod && contains_bytes(msg, b"fod finger down") {
                        if SCREEN_OFF.load(Ordering::SeqCst) {
                            DOWN.store(true, Ordering::SeqCst);
                            arm();
                            start_panel_wake("fod finger down while screen off -> waking panel via uinput");
                        } else if WAKE_PENDING.load(Ordering::SeqCst) {
                            DOWN.store(true, Ordering::SeqCst);
                        } else if armed() {
                            DOWN.store(true, Ordering::SeqCst);
                            arm();
                            trigger_hbm_now();
                        } else {
                            logging::info("fod finger down ignored (FOD not shown, screen on)");
                        }
                    } else if is_fod && contains_bytes(msg, b"fod finger up") {
                        DOWN.store(false, Ordering::SeqCst);
                        WAKE_PENDING.store(false, Ordering::SeqCst);
                        if LIT.swap(false, Ordering::SeqCst) {
                            hbm(false);
                        }
                    }
                }
                Err(e) if e.raw_os_error() == Some(32) => continue,
                Err(e) => {
                    logging::error(&format!("fod watch: kmsg read error: {e}"));
                    break;
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
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
    std::thread::spawn(fod_wake::init);
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
