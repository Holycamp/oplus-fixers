// IDisplayPanelFeature V1 HAL for Tecno CK8nB (Transsion panel, MTK).
// SystemUI / FingerprintService call get/setDisplayPanelFeatureValue with numeric feature ids
// during FOD enroll and unlock. The stock service answered them; with no service the calls fail
// and fingerprint enroll hangs. This one answers "not supported" (-1) to every call and
// back unchanged, and logs each id so the real ones can be mapped from logcat (tag OplusDpfHal).
//
// The panel HBM for FOD is already driven by the kernel (FTS finger-down -> panel_hbm_set_cmdq),
// so this HAL deliberately does not write brightness nodes by itself.
mod bindings;
mod logging;

use rsbinder::{hub, BinderResult, Interface, ProcessState};

use bindings::{BnDisplayPanelFeature, IDisplayPanelFeature};

const SERVICE_NAME: &str = "vendor.oplus.hardware.displaypanelfeature.IDisplayPanelFeature/default";

struct Dpf;
impl Interface for Dpf {}

impl IDisplayPanelFeature for Dpf {
    fn getDisplayPanelFeatureValue(&self, id: i32, values: &mut Vec<i32>) -> BinderResult<i32> {
        logging::info(&format!("get id={id} in={values:?}"));
        // Leave the array untouched and report "not supported". Answering success for unknown ids
        // made system_server enable every panel feature at startup and boot_monitor rebooted.
        Ok(-1)
    }

    fn setDisplayPanelFeatureValue(&self, id: i32, values: &[i32]) -> BinderResult<i32> {
        logging::info(&format!("set id={id} values={values:?}"));
        Ok(-1)
    }

    fn getDisplayPanelInfo(&self, id: i32, info: &mut Vec<String>) -> BinderResult<i32> {
        logging::info(&format!("info id={id}"));
        Ok(-1)
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
