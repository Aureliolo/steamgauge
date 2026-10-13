fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    let windows_msvc = std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|abi| abi == "msvc");
    if !windows_msvc {
        return;
    }
    // The app library links Tauri, which imports from Common Controls 6, and Windows loads that
    // version only for a program whose manifest asks for it: the app's own.
    let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../steamgauge-app/windows-app-manifest.xml");
    println!("cargo::rerun-if-changed={}", manifest.display());
    println!("cargo::rustc-link-arg=/MANIFEST:EMBED");
    println!(
        "cargo::rustc-link-arg=/MANIFESTINPUT:{}",
        manifest.display()
    );
    // tauri-build links the app with the C runtime built in, and puts an empty msvcrt.lib on the
    // search path every program linking the app library inherits. Its arguments choosing the
    // static runtime reach only the app's own programs, so this one names it the same way.
    for library in [
        "libvcruntimed.lib",
        "vcruntime.lib",
        "vcruntimed.lib",
        "libcmtd.lib",
        "msvcrt.lib",
        "msvcrtd.lib",
        "libucrt.lib",
        "libucrtd.lib",
    ] {
        println!("cargo::rustc-link-arg=/NODEFAULTLIB:{library}");
    }
    for library in ["libcmt.lib", "libvcruntime.lib", "ucrt.lib"] {
        println!("cargo::rustc-link-arg=/DEFAULTLIB:{library}");
    }
}
