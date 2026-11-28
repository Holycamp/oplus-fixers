fn main() {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    rsbinder_aidl::Builder::new()
        .source(std::path::PathBuf::from("aidl"))
        .output(std::path::PathBuf::from(out_dir).join("face_aidl.rs"))
        .generate()
        .expect("failed to generate face AIDL bindings");
    println!("cargo:rerun-if-changed=aidl");
}
