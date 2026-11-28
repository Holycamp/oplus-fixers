//! Dynamic FFI wrapper around `/vendor/lib64/libcamera2ndk_vendor.so` and `libmediandk.so`.
//! Opens the front-facing camera directly, streams 640x480 YUV_420_888 frames to CPU via
//! AImageReader, and optionally attaches ColorOS's `previewSurface` (`native_handle_t`)
//! directly as a hardware-accelerated camera output target.

use core::ffi::{c_char, c_int, c_void};
use std::ffi::{CStr, CString};
use std::os::fd::AsRawFd;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::aidl::android::hardware::common::NativeHandle::NativeHandle;
use crate::{log_err, log_info};

const AIMAGE_FORMAT_YUV_420_888: i32 = 0x23;
const AHARDWAREBUFFER_USAGE_CPU_READ_OFTEN: u64 = 3;
const TEMPLATE_PREVIEW: i32 = 1;
const ACAMERA_LENS_FACING: u32 = 0x0008_0005;
const ACAMERA_LENS_FACING_FRONT: u8 = 0;
const ACAMERA_SENSOR_ORIENTATION: u32 = 0x000e_0001;

#[repr(C)]
pub struct ACameraManager {
    _private: [u8; 0],
}
#[repr(C)]
pub struct ACameraDevice {
    _private: [u8; 0],
}
#[repr(C)]
pub struct ACameraMetadata {
    _private: [u8; 0],
}
#[repr(C)]
pub struct ACaptureRequest {
    _private: [u8; 0],
}
#[repr(C)]
pub struct ACameraOutputTarget {
    _private: [u8; 0],
}
#[repr(C)]
pub struct ACaptureSessionOutput {
    _private: [u8; 0],
}
#[repr(C)]
pub struct ACaptureSessionOutputContainer {
    _private: [u8; 0],
}
#[repr(C)]
pub struct ACameraCaptureSession {
    _private: [u8; 0],
}
#[repr(C)]
pub struct AImageReader {
    _private: [u8; 0],
}
#[repr(C)]
pub struct AImage {
    _private: [u8; 0],
}

#[repr(C)]
struct ACameraIdList {
    num_cameras: c_int,
    camera_ids: *mut *const c_char,
}

#[repr(C)]
union ACameraMetadataData {
    u8_ptr: *const u8,
    i32_ptr: *const i32,
    raw: *const c_void,
}

#[repr(C)]
struct ACameraMetadataConstEntry {
    tag: u32,
    entry_type: u8,
    count: u32,
    data: ACameraMetadataData,
}

#[repr(C)]
struct ACameraDeviceStateCallbacks {
    context: *mut c_void,
    on_disconnected: Option<unsafe extern "C" fn(*mut c_void, *mut ACameraDevice)>,
    on_error: Option<unsafe extern "C" fn(*mut c_void, *mut ACameraDevice, c_int)>,
}

#[repr(C)]
struct ACameraCaptureSessionStateCallbacks {
    context: *mut c_void,
    on_closed: Option<unsafe extern "C" fn(*mut c_void, *mut ACameraCaptureSession)>,
    on_ready: Option<unsafe extern "C" fn(*mut c_void, *mut ACameraCaptureSession)>,
    on_active: Option<unsafe extern "C" fn(*mut c_void, *mut ACameraCaptureSession)>,
}

#[repr(C)]
struct AImageReaderImageListener {
    context: *mut c_void,
    on_image_available: Option<unsafe extern "C" fn(*mut c_void, *mut AImageReader)>,
}

/// CPU-accessible YUV_420_888 frame snapshot delivered by AImageReader.
pub struct YuvFrame<'a> {
    pub width: usize,
    pub height: usize,
    pub sensor_orientation: i32,
    pub y: &'a [u8],
    pub y_row_stride: usize,
    pub u: &'a [u8],
    pub u_row_stride: usize,
    pub u_pixel_stride: usize,
    pub v: &'a [u8],
    pub v_row_stride: usize,
    pub v_pixel_stride: usize,
}

/// Owns a C-compatible `native_handle_t` built from an AIDL `NativeHandle` with dup'd FDs.
struct NativeHandleBuf {
    words: Vec<i32>,
    num_fds: usize,
}

