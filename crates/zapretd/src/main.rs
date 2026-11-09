#[cfg(windows)]
mod service;
#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    service::entry()
}
#[cfg(not(windows))]
fn main() {
    eprintln!("This service runs on Windows. macOS uses engines/macos.");
}
