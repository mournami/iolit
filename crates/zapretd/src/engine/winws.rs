//! Existing winws process and Job Object lifecycle, moved from service.rs.
use super::{Engine, EngineState};
use crate::logging::log_tail;
use anyhow::{Context, Result, bail};
use std::{
    os::windows::{io::AsRawHandle, process::CommandExt},
    path::Path,
    process::{Child, Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::{
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        },
        Threading::CREATE_NO_WINDOW,
    },
};
use zapret_core::{Capabilities, Strategy, strategy};
// Cygwin binaries reliably inherit pipes. Drain both streams even when logging
// fails, and bound the on-disk diagnostic data independently of engine output.
fn capture_output<R: std::io::Read + Send + 'static>(
    mut input: R,
    log: Arc<std::sync::Mutex<std::fs::File>>,
) {
    std::thread::spawn(move || {
        use std::io::{Seek, SeekFrom, Write};
        let mut bytes = [0u8; 4096];
        loop {
            let n = match input.read(&mut bytes) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            if let Ok(mut file) = log.lock() {
                if file.metadata().map(|m| m.len()).unwrap_or(0) + n as u64 > 2 * 1024 * 1024 {
                    let _ = file.set_len(0);
                    let _ = file.seek(SeekFrom::Start(0));
                    let _ = file.write_all(b"[Older engine output truncated]\n");
                }
                let _ = file.write_all(&bytes[..n]);
            }
        }
    });
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

#[derive(Default)]
pub struct Winws {
    child: Option<Child>,
    job: Option<Job>,
    started: Option<Instant>,
    warning: Option<String>,
}
impl Engine for Winws {
    const PLATFORM: &'static str = "windows";
    const NAME: &'static str = "winws / WinDivert";
    const CAPABILITIES: Capabilities = Capabilities {
        tcp: true,
        udp: true,
        ipv6: true,
        packet_injection: true,
    };
    fn is_ready(&mut self) -> Result<EngineState> {
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
        Ok(EngineState {
            running: self.child.is_some(),
            uptime_secs: self.started.map(|s| s.elapsed().as_secs()).unwrap_or(0),
            warnings: self.warning.clone().into_iter().collect(),
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
    async fn start(&mut self, strategy: &Strategy, root: &Path, data: &Path) -> Result<()> {
        if self.is_ready()?.running {
            return Ok(());
        }
        let list = data.join("user-domains.txt");
        let args = strategy::resolve_winws_arguments(strategy, root, &list)?;
        if !list.exists() {
            std::fs::write(&list, b"")?;
        }
        let log = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(data.join("engine.log"))?;
        let log = Arc::new(std::sync::Mutex::new(log));
        let job = Job::new()?;
        let mut child = Command::new(root.join("bin/winws.exe"))
            .args(&args)
            .current_dir(root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .context("Не удалось запустить winws. Проверьте драйвер WinDivert и антивирус.")?;
        if let Some(stdout) = child.stdout.take() {
            capture_output(stdout, log.clone());
        }
        if let Some(stderr) = child.stderr.take() {
            capture_output(stderr, log);
        }
        if unsafe { AssignProcessToJobObject(job.0, child.as_raw_handle() as HANDLE) } == 0 {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::last_os_error().into());
        }
        tokio::time::sleep(Duration::from_millis(800)).await;
        if let Some(exit) = child.try_wait()? {
            let details = log_tail(&data.join("engine.log")).join("\n");
            bail!("winws завершился при запуске: {exit}.\n{details}");
        }
        self.child = Some(child);
        self.job = Some(job);
        self.started = Some(Instant::now());
        self.warning = None;
        Ok(())
    }
    fn cleanup(&mut self) -> Result<()> {
        self.stop()
    }
}
impl Drop for Winws {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}
