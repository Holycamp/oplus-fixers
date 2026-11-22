// IDisplayPanelFeature V1 HAL for Tecno CK8nB (Transsion panel, MTK).
// SystemUI / FingerprintService call get/setDisplayPanelFeatureValue with numeric feature ids
// during FOD enroll and unlock. The stock service answered them; with no service the calls fail
// and fingerprint enroll hangs. This one answers success with value 0 and logs each id
// (tag OplusDpfHal) so the ones FOD needs can be mapped from logcat.
//
// The panel HBM for FOD is already driven by the kernel (FTS finger-down -> panel_hbm_set_cmdq),
// so this HAL deliberately does not write brightness nodes by itself.
mod bindings;
mod logging;

use rsbinder::{hub, BinderResult, Interface, ProcessState};

use bindings::{BnDisplayPanelFeature, IDisplayPanelFeature};

const HBM_NODE: &str = "/sys/kernel/tran_display/lcm_hbm_state";

/// id 22 is SystemUI's "FOD highlight" request (1 = on, 0 = off). Writing 1/0 to the panel node
/// was confirmed on device: the node accepts it and reads back "0xOK" (default "0xFA").
fn hbm(on: bool) {
    let v = if on { "1" } else { "0" };
    match std::fs::write(HBM_NODE, v) {
        Ok(_) => logging::info(&format!("hbm {v} written")),
        Err(e) => logging::error(&format!("hbm {v} write failed: {e}")),
    }
}

const SERVICE_NAME: &str = "vendor.oplus.hardware.displaypanelfeature.IDisplayPanelFeature/default";

struct Dpf;
impl Interface for Dpf {}

impl IDisplayPanelFeature for Dpf {
    // The ROM's framework declares `out int[]` (it sends only the array length), not the `inout`
    // of the LineageOS V1 snapshot, so the AIDL in this crate uses `out`.
    fn getDisplayPanelFeatureValue(&self, id: i32, values: &mut Vec<i32>) -> BinderResult<i32> {
        logging::info(&format!("get id={id} len={}", values.len()));
        if values.is_empty() {
            values.push(0);
        }
        for v in values.iter_mut() {
            *v = 0;
        }
        Ok(0)
    }

    fn setDisplayPanelFeatureValue(&self, id: i32, values: &[i32]) -> BinderResult<i32> {
        logging::info(&format!("set id={id} values={values:?}"));
        if id == 22 {
            hbm(values.first().copied().unwrap_or(0) != 0);
        }
        Ok(0)
    }

    fn getDisplayPanelInfo(&self, id: i32, info: &mut Vec<String>) -> BinderResult<i32> {
        logging::info(&format!("info id={id}"));
        info.clear();
        Ok(0)
    }
}

fn main() {
    logging::init();
    logging::init_rsbinder_logger();
    if let Err(e) = ProcessState::init_default() {
        logging::error(&format!("binder init failed: {e:?}"));
        std::process::exit(1);
    }
    ProcessState::start_thread_pool();
    let binder = BnDisplayPanelFeature::new_binder(Dpf);
    match hub::add_service(SERVICE_NAME, binder.as_binder()) {
        Ok(()) => logging::info(&format!("registered {SERVICE_NAME}")),
        Err(e) => {
            logging::error(&format!("register failed: {e:?}"));
            std::process::exit(1);
        }
    }
    let _ = ProcessState::join_thread_pool();
}
