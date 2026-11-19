#!/system/bin/sh
# Run ON THE PHONE as root (Termux: `su`, then `sh probe-charger.sh`).
# It writes target_probe.txt to your Download folder (falls back to
# /data/local/tmp or the current folder) and prints where it saved it.
# Attach that file in the chat. Its last line must be "== done ==".
# Set OUT=/some/path.txt to choose the output file yourself.

probe() {
echo "== who/where =="
id
echo "device=$(getprop ro.product.device) model=$(getprop ro.product.model)"
echo "board=$(getprop ro.board.platform) hardware=$(getprop ro.hardware)"
echo "fingerprint=$(getprop ro.build.fingerprint)"
echo "kernel=$(uname -r)"
echo "selinux=$(getenforce 2>&1)"
echo "boot_completed=$(getprop sys.boot_completed)"
for p in /sys/class/power_supply /sys/devices/platform/charger \
         /sys/class/oplus_chg /proc/charger /proc/oplus_chg; do
  ls -ld "$p" 2>&1
done

echo "== power_supply =="
if [ -d /sys/class/power_supply ]; then
  for d in /sys/class/power_supply/*; do
    [ -e "$d" ] || { echo "(no entries, or not readable as this user)"; break; }
    echo "-- $d"
    for f in type status online present capacity voltage_now current_now \
             charge_type health constant_charge_current_max input_current_limit; do
      [ -r "$d/$f" ] && echo "$f=$(cat "$d/$f" 2>/dev/null)"
    done
    ls "$d" 2>/dev/null | tr '\n' ' '
    echo
  done
else
  echo "/sys/class/power_supply does not exist; searching for it:"
  find /sys -maxdepth 4 -iname '*power_supply*' 2>/dev/null | head -20
fi

echo "== charger/battery platform devices =="
find /sys/devices/platform -maxdepth 1 \( -iname '*charg*' -o -iname '*battery*' \
  -o -iname '*chg*' -o -iname '*gauge*' -o -iname '*mtk*pmic*' \) 2>/dev/null | head -40
for d in /sys/devices/platform/charger /sys/devices/platform/mt-battery; do
  [ -d "$d" ] || continue
  echo "-- $d"
  for f in "$d"/*; do
    [ -f "$f" ] && [ -r "$f" ] && echo "$f=$(head -c 120 "$f" 2>/dev/null)"
  done
done

echo "== oplus_chg / bypass / vooc / ufcs nodes =="
find /proc /sys -maxdepth 5 \( -iname '*oplus_chg*' -o -iname '*bypass*' \
  -o -iname '*vooc*' -o -iname '*ufcs*' -o -iname '*pdc*' -o -iname '*chg_*' \) \
  2>/dev/null | head -150

echo "== vintf entries mentioning charger =="
grep -rI -i -B2 -A8 charger /vendor/etc/vintf 2>/dev/null | head -120

echo "== init rc mentioning charger =="
grep -rIl -i charger /vendor/etc/init 2>/dev/null

echo "== transsion charge nodes (values) =="
for d in /sys/class/power_supply/tran-auto-test /sys/class/tran_class/tran_chg_control \
         /sys/devices/platform/charger/tran_class/tran_chg_control; do
  [ -e "$d" ] || continue
  echo "-- $d"
  ls -ld "$d" 2>&1
  if [ -d "$d" ]; then
    for f in "$d"/*; do
      [ -f "$f" ] && [ -r "$f" ] && echo "$f=$(head -c 120 "$f" 2>/dev/null)"
    done
  else
    echo "value=$(head -c 120 "$d" 2>/dev/null)"
  fi
done
echo "/proc/tran_chg_limit_current=$(head -c 120 /proc/tran_chg_limit_current 2>&1)"
ls -lZ /sys/devices/platform/charger/bypass_charger \
       /sys/devices/platform/charger/fast_chg_indicator \
       /sys/devices/platform/charger/adapter_capacity 2>&1

echo "== binder services (charger / oplus) =="
service list 2>/dev/null | grep -i -e charg -e oplus | head -40

echo "== charger service files on this device =="
ls -l /vendor/bin/hw 2>/dev/null | grep -i -e charge -e chg
ls -l /vendor/lib64 2>/dev/null | grep -i -e 'hardware.charger' -e trancharge
for f in /vendor/etc/init/vendor.oplus.hardware.charger-V11-service.rc \
         /vendor/etc/init/vendor.hardware.trancharge-service.example.rc; do
  echo "-- $f"
  cat "$f" 2>&1
done

echo "== charging props =="
getprop | grep -i -e chg -e charg -e vooc -e bypass -e batt

echo "== done =="
}

OUT="${OUT:-}"
if [ -z "$OUT" ]; then
  for d in /storage/emulated/0/Download /sdcard/Download /data/local/tmp "$PWD"; do
    if [ -d "$d" ] && [ -w "$d" ]; then
      OUT="$d/target_probe.txt"
      break
    fi
  done
fi

probe > "$OUT" 2>&1
echo "Saved: $OUT ($(wc -c < "$OUT") bytes)"
tail -n 1 "$OUT"
