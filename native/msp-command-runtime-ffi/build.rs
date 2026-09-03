use std::env;

fn main() {
    // Android's dynamic linker resolves DT_NEEDED by the library SONAME, not
    // by the build-machine path passed to the JNI shim's imported target.
    // Keep the Rust artifact self-describing while leaving Windows and Linux
    // release artifacts unchanged.
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        println!("cargo:rustc-link-arg-cdylib=-Wl,-soname,libmsp_command_runtime_ffi.so");
    }
}
