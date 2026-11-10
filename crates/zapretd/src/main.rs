#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    zapretd::platform::windows::entry()
}
#[cfg(not(windows))]
fn main() -> anyhow::Result<()> {
    anyhow::bail!("Платформа не поддерживается. zapretd фазы 0 работает только на Windows.")
}
