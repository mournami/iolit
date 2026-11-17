//! The full report behind «Сохранить отчёт». It covers the case the old
//! report could not: a service that does not answer. The app reads what an
//! ordinary user may read (service state, the install record, the update
//! log, Windows service events), adds what the service says when it answers,
//! and masks IP addresses and the user's domains before the text is saved.
use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use zapret_core::{ipc::request_command, redact::redact, *};

/// Enough for the tail of a log; keeps the report small enough to send.
const MAX_FILE: usize = 32 * 1024;

pub async fn build() -> String {
    let mut out = String::new();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    section(
        &mut out,
        "Iolit",
        format!("Версия приложения: {VERSION}\nВремя (unix): {now}"),
    );
    let install = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    local_facts(&mut out, install.as_deref()).await;

    let domains: Vec<String> = request_command(DaemonCommand::Domains)
        .await
        .unwrap_or_default();
    match request_command::<Snapshot>(DaemonCommand::Snapshot).await {
        Ok(mut snapshot) => {
            // Strategy arguments are long and known; the proxy link holds a secret.
            snapshot.strategies.clear();
            snapshot.telegram.link = None;
            section(&mut out, "Служба: состояние", pretty(&snapshot));
        }
        Err(e) => section(
            &mut out,
            "Служба: состояние",
            format!("Служба не ответила: {e:#}"),
        ),
    }
    match request_command::<DoctorResult>(DaemonCommand::Doctor).await {
        Ok(doctor) => section(&mut out, "Служба: Doctor", pretty(&doctor)),
        Err(e) => section(&mut out, "Служба: Doctor", format!("Не получено: {e:#}")),
    }
    let engine_log = match request_command::<Vec<String>>(DaemonCommand::Logs).await {
        Ok(lines) => lines.join("\n"),
        Err(e) => format!("Служба не отдала журнал ({e:#}). {}", read_engine_log()),
    };
    section(&mut out, "Журнал движка (хвост)", engine_log);
    redact(&out, &domains)
}

fn section(out: &mut String, title: &str, body: impl AsRef<str>) {
    out.push_str(&format!(
        "===== {title} =====\n{}\n\n",
        body.as_ref().trim_end()
    ));
}

fn pretty(value: &impl serde::Serialize) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|e| format!("не записалось: {e}"))
}

/// The last `MAX_FILE` bytes of a text file, or why it could not be read.
fn tail(path: &Path) -> String {
    match std::fs::read(path) {
        Ok(bytes) => {
            let start = bytes.len().saturating_sub(MAX_FILE);
            String::from_utf8_lossy(&bytes[start..]).into_owned()
        }
        Err(e) => format!("{}: не прочитан ({e})", path.display()),
    }
}

/// The service keeps the engine log in its data folder, which an ordinary
/// user may not be allowed to read.
fn read_engine_log() -> String {
    let Some(data) = std::env::var_os("ProgramData") else {
        return "Папка данных не найдена.".into();
    };
    tail(&PathBuf::from(data).join("ZapretDesktop").join("engine.log"))
}

