//! Pure-HAL Screen-Off / AOD FOD Wake Helper via `/dev/uinput` (`KEY_WAKEUP`).
//! Drop this file into: ck8nb-vendor-services/dpf-hal/src/fod_wake.rs
//!
//! Because the CK8nB `nt37703` display driver rejects `lcm_hbm_state = 1` while
//! the panel is in `DOZE` (`display state is slept or idle`), this helper wakes
//! the panel into `ON` via `KEY_WAKEUP` (143) before lighting HBM—no SystemUI
//! APK patch required.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const SYN_REPORT: u16 = 0x00;
const KEY_WAKEUP: u16 = 143;

// Linux uinput ioctl numbers (aarch64)
const UI_SET_EVBIT: libc::c_int = 0x40045564;
const UI_SET_KEYBIT: libc::c_int = 0x40045565;
const UI_DEV_CREATE: libc::c_int = 0x5501;

#[repr(C)]
struct InputEvent {
    tv_sec: libc::c_long,
    tv_usec: libc::c_long,
    ev_type: u16,
    code: u16,
    value: i32,
}

#[repr(C)]
struct UinputUserDev {
    name: [u8; 80],
    id_bustype: u16,
    id_vendor: u16,
    id_product: u16,
    id_version: u16,
    ff_effects_max: u32,
    absmax: [i32; 64],
    absmin: [i32; 64],
    absfuzz: [i32; 64],
    absflat: [i32; 64],
}

static UINPUT_FD: Mutex<Option<std::fs::File>> = Mutex::new(None);

fn ensure_uinput_device(guard: &mut Option<std::fs::File>) -> bool {
    if guard.is_some() {
        return true;
    }
    let Ok(mut f) = OpenOptions::new().write(true).open("/dev/uinput") else {
        return false;
    };
    let fd = f.as_raw_fd();
    unsafe {
        libc::ioctl(fd, UI_SET_EVBIT, EV_KEY as libc::c_int);
        libc::ioctl(fd, UI_SET_KEYBIT, KEY_WAKEUP as libc::c_int);
    }

    let mut dev = UinputUserDev {
        name: [0u8; 80],
        id_bustype: 0x06, // BUS_VIRTUAL
        id_vendor: 0x18d1,
        id_product: 0x4ee7,
        id_version: 1,
        ff_effects_max: 0,
        absmax: [0; 64],
        absmin: [0; 64],
        absfuzz: [0; 64],
        absflat: [0; 64],
    };
    let label = b"ck8nb-fod-wake";
    dev.name[..label.len()].copy_from_slice(label);

    let bytes = unsafe {
        std::slice::from_raw_parts(
            (&dev as *const UinputUserDev) as *const u8,
            std::mem::size_of::<UinputUserDev>(),
        )
    };
    if f.write_all(bytes).is_err() {
        return false;
    }
    unsafe {
        if libc::ioctl(fd, UI_DEV_CREATE) < 0 {
            return false;
        }
    }
    thread::sleep(Duration::from_millis(50));
    *guard = Some(f);
    true
}

fn emit_event(f: &mut std::fs::File, ev_type: u16, code: u16, value: i32) {
    let ev = InputEvent {
        tv_sec: 0,
        tv_usec: 0,
        ev_type,
        code,
        value,
    };
    let bytes = unsafe {
        std::slice::from_raw_parts(
            (&ev as *const InputEvent) as *const u8,
            std::mem::size_of::<InputEvent>(),
        )
    };
    let _ = f.write_all(bytes);
}

/// Wakes the display out of DOZE/sleep so `nt37703` accepts `lcm_hbm_state = 1`.
/// Returns `true` if `KEY_WAKEUP` was injected and the panel reached active state.
pub fn wake_panel_for_sofod() -> bool {
    let mut guard = match UINPUT_FD.lock() {
        Ok(g) => g,
        Err(_) => return false,
    };
    if !ensure_uinput_device(&mut guard) {
        return false;
    }
    if let Some(ref mut f) = *guard {
        emit_event(f, EV_KEY, KEY_WAKEUP, 1);
        emit_event(f, EV_SYN, SYN_REPORT, 0);
        emit_event(f, EV_KEY, KEY_WAKEUP, 0);
        emit_event(f, EV_SYN, SYN_REPORT, 0);
    }
    drop(guard);

    // Wait up to 200ms for backlight/panel to leave DOZE into ON state
    for _ in 0..10 {
        thread::sleep(Duration::from_millis(20));
        if let Ok(b) = fs::read_to_string("/sys/class/leds/lcd-backlight/brightness") {
            if b.trim().parse::<i32>().unwrap_or(0) > 0 {
                thread::sleep(Duration::from_millis(40));
                return true;
            }
        }
    }
    true
}
