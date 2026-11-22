#![allow(non_snake_case, unused_imports, dead_code)]
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

pub use vendor::oplus::hardware::performance::IPerformance::{BnPerformance, IPerformance};

/// V4 hash from LineageOS aidl_api/vendor.oplus.hardware.performance/4/.hash
pub const INTERFACE_HASH: &str = "c52602520ce4cad6e05e1e81a8114942f272d5ad";
pub const INTERFACE_VERSION: i32 = 4;
pub const GET_INTERFACE_HASH_TRANSACTION: u32 = 0xfffffe;
pub const GET_INTERFACE_VERSION_TRANSACTION: u32 = 0xffffff;
