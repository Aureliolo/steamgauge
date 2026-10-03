fn main() {
    tauri_build::build();
    // Compatible with the hardware shadow stack, which then guards every return address. Not
    // /DEPENDENTLOADFLAG:0x800 as well: Windows carries an older DirectML.dll in System32, and
    // limiting the program's own imports to System32 would load that one over the one it ships.
    let windows_msvc = std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|abi| abi == "msvc");
    if windows_msvc {
        println!("cargo::rustc-link-arg-bins=/CETCOMPAT");
    }
}
