use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

pub const MAX_MESSAGE: usize = 1024 * 1024;

pub async fn request(command: &str, args: Value) -> Result<Value> {
    tokio::time::timeout(Duration::from_secs(20), exchange(command, args))
        .await
        .context("Служба не отвечает. Проверьте её установку или перезапустите компьютер.")?
}

async fn exchange(command: &str, args: Value) -> Result<Value> {
    #[cfg(unix)]
    let stream = tokio::net::UnixStream::connect(crate::MAC_SOCKET)
        .await
        .context("Служба Zapret не установлена или недоступна. Запустите установщик службы.")?;
    #[cfg(target_os = "macos")]
    if stream.peer_cred()?.uid() != 0 {
        bail!("Ответил процесс без системных прав. Соединение отклонено.");
    }
    #[cfg(windows)]
    let stream = tokio::net::windows::named_pipe::ClientOptions::new()
        .open(crate::WINDOWS_PIPE)
        .context("Служба Zapret не установлена или не запущена. Повторите установку приложения.")?;
    #[cfg(windows)]
    verify_windows_server(&stream)?;
    let mut stream = BufReader::new(stream);
    let mut message = serde_json::to_vec(&serde_json::json!({"cmd": command, "args": args}))?;
    message.push(b'\n');
    stream.get_mut().write_all(&message).await?;
    let mut line = Vec::new();
    // fill_buf caps allocations even when the peer omits the delimiter.
    loop {
        let buffer = stream.fill_buf().await?;
        if buffer.is_empty() {
            bail!("Служба закрыла соединение без ответа");
        }
        let end = buffer
            .iter()
            .position(|x| *x == b'\n')
            .map(|i| i + 1)
            .unwrap_or(buffer.len());
        if line.len() + end > MAX_MESSAGE {
            bail!("Ответ службы слишком большой");
        }
        line.extend_from_slice(&buffer[..end]);
        stream.consume(end);
        if line.last() == Some(&b'\n') {
            break;
        }
    }
    let response: Value = serde_json::from_slice(&line).context("Неверный ответ службы")?;
    if response["ok"] != true {
        bail!(
            "{}",
            response["error"]
                .as_str()
                .unwrap_or("Неизвестная ошибка службы")
        );
    }
    Ok(response.get("data").cloned().unwrap_or(Value::Null))
}

#[cfg(windows)]
fn verify_windows_server(stream: &tokio::net::windows::named_pipe::NamedPipeClient) -> Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::{
        Foundation::HANDLE,
        System::{
            Pipes::GetNamedPipeServerProcessId,
            Services::{
                CloseServiceHandle, OpenSCManagerW, OpenServiceW, QueryServiceStatusEx, SC_HANDLE,
                SC_MANAGER_CONNECT, SC_STATUS_PROCESS_INFO, SERVICE_QUERY_STATUS,
                SERVICE_STATUS_PROCESS,
            },
        },
    };
    struct ServiceHandle(SC_HANDLE);
    impl Drop for ServiceHandle {
        fn drop(&mut self) {
            unsafe {
                CloseServiceHandle(self.0);
            }
        }
    }
    unsafe {
        let mut pid = 0;
        if GetNamedPipeServerProcessId(stream.as_raw_handle() as HANDLE, &mut pid) == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let manager = ServiceHandle(OpenSCManagerW(
            std::ptr::null(),
            std::ptr::null(),
            SC_MANAGER_CONNECT,
        ));
        if manager.0.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        let name: Vec<u16> = "ZapretDesktop".encode_utf16().chain(Some(0)).collect();
        let service = ServiceHandle(OpenServiceW(manager.0, name.as_ptr(), SERVICE_QUERY_STATUS));
        if service.0.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut status: SERVICE_STATUS_PROCESS = std::mem::zeroed();
        let mut size = 0;
        if QueryServiceStatusEx(
            service.0,
            SC_STATUS_PROCESS_INFO,
            &mut status as *mut _ as *mut u8,
            std::mem::size_of_val(&status) as u32,
            &mut size,
        ) == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        if pid == 0 || pid != status.dwProcessId {
            bail!(
                "Ответил процесс, который не является установленной системной службой. Соединение отклонено."
            );
        }
    }
    Ok(())
}
