#![allow(non_snake_case, unused_imports, dead_code)]
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

pub use vendor::oplus::hardware::displaypanelfeature::IDisplayPanelFeature::{BnDisplayPanelFeature, IDisplayPanelFeature};

/// V1 hash from LineageOS aidl_api/vendor.oplus.hardware.displaypanelfeature/1/.hash
pub const INTERFACE_HASH: &str = "2a77fa43b874c191dea05b1585626a634d85ad6d";
pub const INTERFACE_VERSION: i32 = 1;
pub const GET_INTERFACE_HASH_TRANSACTION: u32 = 0xfffffe;
pub const GET_INTERFACE_VERSION_TRANSACTION: u32 = 0xffffff;
