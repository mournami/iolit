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
/// Linux service socket. The service creates the folder `root:iolit 0750` and
/// the socket `0660`, so members of the `iolit` group may connect.
pub const LINUX_SOCKET: &str = "/run/iolit/iolit.sock";
/// Linux data folder: settings, strategy, user domains, engine log.
pub const LINUX_DATA: &str = "/var/lib/iolit";
/// Group whose members may control the Linux service without sudo.
pub const LINUX_GROUP: &str = "iolit";
pub const WINDOWS_PIPE: &str = r"\\.\pipe\ZapretDesktop.Control.v1";
pub const MAC_DATA: &str = "/Library/Application Support/Zapret Desktop";

/// Data folder of a Windows installation inside `program_data`. `install` is
/// the folder that holds the app; the service runs from `install\runtime`.
/// Installations made under the old name keep `ZapretDesktop`, new ones use
/// `Iolit`. The choice follows the admin-only install path, never what exists
/// in ProgramData, where standard users may create folders. The installer
/// (`zsetup::payload::data_dir`) makes the same choice.
pub fn windows_data_dir(
    program_data: &std::path::Path,
    install: &std::path::Path,
) -> std::path::PathBuf {
    let old = install
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("Zapret Desktop"));
    program_data.join(if old { "ZapretDesktop" } else { "Iolit" })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    #[test]
    fn windows_data_folder_follows_the_install_folder() {
        let data = Path::new("data");
        let dir = |install: &str| super::windows_data_dir(data, Path::new(install));
        assert_eq!(dir("files/Zapret Desktop"), data.join("ZapretDesktop"));
        assert_eq!(dir("files/zapret desktop"), data.join("ZapretDesktop"));
        assert_eq!(dir("files/Iolit"), data.join("Iolit"));
        assert_eq!(dir("files/Zapret Desktop Other"), data.join("Iolit"));
    }
}
