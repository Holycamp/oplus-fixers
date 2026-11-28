#!/system/bin/sh
# Run on the phone as root:  adb shell su -c "sh /sdcard/probe-face.sh" > face_probe.txt
# Collects what is needed to decide whether a real face HAL can wrap an existing engine.
echo "== face services (running)"
ps -A | grep -i -E 'face|tranfac'
echo "== init rcs"
grep -l -i -E 'face|tranfac' /vendor/etc/init/*.rc /odm/etc/init/*.rc 2>/dev/null
echo "== vintf declarations"
for f in /vendor/etc/vintf/manifest/*face* /vendor/etc/vintf/manifest/*tranfac* /vendor/etc/vintf/manifest.xml /odm/etc/vintf/manifest*.xml; do
  [ -f "$f" ] && { echo "-- $f"; grep -i -B2 -A6 -E 'face|tranfac' "$f"; }
done
echo "== registered services"
service list 2>/dev/null | grep -i -E 'face|tranfac'
lshal 2>/dev/null | grep -i -E 'face|tranfac'
echo "== engine libs"
ls -l /vendor/lib64/*face* /vendor/lib64/*Tranfac* /vendor/lib64/*stface* /vendor/lib64/*megface* 2>/dev/null
echo "== exported symbols (needs toolbox readelf/nm; skipped if missing)"
for l in libstfaceunlockppl libTranfacFunction libfeature.face libmegface libFaceDetectCA; do
  f=/vendor/lib64/$l.so
  [ -f "$f" ] || continue
  echo "-- $f"
  if command -v readelf >/dev/null 2>&1; then readelf -Ws --dyn-syms "$f" | grep -v UND | head -80
  else strings "$f" | grep -E '^(Java_|_Z|st_|STF|Tran|mgf|Meg)' | head -80; fi
done
echo "== camera ids"
dumpsys media.camera | grep -i -E 'Device [0-9]+ is|facing' | head
echo "== sepolicy denials"
logcat -d -b all | grep -i 'avc' | grep -i -E 'face|tranfac' | tail -30