#[cfg(windows)]
async fn local_facts(out: &mut String, install: Option<&Path>) {
    let system =
        PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()))
            .join("System32");
    let sc = system.join("sc.exe");
    section(
        out,
        "Windows",
        run(
            &system.join("reg.exe"),
            &[
                "query",
                r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion",
                "/v",
                "DisplayVersion",
            ],
        )
        .await
            + &run(
                &system.join("reg.exe"),
                &[
                    "query",
                    r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion",
                    "/v",
                    "CurrentBuild",
                ],
            )
            .await,
    );
    section(
        out,
        "Служба в Windows",
        run(&sc, &["query", "ZapretDesktop"]).await
            + &run(&sc, &["qc", "ZapretDesktop"]).await
            + &run(&sc, &["query", "WinDivert"]).await,
    );
    let Some(install) = install else {
        section(out, "Установка", "Папка программы не определена");
        return;
    };
    section(out, "Установка", install_record(install));
    // The service keeps downloads next to its own exe, in runtime\updates.
    let updates = install.join("runtime").join("updates");
    let listing = match std::fs::read_dir(&updates) {
        Ok(entries) => entries
            .flatten()
            .map(|e| {
                let size = e.metadata().map_or(0, |m| m.len());
                format!("{} ({size} байт)", e.file_name().to_string_lossy())
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Err(e) => format!("нет папки ({e})"),
    };
    section(
        out,
        "Обновление приложения",
        format!(
            "Файлы в updates:\n{listing}\n\nКопия прежней версии (.previous): {}\n\nЖурнал обновления:\n{}",
            if install.join(".previous").exists() {
                "есть"
            } else {
                "нет"
            },
            tail(&updates.join("update-log.txt"))
        ),
    );
    section(
        out,
        "События служб Windows (время местное, хоть и с пометкой Z)",
        service_events(&system).await,
    );
}

#[cfg(not(windows))]
async fn local_facts(out: &mut String, _install: Option<&Path>) {
    section(
        out,
        "Система",
        "Сведения о службе собираются только в Windows",
    );
}

/// Installed version and files of the record missing on disk.
#[cfg(windows)]
fn install_record(install: &Path) -> String {
    let path = install.join("install.json");
    let record: serde_json::Value = match std::fs::read(&path)
        .map_err(anyhow::Error::from)
        .and_then(|b| Ok(serde_json::from_slice(&b)?))
    {
        Ok(record) => record,
        Err(e) => return format!("{}: не прочитан ({e:#})", path.display()),
    };
    let version = record["version"].as_str().unwrap_or("?");
    let files: Vec<&str> = record["files"]
        .as_array()
        .map(|f| f.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    let missing: Vec<&&str> = files
        .iter()
        .filter(|f| !install.join(f.replace('/', "\\")).exists())
        .collect();
    format!(
        "Папка: {}\nУстановлена версия: {version}\nФайлов по списку: {}\nНет на диске: {}",
        install.display(),
        files.len(),
        if missing.is_empty() {
            "—".to_string()
        } else {
            missing
                .iter()
                .map(|f| f.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        }
    )
}

/// Service Control Manager events about this app's service and its driver.
#[cfg(windows)]
async fn service_events(system: &Path) -> String {
    let text = run(
        &system.join("wevtutil.exe"),
        &[
            "qe",
            "System",
            "/q:*[System[Provider[@Name='Service Control Manager']]]",
            "/c:200",
            "/rd:true",
            "/f:text",
            "/l:en-US",
        ],
    )
    .await;
    let events: Vec<String> = text
        .split("Event[")
        .filter(|e| e.contains("Zapret") || e.contains("Iolit") || e.contains("WinDivert"))
        .take(20)
        .map(|e| {
            e.lines()
                .map(str::trim)
                .filter(|l| {
                    l.starts_with("Date:")
                        || l.starts_with("Event ID:")
                        || l.contains("Zapret")
                        || l.contains("Iolit")
                        || l.contains("WinDivert")
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect();
    if events.is_empty() {
        let head: String = text.chars().take(300).collect();
        format!("Событий о службе нет. Начало ответа:\n{head}")
    } else {
        events.join("\n\n")
    }
}

/// Runs a fixed system tool without a console window, for at most 15 s.
#[cfg(windows)]
async fn run(program: &Path, args: &[&str]) -> String {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut command = tokio::process::Command::new(program);
    command
        .args(args)
        .stdin(std::process::Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .kill_on_drop(true);
    let limit = std::time::Duration::from_secs(15);
    match tokio::time::timeout(limit, command.output()).await {
        Ok(Ok(output)) => console_text(&output.stdout) + &console_text(&output.stderr),
        Ok(Err(e)) => format!("{} не запустился: {e}\n", program.display()),
        Err(_) => format!("{} не ответил за 15 с\n", program.display()),
    }
}

/// Console tools write in the OEM code page, CP866 on Russian Windows.
/// UTF-8 output stays as it is.
#[cfg_attr(not(windows), allow(dead_code))]
fn console_text(bytes: &[u8]) -> String {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_owned();
    }
    bytes
        .iter()
        .map(|&b| match b {
            0x00..=0x7F => b as char,
            // А..Я, а..п
            0x80..=0xAF => char::from_u32(0x0410 + u32::from(b - 0x80)).unwrap_or('?'),
            // р..я
            0xE0..=0xEF => char::from_u32(0x0440 + u32::from(b - 0xE0)).unwrap_or('?'),
            0xF0 => 'Ё',
            0xF1 => 'ё',
            // Box drawing and other symbols.
            _ => '?',
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::console_text;

    #[test]
    fn cp866_console_output_is_readable() {
        // "Имя_службы: ZapretDesktop" and "Состояние : 4 RUNNING" from sc.exe.
        let name = [
            0x88, 0xAC, 0xEF, 0x5F, 0xE1, 0xAB, 0xE3, 0xA6, 0xA1, 0xEB, 0x3A, 0x20,
        ];
        assert_eq!(console_text(&name), "Имя_службы: ");
        let state = [0x91, 0xAE, 0xE1, 0xE2, 0xAE, 0xEF, 0xAD, 0xA8, 0xA5, 0xF1];
        assert_eq!(console_text(&state), "Состояниеё");
        assert_eq!(console_text("уже UTF-8".as_bytes()), "уже UTF-8");
    }
}
