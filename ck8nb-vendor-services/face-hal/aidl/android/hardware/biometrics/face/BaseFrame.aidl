package android.hardware.biometrics.face;
import android.hardware.biometrics.face.AcquiredInfo;
@VintfStability
parcelable BaseFrame {
    AcquiredInfo acquiredInfo = AcquiredInfo.UNKNOWN;
    int vendorCode;
    float pan;
    float tilt;
    float distance;
    boolean isCancellable;
}
