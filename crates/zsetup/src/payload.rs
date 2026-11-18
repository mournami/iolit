//! Platform-independent installer logic: the embedded payload, the record of
//! installed files and path checks. Everything here is tested on any OS.
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    io::Read,
    path::{Path, PathBuf},
};

/// Files every payload must contain, relative to the install folder.
pub const REQUIRED: [&str; 4] = [
    "zapret-desktop.exe",
    "runtime/zapretd.exe",
    "runtime/windows/strategies.json",
    "runtime/windows/bin/winws.exe",
];
/// Record of installed files, kept in the install folder.
pub const RECORD: &str = "install.json";
/// Folder in Program Files for new installations.
pub const INSTALL_DIR: &str = "Iolit";
/// Folder of installations made under the old name. They stay there: an
/// update replaces their files in place, and moving a working installation
/// waits for the internal rename.
pub const OLD_INSTALL_DIR: &str = "Zapret Desktop";

/// The install folder inside `program_files`: the old one if an installation
/// is there, the new one otherwise. Both are fixed names, never a path read
/// from the system.
pub fn install_dir(program_files: &Path) -> PathBuf {
    let old = program_files.join(OLD_INSTALL_DIR);
    let installed = [RECORD, "runtime/zapretd.exe"]
        .iter()
        .any(|file| join(&old, file).is_file());
    if installed {
        old
    } else {
        program_files.join(INSTALL_DIR)
    }
}

pub struct Entry {
    /// `/`-separated path relative to the install folder.
    pub path: String,
    pub data: Vec<u8>,
}

/// Parse the embedded `.tar.gz`. Only regular files with plain relative
/// paths are accepted; the gzip CRC catches a damaged download.
pub fn parse(bytes: &[u8]) -> Result<Vec<Entry>> {
    if bytes.is_empty() {
        bail!("Установщик собран без файлов программы");
    }
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    let mut entries = Vec::new();
    let mut seen = BTreeSet::new();
    for entry in archive.entries().context("Файлы установщика повреждены")?
    {
        let mut entry = entry.context("Файлы установщика повреждены")?;
        match entry.header().entry_type() {
            tar::EntryType::Regular => {}
            tar::EntryType::XGlobalHeader | tar::EntryType::XHeader => continue,
            other => bail!("Недопустимая запись в установщике: {other:?}"),
        }
        let path = entry.path()?.to_string_lossy().into_owned();
        if !valid_relative(&path) {
            bail!("Недопустимый путь в установщике: {path}");
        }
        if !seen.insert(path.to_lowercase()) {
            bail!("Повторный файл в установщике: {path}");
        }
        let mut data = Vec::new();
        entry
            .read_to_end(&mut data)
            .context("Файлы установщика повреждены")?;
        entries.push(Entry { path, data });
    }
    if let Some(missing) = REQUIRED.iter().find(|r| !seen.contains(**r)) {
        bail!("В установщике нет файла {missing}");
    }
    Ok(entries)
}

/// `a/b.txt`: non-empty plain components separated by `/`; no drive, root,
/// `.`/`..`, backslash or NTFS stream syntax.
pub fn valid_relative(path: &str) -> bool {
    !path.is_empty()
        && path.len() < 200
        && path.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && !part.ends_with(['.', ' '])
                && !part.contains(['\\', ':', '\0', '*', '?', '"', '<', '>', '|'])
        })
}

/// `root\a\b.txt` for a validated relative path.
pub fn join(root: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(root.to_path_buf(), |path, part| path.join(part))
}

#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub version: String,
    pub files: Vec<String>,
}

impl Record {
    pub fn new(version: &str, files: impl IntoIterator<Item = String>) -> Self {
        let mut files: Vec<_> = files.into_iter().collect();
        files.sort();
        Self {
            version: version.to_owned(),
            files,
        }
    }
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let record: Self = serde_json::from_slice(bytes).context("Список файлов повреждён")?;
        if let Some(bad) = record.files.iter().find(|f| !valid_relative(f)) {
            bail!("Недопустимый путь в списке файлов: {bad}");
        }
        Ok(record)
    }
    /// Files of this record that `next` no longer has. NTFS ignores case.
    pub fn stale<'a>(&'a self, next: &Record) -> Vec<&'a str> {
        let keep: BTreeSet<_> = next.files.iter().map(|f| f.to_lowercase()).collect();
        self.files
            .iter()
            .filter(|f| !keep.contains(&f.to_lowercase()))
            .map(String::as_str)
            .collect()
    }
}

