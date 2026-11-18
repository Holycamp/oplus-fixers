# Stock ColorOS 17.0.1 services: what they read and whether we can reuse them

Source: four aarch64 service binaries pulled from the donor vendor (`strings` / `readelf` only, nothing executed).

## vendor.oplus.hardware.charger-V12-service

- Interface: `vendor.oplus.hardware.charger.ICharger/default` (AIDL, V12). Also references a `V1` service name.
- Links against OPlus-only libs: `libGaiaClient_vnd`, `libosenseaidlhalclient`, `vendor.oplus.hardware.osense.client-V1`, `vendor.oplus.hardware.stability.oplus_project-V3`, `android.frameworks.sensorservice@1.0`.
- Reads the OPlus charger kernel driver, which the Tecno kernel does not have:

| Purpose | Node the stock service reads |
|---|---|
| Fast-charge protocol type | `/sys/class/oplus_chg/usb/fast_chg_type` |
| Adapter power | `/sys/class/oplus_chg/common/adapter_power` |
| VOOC charging active | `/sys/class/oplus_chg/battery/voocchg_ing` |
| PPS charging / power | `/sys/class/oplus_chg/battery/ppschg_ing`, `ppschg_power` |
| Charge path | `/sys/class/oplus_chg/battery/chg_path` |
| USB online / OTG | `/sys/class/oplus_chg/usb/online`, `otg_online` |
| Wireless | `/sys/class/oplus_chg/wireless/*`, `/proc/wireless/*` |
| Control | `/proc/charger/chg_ctl`, `/dev/oplus_chg` |
| Config | `/odm/firmware/fastchg/*` (thermal, smart charge, SOH, EIS tables) |

- Protocol names in the binary: `SVOOC`, `airsvooc`, `airvooc`, `oplus-pps`, `oplus-ufcs`, `third-pps`, `third-ufcs`.
- Only `bypassChgStatus` appears for bypass charging; no dedicated bypass node name. The control path is probably `chg_ctl` or the framework side.

Verdict: cannot run as-is (missing kernel nodes plus OPlus-only libs). Needs either our own `ICharger` V12 service or a path-redirect experiment. Both need the V12 method list, which needs `vendor.oplus.hardware.charger-V12-ndk.so`.

## vendor.oplus.hardware.displaypanelfeature-service

- Interface: `vendor.oplus.hardware.displaypanelfeature.IDisplayPanelFeature/default` (V1). MTK build (links `vendor.mediatek.hardware.pq_aidl-V7`, `libcomposer_ext`, `libdpaservice`).
- Reads `/sys/kernel/oplus_display/*`: `hbm`, `dimlayer_hbm`, `dimlayer_bl_en`, `notify_fppress`, `notify_panel_blank`, `oplus_brightness`, `max_brightness`, `aod_light_mode_set`, `dim_alpha`, `panel_id`.
- Needs props `ro.vendor.oplus.sensor.rect.*` (FOD sensor rectangle).

Verdict: the Tecno kernel has no `/sys/kernel/oplus_display`. Xia's adaptor replaces it. Do not run both: same interface name.

## vendor.oplus.hardware.touch-V2-hbp5-service

- Interface: `IOplusTouch` and `IOplusTouchEventCallback` (V2). Talks to `/dev/hbp_core`, `/dev/hbp%d`, `/proc/touchpanel`, firmware in `/odm/firmware/tp/`.
- Relays fingerprint down/up events (`fingerprint trigger Down`, `fingerprint_active`) from the touch controller.

Verdict: hardware-specific (OPPO HBP touch). Not usable on Tecno. Candidate for a stub service later if the framework spams binder errors.

## vendor.oplus.hardware.oplusSensor-V1-service

- Interface: `vendor.oplus.hardware.oplusSensor` (V1). Spectrum/colour sensor calibration for a `tcs3449` plus ALS calibration under `/proc/sensor/*`, `/mnt/vendor/persist/engineermode/sensor/spectrum`.

Verdict: hardware-specific. Stub candidate only.
