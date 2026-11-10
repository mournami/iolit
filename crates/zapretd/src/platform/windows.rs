//! Windows SCM hosting and authenticated local named pipe transport.
use crate::{
    engine::winws::Winws,
    operation::OperationManager,
    service::{Controller, Daemon},
};
use anyhow::{Context, Result, bail};
use std::{ffi::OsString, path::PathBuf, sync::Arc, time::Duration};
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

async fn run(mut shutdown: watch::Receiver<bool>) -> Result<()> {
    // Executable paths come only from the installed service location.
    let root = std::env::current_exe()?
        .parent()
        .context("Нет каталога службы")?
        .join("windows");
    let data = PathBuf::from(std::env::var_os("ProgramData").context("Нет ProgramData")?)
        .join("ZapretDesktop");
    let controller = Controller::new(root, data, Winws::default())?;
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
    let daemon = Arc::new(Daemon {
        controller: controller.clone(),
        operations: operations.clone(),
    });
    let mut first = true;
    loop {
        let pipe = {
            let security = Security::new()?;
            unsafe {
                ServerOptions::new()
                    .first_pipe_instance(first)
                    .reject_remote_clients(true)
                    .create_with_security_attributes_raw(
                        zapret_core::WINDOWS_PIPE,
                        &security.attributes as *const _ as *mut _,
                    )
            }?
        };
        first = false;
        tokio::select! {r=pipe.connect()=>r?,_=shutdown.changed()=>break}
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
    let _gate = operations.gate.lock().await;
    controller.lock().await.stop()?;
    Ok(())
}
