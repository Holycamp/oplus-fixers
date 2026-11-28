package android.hardware.biometrics.face;
import android.hardware.biometrics.face.Cell;
import android.hardware.biometrics.face.EnrollmentStage;
@VintfStability
parcelable EnrollmentStageConfig {
    EnrollmentStage stage = EnrollmentStage.UNKNOWN;
    Cell[] cells;
}
