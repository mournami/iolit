//! Flowseal `.bat` strategy compiler: the only place batch files are parsed.
//!
//! Batch files are read as text and never executed. Only the single
//! `start ... winws.exe <args>` line is used; everything else is ignored.
//! The output matches the phase-0 Python importer exactly (see the golden
//! fixtures), but unknown or ambiguous syntax is rejected instead of guessed.
use crate::{Strategy, strategy::valid_id};
use anyhow::{Context, Result, bail};
use std::collections::HashSet;

pub const IMPORTED_DESCRIPTION: &str =
    "Импортировано из flowseal; профили игры отключены по умолчанию.";

/// Compile one Flowseal batch file. `file_name` is the bare name, e.g. `general (ALT).bat`.
pub fn compile_batch(file_name: &str, bytes: &[u8]) -> Result<Strategy> {
    let name = file_name
        .strip_suffix(".bat")
        .filter(|stem| !stem.is_empty() && !stem.contains(['/', '\\']))
        .with_context(|| format!("Ожидался файл .bat: {file_name}"))?;
    let text = std::str::from_utf8(bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes))
        .with_context(|| format!("{file_name}: файл не в UTF-8"))?;
    let invocation = winws_invocation(text).with_context(|| file_name.to_owned())?;
    let tokens = tokenize(invocation).with_context(|| file_name.to_owned())?;
    let arguments = convert(tokens).with_context(|| file_name.to_owned())?;
    let id = strategy_id(name);
    if !valid_id(&id) {
        bail!("{file_name}: некорректный идентификатор стратегии {id:?}");
    }
    Ok(Strategy {
        id,
        name: name.to_owned(),
        description: IMPORTED_DESCRIPTION.to_owned(),
        supported: true,
        arguments,
    })
}

/// Compile a set of batch files into a catalog: `general` first, then by id.
pub fn compile_catalog<'a>(
    inputs: impl IntoIterator<Item = (&'a str, &'a [u8])>,
) -> Result<Vec<Strategy>> {
    let mut catalog = inputs
        .into_iter()
        .map(|(name, bytes)| compile_batch(name, bytes))
        .collect::<Result<Vec<_>>>()?;
    let mut ids = HashSet::new();
    if let Some(dup) = catalog.iter().find(|s| !ids.insert(s.id.clone())) {
        bail!("Повторный идентификатор стратегии {}", dup.id);
    }
    if catalog.is_empty() {
        bail!("Нет ни одной стратегии");
    }
    catalog.sort_by(|a, b| (a.id != "general", &a.id).cmp(&(b.id != "general", &b.id)));
    Ok(catalog)
}

/// Serialized `strategies.json`: two-space indent, UTF-8, trailing newline.
pub fn catalog_json(catalog: &[Strategy]) -> Result<String> {
    Ok(serde_json::to_string_pretty(catalog)? + "\n")
}

/// The argument text of the single `start ... winws.exe` line, after joining `^` continuations.
fn winws_invocation(text: &str) -> Result<String> {
    let joined = text.replace("^\r\n", " ").replace("^\n", " ");
    let mut found = None;
    for line in joined.split('\n') {
        let line = line.trim_start();
        let lower = line.to_ascii_lowercase();
        let Some(after) = lower.strip_prefix("start") else {
            continue;
        };
        if !after.starts_with(char::is_whitespace) {
            continue;
        }
        let Some(args) = arguments_after_winws(line, &lower) else {
            continue;
        };
        if found.replace(args.to_owned()).is_some() {
            bail!("Больше одного запуска winws.exe");
        }
    }
    found.context("Не найден запуск winws.exe")
}

fn arguments_after_winws<'a>(line: &'a str, lower: &str) -> Option<&'a str> {
    let mut from = "start".len();
    while let Some(offset) = lower[from..].find("winws.exe") {
        let mut end = from + offset + "winws.exe".len();
        if line[end..].starts_with('"') {
            end += 1;
        }
        let rest = &line[end..];
        if rest.starts_with(char::is_whitespace) {
            let args = rest.trim_start();
            if !args.trim_end().is_empty() {
                return Some(args);
            }
        }
        from = end;
    }
    None
}

/// Whitespace-separated tokens; `"..."` groups text and the quotes are dropped.
fn tokenize(text: String) -> Result<Vec<String>> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut started = false;
    for c in text.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    tokens.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            c => {
                current.push(c);
                started = true;
            }
        }
    }
    if quoted {
        bail!("Незакрытая кавычка в аргументах winws");
    }
    if started {
        tokens.push(current);
    }
    Ok(tokens)
}

