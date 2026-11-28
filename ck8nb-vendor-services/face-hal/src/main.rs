//! Standalone 64-bit SenseTime AIDL Face HAL for Tecno CK8nB (ColorOS 17 / Android 16+).
//! Registers android.hardware.biometrics.face.IFace/default and delegates directly to
//! /vendor/lib64/hw/face.default.so + /vendor/lib64/libstfaceunlockppl.so via hw_get_module("face").

#![allow(non_snake_case, dead_code, unused_imports)]

mod face_ffi;

pub mod aidl {
    include!(concat!(env!("OUT_DIR"), "/face_aidl.rs"));
}

use aidl::android::hardware::biometrics::common::{
    CommonProps::CommonProps,
    ICancellationSignal::{BnCancellationSignal, ICancellationSignal},
    OperationContext::OperationContext,
    SensorStrength::SensorStrength,
};
use aidl::android::hardware::biometrics::face::{
    AcquiredInfo::AcquiredInfo,
    AuthenticationFrame::AuthenticationFrame,
    BaseFrame::BaseFrame,
    EnrollmentFrame::EnrollmentFrame,
    EnrollmentStage::EnrollmentStage,
    EnrollmentStageConfig::EnrollmentStageConfig,
    EnrollmentType::EnrollmentType,
    Error::Error as FaceError,
    FaceSensorType::FaceSensorType,
    Feature::Feature,
    IFace::{BnFace, IFace},
    ISession::{BnSession, ISession},
    ISessionCallback::ISessionCallback,
    SensorProps::SensorProps,
};
use aidl::android::hardware::common::NativeHandle::NativeHandle;
use aidl::android::hardware::keymaster::{
    HardwareAuthToken::HardwareAuthToken,
    HardwareAuthenticatorType::HardwareAuthenticatorType,
    Timestamp::Timestamp,
};

use face_ffi::{FaceDevice, FaceMsg, LegacyHwAuthToken};
use rsbinder::{hub, BinderResult, Interface, ProcessState, Strong};
use std::ffi::CString;
use std::sync::{Mutex, OnceLock};

const SERVICE_NAME: &str = "android.hardware.biometrics.face.IFace/default";
const ANDROID_LOG_INFO: libc::c_int = 4;
const ANDROID_LOG_ERROR: libc::c_int = 6;

#[link(name = "log")]
extern "C" {
    fn __android_log_write(prio: libc::c_int, tag: *const libc::c_char, text: *const libc::c_char) -> libc::c_int;
}

fn log_info(msg: &str) {
    if let Ok(c) = CString::new(msg) {
        unsafe {
            __android_log_write(ANDROID_LOG_INFO, b"Ck8nbFaceHal\0".as_ptr() as *const _, c.as_ptr());
        }
    }
}

fn log_err(msg: &str) {
    if let Ok(c) = CString::new(msg) {
        unsafe {
            __android_log_write(ANDROID_LOG_ERROR, b"Ck8nbFaceHal\0".as_ptr() as *const _, c.as_ptr());
        }
    }
}

struct SendDev(*mut FaceDevice);
unsafe impl Send for SendDev {}
unsafe impl Sync for SendDev {}

static DEVICE: OnceLock<Mutex<SendDev>> = OnceLock::new();
static ACTIVE_CB: Mutex<Option<Strong<dyn ISessionCallback>>> = Mutex::new(None);
static ENUM_IDS: Mutex<Vec<i32>> = Mutex::new(Vec::new());

fn empty_hat() -> HardwareAuthToken {
    HardwareAuthToken {
        challenge: 0,
        userId: 0,
        authenticatorId: 0,
        authenticatorType: HardwareAuthenticatorType::NONE,
        timestamp: Timestamp { milliSeconds: 0 },
        mac: Vec::new(),
    }
}

fn to_legacy_hat(hat: &HardwareAuthToken) -> LegacyHwAuthToken {
    let mut hmac = [0u8; 32];
    let n = hat.mac.len().min(32);
    hmac[..n].copy_from_slice(&hat.mac[..n]);
    LegacyHwAuthToken {
        version: 0,
        challenge: hat.challenge as u64,
        user_id: hat.userId as u64,
        authenticator_id: hat.authenticatorId as u64,
        authenticator_type: (hat.authenticatorType.0 as u32).to_be(),
        timestamp: (hat.timestamp.milliSeconds as u64).to_be(),
        hmac,
    }
}

