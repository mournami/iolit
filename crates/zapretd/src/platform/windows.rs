//! Windows SCM hosting and authenticated local named pipe transport.
use crate::{
    engine::winws::Winws,
    operation::OperationManager,
    selector::NetworkProbes,
    service::{Controller, Daemon},
    updater::{self, HttpFetch, Store, Updater},
};
use anyhow::{Context, Result, bail};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::windows::named_pipe::ServerOptions,
    sync::{Mutex, watch},
};
use windows_service::{
    define_windows_service,
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState,
        ServiceStatus as WinStatus, ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher,
};
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::{
        Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW, SECURITY_ATTRIBUTES,
    },
};
use zapret_core::api::*;
const NAME: &str = "ZapretDesktop";
define_windows_service!(ffi_main, service_main);

pub fn entry() -> Result<()> {
    // The uninstaller runs this before removing the service, so secure DNS
    // turned on in the app does not outlive it.
    if std::env::args().any(|a| a == "--restore-dns") {
        return crate::secure_dns::set(&data_dir()?, false);
    }
    if std::env::args().any(|a| a == "--console") {
        let (tx, rx) = watch::channel(false);
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            tokio::spawn(async move {
                let _ = tokio::signal::ctrl_c().await;
                let _ = tx.send(true);
            });
            run(rx).await
        })
    } else {
        service_dispatcher::start(NAME, ffi_main).context("Запускайте службу через установщик")
    }
}

fn service_main(_: Vec<OsString>) {
    let (tx, rx) = watch::channel(false);
    let handler = move |control| match control {
        ServiceControl::Stop | ServiceControl::Shutdown => {
            let _ = tx.send(true);
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    };
    let Ok(handle) = service_control_handler::register(NAME, handler) else {
        return;
    };
    let status = |state, exit| WinStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: if state == ServiceState::Running {
            ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN
        } else {
            ServiceControlAccept::empty()
        },
        exit_code: ServiceExitCode::Win32(exit),
        checkpoint: 0,
        wait_hint: Duration::from_secs(10),
        process_id: None,
    };
    let _ = handle.set_service_status(status(ServiceState::StartPending, 0));
    let outcome = tokio::runtime::Runtime::new()
        .map_err(anyhow::Error::from)
        .and_then(|rt| {
            rt.block_on(async {
                let _ = handle.set_service_status(status(ServiceState::Running, 0));
                run(rx).await
            })
        });
    let _ = handle.set_service_status(status(
        ServiceState::Stopped,
        if outcome.is_ok() { 0 } else { 1 },
    ));
}

struct Security {
    descriptor: *mut std::ffi::c_void,
    attributes: SECURITY_ATTRIBUTES,
}
impl Security {
    fn new() -> Result<Self> {
        unsafe {
            // Local SYSTEM/admins and interactive users; remote pipe clients are rejected.
            let text: Vec<u16> = "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;IU)"
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let mut descriptor = std::ptr::null_mut();
            if ConvertStringSecurityDescriptorToSecurityDescriptorW(
                text.as_ptr(),
                1,
                &mut descriptor,
                std::ptr::null_mut(),
            ) == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            let attributes = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: descriptor,
                bInheritHandle: 0,
            };
            Ok(Self {
                descriptor,
                attributes,
            })
        }
    }
}
impl Drop for Security {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.descriptor);
        }
    }
}

/// The data folder that belongs to this installation, see
/// [`zapret_core::windows_data_dir`].
fn data_dir() -> Result<PathBuf> {
    let exe = std::env::current_exe()?;
    // zapretd.exe lives in `<install>\runtime`.
    let install = exe
        .parent()
        .and_then(Path::parent)
        .context("Нет папки программы")?;
    let program_data = PathBuf::from(std::env::var_os("ProgramData").context("Нет ProgramData")?);
    Ok(zapret_core::windows_data_dir(&program_data, install))
}

