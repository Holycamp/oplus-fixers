// IDisplayPanelFeature V1 HAL for Tecno CK8nB (Transsion panel, MTK).
// SystemUI / FingerprintService call get/setDisplayPanelFeatureValue with numeric feature ids
// during FOD enroll and unlock. The stock service answered them; with no service the calls fail
// and fingerprint enroll hangs. This one answers success with value 0 and logs each id
// (tag OplusDpfHal) so the ones FOD needs can be mapped from logcat.
//
// The panel HBM for FOD is already driven by the kernel (FTS finger-down -> panel_hbm_set_cmdq),
// so this HAL deliberately does not write brightness nodes by itself.
mod bindings;
mod gfpatch;
mod logging;

use rsbinder::{hub, BinderResult, Interface, ProcessState};

use bindings::{BnDisplayPanelFeature, IDisplayPanelFeature};

const HBM_NODE: &str = "/sys/kernel/tran_display/lcm_hbm_state";

/// id 22 is SystemUI's "FOD highlight" request (1 = on, 0 = off). Writing 1/0 to the panel node
/// was confirmed on device: the node accepts it and reads back "0xOK" (default "0xFA").
fn hbm(on: bool) {
    let v = if on { "1" } else { "0" };
    match std::fs::write(HBM_NODE, v) {
        Ok(_) => logging::info(&format!("hbm {v} written")),
        Err(e) => logging::error(&format!("hbm {v} write failed: {e}")),
    }
}

/// The panel must only light for a press while SystemUI is actually showing the FOD icon (it tells
/// the HAL with feature id 22 = 1 when it shows it, 0 when it hides it). Without this gate every
/// touch on that spot of the home screen or inside apps lit the panel.
const ARM_MS: u64 = 30_000;
/// True while the touch driver is in its screen-off gesture mode (screen off / AOD). SystemUI does
/// not send the FOD "shown" signal there, but the icon is usable, so presses must be accepted.
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
    // The ROM's framework declares `out int[]` (it sends only the array length), not the `inout`
    // of the LineageOS V1 snapshot, so the AIDL in this crate uses `out`.
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
        // id 22 (FOD highlight) is deliberately NOT written to the panel: SystemUI sends it when the
        // icon is shown, which made the panel flash before the finger was down. It only arms /
        // disarms the finger-down watcher (fod_watch), which drives the panel HBM.
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

/// Goodix libgf_hal.so waits for a kernel uevent carrying "TRAN_FULL_HBM_EVENT=FULL_HBM_SET" after it
/// turns the HBM on (strings from the library). This kernel switches the HBM but never announces it,
/// so the HAL broadcasts the same uevent itself (netlink kobject-uevent group 1, root only).
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
        std::thread::sleep(std::time::Duration::from_millis(80)); // let the panel reach full HBM before Goodix exposes
        send_full_hbm_uevent();
        std::thread::sleep(std::time::Duration::from_millis(200));
        send_full_hbm_uevent();
    });
}

// ---- FOD press -> HBM -------------------------------------------------------------------------
// SystemUI never asks for FOD highlight on this ROM (no featureId 22 call in the logs), so the
// HAL does it itself: the FTS driver logs "fod finger down"/"fod finger up" to the kernel log, and
// the panel HBM is switched on while the finger is on the sensor. Runs in its own thread.
static DOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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
        let mut lit = false;
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
                        // someone (Goodix itself, SystemUI or this HAL) switched the HBM on: announce it
                        announce_hbm();
                        // HBM requested with no finger on the sensor (e.g. at the start of enroll):
                        // the icon would glow before it is touched, so drop it again unless a
                        // finger-down record arrives right behind it.
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
                        // "core mode change <old> --> <new>": every mode except TRAN_ACTIVE_MODE
                        // (gesture suspend, AOD, ...) means the screen is off / dozing.
                        let text = String::from_utf8_lossy(msg);
                        if let Some(i) = text.find("--> ") {
                            let new_mode = text[i + 4..].trim().to_string();
                            let off = !new_mode.contains("TRAN_ACTIVE_MODE");
                            SCREEN_OFF.store(off, Ordering::SeqCst);
                            if !off {
                                // screen just came on: the lock screen (with the FOD icon) is up,
                                // even when SystemUI forgets to send the "FOD shown" signal
                                arm();
                            }
                            logging::info(&format!("touch mode -> {new_mode} (screen_off={off})"));
                        }
                    }
                    if has(b"fod finger down") {
                        if SCREEN_OFF.load(Ordering::SeqCst) {
                            // AOD/screen off: the panel only lights through a drawn spot (SystemUI);
                            // the display driver refuses the HBM node here, so do not touch anything.
                            logging::info("fod finger down while screen off: left to SystemUI");
                        } else if armed() {
                            DOWN.store(true, Ordering::SeqCst);
                            arm(); // keep the window open while the user keeps pressing (enroll)
                            if !lit {
                                hbm(true);
                                lit = true;
                            }
                        } else {
                            logging::info("fod finger down ignored (FOD not shown, screen on)");
                        }
                    } else if has(b"fod finger up") {
                        DOWN.store(false, Ordering::SeqCst);
                        if lit {
                            hbm(false);
                            lit = false;
                        }
                    }
                }
                Err(e) if e.raw_os_error() == Some(32) => continue, // EPIPE: records were overwritten
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
            // another service (Xia's adaptor) may already own the name: keep running for the FOD watcher
            logging::error(&format!("register failed: {e:?}; continuing without the binder service"));
            loop {
                std::thread::sleep(std::time::Duration::from_secs(3600));
            }
        }
    }
    let _ = ProcessState::join_thread_pool();
}
