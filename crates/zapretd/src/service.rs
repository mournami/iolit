use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::{
    ffi::OsString,
    os::windows::{io::AsRawHandle, process::CommandExt},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
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
    Foundation::{CloseHandle, HANDLE, LocalFree},
    Security::{
        Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW, SECURITY_ATTRIBUTES,
    },
    System::{
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        },
        Threading::CREATE_NO_WINDOW,
    },
};
use zapret_core::{Capabilities, ServiceStatus, Strategy, strategy};

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

struct Job(HANDLE);
unsafe impl Send for Job {}
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
impl Job {
    fn new() -> Result<Self> {
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return Err(std::io::Error::last_os_error().into());
            }
            let job = Self(handle);
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of_val(&info) as u32,
            ) == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(job)
        }
    }
}

struct Controller {
    root: PathBuf,
    data: PathBuf,
    catalog: Vec<Strategy>,
    active: String,
    child: Option<Child>,
    job: Option<Job>,
    started: Option<Instant>,
    warning: Option<String>,
}
impl Controller {
    fn new() -> Result<Self> {
        // The service executable and immutable engines are installed under Program Files.
        // No request, environment variable or user preference can override an executable path.
        let root = std::env::current_exe()?
            .parent()
            .context("Нет каталога службы")?
            .join("windows");
        let catalog = strategy::load_catalog(&root)?;
        let data = PathBuf::from(std::env::var_os("ProgramData").context("Нет ProgramData")?)
            .join("ZapretDesktop");
        std::fs::create_dir_all(&data)?;
        let saved = std::fs::read_to_string(data.join("strategy")).unwrap_or_default();
        let active = catalog
            .iter()
            .find(|s| s.id == saved.trim())
            .or_else(|| catalog.first())
            .context("Нет стратегий")?
            .id
            .clone();
        Ok(Self {
            root,
            data,
            catalog,
            active,
            child: None,
            job: None,
            started: None,
            warning: None,
        })
    }
    fn status(&mut self) -> Result<ServiceStatus> {
        if let Some(child) = &mut self.child
            && let Some(exit) = child.try_wait()?
        {
            self.warning = Some(format!(
                "Движок завершился: {exit}. Проверьте конфликт с другим zapret или WinDivert."
            ));
            self.child = None;
            self.job = None;
            self.started = None;
        }
        Ok(ServiceStatus {
            installed: true,
            running: self.child.is_some(),
            platform: "windows".into(),
            engine: "winws / WinDivert".into(),
            strategy: self.active.clone(),
            uptime_secs: self.started.map(|s| s.elapsed().as_secs()).unwrap_or(0),
            capabilities: Capabilities {
                tcp: true,
                udp: true,
                ipv6: true,
                packet_injection: true,
            },
            warnings: self.warning.clone().into_iter().collect(),
            packets: 0,
            modified_packets: 0,
            errors: 0,
        })
    }
    fn stop(&mut self) -> Result<()> {
        if let Some(mut child) = self.child.take() {
            // Closing the job kills owned processes even if an explicit kill fails.
            drop(self.job.take());
            let _ = child.kill();
            child.wait()?;
        }
        self.job = None;
        self.started = None;
        Ok(())
    }
    async fn start(&mut self) -> Result<()> {
        if self.status()?.running {
            return Ok(());
        }
        let strategy = self
            .catalog
            .iter()
            .find(|s| s.id == self.active)
            .context("Стратегия не найдена")?;
        let mut args = strategy::resolve_arguments(strategy, &self.root)?;
        let list = self.data.join("user-domains.txt");
        if !list.exists() {
            std::fs::write(&list, b"")?;
        }
        // Imported profiles already reference the installed writable user list below.
        for arg in &mut args {
            if arg.contains("list-general-user.txt") {
                *arg = format!("--hostlist={}", list.display());
            }
        }
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.data.join("engine.log"))?;
        let job = Job::new()?;
        let mut child = Command::new(self.root.join("bin/winws.exe"))
            .args(&args)
            .current_dir(&self.root)
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .context("Не удалось запустить winws. Проверьте драйвер WinDivert и антивирус.")?;
        if unsafe { AssignProcessToJobObject(job.0, child.as_raw_handle() as HANDLE) } == 0 {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::last_os_error().into());
        }
        tokio::time::sleep(Duration::from_millis(800)).await;
        if let Some(exit) = child.try_wait()? {
            bail!("winws завершился при запуске: {exit}. Подробности в журнале.");
        }
        self.child = Some(child);
        self.job = Some(job);
        self.started = Some(Instant::now());
        self.warning = None;
        Ok(())
    }
    async fn dispatch(&mut self, request: Value) -> Result<Value> {
        let args = &request["args"];
        match request["cmd"].as_str().unwrap_or("") {
            "status" => Ok(serde_json::to_value(self.status()?)?),
            "list" => Ok(serde_json::to_value(&self.catalog)?),
            "start" => {
                self.start().await?;
                std::fs::write(self.data.join("enabled"), b"1")?;
                Ok(Value::Null)
            }
            "stop" => {
                let persisted = std::fs::write(self.data.join("enabled"), b"0");
                self.stop()?;
                persisted.context("Движок остановлен, но сохранить настройку не удалось")?;
                Ok(Value::Null)
            }
            "use" => {
                let id = args["name"].as_str().context("Не указана стратегия")?;
                if !strategy::valid_id(id) || !self.catalog.iter().any(|s| s.id == id) {
                    bail!("Неизвестная стратегия");
                }
                let old = self.active.clone();
                let running = self.status()?.running;
                self.stop()?;
                self.active = id.into();
                if running && let Err(e) = self.start().await {
                    self.active = old;
                    let _ = self.start().await;
                    return Err(e);
                }
                std::fs::write(self.data.join("strategy"), id)?;
                Ok(Value::Null)
            }
            "add-domain" => {
                let domain = strategy::normalize_domain(
                    args["domain"].as_str().context("Не указан домен")?,
                )?;
                let file = self.data.join("user-domains.txt");
                let mut text = std::fs::read_to_string(&file).unwrap_or_default();
                if !text.lines().any(|l| l == domain) {
                    text.push_str(&format!("{domain}\n"));
                    std::fs::write(file, text)?;
                }
                if self.status()?.running {
                    self.stop()?;
                    self.start().await?;
                }
                Ok(Value::Null)
            }
            "doctor" => {
                let s = self.status()?;
                Ok(
                    json!({"checks":[{"name":"Системная служба","ok":true,"detail":"Служба доступна"},{"name":"Движок","ok":s.running,"detail":if s.running{"Движок запущен"}else{"Движок выключен"}},{"name":"WinDivert","ok":self.root.join("bin/WinDivert64.sys").is_file(),"detail":"Проверка наличия драйвера. Работу подтверждает запуск движка."}]}),
                )
            }
            "logtail" => {
                let path = self.data.join("engine.log");
                let text = std::fs::read_to_string(path).unwrap_or_default();
                let lines: Vec<_> = text.lines().rev().take(80).collect();
                Ok(json!({"lines":lines.into_iter().rev().collect::<Vec<_>>()}))
            }
            _ => bail!("Неизвестная команда"),
        }
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        let _ = self.stop();
    }
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
    let mut controller = Controller::new()?;
    if std::fs::read_to_string(controller.data.join("enabled")).is_ok_and(|s| s.trim() == "1")
        && let Err(e) = controller.start().await
    {
        controller.warning = Some(e.to_string());
    }
    let controller = Arc::new(Mutex::new(controller));
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
        let controller = controller.clone();
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
                let result = match serde_json::from_slice::<Value>(&line) {
                    Ok(req) => controller.lock().await.dispatch(req).await,
                    Err(e) => Err(e.into()),
                };
                let response = match result {
                    Ok(data) => json!({"ok":true,"data":data}),
                    Err(e) => json!({"ok":false,"error":e.to_string()}),
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
    controller.lock().await.stop()?;
    Ok(())
}