async fn run(mut shutdown: watch::Receiver<bool>) -> Result<()> {
    // Executable paths come only from the installed service location.
    let install = std::env::current_exe()?
        .parent()
        .context("Нет каталога службы")?
        .to_path_buf();
    let data = data_dir()?;
    // Downloaded bundles stay under the admin-only install folder (see updater).
    let key = updater::public_key();
    let store = Store::new(install.join("bundles"), install.join("windows"));
    let (root, bundle, warning) = store.startup(key.as_ref());
    let mut controller = match Controller::new(root, data.clone(), Winws::default()) {
        Ok(controller) => controller,
        Err(e) => {
            let mut controller = Controller::new(install.join("windows"), data, Winws::default())?;
            controller.warning = Some(format!(
                "Скачанный набор стратегий не загрузился, используется встроенный: {e:#}"
            ));
            controller
        }
    };
    controller.bundle = bundle;
    if controller.warning.is_none() {
        controller.warning = warning;
    }
    let operations = Arc::new(OperationManager::new(controller.data.clone()));
    let controller = Arc::new(Mutex::new(controller));
    match operations.recover(&controller).await {
        Ok(false) => {
            let controller = &mut *controller.lock().await;
            if std::fs::read_to_string(controller.data.join("enabled"))
                .is_ok_and(|s| s.trim() == "1")
                && let Err(e) = controller.start().await
            {
                controller.warning = Some(e.to_string());
            }
        }
        Ok(true) => {}
        Err(_) => {} // Recovery failure is visible in snapshot; never enable a half-selected state.
    }
    if let Some(key) = key {
        match (HttpFetch::new(), updater::load_sources(&install)) {
            (Ok(fetch), Ok(sources)) => {
                let updater = Arc::new(Updater::new(
                    controller.clone(),
                    operations.clone(),
                    store,
                    fetch,
                    // Same checks before and after a bundle switch, so the
                    // results compare; the built-in list never changes mid-run.
                    Arc::new(NetworkProbes {
                        targets: zapret_core::targets::TargetList::built_in(),
                    }),
                    key,
                    sources,
                ));
                tokio::spawn(updater.run());
            }
            (fetch, sources) => {
                let error = fetch.err().or(sources.err()).map(|e| format!("{e:#}"));
                controller.lock().await.bundle.error = error;
            }
        }
        match HttpFetch::new() {
            Ok(fetch) => {
                let updater = Arc::new(crate::app_update::AppUpdater::new(
                    operations.clone(),
                    install.join(crate::app_update::DIR),
                    fetch,
                    key,
                    crate::app_update::DEFAULT_SOURCES
                        .map(String::from)
                        .to_vec(),
                    zapret_core::VERSION,
                ));
                tokio::spawn(updater.run());
            }
            Err(e) => operations.app.lock().await.error = Some(format!("{e:#}")),
        }
    }
    tokio::spawn(crate::health::run(
        operations.clone(),
        controller.clone(),
        |c: &mut Controller<Winws>| NetworkProbes {
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
    let create = |first: bool| -> Result<_> {
        let security = Security::new()?;
        Ok(unsafe {
            ServerOptions::new()
                .first_pipe_instance(first)
                .reject_remote_clients(true)
                .create_with_security_attributes_raw(
                    zapret_core::WINDOWS_PIPE,
                    &security.attributes as *const _ as *mut _,
                )
        }?)
    };
    let mut next = create(true)?;
    loop {
        tokio::select! {r=next.connect()=>r?,_=shutdown.changed()=>break}
        // Open the next instance before serving this client, so a parallel
        // request never finds the pipe without a listener (ERROR_PIPE_BUSY).
        let pipe = std::mem::replace(&mut next, create(false)?);
        let daemon = daemon.clone();
        tokio::spawn(async move {
            let outcome = tokio::time::timeout(Duration::from_secs(20), async {
                let mut stream = BufReader::new(pipe);
                let mut line = Vec::new();
                loop {
                    let buf = stream.fill_buf().await?;
                    if buf.is_empty() {
                        bail!("Пустой запрос");
                    }
                    let n = buf
                        .iter()
                        .position(|b| *b == b'\n')
                        .map(|i| i + 1)
                        .unwrap_or(buf.len());
                    if line.len() + n > zapret_core::ipc::MAX_MESSAGE {
                        bail!("Слишком большой запрос");
                    }
                    line.extend_from_slice(&buf[..n]);
                    stream.consume(n);
                    if line.last() == Some(&b'\n') {
                        break;
                    }
                }
                let result = match serde_json::from_slice::<DaemonCommand>(&line) {
                    Ok(req) => daemon.dispatch(req).await,
                    Err(e) => Err(e.into()),
                };
                let response = match result {
                    Ok(data) => ApiResponse {
                        ok: true,
                        data: Some(data),
                        error: None,
                    },
                    Err(e) => ApiResponse {
                        ok: false,
                        data: None,
                        error: Some(e.to_string()),
                    },
                };
                let mut data = serde_json::to_vec(&response)?;
                data.push(b'\n');
                stream.get_mut().write_all(&data).await?;
                Ok::<_, anyhow::Error>(())
            })
            .await;
            let _ = outcome;
        });
    }
    operations.cancel();
    // A running app update holds the gate until the installer stops this
    // service, which is happening now: waiting for it would hold the stop
    // until the update gives up.
    let _gate = if operations.app.lock().await.installing {
        None
    } else {
        Some(operations.gate.lock().await)
    };
    controller.lock().await.stop()?;
    Ok(())
}
