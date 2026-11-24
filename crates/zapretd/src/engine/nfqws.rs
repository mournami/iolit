//! Linux adapter for bol-van's nfqws. Skeleton (step L0 of the Linux plan):
//! the daemon runs and answers, but the engine does not start yet. It fails
//! instead of pretending to bypass anything.
use super::{Engine, EngineState};
use anyhow::{Result, bail};
use std::path::Path;
use zapret_core::{Capabilities, Strategy};

#[derive(Default)]
pub struct Nfqws;

impl Engine for Nfqws {
    const PLATFORM: &'static str = "linux";
    const NAME: &'static str = "nfqws";
    // Nothing is claimed until the engine really runs (step L2).
    const CAPABILITIES: Capabilities = Capabilities {
        tcp: false,
        udp: false,
        ipv6: false,
        packet_injection: false,
    };
    async fn start(&mut self, _: &Strategy, _: &Path, _: &Path) -> Result<()> {
        bail!("Обход на Linux ещё в разработке: движок nfqws пока не подключён")
    }
    fn stop(&mut self) -> Result<()> {
        Ok(())
    }
    fn is_ready(&mut self) -> Result<EngineState> {
        Ok(EngineState {
            running: false,
            uptime_secs: 0,
            warnings: vec!["Движок nfqws для Linux ещё не подключён".into()],
        })
    }
    fn cleanup(&mut self) -> Result<()> {
        Ok(())
    }
}
