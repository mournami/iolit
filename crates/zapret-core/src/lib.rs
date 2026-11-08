pub mod bundle;
pub mod ipc;
#[cfg(feature = "network")]
pub mod probe;
pub mod strategy;

use serde::{Deserialize, Serialize};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const MAC_SOCKET: &str = "/var/run/zapret-desktop/control.sock";
pub const WINDOWS_PIPE: &str = r"\\.\pipe\ZapretDesktop.Control.v1";
pub const MAC_DATA: &str = "/Library/Application Support/Zapret Desktop";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Strategy {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub supported: bool,
    #[serde(default)]
    pub arguments: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Capabilities {
    pub tcp: bool,
    pub udp: bool,
    pub ipv6: bool,
    pub packet_injection: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub installed: bool,
    pub running: bool,
    pub platform: String,
    pub engine: String,
    pub strategy: String,
    pub uptime_secs: u64,
    pub capabilities: Capabilities,
    pub warnings: Vec<String>,
    pub packets: u64,
    pub modified_packets: u64,
    pub errors: u64,
}

impl ServiceStatus {
    pub fn unavailable(reason: String) -> Self {
        Self {
            installed: false,
            running: false,
            platform: std::env::consts::OS.into(),
            engine: if cfg!(target_os = "macos") {
                "PF / BPF"
            } else {
                "WinDivert"
            }
            .into(),
            strategy: String::new(),
            uptime_secs: 0,
            capabilities: Capabilities::default(),
            warnings: vec![reason],
            packets: 0,
            modified_packets: 0,
            errors: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub ok: bool,
    pub latency_ms: u64,
    pub detail: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Operation {
    pub busy: bool,
    pub stage: String,
    pub current: usize,
    pub total: usize,
    pub results: Vec<ProbeResult>,
    pub error: Option<String>,
    pub winner: Option<String>,
    pub notice: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub version: String,
    pub service: ServiceStatus,
    pub strategies: Vec<Strategy>,
    pub operation: Operation,
}
