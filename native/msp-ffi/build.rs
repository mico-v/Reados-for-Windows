use std::{env, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=include/msp_ffi.h");
    println!("cargo:rerun-if-changed=tests/c_contract.c");

    cc::Build::new()
        .file("tests/c_contract.c")
        .include("include")
        .warnings(true)
        .compile("msp_ffi_c_contract");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let export_definition = PathBuf::from("exports/msp_ffi.def");
        println!("cargo:rerun-if-changed={}", export_definition.display());
        println!(
            "cargo:rustc-cdylib-link-arg=/DEF:{}",
            export_definition.display()
        );
    }
}
