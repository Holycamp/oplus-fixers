// Stub face HAL: registers android.hardware.biometrics.face.IFace/default.
// It exists so ColorOS's FaceService always finds a daemon and one sensor
// (an empty sensor list crashes system_server in FaceProvider.getHalInstance).
// Every operation reports UNABLE_TO_PROCESS, so face unlock simply fails.
mod bindings;
mod face;
mod logging;
mod session;

use rsbinder::{hub, ProcessState};

use bindings::android::hardware::biometrics::face::IFace::BnFace;
use face::FaceHal;

const SERVICE_NAME: &str = "android.hardware.biometrics.face.IFace/default";

fn main() {
    logging::init();
    logging::init_rsbinder_logger();
    logging::info("face-stub starting");

    if let Err(e) = ProcessState::init_default() {
        logging::error(&format!("binder init failed: {:?}", e));
        std::process::exit(1);
    }
    ProcessState::start_thread_pool();

    let binder = BnFace::new_binder(FaceHal::new());
    match hub::add_service(SERVICE_NAME, binder.as_binder()) {
        Ok(()) => logging::info(&format!("registered {}", SERVICE_NAME)),
        Err(e) => {
            logging::error(&format!("failed to register {}: {:?}", SERVICE_NAME, e));
            std::process::exit(1);
        }
    }
    let _ = ProcessState::join_thread_pool();
}