impl NativeHandleBuf {
    fn from_aidl(nh: &NativeHandle) -> Option<Self> {
        let num_fds = nh.fds.len();
        let num_ints = nh.ints.len();
        if num_fds == 0 && num_ints == 0 {
            return None;
        }
        let mut words = Vec::with_capacity(3 + num_fds + num_ints);
        // sizeof(native_handle_t) = 3 * sizeof(int) = 12
        words.push(12);
        words.push(num_fds as i32);
        words.push(num_ints as i32);
        let mut duped = 0usize;
        for pfd in &nh.fds {
            let raw = pfd.as_raw_fd();
            let d = unsafe { libc::dup(raw) };
            if d < 0 {
                // Close already dup'd fds before aborting
                for i in 0..duped {
                    unsafe { libc::close(words[3 + i]) };
                }
                return None;
            }
            words.push(d);
            duped += 1;
        }
        for &v in &nh.ints {
            words.push(v);
        }
        Some(Self { words, num_fds })
    }

    fn as_ptr(&self) -> *const c_void {
        self.words.as_ptr() as *const c_void
    }
}

impl Drop for NativeHandleBuf {
    fn drop(&mut self) {
        for i in 0..self.num_fds {
            let fd = self.words[3 + i];
            if fd >= 0 {
                unsafe { libc::close(fd) };
            }
        }
    }
}

#[derive(Copy, Clone)]
struct CamApi {
    mgr_create: unsafe extern "C" fn() -> *mut ACameraManager,
    mgr_delete: unsafe extern "C" fn(*mut ACameraManager),
    mgr_get_id_list: unsafe extern "C" fn(*mut ACameraManager, *mut *mut ACameraIdList) -> i32,
    mgr_del_id_list: unsafe extern "C" fn(*mut ACameraIdList),
    mgr_get_chars: unsafe extern "C" fn(*mut ACameraManager, *const c_char, *mut *mut ACameraMetadata) -> i32,
    meta_get_entry: unsafe extern "C" fn(*const ACameraMetadata, u32, *mut ACameraMetadataConstEntry) -> i32,
    meta_free: unsafe extern "C" fn(*mut ACameraMetadata),
    mgr_open_camera: unsafe extern "C" fn(
        *mut ACameraManager,
        *const c_char,
        *mut ACameraDeviceStateCallbacks,
        *mut *mut ACameraDevice,
    ) -> i32,
    dev_close: unsafe extern "C" fn(*mut ACameraDevice) -> i32,
    dev_create_req: unsafe extern "C" fn(*const ACameraDevice, i32, *mut *mut ACaptureRequest) -> i32,
    req_free: unsafe extern "C" fn(*mut ACaptureRequest),
    req_add_target: unsafe extern "C" fn(*mut ACaptureRequest, *const ACameraOutputTarget) -> i32,
    target_create: unsafe extern "C" fn(*const c_void, *mut *mut ACameraOutputTarget) -> i32,
    target_free: unsafe extern "C" fn(*mut ACameraOutputTarget),
    out_container_create: unsafe extern "C" fn(*mut *mut ACaptureSessionOutputContainer) -> i32,
    out_container_free: unsafe extern "C" fn(*mut ACaptureSessionOutputContainer),
    out_container_add: unsafe extern "C" fn(*mut ACaptureSessionOutputContainer, *const ACaptureSessionOutput) -> i32,
    out_create: unsafe extern "C" fn(*const c_void, *mut *mut ACaptureSessionOutput) -> i32,
    out_free: unsafe extern "C" fn(*mut ACaptureSessionOutput),
    dev_create_session: Option<
        unsafe extern "C" fn(
            *mut ACameraDevice,
            *const ACaptureSessionOutputContainer,
            *const ACameraCaptureSessionStateCallbacks,
            *mut *mut ACameraCaptureSession,
        ) -> i32,
    >,
    dev_create_session_params: Option<
        unsafe extern "C" fn(
            *mut ACameraDevice,
            *const ACaptureSessionOutputContainer,
            *const ACaptureRequest,
            *const ACameraCaptureSessionStateCallbacks,
            *mut *mut ACameraCaptureSession,
        ) -> i32,
    >,
    sess_set_repeating: unsafe extern "C" fn(
        *mut ACameraCaptureSession,
        *mut c_void,
        i32,
        *mut *mut ACaptureRequest,
        *mut i32,
    ) -> i32,
    sess_stop_repeating: unsafe extern "C" fn(*mut ACameraCaptureSession) -> i32,
    sess_close: unsafe extern "C" fn(*mut ACameraCaptureSession),

