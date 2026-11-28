package android.hardware.keymaster;
@VintfStability
@Backing(type="int")
enum HardwareAuthenticatorType {
    NONE = 0,
    PASSWORD = 1,
    FINGERPRINT = 2,
    ANY = -1,
}
