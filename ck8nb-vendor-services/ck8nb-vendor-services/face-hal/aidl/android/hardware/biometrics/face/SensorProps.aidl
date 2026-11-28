package android.hardware.biometrics.face;
import android.hardware.biometrics.common.CommonProps;
import android.hardware.biometrics.face.FaceSensorType;
@VintfStability
parcelable SensorProps {
    CommonProps commonProps;
    FaceSensorType sensorType = FaceSensorType.UNKNOWN;
    boolean halControlsPreview;
    int previewDisplayId;
    int enrollPreviewWidth;
    int enrollPreviewHeight;
    float enrollTranslationX;
    float enrollTranslationY;
    float enrollPreviewScale;
    boolean supportsDetectInteraction;
    boolean supportsSelfIllumination;
    boolean resetLockoutRequiresHardwareAuthToken;
    boolean resetLockoutRequiresChallenge;
}
