//! Doctor: turns facts the daemon collects about the computer into checks the
//! user can act on. No OS calls here; `zapretd` gathers the facts.
use crate::api::{DoctorCheck, SecureDns};
use std::path::Path;

/// What the daemon saw. Lists are empty where the platform has no such thing.
#[derive(Debug, Default, Clone)]
pub struct Facts {
    /// The user turned the bypass on (persisted setting).
    pub enabled: bool,
    pub engine_running: bool,
    pub engine_warnings: Vec<String>,
    /// `winws.exe` and `WinDivert64.sys` are in the bundle.
    pub engine_files: bool,
    /// Bundle folder our engine and driver run from.
    pub engine_dir: String,
    /// WinDivert driver services that exist.
    pub drivers: Vec<Driver>,
    /// Running processes of other bypass programs, see `KNOWN_PROCESSES`.
    pub processes: Vec<Process>,
    /// Services of other bypass programs, see `KNOWN_SERVICES`.
    pub services: Vec<Service>,
    /// Whether the system resolver found the YouTube address; `None`: not checked.
    pub dns: Option<bool>,
    pub secure_dns: SecureDns,
}

#[derive(Debug, Clone)]
pub struct Driver {
    pub name: String,
    pub running: bool,
    /// Image path as the service manager reports it, e.g. `\??\C:\...\WinDivert64.sys`.
    pub image: String,
}

