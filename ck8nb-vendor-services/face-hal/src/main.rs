//! Standalone 64-bit License-Free AIDL Face HAL for Tecno CK8nB (ColorOS 17 / Android 16+).
//! Registers `android.hardware.biometrics.face.IFace/default` (sensorId=1, halControlsPreview=true).
//! Opens the front camera directly via `libcamera2ndk_vendor.so` + `libmediandk.so`, attaches
//! ColorOS's `previewSurface` for live enrollment preview, and runs pure-Rust face recognition
//! with zero proprietary blobs and zero `license.lic` dependency.

#![allow(non_snake_case, dead_code, unused_imports)]

mod camera_ndk;
mod face_engine;

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

use camera_ndk::CameraSession;
use face_engine::{
    analyze_frame, cosine_similarity, has_enrollment, load_templates, match_threshold,
    remove_enrollment, save_templates, DetectResult, DESC_DIM, ENROLL_SAMPLES, FACE_ID_DEFAULT,
};
use rsbinder::{hub, BinderResult, Interface, ProcessState, Strong};
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const SERVICE_NAME: &str = "android.hardware.biometrics.face.IFace/default";
const ANDROID_LOG_INFO: libc::c_int = 4;
const ANDROID_LOG_ERROR: libc::c_int = 6;

#[link(name = "log")]
extern "C" {
    fn __android_log_write(
        prio: libc::c_int,
        tag: *const libc::c_char,
        text: *const libc::c_char,
    ) -> libc::c_int;
}

pub fn log_info(msg: &str) {
    if let Ok(c) = CString::new(msg) {
        unsafe {
            __android_log_write(
                ANDROID_LOG_INFO,
                b"Ck8nbFaceHal\0".as_ptr() as *const _,
                c.as_ptr(),
            );
        }
    }
}

pub fn log_err(msg: &str) {
    if let Ok(c) = CString::new(msg) {
        unsafe {
            __android_log_write(
                ANDROID_LOG_ERROR,
                b"Ck8nbFaceHal\0".as_ptr() as *const _,
                c.as_ptr(),
            );
        }
    }
}

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

fn make_auth_frame(info: AcquiredInfo) -> AuthenticationFrame {
    AuthenticationFrame {
        data: BaseFrame {
            acquiredInfo: info,
            vendorCode: 0,
            pan: 0.0,
            tilt: 0.0,
            distance: 0.0,
            isCancellable: true,
        },
    }
}

fn make_enroll_frame(info: AcquiredInfo) -> EnrollmentFrame {
    EnrollmentFrame {
        cell: None,
        stage: EnrollmentStage::GATHERING_ENROLLMENT_DATA,
        data: BaseFrame {
            acquiredInfo: info,
            vendorCode: 0,
            pan: 0.0,
            tilt: 0.0,
            distance: 0.0,
            isCancellable: true,
        },
    }
}

struct CancelSignal {
    stop: Arc<AtomicBool>,
}
impl Interface for CancelSignal {}
impl ICancellationSignal for CancelSignal {
    fn cancel(&self) -> BinderResult<()> {
        log_info("ICancellationSignal::cancel");
        self.stop.store(true, Ordering::SeqCst);
        Ok(())
    }
}

struct FaceSession {
    user_id: i32,
    cb: Strong<dyn ISessionCallback>,
    active_stop: Arc<Mutex<Option<Arc<AtomicBool>>>>,
}
impl Interface for FaceSession {}

impl FaceSession {
    fn cancel_previous(&self) -> Arc<AtomicBool> {
        let stop = Arc::new(AtomicBool::new(false));
        let mut guard = self.active_stop.lock().unwrap();
        if let Some(prev) = guard.replace(stop.clone()) {
            prev.store(true, Ordering::SeqCst);
        }
        stop
    }
}

impl ISession for FaceSession {
    fn generateChallenge(&self) -> BinderResult<()> {
        let challenge: i64 = 0x434b_386e_4200_0002;
        log_info(&format!("generateChallenge -> {challenge:#x}"));
        let _ = self.cb.onChallengeGenerated(challenge);
        Ok(())
    }

