#[cfg(windows)]
mod service;
#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    service::entry()
}
#[cfg(not(windows))]
fn main() -> anyhow::Result<()> {
    anyhow::bail!("Платформа не поддерживается. zapretd фазы 0 работает только на Windows.")
}
