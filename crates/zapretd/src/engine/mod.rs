//! One engine per daemon. Adapters own only their packet-engine resources.
use anyhow::Result;
use std::{future::Future, path::Path};
use zapret_core::{Capabilities, Strategy};

#[cfg(target_os = "linux")]
pub mod nfqws;
#[cfg(windows)]
pub mod winws;

pub struct EngineState {
    pub running: bool,
    pub uptime_secs: u64,
    pub warnings: Vec<String>,
}

pub trait Engine: Send {
    const PLATFORM: &'static str;
    const NAME: &'static str;
    const CAPABILITIES: Capabilities;
    fn start(
        &mut self,
        strategy: &Strategy,
        root: &Path,
        data: &Path,
    ) -> impl Future<Output = Result<()>> + Send;
    fn stop(&mut self) -> Result<()>;
    fn is_ready(&mut self) -> Result<EngineState>;
    fn cleanup(&mut self) -> Result<()>;
}

/// Unsupported platforms must fail rather than simulate a running engine.
#[cfg(not(any(windows, target_os = "linux")))]
pub struct UnsupportedEngine;
#[cfg(not(any(windows, target_os = "linux")))]
impl Engine for UnsupportedEngine {
    const PLATFORM: &'static str = std::env::consts::OS;
    const NAME: &'static str = "unsupported";
    const CAPABILITIES: Capabilities = Capabilities {
        tcp: false,
        udp: false,
        ipv6: false,
        packet_injection: false,
    };
    async fn start(&mut self, _: &Strategy, _: &Path, _: &Path) -> Result<()> {
        anyhow::bail!("Платформа не поддерживается")
    }
    fn stop(&mut self) -> Result<()> {
        anyhow::bail!("Платформа не поддерживается")
    }
    fn is_ready(&mut self) -> Result<EngineState> {
        anyhow::bail!("Платформа не поддерживается")
    }
    fn cleanup(&mut self) -> Result<()> {
        anyhow::bail!("Платформа не поддерживается")
    }
}