    fn revokeChallenge(&self, challenge: i64) -> BinderResult<()> {
        log_info(&format!("revokeChallenge {challenge:#x}"));
        let _ = self.cb.onChallengeRevoked(challenge);
        Ok(())
    }

    fn getEnrollmentConfig(
        &self,
        _enrollmentType: EnrollmentType,
    ) -> BinderResult<Vec<EnrollmentStageConfig>> {
        Ok(Vec::new())
    }

    fn enroll(
        &self,
        _hat: &HardwareAuthToken,
        _type: EnrollmentType,
        _features: &[Feature],
        previewSurface: Option<&NativeHandle>,
    ) -> BinderResult<Strong<dyn ICancellationSignal>> {
        log_info(&format!(
            "ISession::enroll user={} has_preview={}",
            self.user_id,
            previewSurface.is_some()
        ));
        let stop = self.cancel_previous();
        let stop_bg = stop.clone();
        let cb = self.cb.clone();
        let user_id = self.user_id;

        // Open the camera synchronously here so previewSurface NativeHandle is dup'd before return.
        let samples: Arc<Mutex<Vec<[f32; DESC_DIM]>>> = Arc::new(Mutex::new(Vec::new()));
        let last_sample = Arc::new(Mutex::new(Instant::now() - Duration::from_secs(1)));
        let last_hint = Arc::new(Mutex::new(Instant::now() - Duration::from_secs(1)));
        let done = Arc::new(AtomicBool::new(false));

        let samples_cb = samples.clone();
        let last_sample_cb = last_sample.clone();
        let last_hint_cb = last_hint.clone();
        let done_cb = done.clone();
        let stop_cb = stop.clone();
        let cb_frame = cb.clone();

        let cam_res = CameraSession::start(
            previewSurface,
            stop.clone(),
            Box::new(move |frame| {
                if done_cb.load(Ordering::Relaxed) || stop_cb.load(Ordering::Relaxed) {
                    return;
                }
                let now = Instant::now();
                let mut ls = last_sample_cb.lock().unwrap();
                if now.duration_since(*ls) < Duration::from_millis(140) {
                    return;
                }
                match analyze_frame(frame) {
                    DetectResult::Good(desc) => {
                        *ls = now;
                        drop(ls);
                        let mut vec = samples_cb.lock().unwrap();
                        if vec.len() < ENROLL_SAMPLES {
                            vec.push(desc);
                            let count = vec.len();
                            let remaining = (ENROLL_SAMPLES - count) as i32;
                            let _ = cb_frame.onEnrollmentFrame(&make_enroll_frame(AcquiredInfo::GOOD));
                            if remaining == 0 {
                                match save_templates(user_id, &vec) {
                                    Ok(()) => {
                                        log_info(&format!(
                                            "enrollment complete: saved {count} templates for user={user_id}"
                                        ));
                                        done_cb.store(true, Ordering::SeqCst);
                                        stop_cb.store(true, Ordering::SeqCst);
                                        let _ = cb_frame.onEnrollmentProgress(FACE_ID_DEFAULT, 0);
                                    }
                                    Err(e) => {
                                        log_err(&format!("save_templates failed: {e}"));
                                        done_cb.store(true, Ordering::SeqCst);
                                        stop_cb.store(true, Ordering::SeqCst);
                                        let _ = cb_frame.onError(FaceError::UNABLE_TO_PROCESS, 0);
                                    }
                                }
                            } else {
                                log_info(&format!("enrollment progress {count}/{ENROLL_SAMPLES}"));
                                let _ = cb_frame.onEnrollmentProgress(FACE_ID_DEFAULT, remaining);
                            }
                        }
                    }
                    other => {
                        drop(ls);
                        let mut lh = last_hint_cb.lock().unwrap();
                        if now.duration_since(*lh) >= Duration::from_millis(500) {
                            *lh = now;
                            let info = match other {
                                DetectResult::TooDark => AcquiredInfo::TOO_DARK,
                                DetectResult::TooBright => AcquiredInfo::TOO_BRIGHT,
                                DetectResult::TooSmall => AcquiredInfo::TOO_FAR,
                                _ => AcquiredInfo::NOT_DETECTED,
                            };
                            let _ = cb_frame.onEnrollmentFrame(&make_enroll_frame(info));
                        }
                    }
                }
            }),
        );

        match cam_res {
            Ok(cam) => {
                std::thread::spawn(move || {
                    let _cam = cam; // keep CameraSession alive until done/cancelled/timeout
                    let start = Instant::now();
                    while !stop_bg.load(Ordering::Relaxed)
                        && !done.load(Ordering::Relaxed)
                        && start.elapsed() < Duration::from_secs(25)
                    {
                        std::thread::sleep(Duration::from_millis(40));
                    }
                    let was_done = done.load(Ordering::SeqCst);
                    let was_cancelled = stop_bg.load(Ordering::SeqCst);
                    drop(_cam);
                    if !was_done {
                        if was_cancelled {
                            let _ = cb.onError(FaceError::CANCELED, 0);
                        } else {
                            log_info("enrollment timed out");
                            let _ = cb.onError(FaceError::TIMEOUT, 0);
                        }
                    }
                });
            }
            Err(e) => {
                log_err(&format!("enroll camera start failed: {e}"));
                let _ = self.cb.onError(FaceError::HW_UNAVAILABLE, 0);
            }
        }

        Ok(BnCancellationSignal::new_binder(CancelSignal { stop }))
    }

