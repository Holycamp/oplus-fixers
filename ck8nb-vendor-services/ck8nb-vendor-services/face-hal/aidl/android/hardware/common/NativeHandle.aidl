package android.hardware.common;
@VintfStability
parcelable NativeHandle {
    ParcelFileDescriptor[] fds;
    int[] ints;
}
