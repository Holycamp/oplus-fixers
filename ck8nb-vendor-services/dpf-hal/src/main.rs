// IDisplayPanelFeature V1 HAL for Tecno CK8nB (Transsion panel, MTK).
// Preserves full rsbinder bindings, gfpatch (in-memory libgf_hal.so patch), and screen-on FOD gate,
// plus adds pure-HAL screen-off/AOD FOD wake via /dev/uinput KEY_WAKEUP (fod_wake.rs).
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

const ARM_MS: u64 = 30_000;
static SCREEN_OFF: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
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
    now_ms() < ARMED_UNTIL.load(std::sync::atomic::Ordering::SeqCst)
}

const SERVICE_NAME: &str = "vendor.oplus.hardware.displaypanelfeature.IDisplayPanelFeature/default";

struct Dpf;
impl Interface for Dpf {}

impl IDisplayPanelFeature for Dpf {
    fn getDisplayPanelFeatureValue(&self, id: i32, values: &mut Vec<i32>) -> BinderResult<i32> {
        logging::info(&format!("get id={id} len={}", values.len()));
        if values.is_empty() {
            values.push(0);
        }
        for v in values.iter_mut() {
            *v = 0;
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
        send_full_hbm_uevent();
        std::thread::sleep(std::time::Duration::from_millis(200));
        send_full_hbm_uevent();
    });
}

static DOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static LIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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
        logging::info("fod watch running (kmsg)");
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
                    let has = |needle: &[u8]| msg.windows(needle.len()).any(|w| w == needle);
                    if has(b"tran_lcm_hbm_store value= 1") {
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
                    if has(b"core mode change") {
                        let text = String::from_utf8_lossy(msg);
                        if let Some(i) = text.find("--> ") {
                            let new_mode = text[i + 4..].trim().to_string();
                            let off = !new_mode.contains("TRAN_ACTIVE_MODE");
                            SCREEN_OFF.store(off, Ordering::SeqCst);
                            if !off {
                                arm();
                            }
                            logging::info(&format!("touch mode -> {new_mode} (screen_off={off})"));
                        }
                    }
                    if has(b"fod finger down") {
                        if SCREEN_OFF.load(Ordering::SeqCst) {
                            // Pure-HAL SOFOD: wake panel out of DOZE via /dev/uinput KEY_WAKEUP,
                            // then write lcm_hbm_state=1 once nt37703 enters active state.
                            DOWN.store(true, Ordering::SeqCst);
                            arm();
                            logging::info("fod finger down while screen off -> waking panel via uinput");
                            std::thread::spawn(|| {
                                fod_wake::wake_panel_for_sofod();
                                if DOWN.load(Ordering::SeqCst) {
                                    hbm(true);
                                    LIT.store(true, Ordering::SeqCst);
                                }
                            });
                        } else if armed() {
                            DOWN.store(true, Ordering::SeqCst);
                            arm();
                            if !LIT.swap(true, Ordering::SeqCst) {
                                hbm(true);
                            }
                        } else {
                            logging::info("fod finger down ignored (FOD not shown, screen on)");
                        }
                    } else if has(b"fod finger up") {
                        DOWN.store(false, Ordering::SeqCst);
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