    fn authenticate(&self, operationId: i64) -> BinderResult<Strong<dyn ICancellationSignal>> {
        log_info(&format!("ISession::authenticate user={} op={operationId}", self.user_id));
        let stop = self.cancel_previous();
        let templates = load_templates(self.user_id);
        if templates.is_empty() {
            log_info("authenticate: no enrolled templates");
            let _ = self.cb.onError(FaceError::UNABLE_TO_PROCESS, 0);
            return Ok(BnCancellationSignal::new_binder(CancelSignal { stop }));
        }
        let threshold = match_threshold(self.user_id);
        let stop_bg = stop.clone();
        let cb = self.cb.clone();
        let matched = Arc::new(AtomicBool::new(false));
        let last_check = Arc::new(Mutex::new(Instant::now() - Duration::from_secs(1)));

        let matched_cb = matched.clone();
        let stop_cb = stop.clone();
        let cb_frame = cb.clone();
        let templates_arc = Arc::new(templates);

        let cam_res = CameraSession::start(
            None,
            stop.clone(),
            Box::new(move |frame| {
                if matched_cb.load(Ordering::Relaxed) || stop_cb.load(Ordering::Relaxed) {
                    return;
                }
                let now = Instant::now();
                let mut lc = last_check.lock().unwrap();
                if now.duration_since(*lc) < Duration::from_millis(50) {
                    return;
                }
                *lc = now;
                drop(lc);

                if let DetectResult::Good(probe) = analyze_frame(frame) {
                    let mut best = 0.0f32;
                    for t in templates_arc.iter() {
                        let s = cosine_similarity(&probe, t);
                        if s > best {
                            best = s;
                        }
                    }
                    log_info(&format!("auth sim={best:.3} (thr={threshold:.2})"));
                    if best >= threshold {
                        if !matched_cb.swap(true, Ordering::SeqCst) {
                            stop_cb.store(true, Ordering::SeqCst);
                            let _ = cb_frame.onAuthenticationSucceeded(FACE_ID_DEFAULT, &empty_hat());
                        }
                    }
                }
            }),
        );

        match cam_res {
            Ok(cam) => {
                std::thread::spawn(move || {
                    let _cam = cam;
                    let start = Instant::now();
                    while !stop_bg.load(Ordering::Relaxed)
                        && !matched.load(Ordering::Relaxed)
                        && start.elapsed() < Duration::from_secs(5)
                    {
                        std::thread::sleep(Duration::from_millis(30));
                    }
                    let ok = matched.load(Ordering::SeqCst);
                    let cancelled = stop_bg.load(Ordering::SeqCst);
                    drop(_cam);
                    if !ok {
                        if cancelled {
                            let _ = cb.onError(FaceError::CANCELED, 0);
                        } else {
                            log_info("authenticate: no match within timeout");
                            let _ = cb.onAuthenticationFailed();
                        }
                    }
                });
            }
            Err(e) => {
                log_err(&format!("auth camera start failed: {e}"));
                let _ = self.cb.onError(FaceError::HW_UNAVAILABLE, 0);
            }
        }

        Ok(BnCancellationSignal::new_binder(CancelSignal { stop }))
    }

