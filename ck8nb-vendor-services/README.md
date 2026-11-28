# ck8nb-vendor-services

Vendor services for ColorOS / OxygenOS ports on Transsion MediaTek hardware (Tecno Camon 20s Pro 5G, CK8nB, mt6893), so fixes live in the vendor partition instead of patched framework jars.

## Services

| Service | Status | Source |
|---|---|---|
| Charger HAL (`vendor.oplus.hardware.charger.ICharger`): SuperVOOC icon and wattage, bypass charging, 80% limit, battery health | Not yet built or tested on device | `charger/`, from [RapliVx/OPlus-Charger-Bridge-Transsion](https://github.com/RapliVx/OPlus-Charger-Bridge-Transsion) |
| Brightness / display panel feature | Use Xia's [OPlus-Brightness-Adaptor](https://github.com/ryanistr/OPlus-Brightness-Adaptor) | upstream |
| Face stub (`android.hardware.biometrics.face.IFace`): boot fix, no face unlock | Written, host-checked, not tested on device | `face-stub/`, derived from [ryanistr/vendor-senseface-aidl](https://github.com/ryanistr/vendor-senseface-aidl) |
| FOD | Open: check Xia's adaptor first, see `docs/stock-analysis.md` | n/a |

## Charger HAL

It implements ICharger V12 and reads the Transsion nodes your CK8nB exposes, all confirmed by `scripts/probe-charger.sh`:

| Function | Node |
|---|---|
| Adapter watts | `/sys/devices/platform/charger/adapter_capacity` |
| Bypass charging | `/sys/devices/platform/charger/bypass_charger` |
| 80% limit | `/sys/devices/platform/charger/sc_tuisoc` |
| Health / cycles | `battery_health_percent`, `tran_battery_cycle` |

### Build

Push this repo to GitHub. The `build-charger` workflow compiles the aarch64 binary and uploads `ck8nb-charger-hal` as an artifact. You can also run it from the Actions tab (workflow_dispatch).

### Install (unpacked vendor)

The downloaded artifact is a `vendor/` folder that mirrors the partition. Merge it into your unpacked vendor, overwriting when asked:

| File in the artifact | Replaces |
|---|---|
| `vendor/bin/hw/vendor.oplus.hardware.charger-V11-service` | the existing binary of the same name (keeps its SELinux label) |
| `vendor/etc/init/vendor.oplus.hardware.charger-V11-service.rc` | the existing rc |
| `vendor/etc/vintf/manifest/manifest_oplus_charger_aidl.xml` | the existing manifest (optional, see below) |

The binary must stay `0755` in `vendor_fs_config`. The source copies of the rc and manifest are in `charger/vendor/`.

The manifest is optional. The service starts without it because `ICharger/default` is already declared. The stock file says version 11 and the new service reports 12, so the new file only makes them match. If a boot goes wrong, put the original manifest back first.

VINTF fragments get wiped when the vendor is rebuilt, so keep these in the unpacked tree.

### First boot checks

```
service list | grep charger
getprop init.svc.oplus_charger_hal_service
logcat -d | grep -i -e charger -e avc
```

Send me the logcat. SELinux denials on the sysfs nodes are the likely first problem.

## Face stub

Fixes the boot loop where `FaceProvider.getHalInstance` crashes system_server because `IFace` is declared in the VINTF manifest but nothing serves it. The stub registers one sensor and answers every enroll/authenticate with UNABLE_TO_PROCESS, so face unlock does not work but boot does. For real face unlock use Xia's service.

Build: run the `build-face-stub` workflow, download `ck8nb-face-stub`. Install:

| File in the artifact | Goes to |
|---|---|
| `vendor/bin/hw/android.hardware.biometrics.face-service` | same path, mode 0755 (uid 0, gid 2000) |
| `vendor/etc/init/vendor.face-stub.rc` | same path |
| `vendor/etc/vintf/manifest/face_stub.xml` | only if no manifest already declares `android.hardware.biometrics.face` |

Delete any other face HAL rc that starts a different `IFace` service, and add to `vendor_file_contexts`:
`/vendor/bin/hw/android\.hardware\.biometrics\.face-service u:object_r:hal_face_default_exec:s0`

### Checked against a real CK8nB vendor listing (COS 17.0.1 EU)

| Finding | What to do |
|---|---|
| `vendor/etc/init/android.hardware.biometrics.face@1.0-service.rc` and its `.example` binary are present, so the stock HIDL face daemon runs next to the stub | Keep it. It is what makes the Add face button work. It does not conflict with the AIDL stub |
| Both `manifest_face_default.xml` and `face_stub.xml` are in `vendor/etc/vintf/manifest/` | Keep only one declaration of `android.hardware.biometrics.face`. If you keep `manifest_face_default.xml`, delete `face_stub.xml`; if you keep `face_stub.xml`, make sure its `<version>` matches what the stub serves (3). Two declarations make VINTF verification noisy and can pick the wrong version |
| `vendor.transsion.hardware.tranfacaidl-service.xml` and `vendor.transsion.hardware.tranfacaidl-V1-ndk_platform.so` exist but no `tranfacaidl` service binary is in `vendor/bin/hw` | The Transsion face service is declared but not shipped on this build, so there is nothing to wrap yet. Run `scripts/probe-face.sh` and look for it in the stock vendor image |
| `libstfaceunlockppl.so` (32 and 64 bit), `libTranfacFunction.so`, `libfeature.face.so`, `libmegface.so`, `libFaceDetectCA.so` are present | These are the engine candidates. A real HAL has to call one of them, which needs its exported API (see `scripts/probe-face.sh`) |

### Why the enrollment camera is black

This is intended, not a bug. `FaceEnrollClient` hands the HAL a preview surface and calls `enroll`; the stub answers `UNABLE_TO_PROCESS` immediately and never opens the camera, so the preview never gets a frame and Settings shows "failed to add face data". The stub only keeps system_server from crashing. Real face enrollment needs a HAL that opens the front camera, runs a face engine and reports enrollment progress, or the services.jar camera bridge approach.

## Licence

`face-stub/` is Apache-2.0 (AIDL from AOSP, scaffolding from Xia's repo).

`charger/` is GPL-3.0 (its `LICENSE` file; its `Cargo.toml` says Apache-2.0, which conflicts, so ask the author). Keep the whole repo GPL-3.0 while it includes that code.
