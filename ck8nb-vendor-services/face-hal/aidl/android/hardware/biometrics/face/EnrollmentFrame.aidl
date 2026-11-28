package android.hardware.biometrics.face;
import android.hardware.biometrics.face.BaseFrame;
import android.hardware.biometrics.face.Cell;
import android.hardware.biometrics.face.EnrollmentStage;
@VintfStability
parcelable EnrollmentFrame {
    @nullable Cell cell;
    EnrollmentStage stage = EnrollmentStage.UNKNOWN;
    BaseFrame data;
}
