//! FFI bridge to Tecno CK8nB's 64-bit /vendor/lib64/hw/face.default.so (SenseTime libstfaceunlockppl.so).
//! Struct offsets verified directly from 64-bit face.default.so face_open() @ 0x9658.

use core::ffi::{c_char, c_int, c_void};

#[repr(C)]
pub struct HwModuleMethods {
    pub open: Option<
        unsafe extern "C" fn(
            module: *const HwModule,
            id: *const c_char,
            device: *mut *mut HwDevice,
        ) -> c_int,
    >,
}

#[repr(C)]
pub struct HwModule {
    pub tag: u32,
    pub module_api_version: u16,
    pub hal_api_version: u16,
    pub id: *const c_char,
    pub name: *const c_char,
    pub author: *const c_char,
    pub methods: *const HwModuleMethods,
    pub dso: *mut c_void,
    pub reserved: [u64; 32 - 7],
}

#[repr(C)]
pub struct HwDevice {
    pub tag: u32,
    pub version: u32,
    pub module: *mut HwModule,
    pub reserved: [u64; 12],
    pub close: Option<unsafe extern "C" fn(device: *mut HwDevice) -> c_int>,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct LegacyHwAuthToken {
    pub version: u8,
    pub challenge: u64,
    pub user_id: u64,
    pub authenticator_id: u64,
    pub authenticator_type: u32,
    pub timestamp: u64,
    pub hmac: [u8; 32],
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct FaceEnrollMsg {
    pub face_id: u32,
    pub gid: u32,
    pub samples_remaining: u32,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct FaceAuthMsg {
    pub face_id: u32,
    pub gid: u32,
    pub hat: LegacyHwAuthToken,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct FaceAcquiredMsg {
    pub acquired_info: i32,
    pub vendor_code: i32,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct FaceRemovedMsg {
    pub face_id: u32,
    pub gid: u32,
    pub remaining: u32,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union FaceMsgData {
    pub error: i32,
    pub acquired: FaceAcquiredMsg,
    pub enroll: FaceEnrollMsg,
    pub removed: FaceRemovedMsg,
    pub enumerated: FaceRemovedMsg,
    pub authenticated: FaceAuthMsg,
    pub raw: [u8; 96],
}

#[repr(C)]
pub struct FaceMsg {
    pub msg_type: i32, // 1=ERROR, 2=ACQUIRED, 3=ENROLLING, 4=REMOVED, 5=AUTHENTICATED, 6=ENUMERATING
    pub _pad: i32,
    pub data: FaceMsgData,
}

pub type FaceNotifyFn = unsafe extern "C" fn(msg: *const FaceMsg);

/// Exact 64-bit face_device_t layout verified from /vendor/lib64/hw/face.default.so face_open():
///   0x00..0x78: HwDevice common (close @ 0x70)
///   0x78: _pad
///   0x80: notify
///   0x88: set_notify
///   0x90: pre_enroll
///   0x98: enroll
///   0xa0: post_enroll
///   0xa8: set_feature
///   0xb0: get_feature
///   0xb8: get_authenticator_id
///   0xc0: cancel
///   0xc8: enumerate
///   0xd0: remove
///   0xd8: set_active_group
///   0xe0: authenticate
///   0xe8: user_activity
///   0xf0: reset_lockout
#[repr(C)]
pub struct FaceDevice {
    pub common: HwDevice,                                                                 // 0x00..0x78
    pub _pad_78: u64,                                                                     // 0x78
    pub notify: Option<FaceNotifyFn>,                                                     // 0x80
    pub set_notify: Option<unsafe extern "C" fn(*mut FaceDevice, FaceNotifyFn) -> c_int>, // 0x88
    pub pre_enroll: Option<unsafe extern "C" fn(*mut FaceDevice, u32, *mut u64) -> c_int>,// 0x90
    pub enroll: Option<
        unsafe extern "C" fn(*mut FaceDevice, *const LegacyHwAuthToken, u32, *const u32, u64) -> c_int,
    >,                                                                                    // 0x98
    pub post_enroll: Option<unsafe extern "C" fn(*mut FaceDevice) -> c_int>,              // 0xa0
    pub set_feature: Option<
        unsafe extern "C" fn(*mut FaceDevice, u32, bool, *const LegacyHwAuthToken, u32) -> c_int,
    >,                                                                                    // 0xa8
    pub get_feature: Option<unsafe extern "C" fn(*mut FaceDevice, u32, u32, *mut bool) -> c_int>, // 0xb0
    pub get_authenticator_id: Option<unsafe extern "C" fn(*mut FaceDevice, *mut u64) -> c_int>,   // 0xb8
    pub cancel: Option<unsafe extern "C" fn(*mut FaceDevice) -> c_int>,                   // 0xc0
    pub enumerate: Option<unsafe extern "C" fn(*mut FaceDevice) -> c_int>,                // 0xc8
    pub remove: Option<unsafe extern "C" fn(*mut FaceDevice, u32) -> c_int>,              // 0xd0
    pub set_active_group: Option<unsafe extern "C" fn(*mut FaceDevice, i32, *const c_char) -> c_int>, // 0xd8
    pub authenticate: Option<unsafe extern "C" fn(*mut FaceDevice, u64) -> c_int>,        // 0xe0
    pub user_activity: Option<unsafe extern "C" fn(*mut FaceDevice) -> c_int>,            // 0xe8
    pub reset_lockout: Option<unsafe extern "C" fn(*mut FaceDevice, *const LegacyHwAuthToken) -> c_int>, // 0xf0
}

type HwGetModuleFn =
    unsafe extern "C" fn(id: *const c_char, module: *mut *const HwModule) -> c_int;

pub unsafe fn open_face_device() -> Result<*mut FaceDevice, c_int> {
    let lib = libc::dlopen(b"libhardware.so\0".as_ptr() as *const c_char, libc::RTLD_NOW);
    if lib.is_null() {
        return Err(-10);
    }
    let sym = libc::dlsym(lib, b"hw_get_module\0".as_ptr() as *const c_char);
    if sym.is_null() {
        return Err(-11);
    }
    let hw_get_module: HwGetModuleFn = core::mem::transmute(sym);
    let mut module: *const HwModule = core::ptr::null();
    let rc = hw_get_module(b"face\0".as_ptr() as *const c_char, &mut module);
    if rc != 0 || module.is_null() {
        return Err(if rc != 0 { rc } else { -1 });
    }
    let methods = (*module).methods;
    if methods.is_null() {
        return Err(-2);
    }
    let Some(open_fn) = (*methods).open else {
        return Err(-3);
    };
    let mut dev: *mut HwDevice = core::ptr::null_mut();
    let rc = open_fn(module, b"face\0".as_ptr() as *const c_char, &mut dev);
    if rc != 0 || dev.is_null() {
        return Err(if rc != 0 { rc } else { -4 });
    }
    Ok(dev as *mut FaceDevice)
}
