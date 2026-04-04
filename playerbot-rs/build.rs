fn main() {
    let ffi_header = "../cpp_wrapper/botffi.h";

    println!("cargo:rerun-if-changed={ffi_header}");

    let bindings = bindgen::Builder::default()
        .header(ffi_header)
        // Only generate types defined in botffi.h
        .allowlist_type("BotHandle")
        .allowlist_type("UnitHandle")
        .allowlist_type("BotPosition")
        .allowlist_type("BotUnitSnapshot")
        .allowlist_type("BotWorldSnapshot")
        .allowlist_type("BotAuraInfo")
        .allowlist_type("BotThreatEntry")
        .allowlist_type("BotSafePosition")
        .allowlist_type("BotCallbacks")
        // Don't generate the Rust exports — we implement those ourselves
        .blocklist_function("playerbot_.*")
        // Use Rust bool instead of u8 for C bool
        .use_core()
        // Derive common traits for all generated structs
        .derive_debug(true)
        .derive_copy(true)
        .derive_default(true)
        // Suppress unused import warnings in generated code
        .raw_line("#[allow(dead_code, non_snake_case, non_camel_case_types)]")
        .generate()
        .expect("bindgen failed to generate FFI bindings from botffi.h");

    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("failed to write bindings.rs");
}