    // libmediandk.so
    ir_new_with_usage: unsafe extern "C" fn(i32, i32, i32, u64, i32, *mut *mut AImageReader) -> i32,
    ir_get_window_nh: unsafe extern "C" fn(*mut AImageReader, *mut *const c_void) -> i32,
    ir_set_listener: unsafe extern "C" fn(*mut AImageReader, *mut AImageReaderImageListener) -> i32,
    ir_acquire_latest: unsafe extern "C" fn(*mut AImageReader, *mut *mut AImage) -> i32,
    ir_delete: unsafe extern "C" fn(*mut AImageReader),
    img_get_width: unsafe extern "C" fn(*const AImage, *mut i32) -> i32,
    img_get_height: unsafe extern "C" fn(*const AImage, *mut i32) -> i32,
    img_get_plane_data: unsafe extern "C" fn(*const AImage, i32, *mut *mut u8, *mut i32) -> i32,
    img_get_row_stride: unsafe extern "C" fn(*const AImage, i32, *mut i32) -> i32,
    img_get_pixel_stride: unsafe extern "C" fn(*const AImage, i32, *mut i32) -> i32,
    img_delete: unsafe extern "C" fn(*mut AImage),
}

unsafe fn load_sym<T>(lib: *mut c_void, name: &[u8]) -> Result<T, String> {
    let p = libc::dlsym(lib, name.as_ptr() as *const c_char);
    if p.is_null() {
        let s = std::str::from_utf8(&name[..name.len() - 1]).unwrap_or("?");
        return Err(format!("missing symbol {s}"));
    }
    Ok(core::mem::transmute_copy(&p))
}

unsafe fn load_opt_sym<T>(lib: *mut c_void, name: &[u8]) -> Option<T> {
    let p = libc::dlsym(lib, name.as_ptr() as *const c_char);
    if p.is_null() {
        None
    } else {
        Some(core::mem::transmute_copy(&p))
    }
}

impl CamApi {
    fn load() -> Result<Self, String> {
        unsafe {
            let cam = libc::dlopen(b"libcamera2ndk_vendor.so\0".as_ptr() as *const c_char, libc::RTLD_NOW);
            if cam.is_null() {
                return Err("dlopen(libcamera2ndk_vendor.so) failed".into());
            }
            let media = libc::dlopen(b"libmediandk.so\0".as_ptr() as *const c_char, libc::RTLD_NOW);
            if media.is_null() {
                return Err("dlopen(libmediandk.so) failed".into());
            }
            let dev_create_session = load_opt_sym(cam, b"ACameraDevice_createCaptureSession\0");
            let dev_create_session_params =
                load_opt_sym(cam, b"ACameraDevice_createCaptureSessionWithSessionParameters\0");
            if dev_create_session.is_none() && dev_create_session_params.is_none() {
                return Err("missing ACameraDevice_createCaptureSession*".into());
            }
            Ok(Self {
                mgr_create: load_sym(cam, b"ACameraManager_create\0")?,
                mgr_delete: load_sym(cam, b"ACameraManager_delete\0")?,
                mgr_get_id_list: load_sym(cam, b"ACameraManager_getCameraIdList\0")?,
                mgr_del_id_list: load_sym(cam, b"ACameraManager_deleteCameraIdList\0")?,
                mgr_get_chars: load_sym(cam, b"ACameraManager_getCameraCharacteristics\0")?,
                meta_get_entry: load_sym(cam, b"ACameraMetadata_getConstEntry\0")?,
                meta_free: load_sym(cam, b"ACameraMetadata_free\0")?,
                mgr_open_camera: load_sym(cam, b"ACameraManager_openCamera\0")?,
                dev_close: load_sym(cam, b"ACameraDevice_close\0")?,
                dev_create_req: load_sym(cam, b"ACameraDevice_createCaptureRequest\0")?,
                req_free: load_sym(cam, b"ACaptureRequest_free\0")?,
                req_add_target: load_sym(cam, b"ACaptureRequest_addTarget\0")?,
                target_create: load_sym(cam, b"ACameraOutputTarget_create\0")?,
                target_free: load_sym(cam, b"ACameraOutputTarget_free\0")?,
                out_container_create: load_sym(cam, b"ACaptureSessionOutputContainer_create\0")?,
                out_container_free: load_sym(cam, b"ACaptureSessionOutputContainer_free\0")?,
                out_container_add: load_sym(cam, b"ACaptureSessionOutputContainer_add\0")?,
                out_create: load_sym(cam, b"ACaptureSessionOutput_create\0")?,
                out_free: load_sym(cam, b"ACaptureSessionOutput_free\0")?,
                dev_create_session,
                dev_create_session_params,
                sess_set_repeating: load_sym(cam, b"ACameraCaptureSession_setRepeatingRequest\0")?,
                sess_stop_repeating: load_sym(cam, b"ACameraCaptureSession_stopRepeating\0")?,
                sess_close: load_sym(cam, b"ACameraCaptureSession_close\0")?,
                ir_new_with_usage: load_sym(media, b"AImageReader_newWithUsage\0")?,
                ir_get_window_nh: load_sym(media, b"AImageReader_getWindowNativeHandle\0")?,
                ir_set_listener: load_sym(media, b"AImageReader_setImageListener\0")?,
                ir_acquire_latest: load_sym(media, b"AImageReader_acquireLatestImage\0")?,
                ir_delete: load_sym(media, b"AImageReader_delete\0")?,
                img_get_width: load_sym(media, b"AImage_getWidth\0")?,
                img_get_height: load_sym(media, b"AImage_getHeight\0")?,
                img_get_plane_data: load_sym(media, b"AImage_getPlaneData\0")?,
                img_get_row_stride: load_sym(media, b"AImage_getPlaneRowStride\0")?,
                img_get_pixel_stride: load_sym(media, b"AImage_getPlanePixelStride\0")?,
                img_delete: load_sym(media, b"AImage_delete\0")?,
            })
        }
    }
}

