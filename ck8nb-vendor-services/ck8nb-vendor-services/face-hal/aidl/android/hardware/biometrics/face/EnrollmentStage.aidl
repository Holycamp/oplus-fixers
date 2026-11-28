package android.hardware.biometrics.face;
@VintfStability
@Backing(type="byte")
enum EnrollmentStage {
    UNKNOWN = 0,
    FIRST_FRAME_RECEIVED = 1,
    WAITING_FOR_CENTERING = 2,
    HOLDING_STILL_IN_CENTER = 3,
    GATHERING_ENROLLMENT_DATA = 4,
    ENROLLING_MOVEMENT_1 = 5,
    ENROLLING_MOVEMENT_2 = 6,
    ENROLLMENT_FINISHED = 7,
}