/// Whether a service image path lies inside `dir`. Driver paths come as
/// `\??\C:\...`, sometimes quoted; comparison ignores case like NTFS.
pub fn image_inside(image: &str, dir: &Path) -> bool {
    let image = image.trim().trim_matches('"');
    let image = image
        .strip_prefix(r"\??\")
        .or_else(|| image.strip_prefix(r"\\?\"))
        .unwrap_or(image)
        .to_lowercase();
    let dir = dir.to_string_lossy();
    let dir = dir.trim_end_matches('\\').to_lowercase();
    !dir.is_empty() && image.starts_with(&dir) && image[dir.len()..].starts_with('\\')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn archive(files: &[(&str, &str)]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (path, data) in files {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Regular);
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            builder
                .append_data(&mut header, path, data.as_bytes())
                .unwrap();
        }
        let tar = builder.into_inner().unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        std::io::Write::write_all(&mut gz, &tar).unwrap();
        gz.finish().unwrap()
    }

    fn complete() -> Vec<(&'static str, &'static str)> {
        REQUIRED.iter().map(|p| (*p, "x")).collect()
    }

    #[test]
    fn payload_requires_the_app_service_and_engine() {
        let mut files = complete();
        files.push(("licenses/WinDivert-LICENSE", "license"));
        let entries = parse(&archive(&files)).unwrap();
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[4].data, b"license");
        assert!(parse(&archive(&complete()[1..])).is_err());
        assert!(parse(b"").is_err());
        let mut bytes = archive(&complete());
        let middle = bytes.len() / 2;
        bytes[middle] ^= 0xff;
        assert!(parse(&bytes).is_err(), "damaged payload must fail");
    }

    #[test]
    fn payload_paths_stay_inside_the_install_folder() {
        for bad in [
            "../x",
            "/x",
            "C:/x",
            "a//b",
            "a/./b",
            "a\\b",
            "a/b:stream",
            "a/b.",
            "",
        ] {
            assert!(!valid_relative(bad), "{bad}");
        }
        assert!(valid_relative("runtime/windows/lists/list-general.txt"));
        let mut files = complete();
        files.push(("RUNTIME/zapretd.exe", "twice"));
        assert!(
            parse(&archive(&files)).is_err(),
            "case-insensitive duplicate"
        );
        assert_eq!(
            join(Path::new("root"), "runtime/zapretd.exe"),
            Path::new("root").join("runtime").join("zapretd.exe")
        );
    }

    #[test]
    fn record_lists_files_removed_by_an_update() {
        let old = Record::new("0.1.0", ["a.txt", "B.txt", "c.txt"].map(String::from));
        let new = Record::new("0.1.1", ["b.txt", "c.txt", "d.txt"].map(String::from));
        assert_eq!(old.stale(&new), ["a.txt"]);
        let text = serde_json::to_vec(&new).unwrap();
        assert_eq!(Record::parse(&text).unwrap(), new);
        assert!(Record::parse(br#"{"version":"1","files":["../x"]}"#).is_err());
    }

    #[test]
    fn an_installation_under_the_old_name_keeps_its_folder() {
        let root = std::env::temp_dir().join(format!("zsetup-dir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let old = root.join(OLD_INSTALL_DIR);
        let new = root.join(INSTALL_DIR);
        std::fs::create_dir_all(old.join("runtime")).unwrap();
        // An empty or leftover folder is not an installation.
        std::fs::write(old.join("update-log.txt"), "").unwrap();
        assert_eq!(install_dir(&root), new);
        std::fs::write(old.join("runtime").join("zapretd.exe"), "").unwrap();
        assert_eq!(install_dir(&root), old);
        std::fs::remove_file(old.join("runtime").join("zapretd.exe")).unwrap();
        std::fs::write(old.join(RECORD), "{}").unwrap();
        assert_eq!(install_dir(&root), old);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn driver_image_must_be_inside_our_folder() {
        let dir = Path::new(r"C:\Program Files\Zapret Desktop");
        for inside in [
            r"\??\C:\Program Files\Zapret Desktop\runtime\windows\bin\WinDivert64.sys",
            r#""C:\program files\zapret desktop\runtime\windows\bin\WinDivert64.sys""#,
        ] {
            assert!(image_inside(inside, dir), "{inside}");
        }
        for outside in [
            r"\??\C:\Program Files\Zapret Desktop Other\WinDivert64.sys",
            r"\??\C:\GoodbyeDPI\WinDivert64.sys",
            r"C:\Program Files\Zapret Desktop",
        ] {
            assert!(!image_inside(outside, dir), "{outside}");
        }
    }
}