struct ListenerCtx {
    api: CamApi,
    sensor_orientation: i32,
    stop: Arc<AtomicBool>,
    callback: Box<dyn FnMut(&YuvFrame) + Send>,
}

unsafe extern "C" fn on_device_disconnected(_ctx: *mut c_void, _dev: *mut ACameraDevice) {
    log_err("camera device disconnected");
}

unsafe extern "C" fn on_device_error(_ctx: *mut c_void, _dev: *mut ACameraDevice, err: c_int) {
    log_err(&format!("camera device error={err}"));
}

unsafe extern "C" fn on_session_closed(_ctx: *mut c_void, _s: *mut ACameraCaptureSession) {}
unsafe extern "C" fn on_session_ready(_ctx: *mut c_void, _s: *mut ACameraCaptureSession) {}
unsafe extern "C" fn on_session_active(_ctx: *mut c_void, _s: *mut ACameraCaptureSession) {}

unsafe extern "C" fn on_image_available(ctx: *mut c_void, reader: *mut AImageReader) {
    if ctx.is_null() || reader.is_null() {
        return;
    }
    let st = &mut *(ctx as *mut ListenerCtx);
    let mut img: *mut AImage = ptr::null_mut();
    let rc = (st.api.ir_acquire_latest)(reader, &mut img);
    if rc != 0 || img.is_null() {
        return;
    }
    if !st.stop.load(Ordering::Relaxed) {
        let mut w: i32 = 0;
        let mut h: i32 = 0;
        (st.api.img_get_width)(img, &mut w);
        (st.api.img_get_height)(img, &mut h);
        let mut y_ptr: *mut u8 = ptr::null_mut();
        let mut y_len: i32 = 0;
        let mut y_stride: i32 = 0;
        let mut u_ptr: *mut u8 = ptr::null_mut();
        let mut u_len: i32 = 0;
        let mut u_stride: i32 = 0;
        let mut u_pstride: i32 = 1;
        let mut v_ptr: *mut u8 = ptr::null_mut();
        let mut v_len: i32 = 0;
        let mut v_stride: i32 = 0;
        let mut v_pstride: i32 = 1;

        let ok_y = (st.api.img_get_plane_data)(img, 0, &mut y_ptr, &mut y_len) == 0
            && (st.api.img_get_row_stride)(img, 0, &mut y_stride) == 0;
        let ok_u = (st.api.img_get_plane_data)(img, 1, &mut u_ptr, &mut u_len) == 0
            && (st.api.img_get_row_stride)(img, 1, &mut u_stride) == 0
            && (st.api.img_get_pixel_stride)(img, 1, &mut u_pstride) == 0;
        let ok_v = (st.api.img_get_plane_data)(img, 2, &mut v_ptr, &mut v_len) == 0
            && (st.api.img_get_row_stride)(img, 2, &mut v_stride) == 0
            && (st.api.img_get_pixel_stride)(img, 2, &mut v_pstride) == 0;

        if ok_y
            && ok_u
            && ok_v
            && w > 0
            && h > 0
            && !y_ptr.is_null()
            && !u_ptr.is_null()
            && !v_ptr.is_null()
            && y_len > 0
            && u_len > 0
            && v_len > 0
        {
            let frame = YuvFrame {
                width: w as usize,
                height: h as usize,
                sensor_orientation: st.sensor_orientation,
                y: core::slice::from_raw_parts(y_ptr, y_len as usize),
                y_row_stride: y_stride as usize,
                u: core::slice::from_raw_parts(u_ptr, u_len as usize),
                u_row_stride: u_stride as usize,
                u_pixel_stride: u_pstride.max(1) as usize,
                v: core::slice::from_raw_parts(v_ptr, v_len as usize),
                v_row_stride: v_stride as usize,
                v_pixel_stride: v_pstride.max(1) as usize,
            };
            (st.callback)(&frame);
        }
    }
    (st.api.img_delete)(img);
}

