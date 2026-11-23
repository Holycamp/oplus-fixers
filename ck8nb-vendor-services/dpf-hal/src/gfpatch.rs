// Goodix libgf_hal.so (Transsion build) only accepts the "FULL_HBM_SET" uevent when it comes from
// the kernel (netlink sender pid == 0). Userspace cannot send as pid 0, so enrollment never starts.
// This module makes a patched copy of the library at runtime (the one "sender pid != 0 -> drop"
// branch is turned into a nop), bind-mounts it over the original and restarts the fingerprint HAL.
// Nothing on disk in /vendor is modified, and it is skipped on libraries that don't match.
use crate::logging;
use std::ffi::CString;

const LIBS: [&str; 2] = ["/vendor/lib64/libgf_hal.so", "/vendor/lib/libgf_hal.so"];
const FP_SERVICES: [&str; 3] = ["vendor.tran_fps_hal", "vendor.fps_hal_transsion", "vendor.fps_hal_rianixia"];

/// ldr w8,[sp,#0x80]; cmp w8,#1; b.ne ..; ldr w8,[sp,#0x7c]; cbnz w8,..   <- patch the cbnz
fn find_patch_offset(d: &[u8]) -> Option<usize> {
    const A: [u8; 4] = [0xe8, 0x83, 0x40, 0xb9];
    const B: [u8; 4] = [0x1f, 0x05, 0x00, 0x71];
    const C: [u8; 4] = [0xe8, 0x7f, 0x40, 0xb9];
    let mut found = None;
    let mut i = 0;
    while i + 20 <= d.len() {
        if d[i..i + 4] == A
            && d[i + 4..i + 8] == B
            && d[i + 11] == 0x54
            && d[i + 12..i + 16] == C
            && d[i + 19] == 0x35
            && d[i + 16] & 0x1f == 8
        {
            if found.is_some() {
                return None; // ambiguous
            }
            found = Some(i + 16);
        }
        i += 4;
    }
    found
}

fn already_mounted(path: &str) -> bool {
    std::fs::read_to_string("/proc/self/mountinfo")
        .map(|m| m.lines().any(|l| l.split(' ').nth(4) == Some(path)))
        .unwrap_or(false)
}

fn copy_selinux(src: &str, dst: &str) {
    let (s, d) = (CString::new(src).unwrap(), CString::new(dst).unwrap());
    let name = CString::new("security.selinux").unwrap();
    let mut buf = [0u8; 256];
    unsafe {
        let n = libc::getxattr(s.as_ptr(), name.as_ptr(), buf.as_mut_ptr() as *mut _, buf.len());
        if n > 0 {
            libc::setxattr(d.as_ptr(), name.as_ptr(), buf.as_ptr() as *const _, n as usize, 0);
        }
    }
}

fn patch_one(lib: &str) -> bool {
    if !std::path::Path::new(lib).exists() || already_mounted(lib) {
        return false;
    }
    let mut data = match std::fs::read(lib) {
        Ok(d) => d,
        Err(e) => {
            logging::error(&format!("gfpatch: read {lib}: {e}"));
            return false;
        }
    };
    let off = match find_patch_offset(&data) {
        Some(o) => o,
        None => {
            logging::info(&format!("gfpatch: {lib}: pattern not found (already patched or different build), skipped"));
            return false;
        }
    };
    data[off..off + 4].copy_from_slice(&[0x1f, 0x20, 0x03, 0xd5]); // nop
    let dir = "/dev/ck8nb_gf";
    let _ = std::fs::create_dir_all(dir);
    let tmp = format!("{dir}/{}", lib.replace('/', "_"));
    if let Err(e) = std::fs::write(&tmp, &data) {
        logging::error(&format!("gfpatch: write {tmp}: {e}"));
        return false;
    }
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644));
    copy_selinux(lib, &tmp);
    let (s, t) = (CString::new(tmp.clone()).unwrap(), CString::new(lib).unwrap());
    let rc = unsafe { libc::mount(s.as_ptr(), t.as_ptr(), std::ptr::null(), libc::MS_BIND, std::ptr::null()) };
    if rc != 0 {
        logging::error(&format!("gfpatch: bind mount over {lib} failed: {}", std::io::Error::last_os_error()));
        return false;
    }
    logging::info(&format!("gfpatch: patched {lib} at 0x{off:x} and bind-mounted"));
    true
}

pub fn run() {
    // vendor is mounted long before late_start, but give init a moment on slow boots
    let mut any = false;
    for lib in LIBS {
        any |= patch_one(lib);
    }
    if any {
        for s in FP_SERVICES {
            let _ = std::process::Command::new("/system/bin/setprop")
                .args(["ctl.restart", s])
                .status();
        }
        logging::info("gfpatch: fingerprint HAL restart requested");
    }
}
