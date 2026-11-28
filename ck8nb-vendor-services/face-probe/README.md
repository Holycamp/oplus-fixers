# face-probe

Drives the stock `/vendor/lib64/hw/face.default.so` (Transsion/SenseTime legacy face HAL) directly
and prints every callback message as hex. Used to record the real message layout before an AIDL HAL
is written. It changes nothing on the system.

Build: run the `build-face-probe` workflow, download `ck8nb-face-probe`.

Run (root, SELinux permissive is fine):
```
adb push face-probe /data/local/tmp/
adb shell su -c "chmod 755 /data/local/tmp/face-probe"
adb shell su -c "mkdir -p /data/vendor_de/0/facedata"
adb shell su -c "cd /data/local/tmp && LD_LIBRARY_PATH=/vendor/lib64:/system/lib64 ./face-probe info" > probe_info.txt
adb shell su -c "cd /data/local/tmp && LD_LIBRARY_PATH=/vendor/lib64:/system/lib64 ./face-probe enroll 30" > probe_enroll.txt
adb logcat -d -b all | findstr /i "FaceHalHW FaceProbe avc" > probe_log.txt
```
During `enroll`, look at the front camera for 30 s. Send the three files back.
