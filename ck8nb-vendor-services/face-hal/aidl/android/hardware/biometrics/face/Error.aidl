package android.hardware.biometrics.face;
@VintfStability
@Backing(type="byte")
enum Error {
    UNKNOWN = 0,
    HW_UNAVAILABLE = 1,
    UNABLE_TO_PROCESS = 2,
    TIMEOUT = 3,
    NO_SPACE = 4,
    CANCELED = 5,
    UNABLE_TO_REMOVE = 6,
    VENDOR = 7,
    REENROLL_REQUIRED = 8,
}