unsafe extern "C" fn face_notify_cb(msg: *const FaceMsg) {
    if msg.is_null() {
        return;
    }
    let m = &*msg;
    let cb = ACTIVE_CB.lock().unwrap().clone();
    let Some(cb) = cb else {
        return;
    };
    match m.msg_type {
        1 => {
            // ERROR
            let err_code = m.data.error;
            let err = match err_code {
                1 => FaceError::HW_UNAVAILABLE,
                2 => FaceError::UNABLE_TO_PROCESS,
                3 => FaceError::TIMEOUT,
                4 => FaceError::NO_SPACE,
                5 => FaceError::CANCELED,
                6 => FaceError::UNABLE_TO_REMOVE,
                _ => FaceError::VENDOR,
            };
            log_info(&format!("notify ERROR code={err_code}"));
            let _ = cb.onError(err, err_code);
        }
        2 => {
            // ACQUIRED
            let acq = m.data.acquired;
            let frame = EnrollmentFrame {
                cell: None,
                stage: EnrollmentStage::GATHERING_ENROLLMENT_DATA,
                data: BaseFrame {
                    acquiredInfo: AcquiredInfo::GOOD,
                    vendorCode: acq.vendor_code,
                    pan: 0.0,
                    tilt: 0.0,
                    distance: 0.0,
                    isCancellable: true,
                },
            };
            let _ = cb.onEnrollmentFrame(&frame);
        }
        3 => {
            // TEMPLATE_ENROLLING
            let e = m.data.enroll;
            log_info(&format!("notify ENROLLING id={} remaining={}", e.face_id, e.samples_remaining));
            let _ = cb.onEnrollmentProgress(e.face_id as i32, e.samples_remaining as i32);
        }
        4 => {
            // TEMPLATE_REMOVED
            let r = m.data.removed;
            log_info(&format!("notify REMOVED id={} remaining={}", r.face_id, r.remaining));
            let _ = cb.onEnrollmentsRemoved(&[r.face_id as i32]);
        }
        5 => {
            // AUTHENTICATED
            let a = m.data.authenticated;
            log_info(&format!("notify AUTHENTICATED id={}", a.face_id));
            if a.face_id != 0 {
                let _ = cb.onAuthenticationSucceeded(a.face_id as i32, &empty_hat());
            } else {
                let _ = cb.onAuthenticationFailed();
            }
        }
        6 => {
            // TEMPLATE_ENUMERATING
            let en = m.data.enumerated;
            let mut ids = ENUM_IDS.lock().unwrap();
            if en.face_id != 0 {
                ids.push(en.face_id as i32);
            }
            if en.remaining == 0 {
                let out = std::mem::take(&mut *ids);
                drop(ids);
                log_info(&format!("notify ENUMERATED ids={out:?}"));
                let _ = cb.onEnrollmentsEnumerated(&out);
            }
        }
        other => {
            log_info(&format!("notify unknown msg_type={other}"));
        }
    }
}

struct CancelSignal;
impl Interface for CancelSignal {}
impl ICancellationSignal for CancelSignal {
    fn cancel(&self) -> BinderResult<()> {
        log_info("ICancellationSignal::cancel");
        if let Some(m) = DEVICE.get() {
            let guard = m.lock().unwrap();
            unsafe {
                if let Some(f) = (*guard.0).cancel {
                    f(guard.0);
                }
            }
        }
        Ok(())
    }
}

struct FaceSession {
    user_id: i32,
    cb: Strong<dyn ISessionCallback>,
}
impl Interface for FaceSession {}

impl ISession for FaceSession {
    fn generateChallenge(&self) -> BinderResult<()> {
        let mut challenge: u64 = 0;
        if let Some(m) = DEVICE.get() {
            let guard = m.lock().unwrap();
            unsafe {
                if let Some(f) = (*guard.0).pre_enroll {
                    f(guard.0, 600, &mut challenge);
                }
            }
        }
        if challenge == 0 {
            challenge = 0x434b_386e_4200_0001;
        }
        log_info(&format!("generateChallenge -> {challenge:#x}"));
        let _ = self.cb.onChallengeGenerated(challenge as i64);
        Ok(())
    }

