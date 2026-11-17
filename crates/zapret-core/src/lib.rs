pub mod app_update;
pub mod bundle;
pub mod compiler;
pub mod composite;
pub mod doctor;
pub mod ipc;
#[cfg(feature = "network")]
pub mod probe;
pub mod redact;
pub mod strategy;
pub mod targets;

pub mod api;
pub use api::*;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const MAC_SOCKET: &str = "/var/run/zapret-desktop/control.sock";
pub const WINDOWS_PIPE: &str = r"\\.\pipe\ZapretDesktop.Control.v1";
pub const MAC_DATA: &str = "/Library/Application Support/Zapret Desktop";
