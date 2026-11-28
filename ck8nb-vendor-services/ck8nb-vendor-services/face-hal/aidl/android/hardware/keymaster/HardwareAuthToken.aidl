package android.hardware.keymaster;
import android.hardware.keymaster.HardwareAuthenticatorType;
import android.hardware.keymaster.Timestamp;
@VintfStability
parcelable HardwareAuthToken {
    long challenge;
    long userId;
    long authenticatorId;
    HardwareAuthenticatorType authenticatorType = HardwareAuthenticatorType.NONE;
    Timestamp timestamp;
    byte[] mac;
}