unsafe fn find_front_camera(api: &CamApi, mgr: *mut ACameraManager) -> (String, i32) {
    let mut id_list: *mut ACameraIdList = ptr::null_mut();
    if (api.mgr_get_id_list)(mgr, &mut id_list) == 0 && !id_list.is_null() {
        let count = (*id_list).num_cameras.max(0) as usize;
        let ids = core::slice::from_raw_parts((*id_list).camera_ids, count);
        let mut found: Option<(String, i32)> = None;
        for &cid_ptr in ids {
            if cid_ptr.is_null() {
                continue;
            }
            let mut meta: *mut ACameraMetadata = ptr::null_mut();
            if (api.mgr_get_chars)(mgr, cid_ptr, &mut meta) == 0 && !meta.is_null() {
                let mut facing = ACameraMetadataConstEntry {
                    tag: 0,
                    entry_type: 0,
                    count: 0,
                    data: ACameraMetadataData { raw: ptr::null() },
                };
                let is_front = (api.meta_get_entry)(meta, ACAMERA_LENS_FACING, &mut facing) == 0
                    && facing.count > 0
                    && !facing.data.u8_ptr.is_null()
                    && *facing.data.u8_ptr == ACAMERA_LENS_FACING_FRONT;
                let mut orient_val = 270;
                let mut orient = ACameraMetadataConstEntry {
                    tag: 0,
                    entry_type: 0,
                    count: 0,
                    data: ACameraMetadataData { raw: ptr::null() },
                };
                if (api.meta_get_entry)(meta, ACAMERA_SENSOR_ORIENTATION, &mut orient) == 0
                    && orient.count > 0
                    && !orient.data.i32_ptr.is_null()
                {
                    orient_val = *orient.data.i32_ptr;
                }
                (api.meta_free)(meta);
                if is_front {
                    let s = CStr::from_ptr(cid_ptr).to_string_lossy().into_owned();
                    found = Some((s, orient_val));
                    break;
                }
            }
        }
        (api.mgr_del_id_list)(id_list);
        if let Some(f) = found {
            return f;
        }
    }
    ("1".to_string(), 270)
}

/// Active front-camera capture session. Dropping stops repeating and releases all resources.
pub struct CameraSession {
    api: CamApi,
    mgr: *mut ACameraManager,
    dev: *mut ACameraDevice,
    session: *mut ACameraCaptureSession,
    req: *mut ACaptureRequest,
    container: *mut ACaptureSessionOutputContainer,
    ir_out: *mut ACaptureSessionOutput,
    ir_target: *mut ACameraOutputTarget,
    pv_out: *mut ACaptureSessionOutput,
    pv_target: *mut ACameraOutputTarget,
    reader: *mut AImageReader,
    listener_ctx: *mut ListenerCtx,
    _preview_buf: Option<NativeHandleBuf>,
    stop: Arc<AtomicBool>,
}

unsafe impl Send for CameraSession {}