    fn revokeChallenge(&self, challenge: i64) -> BinderResult<()> {
        if let Some(m) = DEVICE.get() {
            let guard = m.lock().unwrap();
            unsafe {
                if let Some(f) = (*guard.0).post_enroll {
                    f(guard.0);
                }
            }
        }
        log_info(&format!("revokeChallenge {challenge:#x}"));
        let _ = self.cb.onChallengeRevoked(challenge);
        Ok(())
    }

    fn getEnrollmentConfig(&self, _enrollmentType: EnrollmentType) -> BinderResult<Vec<EnrollmentStageConfig>> {
        Ok(Vec::new())
    }

    fn enroll(
        &self,
        hat: &HardwareAuthToken,
        _type: EnrollmentType,
        _features: &[Feature],
        _previewSurface: Option<&NativeHandle>,
    ) -> BinderResult<Strong<dyn ICancellationSignal>> {
        log_info("ISession::enroll");
        let lhat = to_legacy_hat(hat);
        if let Some(m) = DEVICE.get() {
            let guard = m.lock().unwrap();
            unsafe {
                if let Some(f) = (*guard.0).enroll {
                    let rc = f(guard.0, &lhat, 60, core::ptr::null(), 0);
                    log_info(&format!("face_enroll rc={rc}"));
                }
            }
        }
        Ok(BnCancellationSignal::new_binder(CancelSignal))
    }

    fn authenticate(&self, operationId: i64) -> BinderResult<Strong<dyn ICancellationSignal>> {
        log_info(&format!("ISession::authenticate op={operationId}"));
        if let Some(m) = DEVICE.get() {
            let guard = m.lock().unwrap();
            unsafe {
                if let Some(f) = (*guard.0).authenticate {
                    let rc = f(guard.0, operationId as u64);
                    log_info(&format!("face_authenticate rc={rc}"));
                }
            }
        }
        Ok(BnCancellationSignal::new_binder(CancelSignal))
    }

    fn detectInteraction(&self) -> BinderResult<Strong<dyn ICancellationSignal>> {
        let _ = self.cb.onError(FaceError::UNABLE_TO_PROCESS, 0);
        Ok(BnCancellationSignal::new_binder(CancelSignal))
    }

    fn enumerateEnrollments(&self) -> BinderResult<()> {
        log_info("ISession::enumerateEnrollments");
        ENUM_IDS.lock().unwrap().clear();
        let mut called = false;
        if let Some(m) = DEVICE.get() {
            let guard = m.lock().unwrap();
            unsafe {
                if let Some(f) = (*guard.0).enumerate {
                    f(guard.0);
                    called = true;
                }
            }
        }
        if !called {
            let _ = self.cb.onEnrollmentsEnumerated(&[]);
        }
        Ok(())
    }

    fn removeEnrollments(&self, enrollmentIds: &[i32]) -> BinderResult<()> {
        log_info(&format!("ISession::removeEnrollments {enrollmentIds:?}"));
        if let Some(m) = DEVICE.get() {
            let guard = m.lock().unwrap();
            unsafe {
                if let Some(f) = (*guard.0).remove {
                    for &id in enrollmentIds {
                        f(guard.0, id as u32);
                    }
                }
            }
        }
        let _ = self.cb.onEnrollmentsRemoved(enrollmentIds);
        Ok(())
    }

    fn getFeatures(&self) -> BinderResult<()> {
        let _ = self.cb.onFeaturesRetrieved(&[]);
        Ok(())
    }

    fn setFeature(&self, _hat: &HardwareAuthToken, feature: Feature, _enabled: bool) -> BinderResult<()> {
        let _ = self.cb.onFeatureSet(feature);
        Ok(())
    }

    fn getAuthenticatorId(&self) -> BinderResult<()> {
        let _ = self.cb.onAuthenticatorIdRetrieved(0);
        Ok(())
    }

    fn invalidateAuthenticatorId(&self) -> BinderResult<()> {
        let _ = self.cb.onAuthenticatorIdInvalidated(0);
        Ok(())
    }

    fn resetLockout(&self, hat: &HardwareAuthToken) -> BinderResult<()> {
        let lhat = to_legacy_hat(hat);
        if let Some(m) = DEVICE.get() {
            let guard = m.lock().unwrap();
            unsafe {
                if let Some(f) = (*guard.0).reset_lockout {
                    f(guard.0, &lhat);
                }
            }
        }
        let _ = self.cb.onLockoutCleared();
        Ok(())
    }