fn convert(tokens: Vec<String>) -> Result<Vec<String>> {
    let mut profiles: Vec<Vec<String>> = vec![Vec::new()];
    for token in tokens {
        if token == "--new" {
            profiles.push(Vec::new());
        } else {
            profiles.last_mut().expect("never empty").push(token);
        }
    }
    let mut out = Vec::new();
    for profile in profiles {
        if profile.is_empty() {
            bail!("Пустой профиль между --new");
        }
        // Game profiles depend on service.bat state; they stay disabled by default.
        if profile.iter().any(|a| {
            a.starts_with("--filter-tcp=%GameFilter") || a.starts_with("--filter-udp=%GameFilter")
        }) {
            continue;
        }
        if !out.is_empty() {
            out.push("--new".to_owned());
        }
        for arg in profile {
            out.push(convert_argument(&arg)?);
        }
    }
    if out.is_empty() {
        bail!("Нет аргументов winws после фильтрации");
    }
    Ok(out)
}

fn convert_argument(arg: &str) -> Result<String> {
    let converted = strip_game_filter(arg)
        .replace("%BIN%", "@bin/")
        .replace("%LISTS%", "@lists/")
        .replace('\\', "/");
    if converted.contains('%') || !converted.starts_with("--") {
        bail!("Неизвестный синтаксис: {arg}");
    }
    let flag = converted[2..].split('=').next().unwrap_or_default();
    if flag.is_empty()
        || !flag
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        bail!("Недопустимый аргумент: {arg}");
    }
    Ok(converted)
}

/// Remove `%GameFilterTCP%` / `%GameFilterUDP%` and one comma right before each.
fn strip_game_filter(arg: &str) -> String {
    let mut out = arg.to_owned();
    for var in ["%GameFilterTCP%", "%GameFilterUDP%"] {
        while let Some(pos) = out.find(var) {
            let start = if out[..pos].ends_with(',') {
                pos - 1
            } else {
                pos
            };
            out.replace_range(start..pos + var.len(), "");
        }
    }
    out
}

fn strategy_id(name: &str) -> String {
    let lower = name.to_lowercase();
    let mut id = String::new();
    for c in lower.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            id.push(c);
        } else if !id.ends_with('-') {
            id.push('-');
        }
    }
    id.trim_matches('-').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bat(body: &str) -> Vec<u8> {
        format!("@echo off\r\nset \"BIN=%~dp0bin\\\"\r\n{body}\r\n").into_bytes()
    }

    #[test]
    fn joins_continuations_quotes_and_profiles() {
        let s = compile_batch(
            "general (ALT 2).bat",
            &bat("start \"zapret: %~n0\" /min \"%BIN%winws.exe\" --wf-tcp=80,%GameFilterTCP% ^\r\n--filter-tcp=80 --hostlist=\"%LISTS%list general.txt\" --new ^\r\n--filter-tcp=%GameFilterTCP% --dpi-desync=fake --new --dpi-desync-fake-tls=\"%BIN%tls.bin\""),
        )
        .unwrap();
        assert_eq!(s.id, "general-alt-2");
        assert_eq!(s.name, "general (ALT 2)");
        assert_eq!(
            s.arguments,
            [
                "--wf-tcp=80",
                "--filter-tcp=80",
                "--hostlist=@lists/list general.txt",
                "--new",
                "--dpi-desync-fake-tls=@bin/tls.bin",
            ]
        );
    }

    #[test]
    fn rejects_unknown_or_ambiguous_syntax() {
        for (case, body) in [
            ("no winws", "start notepad.exe --x=1"),
            (
                "unknown variable",
                "start \"\" winws.exe --hostlist=%USERPROFILE%\\x",
            ),
            ("bare word", "start \"\" winws.exe --x=1 & del x"),
            ("uppercase flag", "start \"\" winws.exe --Wf-tcp=80"),
            ("open quote", "start \"\" winws.exe --hostlist=\"x"),
            (
                "empty profile",
                "start \"\" winws.exe --x=1 --new --new --y=2",
            ),
            (
                "two launches",
                "start \"\" winws.exe --x=1\r\nstart \"\" winws.exe --y=1",
            ),
        ] {
            assert!(compile_batch("general.bat", &bat(body)).is_err(), "{case}");
        }
        assert!(compile_batch("general.cmd", &bat("start \"\" winws.exe --x=1")).is_err());
        assert!(compile_batch("../general.bat", &bat("start \"\" winws.exe --x=1")).is_err());
    }

    #[test]
    fn catalog_puts_general_first_and_rejects_duplicates() {
        let line = bat("start \"\" winws.exe --x=1");
        let catalog = compile_catalog([
            ("general (ALT).bat", line.as_slice()),
            ("general.bat", line.as_slice()),
        ])
        .unwrap();
        assert_eq!(catalog[0].id, "general");
        assert_eq!(catalog[1].id, "general-alt");
        assert!(
            compile_catalog([
                ("general (ALT).bat", line.as_slice()),
                ("general ALT.bat", line.as_slice()),
            ])
            .is_err()
        );
    }
}