    fn detectInteraction(&self) -> BinderResult<Strong<dyn ICancellationSignal>> {
        let stop = Arc::new(AtomicBool::new(true));
        let _ = self.cb.onError(FaceError::UNABLE_TO_PROCESS, 0);
        Ok(BnCancellationSignal::new_binder(CancelSignal { stop }))
    }

    fn enumerateEnrollments(&self) -> BinderResult<()> {
        let ids: Vec<i32> = if has_enrollment(self.user_id) {
            vec![FACE_ID_DEFAULT]
        } else {
            Vec::new()
        };
        log_info(&format!("ISession::enumerateEnrollments user={} -> {ids:?}", self.user_id));
        let _ = self.cb.onEnrollmentsEnumerated(&ids);
        Ok(())
    }

    fn removeEnrollments(&self, enrollmentIds: &[i32]) -> BinderResult<()> {
        log_info(&format!("ISession::removeEnrollments user={} ids={enrollmentIds:?}", self.user_id));
        remove_enrollment(self.user_id);
        let _ = self.cb.onEnrollmentsRemoved(enrollmentIds);
        Ok(())
    }

    fn getFeatures(&self) -> BinderResult<()> {
        let _ = self.cb.onFeaturesRetrieved(&[]);
        Ok(())
    }

    fn setFeature(
        &self,
        _hat: &HardwareAuthToken,
        feature: Feature,
        _enabled: bool,
    ) -> BinderResult<()> {
        let _ = self.cb.onFeatureSet(feature);
        Ok(())
    }

    fn getAuthenticatorId(&self) -> BinderResult<()> {
        let id = if has_enrollment(self.user_id) { 1 } else { 0 };
        let _ = self.cb.onAuthenticatorIdRetrieved(id);
        Ok(())
    }

    fn invalidateAuthenticatorId(&self) -> BinderResult<()> {
        let _ = self.cb.onAuthenticatorIdInvalidated(0);
        Ok(())
    }

    fn resetLockout(&self, _hat: &HardwareAuthToken) -> BinderResult<()> {
        let _ = self.cb.onLockoutCleared();
        Ok(())
    }

    fn close(&self) -> BinderResult<()> {
        log_info(&format!("ISession::close user={}", self.user_id));
        if let Some(prev) = self.active_stop.lock().unwrap().take() {
            prev.store(true, Ordering::SeqCst);
        }
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
                sensorId: 1,
                sensorStrength: SensorStrength::CONVENIENCE,
                maxEnrollmentsPerUser: 1,
                componentInfo: Vec::new(),
            },
            sensorType: FaceSensorType::RGB,
            halControlsPreview: true,
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
        let dir = format!("/data/vendor_de/{userId}/facedata");
        let _ = std::fs::create_dir_all(&dir);
        Ok(BnSession::new_binder(FaceSession {
            user_id: userId,
            cb: cb.clone(),
            active_stop: Arc::new(Mutex::new(None)),
        }))
    }
}

fn main() {
    log_info("Ck8nbFaceHal v2 starting (standalone license-free Camera2 NDK AIDL Face HAL)");
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
