use crate::Strategy;
use anyhow::{Context, Result, bail};
use std::{
    collections::HashSet,
    path::{Component, Path},
};

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 100
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        && !id.contains("..")
        && !id.starts_with('.')
}

pub fn normalize_domain(input: &str) -> Result<String> {
    let input = input.trim().trim_end_matches('.');
    if input.is_empty() || input.contains(['/', ':', '@', '*', '\\', '\n', '\r']) {
        bail!("Введите домен без адреса страницы, например example.com");
    }
    let host = url::Host::parse(input).context("Некорректное имя домена")?;
    let url::Host::Domain(domain) = host else {
        bail!("Нужен домен, а не IP-адрес");
    };
    if domain.len() > 253
        || !domain.contains('.')
        || domain.split('.').any(|l| {
            l.is_empty()
                || l.len() > 63
                || l.starts_with('-')
                || l.ends_with('-')
                || !l.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
    {
        bail!("Некорректное имя домена");
    }
    Ok(domain.to_lowercase())
}

/// Readable form of a stored domain: `xn--e1afmkfd.xn--p1ai` → `пример.рф`.
/// `normalize_domain` turns it back into the stored form.
pub fn display_domain(domain: &str) -> String {
    match idna::domain_to_unicode(domain) {
        (unicode, Ok(())) => unicode,
        _ => domain.to_owned(),
    }
}

pub fn load_catalog(root: &Path) -> Result<Vec<Strategy>> {
    let catalog: Vec<Strategy> =
        serde_json::from_slice(&std::fs::read(root.join("strategies.json"))?)?;
    let mut ids = HashSet::new();
    for s in &catalog {
        if !ids.insert(&s.id) {
            bail!("Некорректный или повторный идентификатор стратегии");
        }
        validate(s)?;
    }
    Ok(catalog)
}

/// Checks a strategy from the bundle or the daemon's own composite file.
pub fn validate(s: &Strategy) -> Result<()> {
    if !valid_id(&s.id) {
        bail!("Некорректный или повторный идентификатор стратегии");
    }
    if s.arguments.is_empty() {
        bail!("Пустая стратегия {}", s.id);
    }
    for arg in &s.arguments {
        if !arg.starts_with("--") || arg.contains(['\n', '\r', '\0']) {
            bail!("Недопустимый аргумент стратегии");
        }
    }
    Ok(())
}

pub fn resolve_arguments(strategy: &Strategy, root: &Path) -> Result<Vec<String>> {
    let root = root.canonicalize()?;
    strategy
        .arguments
        .iter()
        .map(|arg| {
            if let Some((flag, file)) = arg.split_once("=@") {
                let path = Path::new(file);
                if path
                    .components()
                    .any(|c| !matches!(c, Component::Normal(_)))
                {
                    bail!("Выход за каталог набора запрещён");
                }
                let resolved = root.join(path).canonicalize()?;
                if !resolved.starts_with(&root) || !resolved.is_file() {
                    bail!("Файл стратегии находится вне набора");
                }
                let path = resolved.to_string_lossy();
                // Cygwin's argv/path conversion rejects Win32 verbatim prefixes.
                // Keep the canonical path for the containment check above; render only afterwards.
                let rendered = if cfg!(windows) {
                    conventional_windows_path(&path)
                } else {
                    path.into_owned()
                };
                Ok(format!("{flag}={rendered}"))
            } else {
                Ok(arg.clone())
            }
        })
        .collect()
}

/// Existing winws overlay: writable user domains replace the imported empty list.
pub fn resolve_winws_arguments(
    strategy: &Strategy,
    root: &Path,
    user_list: &Path,
) -> Result<Vec<String>> {
    let mut args = resolve_arguments(strategy, root)?;
    for arg in &mut args {
        if arg.contains("list-general-user.txt") {
            *arg = format!("--hostlist={}", user_list.display());
        }
    }
    Ok(args)
}

fn conventional_windows_path(path: &str) -> String {
    if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else {
        path.strip_prefix(r"\\?\").unwrap_or(path).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cygwin_arguments_use_conventional_windows_paths() {
        assert_eq!(
            conventional_windows_path(r"\\?\C:\Program Files\Zapret\list.txt"),
            r"C:\Program Files\Zapret\list.txt"
        );
        assert_eq!(
            conventional_windows_path(r"\\?\UNC\server\share\list.txt"),
            r"\\server\share\list.txt"
        );
        assert_eq!(
            conventional_windows_path(r"C:\Zapret\list.txt"),
            r"C:\Zapret\list.txt"
        );
    }
    #[test]
    fn domain_input_rejects_injection_and_ip() {
        for d in [
            "example.com\n--fake",
            "https://example.com",
            "127.0.0.1",
            "[::1]",
            "a..com",
            "*.com",
            "x.com;rm -rf /",
            "foo",
        ] {
            assert!(normalize_domain(d).is_err(), "{d}");
        }
        assert_eq!(
            normalize_domain("Пример.РФ.").unwrap(),
            "xn--e1afmkfd.xn--p1ai"
        );
        assert_eq!(normalize_domain("EXAMPLE.COM").unwrap(), "example.com");
        assert_eq!(display_domain("xn--e1afmkfd.xn--p1ai"), "пример.рф");
        assert_eq!(display_domain("example.com"), "example.com");
        assert_eq!(
            normalize_domain(&display_domain("xn--e1afmkfd.xn--p1ai")).unwrap(),
            "xn--e1afmkfd.xn--p1ai"
        );
    }
    #[test]
    fn paths_cannot_escape_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let s = Strategy {
            id: "test".into(),
            name: String::new(),
            description: String::new(),
            supported: true,
            arguments: vec!["--hostlist=@../outside".into()],
        };
        assert!(resolve_arguments(&s, dir.path()).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn symlink_cannot_escape_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("list")).unwrap();
        let s = Strategy {
            id: "test".into(),
            name: String::new(),
            description: String::new(),
            supported: true,
            arguments: vec!["--hostlist=@list".into()],
        };
        assert!(resolve_arguments(&s, dir.path()).is_err());
    }
}
