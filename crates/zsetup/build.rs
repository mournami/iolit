//! Embeds the payload made by `zbuild installer-payload` (path in
//! ZAPRET_SETUP_PAYLOAD) and asks Windows for administrator rights at launch.
use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=ZAPRET_SETUP_PAYLOAD");
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR")).join("payload.tar.gz");
    match env::var_os("ZAPRET_SETUP_PAYLOAD") {
        Some(path) => {
            println!("cargo:rerun-if-changed={}", PathBuf::from(&path).display());
            fs::copy(&path, &out).expect("ZAPRET_SETUP_PAYLOAD must point to the payload");
        }
        // Workspace checks and tests build without a payload; such an
        // installer refuses to run.
        None => fs::write(&out, b"").expect("empty payload"),
    }
    let windows = env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    let msvc = env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if windows && msvc {
        // Program Files, services and HKLM need elevation; UAC asks once at start.
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-bins=/MANIFESTUAC:level='requireAdministrator' uiAccess='false'"
        );
    }
}
