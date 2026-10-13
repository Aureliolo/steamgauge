fn main() {
    let windows_msvc = std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|abi| abi == "msvc");
    // Tauri puts its manifest into the resource file it links into binaries only, so the test
    // programs of the library would start without Common Controls 6 and fail at load on an
    // import comctl32 version 5 lacks. The linker embeds the same manifest into every program.
    let windows = if windows_msvc {
        tauri_build::WindowsAttributes::new_without_app_manifest()
    } else {
        tauri_build::WindowsAttributes::new()
    };
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("tauri-build");
    if windows_msvc {
        let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("windows-app-manifest.xml");
        println!("cargo::rerun-if-changed={}", manifest.display());
        println!("cargo::rustc-link-arg=/MANIFEST:EMBED");
        println!(
            "cargo::rustc-link-arg=/MANIFESTINPUT:{}",
            manifest.display()
        );
        // Compatible with the hardware shadow stack, which then guards every return address. Not
        // /DEPENDENTLOADFLAG:0x800 as well: Windows carries an older DirectML.dll in System32, and
        // limiting the program's own imports to System32 would load that one over the one it
        // ships.
        println!("cargo::rustc-link-arg-bins=/CETCOMPAT");
    }
}