#[derive(Debug, Clone)]
pub struct Process {
    /// Executable file name, e.g. `goodbyedpi.exe`.
    pub name: String,
    pub path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Service {
    pub name: String,
    pub running: bool,
}

/// Driver service names used by WinDivert 2.x and 1.4.
pub const DRIVER_SERVICES: [&str; 2] = ["WinDivert", "WinDivert14"];
/// Executables of bypass programs that also capture packets, with their names.
pub const KNOWN_PROCESSES: [(&str, &str); 4] = [
    ("goodbyedpi.exe", "GoodbyeDPI"),
    ("winws.exe", "zapret (winws) из другой папки"),
    ("ciadpi.exe", "ByeDPI"),
    ("spoofdpi.exe", "SpoofDPI"),
];
/// Services such programs install: Flowseal and bol-van zapret, GoodbyeDPI.
pub const KNOWN_SERVICES: [(&str, &str); 4] = [
    ("zapret", "служба zapret (Flowseal)"),
    ("winws1", "служба winws1 (zapret)"),
    ("winws2", "служба winws2 (zapret)"),
    ("GoodbyeDPI", "служба GoodbyeDPI"),
];

pub fn checks(f: &Facts) -> Vec<DoctorCheck> {
    let mut checks = vec![check("Системная служба", true, "Служба отвечает")];
    checks.push(engine(f));
    checks.push(if f.engine_files {
        check("Файлы движка", true, "winws и драйвер WinDivert на месте")
    } else {
        check(
            "Файлы движка",
            false,
            "Нет winws.exe или WinDivert64.sys. Антивирус мог удалить их: добавьте папку \
             программы в исключения и переустановите.",
        )
    });
    if let Some(driver) = driver(f) {
        checks.push(driver);
    }
    checks.push(conflicts(f));
    if let Some(dns) = dns(f) {
        checks.push(dns);
    }
    checks
}

fn check(name: &str, ok: bool, detail: impl Into<String>) -> DoctorCheck {
    DoctorCheck {
        name: name.into(),
        ok,
        detail: Some(detail.into()),
    }
}

fn engine(f: &Facts) -> DoctorCheck {
    match (f.enabled, f.engine_running) {
        (_, true) => check("Движок", true, "Запущен"),
        (true, false) => {
            let why = f.engine_warnings.join(" ");
            check(
                "Движок",
                false,
                format!(
                    "Обход включён, но движок не работает. {}",
                    if why.is_empty() {
                        "Включите обход заново на главной.".into()
                    } else {
                        why
                    }
                ),
            )
        }
        (false, false) => check("Движок", true, "Выключен, пока вы не включите обход"),
    }
}

fn driver(f: &Facts) -> Option<DoctorCheck> {
    if f.drivers.is_empty() && f.engine_dir.is_empty() {
        return None;
    }
    let dir = Path::new(&f.engine_dir);
    let foreign: Vec<_> = f
        .drivers
        .iter()
        .filter(|d| d.running && !path_inside(&d.image, dir))
        .map(|d| d.name.as_str())
        .collect();
    Some(if !foreign.is_empty() {
        check(
            "Драйвер WinDivert",
            false,
            format!(
                "Драйвер {} загружен другой программой. Закройте её или остановите WinDivert \
                 ярлыком «Iolit — остановить WinDivert», затем включите обход.",
                foreign.join(", ")
            ),
        )
    } else if f.drivers.iter().any(|d| d.running) {
        check("Драйвер WinDivert", true, "Загружен из папки программы")
    } else {
        check(
            "Драйвер WinDivert",
            true,
            "Не загружен; загрузится при включении обхода",
        )
    })
}

fn conflicts(f: &Facts) -> DoctorCheck {
    let dir = Path::new(&f.engine_dir);
    let mut running = Vec::new();
    for process in &f.processes {
        let Some((_, label)) = KNOWN_PROCESSES
            .iter()
            .find(|(exe, _)| process.name.eq_ignore_ascii_case(exe))
        else {
            continue;
        };
        // Our own winws runs from the bundle folder.
        let ours = process.name.eq_ignore_ascii_case("winws.exe")
            && process.path.as_deref().is_some_and(|p| path_inside(p, dir));
        if !ours && !running.contains(label) {
            running.push(*label);
        }
    }
    let mut stopped = Vec::new();
    for service in &f.services {
        if let Some((_, label)) = KNOWN_SERVICES
            .iter()
            .find(|(name, _)| service.name.eq_ignore_ascii_case(name))
        {
            if service.running {
                running.push(*label);
            } else {
                stopped.push(*label);
            }
        }
    }
    if !running.is_empty() {
        return check(
            "Другие программы обхода",
            false,
            format!(
                "Работает: {}. Две программы обхода мешают друг другу: закройте или удалите \
                 лишнюю.",
                running.join(", ")
            ),
        );
    }
    if !stopped.is_empty() {
        return check(
            "Другие программы обхода",
            true,
            format!(
                "Установлена, но остановлена: {}. Не запускайте её вместе с Iolit.",
                stopped.join(", ")
            ),
        );
    }
    check("Другие программы обхода", true, "Не найдены")
}

fn dns(f: &Facts) -> Option<DoctorCheck> {
    let resolves = f.dns?;
    let provider = f.secure_dns.provider.as_deref().unwrap_or("публичный DNS");
    Some(match (resolves, f.secure_dns.enabled) {
        (true, true) => check(
            "DNS",
            true,
            format!("Адреса приходят через защищённый DNS ({provider})"),
        ),
        (true, false) => check("DNS", true, "Адрес YouTube определяется"),
        (false, true) => check(
            "DNS",
            false,
            format!(
                "Защищённый DNS ({provider}) включён, но адрес YouTube не определяется. \
                 Выключите его в «Настройках» и включите снова."
            ),
        ),
        (false, false) if f.secure_dns.supported => check(
            "DNS",
            false,
            "Системный DNS не находит адрес YouTube, обычно его блокирует провайдер. \
             Включите защищённый DNS в «Настройках».",
        ),
        (false, false) => check(
            "DNS",
            false,
            "Системный DNS не находит адрес YouTube, обычно его блокирует провайдер. \
             Включите безопасный DNS в браузере.",
        ),
    })
}

/// Whether a path lies inside `dir`. Driver paths come as `\??\C:\...`,
/// sometimes quoted; comparison ignores case like NTFS. Same rule as the
/// installer's `image_inside`.
pub fn path_inside(path: &str, dir: &Path) -> bool {
    let path = path.trim().trim_matches('"');
    let path = path
        .strip_prefix(r"\??\")
        .or_else(|| path.strip_prefix(r"\\?\"))
        .unwrap_or(path)
        .replace('/', "\\")
        .to_lowercase();
    let dir = dir.to_string_lossy().replace('/', "\\");
    let dir = dir.trim_end_matches('\\').to_lowercase();
    !dir.is_empty() && path.starts_with(&dir) && path[dir.len()..].starts_with('\\')
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIR: &str = r"C:\Program Files\Zapret Desktop\runtime\windows";

    fn healthy() -> Facts {
        Facts {
            enabled: true,
            engine_running: true,
            engine_warnings: vec![],
            engine_files: true,
            engine_dir: DIR.into(),
            drivers: vec![Driver {
                name: "WinDivert".into(),
                running: true,
                image: format!(r"\??\{DIR}\bin\WinDivert64.sys"),
            }],
            processes: vec![Process {
                name: "winws.exe".into(),
                path: Some(format!(r"{DIR}\bin\winws.exe")),
            }],
            services: vec![],
            dns: Some(true),
            secure_dns: SecureDns::default(),
        }
    }

    fn find<'a>(checks: &'a [DoctorCheck], name: &str) -> &'a DoctorCheck {
        checks.iter().find(|c| c.name == name).unwrap()
    }

    #[test]
    fn healthy_computer_passes_every_check() {
        let checks = checks(&healthy());
        assert!(checks.iter().all(|c| c.ok), "{checks:?}");
        assert_eq!(checks.len(), 6);
    }

    #[test]
    fn foreign_driver_and_bypass_programs_are_conflicts() {
        let mut f = healthy();
        f.drivers[0].image = r"\??\C:\GoodbyeDPI\x86_64\WinDivert64.sys".into();
        f.processes.push(Process {
            name: "GoodbyeDPI.exe".into(),
            path: None,
        });
        f.processes.push(Process {
            name: "winws.exe".into(),
            path: Some(r"C:\zapret-discord-youtube\bin\winws.exe".into()),
        });
        f.services.push(Service {
            name: "zapret".into(),
            running: true,
        });
        let checks = checks(&f);
        assert!(!find(&checks, "Драйвер WinDivert").ok);
        let other = find(&checks, "Другие программы обхода");
        assert!(!other.ok);
        let detail = other.detail.as_deref().unwrap();
        for label in ["GoodbyeDPI", "из другой папки", "Flowseal"] {
            assert!(detail.contains(label), "{detail}");
        }
    }

    #[test]
    fn stopped_service_is_a_note_not_a_failure() {
        let mut f = healthy();
        f.services.push(Service {
            name: "GoodbyeDPI".into(),
            running: false,
        });
        let other = checks(&f)
            .into_iter()
            .find(|c| c.name == "Другие программы обхода")
            .unwrap();
        assert!(other.ok);
        assert!(other.detail.unwrap().contains("остановлена"));
    }

    #[test]
    fn engine_and_dns_explain_what_to_do() {
        let mut f = healthy();
        f.engine_running = false;
        f.engine_warnings = vec!["Движок завершился: код 1.".into()];
        f.dns = Some(false);
        f.secure_dns.supported = true;
        let checks = checks(&f);
        let engine = find(&checks, "Движок");
        assert!(!engine.ok && engine.detail.as_deref().unwrap().contains("код 1"));
        assert!(
            find(&checks, "DNS")
                .detail
                .as_deref()
                .unwrap()
                .contains("Включите защищённый DNS")
        );
        // Off by the user's choice is not a failure.
        let mut off = healthy();
        off.enabled = false;
        off.engine_running = false;
        assert!(find(&super::checks(&off), "Движок").ok);
        // No DNS fact (not Windows): no DNS row.
        off.dns = None;
        assert!(super::checks(&off).iter().all(|c| c.name != "DNS"));
    }

    #[test]
    fn paths_inside_the_bundle_folder() {
        let dir = Path::new(DIR);
        assert!(path_inside(
            r#""C:\program files\zapret desktop\runtime\windows\bin\WinDivert64.sys""#,
            dir
        ));
        assert!(!path_inside(
            r"C:\Program Files\Zapret Desktop\runtime\windows-old\x.sys",
            dir
        ));
        assert!(!path_inside(r"C:\x.sys", Path::new("")));
    }
}
