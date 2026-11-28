package android.hardware.biometrics.common;
import android.hardware.biometrics.common.OperationReason;
@VintfStability
parcelable OperationContext {
    int id = 0;
    OperationReason reason = OperationReason.UNKNOWN;
    boolean isAod = false;
    boolean isCrypto = false;
    int wakeReason = 0;
    int displayState = 0;
}
