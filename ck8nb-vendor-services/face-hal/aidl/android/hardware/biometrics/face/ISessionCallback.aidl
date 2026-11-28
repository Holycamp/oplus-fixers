package android.hardware.biometrics.face;
import android.hardware.biometrics.face.AuthenticationFrame;
import android.hardware.biometrics.face.EnrollmentFrame;
import android.hardware.biometrics.face.Error;
import android.hardware.biometrics.face.Feature;
import android.hardware.keymaster.HardwareAuthToken;
@VintfStability
interface ISessionCallback {
    void onChallengeGenerated(long challenge);
    void onChallengeRevoked(long challenge);
    void onAuthenticationFrame(in AuthenticationFrame frame);
    void onEnrollmentFrame(in EnrollmentFrame frame);
    void onError(Error error, int vendorCode);
    void onEnrollmentProgress(int enrollmentId, int remaining);
    void onAuthenticationSucceeded(int enrollmentId, in HardwareAuthToken hat);
    void onAuthenticationFailed();
    void onLockoutTimed(long durationMillis);
    void onLockoutPermanent();
    void onLockoutCleared();
    void onInteractionDetected();
    void onEnrollmentsEnumerated(in int[] enrollmentIds);
    void onFeaturesRetrieved(in Feature[] features);
    void onFeatureSet(Feature feature);
    void onEnrollmentsRemoved(in int[] enrollmentIds);
    void onAuthenticatorIdRetrieved(long authenticatorId);
    void onAuthenticatorIdInvalidated(long newAuthenticatorId);
    void onSessionClosed();
}
