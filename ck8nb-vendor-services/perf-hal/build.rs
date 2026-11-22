// Generates the IPerformance V4 bindings (AIDL from LineageOS android_hardware_oplus, Apache-2.0)
// and injects the reserved getInterfaceVersion/getInterfaceHash transaction arms.
use std::{env, path::PathBuf};

fn main() {
    let aidl_dir = PathBuf::from("aidl");
    let base = aidl_dir.join("vendor/oplus/hardware/performance");
    let files = ["IPerformance","MmWatermarkScale","OsvelteStatus","OsvelteVersionRet","ProcMemStat","ProcMemStatRet","ProcReqHal","ProcReqItemHal","TaskGroup","TaskWakeInfo","ThreadInfo","UploadType"];
    let out_file = PathBuf::from(env::var("OUT_DIR").unwrap()).join("bindings.rs");

    let mut b = rsbinder_aidl::Builder::new();
    for f in files {
        let p = base.join(format!("{f}.aidl"));
        println!("cargo:rerun-if-changed={}", p.display());
        assert!(p.exists(), "missing AIDL: {}", p.display());
        b = b.source(p);
    }
    b.include_dir(&aidl_dir).output(&out_file).generate().expect("aidl codegen failed");

    let content = std::fs::read_to_string(&out_file).unwrap();
    let needle = "_ => ::core::result::Result::Err(rsbinder::StatusCode::UnknownTransaction),";
    let mut patched = String::with_capacity(content.len() + 768);
    let mut n = 0;
    for line in content.lines() {
        if line.trim() == needle {
            let i = &line[..line.len() - line.trim_start().len()];
            patched.push_str(&format!(
                "{i}crate::bindings::GET_INTERFACE_HASH_TRANSACTION => {{\n\
                 {i}    _reply.write(&rsbinder::Status::from(rsbinder::StatusCode::Ok))?;\n\
                 {i}    _reply.write(&crate::bindings::INTERFACE_HASH.to_string())?;\n\
                 {i}    Ok(())\n{i}}}\n\
                 {i}crate::bindings::GET_INTERFACE_VERSION_TRANSACTION => {{\n\
                 {i}    _reply.write(&rsbinder::Status::from(rsbinder::StatusCode::Ok))?;\n\
                 {i}    _reply.write(&crate::bindings::INTERFACE_VERSION)?;\n\
                 {i}    Ok(())\n{i}}}\n{i}{needle}\n"));
            n += 1;
        } else {
            patched.push_str(line);
            patched.push('\n');
        }
    }
    assert!(n > 0, "no fallback arm found");
    std::fs::write(&out_file, patched).unwrap();
    println!("cargo:rustc-link-lib=dylib=log");
}
