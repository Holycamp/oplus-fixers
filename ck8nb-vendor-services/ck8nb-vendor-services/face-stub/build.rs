// face-stub build.rs (from ryanistr/vendor-senseface-aidl, Apache-2.0)
//
// Generates the face AIDL bindings with rsbinder-aidl and points the linker at
// the ROM's own libraries so DT_NEEDED only ever names something the device can
// resolve. Every path here comes from the oos worktree; nothing is invented.
use std::path::PathBuf;
use std::env;

fn main() {
    let aidl_dir = PathBuf::from("aidl");

    // The face interface pulls in biometrics.common, android.hardware.common and
    // android.hardware.keymaster, so all four packages must be searchable.
    let iface = aidl_dir.join("android/hardware/biometrics/face/IFace.aidl");
    let isession = aidl_dir.join("android/hardware/biometrics/face/ISession.aidl");
    let icb = aidl_dir.join("android/hardware/biometrics/face/ISessionCallback.aidl");
    let icancel = aidl_dir.join("android/hardware/biometrics/common/ICancellationSignal.aidl");

    for p in [&iface, &isession, &icb, &icancel] {
        println!("cargo:rerun-if-changed={}", p.display());
        assert!(p.exists(), "missing AIDL source: {}", p.display());
    }

    let out_dir = env::var("OUT_DIR").unwrap();
    let out_file = PathBuf::from(&out_dir).join("bindings.rs");

    rsbinder_aidl::Builder::new()
        .source(iface)
        .source(isession)
        .source(icb)
        .source(icancel)
        .include_dir(&aidl_dir)
        .output(&out_file)
        .generate()
        .expect("failed to generate face AIDL bindings");

    // rsbinder's generated traits are async in 0.6; the service impls use
    // #[async_trait], so the generated traits need the attribute too.
    let mut content = std::fs::read_to_string(&out_file).unwrap();
    for t in [
        "IFaceAsyncService",
        "ISessionAsyncService",
        "ISessionCallbackAsyncService",
        "ICancellationSignalAsyncService",
    ] {
        let target = format!("pub trait {}: rsbinder::Interface", t);
        let repl = format!("#[async_trait::async_trait]\npub trait {}: rsbinder::Interface", t);
        content = content.replace(&target, &repl);
    }

    // The framework's IFace$Stub answers getInterfaceHash/getInterfaceVersion at
    // AIDL's reserved codes 0xfffffe / 0xffffff, not as sequential methods — the
    // frozen V3 .aidl snapshot omits them, so rsbinder-aidl never generates them.
    // Without these arms both calls answer UNKNOWN_TRANSACTION, and
    // FaceStartUserClient treats that as a remote exception and finishes the
    // client without authenticating. Inject them into every generated dispatch.
    let mut patched = String::with_capacity(content.len() + 768);
    let needle = "_ => Err(rsbinder::StatusCode::UnknownTransaction),";
    let mut injected = 0usize;
    for line in content.lines() {
        if line.trim() == needle {
            let ind = &line[..line.len() - line.trim_start().len()];
            patched.push_str(&format!(
                "{i}{h} => {{\n\
                 {i}    _reply.write(&rsbinder::Status::from(rsbinder::StatusCode::Ok))?;\n\
                 {i}    _reply.write(&crate::bindings::INTERFACE_HASH.to_string())?;\n\
                 {i}    Ok(())\n\
                 {i}}}\n\
                 {i}{v} => {{\n\
                 {i}    _reply.write(&rsbinder::Status::from(rsbinder::StatusCode::Ok))?;\n\
                 {i}    _reply.write(&crate::bindings::INTERFACE_VERSION)?;\n\
                 {i}    Ok(())\n\
                 {i}}}\n\
                 {i}{needle}\n",
                i = ind,
                h = "crate::bindings::GET_INTERFACE_HASH_TRANSACTION",
                v = "crate::bindings::GET_INTERFACE_VERSION_TRANSACTION",
                needle = needle,
            ));
            injected += 1;
        } else {
            patched.push_str(line);
            patched.push('\n');
        }
    }
    assert!(
        injected > 0,
        "no on_transact fallback arm found in generated bindings"
    );
    content = patched;

    std::fs::write(&out_file, content).unwrap();

    // Only liblog is needed; it is part of the NDK sysroot (no ROM tree required).
    println!("cargo:rustc-link-lib=dylib=log");
}
