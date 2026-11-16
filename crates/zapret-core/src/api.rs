//! Shared daemon wire types. Field names are also the desktop UI contract.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Commands are fixed; no executable or filesystem path is accepted over IPC.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "cmd",
    content = "args",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum DaemonCommand {
    Snapshot,
    Enable,
    Disable,
    SetStrategy {
        strategy_id: String,
    },
    Diagnose,
    SelectStrategy,
    /// Selection only for the checks the background check found broken,
    /// keeping the current strategy for everything else.
    RepairStrategy,
    /// Runs the downloaded and verified installer of a newer application
    /// version (`Snapshot::app`). The service and the window restart.
    UpdateApp,
    CancelOperation,
    Doctor,
    AddDomain {
        domain: String,
    },
    /// Removes one domain from the user list; accepts what `AddDomain` accepts.
    RemoveDomain {
        domain: String,
    },
    /// The user list in readable form (Unicode for IDN). Never part of reports.
    Domains,
    Logs,
    Report,
    /// Cloudflare DNS over HTTPS on the active adapters (Windows 11); off
    /// restores each adapter's previous DNS settings.
    SetSecureDns {
        enabled: bool,
    },
    /// Local MTProto proxy for Telegram Desktop on `127.0.0.1`; independent of
    /// `Enable`/`Disable` and kept across restarts.
    SetTelegramProxy {
        enabled: bool,
    },
}

// One short-lived value per request; boxing the snapshot would buy nothing.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(untagged)]
pub enum DaemonResponse {
    Snapshot(Snapshot),
    Doctor(DoctorResult),
    Logs(Vec<String>),
    Domains(Vec<String>),
    Report(Report),
    Empty(()),
}

/// Retain the existing local IPC success/error envelope.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub struct ApiResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub data: Option<DaemonResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub struct DoctorCheck {
    pub name: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub struct DoctorResult {
    pub checks: Vec<DoctorCheck>,
}

/// Deliberate allowlist: no logs, domains, warnings or address-bearing probe details.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub struct Report {
    pub app_version: String,
    pub platform: String,
    pub engine: String,
    pub running: bool,
    pub strategy: String,
    pub capabilities: Capabilities,
    #[ts(type = "number")]
    pub packets: u64,
    #[ts(type = "number")]
    pub errors: u64,
    pub checks: Vec<ReportCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub struct ReportCheck {
    pub service: String,
    pub protocol: String,
    pub ok: bool,
    #[ts(type = "number")]
    pub latency_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub struct Strategy {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub supported: bool,
    #[serde(default)]
    pub arguments: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, TS)]
pub struct Capabilities {
    pub tcp: bool,
    pub udp: bool,
    pub ipv6: bool,
    pub packet_injection: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub struct ServiceStatus {
    pub installed: bool,
    pub running: bool,
    pub platform: String,
    pub engine: String,
    pub strategy: String,
    #[ts(type = "number")]
    pub uptime_secs: u64,
    pub capabilities: Capabilities,
    pub warnings: Vec<String>,
    #[ts(type = "number")]
    pub packets: u64,
    #[ts(type = "number")]
    pub modified_packets: u64,
    #[ts(type = "number")]
    pub errors: u64,
    #[serde(default)]
    pub secure_dns: SecureDns,
}

/// System-wide secure DNS set by the service, see `SetSecureDns`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, TS)]
pub struct SecureDns {
    /// Windows 11 or newer.
    pub supported: bool,
    /// Turned on by this app; the previous settings are saved.
    pub enabled: bool,
    /// Resolver that answered when it was turned on, e.g. "Cloudflare".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub provider: Option<String>,
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
            secure_dns: SecureDns::default(),
            errors: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub struct ProbeResult {
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub ok: bool,
    #[ts(type = "number")]
    pub latency_ms: u64,
    pub detail: String,
    /// The service answered, but something outside the bypass needs the
    /// user's attention (the system DNS did not know the address).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, TS)]
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

/// Strategy bundle in use and the state of signed bundle updates.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
pub struct BundleStatus {
    /// Downloaded bundle version; `None` while the installer's bundle is used.
    pub version: Option<String>,
    pub flowseal: Option<String>,
    /// False when this build has no bundle public key: updates never run.
    pub updates_enabled: bool,
    /// Unix seconds.
    #[ts(type = "number | null")]
    pub last_check: Option<u64>,
    /// Unix seconds of the last switch to a new bundle.
    #[ts(type = "number | null")]
    pub last_update: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
pub struct Snapshot {
    pub version: String,
    pub service: ServiceStatus,
    pub strategies: Vec<Strategy>,
    pub operation: Operation,
    #[serde(default)]
    pub bundle: BundleStatus,
    #[serde(default)]
    pub health: Health,
    #[serde(default)]
    pub app: AppUpdate,
    #[serde(default)]
    pub telegram: TelegramProxy,
}

/// Local proxy that carries Telegram Desktop's traffic to Telegram's own
/// WebSocket gateway, see `SetTelegramProxy`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
pub struct TelegramProxy {
    /// Turned on by the user; kept across restarts.
    pub enabled: bool,
    /// The local port is open.
    pub running: bool,
    /// `tg://proxy` link with this computer's secret. Never part of reports.
    pub link: Option<String>,
    /// Telegram connections open now.
    pub active: u32,
    /// Connections since the proxy started: through the WebSocket gateway,
    /// straight to a datacenter, and those that reached neither.
    pub websocket: u32,
    pub direct: u32,
    pub failed: u32,
    /// Why an enabled proxy is not running.
    pub error: Option<String>,
}

impl TelegramProxy {
    /// The link, if it is what the service makes: a local address and a hex
    /// secret. Checked before the desktop app hands it to the system.
    pub fn own_link(&self) -> Option<&str> {
        let link = self.link.as_deref()?;
        let rest = link.strip_prefix("tg://proxy?server=127.0.0.1&port=")?;
        let (port, secret) = rest.split_once("&secret=dd")?;
        (port.parse::<u16>().is_ok()
            && secret.len() == 32
            && secret.bytes().all(|b| b.is_ascii_hexdigit()))
        .then_some(link)
    }
}

/// Application updates: found and downloaded by the service, installed only
/// when the user asks (`UpdateApp`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct AppUpdate {
    /// Newer version downloaded and verified, waiting for `UpdateApp`.
    pub available: Option<String>,
    /// The installer is running; the service is about to restart.
    pub installing: bool,
    /// Unix seconds.
    #[ts(type = "number | null")]
    pub last_check: Option<u64>,
    pub error: Option<String>,
}

/// Background check while the bypass is on: every 10 minutes and after the
/// network changes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
pub struct Health {
    /// Unix time of the last background check; 0 before the first one.
    #[ts(type = "number")]
    pub checked_at: u64,
    /// Ids of checks that passed earlier with this strategy and fail now.
    pub broken: Vec<String>,
}

/// Schema root describes both directions of the daemon transport.
#[derive(JsonSchema)]
pub struct DaemonApi {
    pub request: DaemonCommand,
    pub response: ApiResponse,
}
