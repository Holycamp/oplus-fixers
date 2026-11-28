package android.hardware.biometrics.face;
import android.hardware.biometrics.common.ICancellationSignal;
import android.hardware.biometrics.common.OperationContext;
import android.hardware.biometrics.face.EnrollmentStageConfig;
import android.hardware.biometrics.face.EnrollmentType;
import android.hardware.biometrics.face.Feature;
import android.hardware.common.NativeHandle;
import android.hardware.keymaster.HardwareAuthToken;
@VintfStability
interface ISession {
    void generateChallenge();
    void revokeChallenge(in long challenge);
    EnrollmentStageConfig[] getEnrollmentConfig(in EnrollmentType enrollmentType);
    ICancellationSignal enroll(in HardwareAuthToken hat, in EnrollmentType type, in Feature[] features, in @nullable NativeHandle previewSurface);
    ICancellationSignal authenticate(in long operationId);
    ICancellationSignal detectInteraction();
    void enumerateEnrollments();
    void removeEnrollments(in int[] enrollmentIds);
    void getFeatures();
    void setFeature(in HardwareAuthToken hat, in Feature feature, boolean enabled);
    void getAuthenticatorId();
    void invalidateAuthenticatorId();
    void resetLockout(in HardwareAuthToken hat);
    void close();
    ICancellationSignal authenticateWithContext(in long operationId, in OperationContext context);
    ICancellationSignal enrollWithContext(in HardwareAuthToken hat, in EnrollmentType type, in Feature[] features, in @nullable NativeHandle previewSurface, in OperationContext context);
    ICancellationSignal detectInteractionWithContext(in OperationContext context);
    void onContextChanged(in OperationContext context);
}
