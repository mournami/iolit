//! Collects what Doctor needs from the operating system; the checks
//! themselves are in `zapret_core::doctor`. Reads only: nothing here stops,
//! changes or reports another program.
use std::{path::Path, time::Duration};
use zapret_core::doctor::{Driver, Facts, Process, Service};

/// Wait for the DNS answer; Doctor must stay well inside the request timeout.
const DNS_WAIT: Duration = Duration::from_secs(3);

/// Engine state comes from the controller; the rest is read here. Slow
/// (a DNS query), so callers run it on a blocking thread.
pub fn facts(root: &Path, data: &Path, mut engine: Facts) -> Facts {
    engine.engine_files =
        root.join("bin/winws.exe").is_file() && root.join("bin/WinDivert64.sys").is_file();
    engine.engine_dir = root.to_string_lossy().into_owned();
    engine.secure_dns = crate::secure_dns::state(data);
    engine.dns = crate::secure_dns::system_resolves("www.youtube.com", DNS_WAIT);
    let (drivers, services) = imp::services();
    engine.drivers = drivers;
    engine.services = services;
    engine.processes = imp::processes();
    engine
}

#[cfg(not(windows))]
mod imp {
    use super::{Driver, Process, Service};
    pub fn services() -> (Vec<Driver>, Vec<Service>) {
        (Vec::new(), Vec::new())
    }
    pub fn processes() -> Vec<Process> {
        Vec::new()
    }
}

#[cfg(windows)]
mod imp {
    use super::{Driver, Process, Service};
    use windows_service::{
        service::{ServiceAccess, ServiceState},
        service_manager::{ServiceManager, ServiceManagerAccess},
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                TH32CS_SNAPPROCESS,
            },
            Threading::{
                OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                QueryFullProcessImageNameW,
            },
        },
    };
    use zapret_core::doctor::{DRIVER_SERVICES, KNOWN_PROCESSES, KNOWN_SERVICES};

    pub fn services() -> (Vec<Driver>, Vec<Service>) {
        let Ok(manager) =
            ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        else {
            return (Vec::new(), Vec::new());
        };
        let open = |name: &str| {
            manager
                .open_service(
                    name,
                    ServiceAccess::QUERY_STATUS | ServiceAccess::QUERY_CONFIG,
                )
                .ok()
        };
        let running = |service: &windows_service::service::Service| {
            service
                .query_status()
                .is_ok_and(|s| s.current_state != ServiceState::Stopped)
        };
        let drivers = DRIVER_SERVICES
            .iter()
            .filter_map(|name| {
                let service = open(name)?;
                Some(Driver {
                    name: (*name).into(),
                    running: running(&service),
                    image: service
                        .query_config()
                        .map(|c| c.executable_path.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                })
            })
            .collect();
        let services = KNOWN_SERVICES
            .iter()
            .filter_map(|(name, _)| {
                let service = open(name)?;
                Some(Service {
                    name: (*name).into(),
                    running: running(&service),
                })
            })
            .collect();
        (drivers, services)
    }

    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }

    /// Running processes whose names are in `KNOWN_PROCESSES`, with paths.
    pub fn processes() -> Vec<Process> {
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return Vec::new();
        }
        let snapshot = Handle(snapshot);
        let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut found = Vec::new();
        let mut more = unsafe { Process32FirstW(snapshot.0, &mut entry) } != 0;
        while more {
            let len = entry.szExeFile.iter().position(|c| *c == 0).unwrap_or(0);
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
            if KNOWN_PROCESSES
                .iter()
                .any(|(exe, _)| name.eq_ignore_ascii_case(exe))
            {
                found.push(Process {
                    path: path(entry.th32ProcessID),
                    name,
                });
            }
            more = unsafe { Process32NextW(snapshot.0, &mut entry) } != 0;
        }
        found
    }

    fn path(pid: u32) -> Option<String> {
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process.is_null() {
            return None;
        }
        let process = Handle(process);
        let mut buffer = [0u16; 1024];
        let mut len = buffer.len() as u32;
        let ok = unsafe {
            QueryFullProcessImageNameW(process.0, PROCESS_NAME_WIN32, buffer.as_mut_ptr(), &mut len)
        };
        (ok != 0).then(|| String::from_utf16_lossy(&buffer[..len as usize]))
    }
}