impl CameraSession {
    pub fn start(
        preview_nh: Option<&NativeHandle>,
        stop: Arc<AtomicBool>,
        callback: Box<dyn FnMut(&YuvFrame) + Send>,
    ) -> Result<Self, String> {
        let api = CamApi::load()?;
        unsafe {
            let mgr = (api.mgr_create)();
            if mgr.is_null() {
                return Err("ACameraManager_create returned null".into());
            }
            let (cam_id, sensor_orientation) = find_front_camera(&api, mgr);
            log_info(&format!("opening front camera id={cam_id} orientation={sensor_orientation}"));

            let c_id = CString::new(cam_id.as_str()).map_err(|_| "invalid camera id")?;
            let mut dev_cbs = ACameraDeviceStateCallbacks {
                context: ptr::null_mut(),
                on_disconnected: Some(on_device_disconnected),
                on_error: Some(on_device_error),
            };
            let mut dev: *mut ACameraDevice = ptr::null_mut();
            let rc = (api.mgr_open_camera)(mgr, c_id.as_ptr(), &mut dev_cbs, &mut dev);
            if rc != 0 || dev.is_null() {
                (api.mgr_delete)(mgr);
                return Err(format!("ACameraManager_openCamera({cam_id}) failed rc={rc}"));
            }

            let mut reader: *mut AImageReader = ptr::null_mut();
            let rc = (api.ir_new_with_usage)(
                640,
                480,
                AIMAGE_FORMAT_YUV_420_888,
                AHARDWAREBUFFER_USAGE_CPU_READ_OFTEN,
                4,
                &mut reader,
            );
            if rc != 0 || reader.is_null() {
                (api.dev_close)(dev);
                (api.mgr_delete)(mgr);
                return Err(format!("AImageReader_newWithUsage failed rc={rc}"));
            }

            let listener_ctx = Box::into_raw(Box::new(ListenerCtx {
                api,
                sensor_orientation,
                stop: stop.clone(),
                callback,
            }));
            let mut listener = AImageReaderImageListener {
                context: listener_ctx as *mut c_void,
                on_image_available: Some(on_image_available),
            };
            (api.ir_set_listener)(reader, &mut listener);

            let mut ir_nh: *const c_void = ptr::null();
            let rc = (api.ir_get_window_nh)(reader, &mut ir_nh);
            if rc != 0 || ir_nh.is_null() {
                (api.ir_delete)(reader);
                drop(Box::from_raw(listener_ctx));
                (api.dev_close)(dev);
                (api.mgr_delete)(mgr);
                return Err(format!("AImageReader_getWindowNativeHandle failed rc={rc}"));
            }

            let mut container: *mut ACaptureSessionOutputContainer = ptr::null_mut();
            (api.out_container_create)(&mut container);

            let mut ir_out: *mut ACaptureSessionOutput = ptr::null_mut();
            let mut ir_target: *mut ACameraOutputTarget = ptr::null_mut();
            let rc_o = (api.out_create)(ir_nh, &mut ir_out);
            let rc_t = (api.target_create)(ir_nh, &mut ir_target);
            if rc_o != 0 || rc_t != 0 || ir_out.is_null() || ir_target.is_null() {
                if !ir_out.is_null() {
                    (api.out_free)(ir_out);
                }
                if !ir_target.is_null() {
                    (api.target_free)(ir_target);
                }
                (api.out_container_free)(container);
                (api.ir_delete)(reader);
                drop(Box::from_raw(listener_ctx));
                (api.dev_close)(dev);
                (api.mgr_delete)(mgr);
                return Err(format!("AImageReader output setup failed rc_o={rc_o} rc_t={rc_t}"));
            }
            (api.out_container_add)(container, ir_out);

            let mut req: *mut ACaptureRequest = ptr::null_mut();
            let rc = (api.dev_create_req)(dev, TEMPLATE_PREVIEW, &mut req);
            if rc != 0 || req.is_null() {
                (api.out_free)(ir_out);
                (api.target_free)(ir_target);
                (api.out_container_free)(container);
                (api.ir_delete)(reader);
                drop(Box::from_raw(listener_ctx));
                (api.dev_close)(dev);
                (api.mgr_delete)(mgr);
                return Err(format!("ACameraDevice_createCaptureRequest failed rc={rc}"));
            }
            (api.req_add_target)(req, ir_target);

            // Attach ColorOS's SurfaceView NativeHandle for live hardware preview during enrollment.
            let preview_buf = preview_nh.and_then(NativeHandleBuf::from_aidl);
            let mut pv_out: *mut ACaptureSessionOutput = ptr::null_mut();
            let mut pv_target: *mut ACameraOutputTarget = ptr::null_mut();
            if let Some(ref buf) = preview_buf {
                let r1 = (api.out_create)(buf.as_ptr(), &mut pv_out);
                let r2 = (api.target_create)(buf.as_ptr(), &mut pv_target);
                if r1 == 0 && r2 == 0 && !pv_out.is_null() && !pv_target.is_null() {
                    (api.out_container_add)(container, pv_out);
                    (api.req_add_target)(req, pv_target);
                    log_info("attached ColorOS previewSurface to camera capture session");
                } else {
                    log_err(&format!(
                        "previewSurface attach failed (r1={r1}, r2={r2}); continuing with AImageReader only"
                    ));
                    if !pv_out.is_null() {
                        (api.out_free)(pv_out);
                        pv_out = ptr::null_mut();
                    }
                    if !pv_target.is_null() {
                        (api.target_free)(pv_target);
                        pv_target = ptr::null_mut();
                    }
                }
            }

            let sess_cbs = ACameraCaptureSessionStateCallbacks {
                context: ptr::null_mut(),
                on_closed: Some(on_session_closed),
                on_ready: Some(on_session_ready),
                on_active: Some(on_session_active),
            };
            let mut session: *mut ACameraCaptureSession = ptr::null_mut();
            let rc = if let Some(f) = api.dev_create_session {
                f(dev, container, &sess_cbs, &mut session)
            } else if let Some(f) = api.dev_create_session_params {
                f(dev, container, ptr::null(), &sess_cbs, &mut session)
            } else {
                -1
            };
            if rc != 0 || session.is_null() {
                (api.req_free)(req);
                if !pv_out.is_null() {
                    (api.out_free)(pv_out);
                }
                if !pv_target.is_null() {
                    (api.target_free)(pv_target);
                }
                (api.out_free)(ir_out);
                (api.target_free)(ir_target);
                (api.out_container_free)(container);
                (api.ir_delete)(reader);
                drop(Box::from_raw(listener_ctx));
                (api.dev_close)(dev);
                (api.mgr_delete)(mgr);
                return Err(format!("ACameraDevice_createCaptureSession failed rc={rc}"));
            }

            let mut req_ptr = req;
            let mut seq_id: i32 = 0;
            let rc = (api.sess_set_repeating)(session, ptr::null_mut(), 1, &mut req_ptr, &mut seq_id);
            if rc != 0 {
                log_err(&format!("ACameraCaptureSession_setRepeatingRequest rc={rc}"));
            } else {
                log_info("front camera repeating preview started");
            }

            Ok(Self {
                api,
                mgr,
                dev,
                session,
                req,
                container,
                ir_out,
                ir_target,
                pv_out,
                pv_target,
                reader,
                listener_ctx,
                _preview_buf: preview_buf,
                stop,
            })
        }
    }
}

