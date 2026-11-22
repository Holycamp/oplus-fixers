#![allow(non_snake_case, unused_imports, dead_code)]
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

pub use vendor::oplus::hardware::touch::IOplusTouch::{BnOplusTouch, IOplusTouch};
pub use vendor::oplus::hardware::touch::IOplusTouchEventCallback::IOplusTouchEventCallback;
pub use vendor::oplus::hardware::touch::OplusTouchInfo::OplusTouchInfo;
pub use vendor::oplus::hardware::touch::OplusTouchStatus::OplusTouchStatus;

/// V2 hash from LineageOS aidl_api/vendor.oplus.hardware.touch/2/.hash
pub const INTERFACE_HASH: &str = "015ebfff0763989a367c9255d9742b3cdf124082";
pub const INTERFACE_VERSION: i32 = 2;
pub const GET_INTERFACE_HASH_TRANSACTION: u32 = 0xfffffe;
pub const GET_INTERFACE_VERSION_TRANSACTION: u32 = 0xffffff;
