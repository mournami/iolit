#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    zapretd::platform::windows::entry()
}
#[cfg(target_os = "linux")]
fn main() -> anyhow::Result<()> {
    zapretd::platform::linux::entry()
}
#[cfg(not(any(windows, target_os = "linux")))]
fn main() -> anyhow::Result<()> {
    anyhow::bail!("Платформа не поддерживается. zapretd работает на Windows и Linux.")
}