impl Drop for CameraSession {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        unsafe {
            if !self.session.is_null() {
                (self.api.sess_stop_repeating)(self.session);
                (self.api.sess_close)(self.session);
            }
            if !self.dev.is_null() {
                (self.api.dev_close)(self.dev);
            }
            if !self.req.is_null() {
                (self.api.req_free)(self.req);
            }
            if !self.pv_out.is_null() {
                (self.api.out_free)(self.pv_out);
            }
            if !self.pv_target.is_null() {
                (self.api.target_free)(self.pv_target);
            }
            if !self.ir_out.is_null() {
                (self.api.out_free)(self.ir_out);
            }
            if !self.ir_target.is_null() {
                (self.api.target_free)(self.ir_target);
            }
            if !self.container.is_null() {
                (self.api.out_container_free)(self.container);
            }
            if !self.reader.is_null() {
                let mut empty = AImageReaderImageListener {
                    context: ptr::null_mut(),
                    on_image_available: None,
                };
                (self.api.ir_set_listener)(self.reader, &mut empty);
                (self.api.ir_delete)(self.reader);
            }
            if !self.listener_ctx.is_null() {
                drop(Box::from_raw(self.listener_ctx));
            }
            if !self.mgr.is_null() {
                (self.api.mgr_delete)(self.mgr);
            }
        }
        log_info("front camera session closed");
    }
}
