fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=HERDR_BUILD_CHANNEL");
    println!("cargo:rerun-if-env-changed=HERDR_BUILD_ID");
    println!("cargo:rerun-if-env-changed=HERDR_BUILD_COMMIT");
    alias_test_binary();
}

/// The fork installs as `neoherdr`, but upstream's integration tests locate
/// the binary through `CARGO_BIN_EXE_herdr`. Aliasing that name here keeps
/// those test files byte-identical to upstream.
// ponytail: derives the profile directory from OUT_DIR's layout
// (<profile>/build/<pkg>-<hash>/out); rename the tests' env var instead if
// cargo ever changes it.
fn alias_test_binary() {
    let Some(out_dir) = std::env::var_os("OUT_DIR") else {
        return;
    };
    let Some(profile_dir) = std::path::Path::new(&out_dir).ancestors().nth(3) else {
        return;
    };
    let suffix = if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        ".exe"
    } else {
        ""
    };
    println!(
        "cargo:rustc-env=CARGO_BIN_EXE_herdr={}",
        profile_dir.join(format!("neoherdr{suffix}")).display()
    );
}
