//! Zapret Desktop installer for Windows: one small window in the app's
//! style, no pages, no choices. It starts installing at once and opens the
//! app when done. The install folder is fixed; see `win.rs` for the steps and
//! `gui.rs` for the window.
#![cfg_attr(not(windows), allow(dead_code))]
#![windows_subsystem = "windows"]

#[cfg(windows)]
mod gui;
mod payload;
#[cfg(windows)]
mod win;

/// Written by `zbuild installer-payload`; empty in plain workspace builds.
static PAYLOAD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload.tar.gz"));

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    win::main(PAYLOAD)
}

#[cfg(not(windows))]
fn main() -> std::process::ExitCode {
    eprintln!(
        "zapret-setup работает только в Windows (файлов в установщике: {} байт)",
        PAYLOAD.len()
    );
    std::process::ExitCode::FAILURE
}
