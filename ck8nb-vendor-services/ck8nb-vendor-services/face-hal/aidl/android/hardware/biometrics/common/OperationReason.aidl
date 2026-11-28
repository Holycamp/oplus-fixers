package android.hardware.biometrics.common;
@VintfStability
@Backing(type="byte")
enum OperationReason {
    UNKNOWN = 0,
    BIOMETRIC_PROMPT = 1,
    KEYGUARD = 2,
}
