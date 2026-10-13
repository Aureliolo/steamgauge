fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    // ONNX Runtime's CoreML provider asks `@available` which macOS it runs on, and clang compiles
    // that to a call into its own runtime library. rustc links Apple targets without the
    // compiler's default libraries, so the library is named here or every binary that links the
    // provider fails on ___isPlatformVersionAtLeast.
    let macos = std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "macos");
    if !macos || std::env::var_os("CARGO_FEATURE_COREML").is_none() {
        return;
    }
    let output = std::process::Command::new("xcrun")
        .args(["clang", "-print-resource-dir"])
        .output()
        .expect("xcrun runs on the machine that builds for macOS");
    assert!(
        output.status.success(),
        "xcrun clang -print-resource-dir failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let resources = String::from_utf8(output.stdout).expect("a path clang printed");
    let runtime = std::path::Path::new(resources.trim())
        .join("lib")
        .join("darwin");
    assert!(
        runtime.join("libclang_rt.osx.a").is_file(),
        "clang's runtime library is not in {}",
        runtime.display()
    );
    println!("cargo::rustc-link-search=native={}", runtime.display());
    println!("cargo::rustc-link-lib=static=clang_rt.osx");
}
