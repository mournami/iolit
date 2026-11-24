//! Linux hosting: a plain foreground process for any init system (systemd,
//! OpenRC, runit) and a unix socket for the window and `iolitctl`.
//!
//! Layout (see the Linux plan): the program in `/opt/iolit` (`bin/zapretd`,
//! `runtime/linux`), data in [`zapret_core::LINUX_DATA`], the socket in
//! [`zapret_core::LINUX_SOCKET`]. Nothing depends on systemd: no sd_notify, no
//! socket activation, the service creates its own runtime folder.
use crate::{
    engine::nfqws::Nfqws,
    operation::OperationManager,
    selector::NetworkProbes,
    service::{Controller, Daemon},
};
use anyhow::{Context, Result, bail};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, chown},
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{
    net::UnixListener,
    signal::unix::{SignalKind, signal},
    sync::{Mutex, watch},
};

pub fn entry() -> Result<()> {
    if euid()? != 0 {
        bail!("Служба Iolit работает только от root. Установите её через install.sh.");
    }
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let (tx, rx) = watch::channel(false);
        let mut term = signal(SignalKind::terminate())?;
        let mut int = signal(SignalKind::interrupt())?;
        tokio::spawn(async move {
            tokio::select! { _ = term.recv() => {}, _ = int.recv() => {} }
            let _ = tx.send(true);
        });
        run(rx).await
    })
}

/// Effective user id from procfs, so the service needs no libc bindings.
fn euid() -> Result<u32> {
    let status = fs::read_to_string("/proc/self/status")?;
    parse_euid(&status).context("Не удалось узнать пользователя службы")
}

fn parse_euid(status: &str) -> Option<u32> {
    // "Uid:\treal\teffective\tsaved\tfs"
    let line = status.lines().find(|l| l.starts_with("Uid:"))?;
    line.split_whitespace().nth(2)?.parse().ok()
}

/// Group id of `name` from an `/etc/group`-format text.
fn parse_gid(group_file: &str, name: &str) -> Option<u32> {
    group_file.lines().find_map(|line| {
        let mut fields = line.split(':');
        (fields.next()? == name).then_some(())?;
        fields.nth(1)?.parse().ok()
    })
}

/// `/opt/iolit` for `/opt/iolit/bin/zapretd`.
fn install_dir() -> Result<PathBuf> {
    let exe = std::env::current_exe()?;
    exe.parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .context("Нет папки программы")
}

/// Binds the control socket: folder `root:iolit 0750`, socket `0660`. Without
/// the group only root may connect.
fn bind_socket() -> Result<UnixListener> {
    let socket = Path::new(zapret_core::LINUX_SOCKET);
    let folder = socket.parent().context("Нет папки сокета")?;
    fs::create_dir_all(folder)?;
    let gid = fs::read_to_string("/etc/group")
        .ok()
        .and_then(|text| parse_gid(&text, zapret_core::LINUX_GROUP));
    if gid.is_none() {
        eprintln!(
            "Группы {} нет: управлять службой сможет только root",
            zapret_core::LINUX_GROUP
        );
    }
    let mode = if gid.is_some() { 0o750 } else { 0o700 };
    chown(folder, Some(0), gid)?;
    fs::set_permissions(folder, fs::Permissions::from_mode(mode))?;
    // A socket left by a crashed run blocks bind; nobody else writes here.
    match fs::remove_file(socket) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
        _ => {}
    }
    let listener = UnixListener::bind(socket).context("Не удалось открыть сокет службы")?;
    chown(socket, Some(0), gid)?;
    // Connecting needs write access to the socket.
    let socket_mode = if gid.is_some() { 0o660 } else { 0o600 };
    fs::set_permissions(socket, fs::Permissions::from_mode(socket_mode))?;
    Ok(listener)
}

async fn run(mut shutdown: watch::Receiver<bool>) -> Result<()> {
    // Executable paths come only from the installed program folder.
    let install = install_dir()?;
    let data = PathBuf::from(zapret_core::LINUX_DATA);
    fs::create_dir_all(&data)?;
    // Settings, user domains and the Telegram proxy secret: root only.
    fs::set_permissions(&data, fs::Permissions::from_mode(0o700))?;
    // Strategy bundles for Linux are not published yet (step L1), so only the
    // built-in one is used and the bundle updater does not run. The program
    // itself is updated by a new archive, never by the service.
    let controller = Controller::new(install.join("runtime/linux"), data, Nfqws)?;
    let operations = Arc::new(OperationManager::new(controller.data.clone()));
    let controller = Arc::new(Mutex::new(controller));
    match operations.recover(&controller).await {
        Ok(false) => {
            let controller = &mut *controller.lock().await;
            if fs::read_to_string(controller.data.join("enabled")).is_ok_and(|s| s.trim() == "1")
                && let Err(e) = controller.start().await
            {
                controller.warning = Some(e.to_string());
            }
        }
        Ok(true) => {}
        Err(_) => {} // Recovery failure is visible in snapshot; never enable a half-selected state.
    }
    tokio::spawn(crate::health::run(
        operations.clone(),
        controller.clone(),
        |c: &mut Controller<Nfqws>| NetworkProbes {
            targets: c.targets.clone(),
        },
        shutdown.clone(),
    ));
    let telegram = Arc::new(crate::telegram::Manager::new(
        controller.lock().await.data.clone(),
    ));
    telegram.restore().await;
    let daemon = Arc::new(Daemon {
        controller: controller.clone(),
        operations: operations.clone(),
        telegram,
    });
    let listener = bind_socket()?;
    eprintln!("Служба Iolit {} запущена", zapret_core::VERSION);
    loop {
        let stream = tokio::select! {
            r = listener.accept() => r?.0,
            _ = shutdown.changed() => break,
        };
        tokio::spawn(super::serve::client(daemon.clone(), stream));
    }
    let _ = fs::remove_file(zapret_core::LINUX_SOCKET);
    super::serve::shutdown(&daemon, &operations).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_uid_is_the_second_field() {
        let status = "Name:\tzapretd\nUid:\t1000\t0\t0\t0\nGid:\t0\t0\t0\t0\n";
        assert_eq!(parse_euid(status), Some(0));
        assert_eq!(parse_euid("Name:\tx\n"), None);
    }

    #[test]
    fn group_id_is_found_by_exact_name() {
        let groups = "root:x:0:\niolit-old:x:900:\niolit:x:975:alice,bob\n";
        assert_eq!(parse_gid(groups, "iolit"), Some(975));
        assert_eq!(parse_gid(groups, "nobody"), None);
        assert_eq!(parse_gid("iolit:x:abc:\n", "iolit"), None);
    }
}