    fn close(&self) -> BinderResult<()> {
        log_info(&format!("ISession::close user={}", self.user_id));
        *ACTIVE_CB.lock().unwrap() = None;
        let _ = self.cb.onSessionClosed();
        Ok(())
    }

    fn authenticateWithContext(
        &self,
        operationId: i64,
        _context: &OperationContext,
    ) -> BinderResult<Strong<dyn ICancellationSignal>> {
        self.authenticate(operationId)
    }

    fn enrollWithContext(
        &self,
        hat: &HardwareAuthToken,
        r#type: EnrollmentType,
        features: &[Feature],
        previewSurface: Option<&NativeHandle>,
        _context: &OperationContext,
    ) -> BinderResult<Strong<dyn ICancellationSignal>> {
        self.enroll(hat, r#type, features, previewSurface)
    }

    fn detectInteractionWithContext(
        &self,
        _context: &OperationContext,
    ) -> BinderResult<Strong<dyn ICancellationSignal>> {
        self.detectInteraction()
    }

    fn onContextChanged(&self, _context: &OperationContext) -> BinderResult<()> {
        Ok(())
    }
}

struct FaceHal;
impl Interface for FaceHal {}

impl IFace for FaceHal {
    fn getSensorProps(&self) -> BinderResult<Vec<SensorProps>> {
        log_info("IFace::getSensorProps");
        Ok(vec![SensorProps {
            commonProps: CommonProps {
                sensorId: 0,
                sensorStrength: SensorStrength::CONVENIENCE,
                maxEnrollmentsPerUser: 1,
                componentInfo: Vec::new(),
            },
            sensorType: FaceSensorType::RGB,
            halControlsPreview: false,
            previewDisplayId: 0,
            enrollPreviewWidth: 640,
            enrollPreviewHeight: 480,
            enrollTranslationX: 0.0,
            enrollTranslationY: 0.0,
            enrollPreviewScale: 1.0,
            supportsDetectInteraction: false,
            supportsSelfIllumination: false,
            resetLockoutRequiresHardwareAuthToken: false,
            resetLockoutRequiresChallenge: false,
        }])
    }

    fn createSession(
        &self,
        sensorId: i32,
        userId: i32,
        cb: &Strong<dyn ISessionCallback>,
    ) -> BinderResult<Strong<dyn ISession>> {
        log_info(&format!("IFace::createSession sensor={sensorId} user={userId}"));
        *ACTIVE_CB.lock().unwrap() = Some(cb.clone());
        let dir = format!("/data/vendor_de/{userId}/facedata");
        let _ = std::fs::create_dir_all(&dir);
        if let Some(m) = DEVICE.get() {
            let guard = m.lock().unwrap();
            if let Ok(cdir) = CString::new(dir.as_str()) {
                unsafe {
                    if let Some(f) = (*guard.0).set_active_group {
                        let rc = f(guard.0, userId, cdir.as_ptr());
                        log_info(&format!("set_active_group({userId}, {dir}) rc={rc}"));
                    }
                }
            }
        }
        Ok(BnSession::new_binder(FaceSession {
            user_id: userId,
            cb: cb.clone(),
        }))
    }
}

fn main() {
    log_info("Ck8nbFaceHal starting (64-bit SenseTime face.default.so AIDL bridge)");
    unsafe {
        match face_ffi::open_face_device() {
            Ok(dev) => {
                if let Some(set_notify) = (*dev).set_notify {
                    let rc = set_notify(dev, face_notify_cb);
                    log_info(&format!("face_set_notify rc={rc}"));
                }
                let _ = DEVICE.set(Mutex::new(SendDev(dev)));
                log_info("opened /vendor/lib64/hw/face.default.so cleanly");
            }
            Err(e) => {
                log_err(&format!("open_face_device failed rc={e}"));
            }
        }
    }

    if let Err(e) = ProcessState::init_default() {
        log_err(&format!("binder init failed: {e:?}"));
        std::process::exit(1);
    }
    ProcessState::start_thread_pool();
    let binder = BnFace::new_binder(FaceHal);
    match hub::add_service(SERVICE_NAME, binder.as_binder()) {
        Ok(()) => log_info(&format!("registered {SERVICE_NAME}")),
        Err(e) => {
            log_err(&format!("register {SERVICE_NAME} failed: {e:?}"));
            std::process::exit(1);
        }
    }
    let _ = ProcessState::join_thread_pool();
}
