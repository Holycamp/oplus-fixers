// Stub ISession: answers every call with a valid callback so the framework never
// hangs, and reports UNABLE_TO_PROCESS for enroll/authenticate.
#![allow(non_snake_case, dead_code)]

use rsbinder::{Interface, Strong};

use crate::bindings::android::hardware::biometrics::common::ICancellationSignal::{
    BnCancellationSignal, ICancellationSignal,
};
use crate::bindings::android::hardware::biometrics::common::OperationContext::OperationContext;
use crate::bindings::android::hardware::biometrics::face::EnrollmentStageConfig::EnrollmentStageConfig;
use crate::bindings::android::hardware::biometrics::face::EnrollmentType::EnrollmentType;
use crate::bindings::android::hardware::biometrics::face::Error::Error as FaceError;
use crate::bindings::android::hardware::biometrics::face::Feature::Feature;
use crate::bindings::android::hardware::biometrics::face::ISession::ISession;
use crate::bindings::android::hardware::biometrics::face::ISessionCallback::ISessionCallback;
use crate::bindings::android::hardware::common::NativeHandle::NativeHandle;
use crate::bindings::android::hardware::keymaster::HardwareAuthToken::HardwareAuthToken;
use crate::logging;

pub struct NoopCancel;
impl Interface for NoopCancel {}
impl ICancellationSignal for NoopCancel {
    fn cancel(&self) -> rsbinder::status::Result<()> {
        Ok(())
    }
}

pub struct FaceSession {
    callback: Strong<dyn ISessionCallback>,
}

// The callback binder is safe to share across binder threads.
unsafe impl Send for FaceSession {}
unsafe impl Sync for FaceSession {}

impl FaceSession {
    pub fn new(_sensor_id: i32, _user_id: i32, callback: Strong<dyn ISessionCallback>) -> Self {
        Self { callback }
    }

    fn fail(&self) -> rsbinder::status::Result<Strong<dyn ICancellationSignal>> {
        let _ = self.callback.onError(FaceError::UNABLE_TO_PROCESS, 0);
        Ok(BnCancellationSignal::new_binder(NoopCancel))
    }
}

impl Interface for FaceSession {}

impl ISession for FaceSession {
    fn generateChallenge(&self) -> rsbinder::status::Result<()> {
        self.callback.onChallengeGenerated(0x5FACE000)?;
        Ok(())
    }

    fn revokeChallenge(&self, challenge: i64) -> rsbinder::status::Result<()> {
        self.callback.onChallengeRevoked(challenge)?;
        Ok(())
    }

    fn getEnrollmentConfig(&self, _type: EnrollmentType) -> rsbinder::status::Result<Vec<EnrollmentStageConfig>> {
        Ok(vec![])
    }

    fn enroll(
        &self,
        _hat: &HardwareAuthToken,
        _type: i8,
        _features: &[u8],
        _preview_surface: Option<&NativeHandle>,
    ) -> rsbinder::status::Result<Strong<dyn ICancellationSignal>> {
        logging::info("stub: enroll -> UNABLE_TO_PROCESS");
        self.fail()
    }

    fn authenticate(&self, _operation_id: i64) -> rsbinder::status::Result<Strong<dyn ICancellationSignal>> {
        logging::info("stub: authenticate -> UNABLE_TO_PROCESS");
        self.fail()
    }

    fn detectInteraction(&self) -> rsbinder::status::Result<Strong<dyn ICancellationSignal>> {
        self.fail()
    }

    fn enumerateEnrollments(&self) -> rsbinder::status::Result<()> {
        self.callback.onEnrollmentsEnumerated(&[])?;
        Ok(())
    }

    fn removeEnrollments(&self, enrollment_ids: &[i32]) -> rsbinder::status::Result<()> {
        self.callback.onEnrollmentsRemoved(enrollment_ids)?;
        Ok(())
    }

    fn getFeatures(&self) -> rsbinder::status::Result<()> {
        self.callback.onFeaturesRetrieved(&[])?;
        Ok(())
    }

    fn setFeature(&self, _hat: &HardwareAuthToken, feature: Feature, _enabled: bool) -> rsbinder::status::Result<()> {
        self.callback.onFeatureSet(feature)?;
        Ok(())
    }

    fn getAuthenticatorId(&self) -> rsbinder::status::Result<()> {
        self.callback.onAuthenticatorIdRetrieved(0)?;
        Ok(())
    }

    fn invalidateAuthenticatorId(&self) -> rsbinder::status::Result<()> {
        self.callback.onAuthenticatorIdInvalidated(0)?;
        Ok(())
    }

    fn resetLockout(&self, _hat: &HardwareAuthToken) -> rsbinder::status::Result<()> {
        self.callback.onLockoutCleared()?;
        Ok(())
    }

    fn close(&self) -> rsbinder::status::Result<()> {
        self.callback.onSessionClosed()?;
        Ok(())
    }

    fn authenticateWithContext(
        &self,
        operation_id: i64,
        _context: &OperationContext,
    ) -> rsbinder::status::Result<Strong<dyn ICancellationSignal>> {
        self.authenticate(operation_id)
    }

    fn enrollWithContext(
        &self,
        hat: &HardwareAuthToken,
        r#type: i8,
        features: &[u8],
        preview_surface: Option<&NativeHandle>,
        _context: &OperationContext,
    ) -> rsbinder::status::Result<Strong<dyn ICancellationSignal>> {
        self.enroll(hat, r#type, features, preview_surface)
    }

    fn detectInteractionWithContext(
        &self,
        _context: &OperationContext,
    ) -> rsbinder::status::Result<Strong<dyn ICancellationSignal>> {
        self.detectInteraction()
    }

    fn onContextChanged(&self, _context: &OperationContext) -> rsbinder::status::Result<()> {
        Ok(())
    }
}
