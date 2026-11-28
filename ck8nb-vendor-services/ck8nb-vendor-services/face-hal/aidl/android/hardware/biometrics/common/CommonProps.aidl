package android.hardware.biometrics.common;
import android.hardware.biometrics.common.ComponentInfo;
import android.hardware.biometrics.common.SensorStrength;
@VintfStability
parcelable CommonProps {
    int sensorId;
    SensorStrength sensorStrength = SensorStrength.CONVENIENCE;
    int maxEnrollmentsPerUser;
    ComponentInfo[] componentInfo;
}
