//! Payload of the Windows installer (`crates/zsetup`): the app, the service, the
//! prepared engine runtime and licenses, laid out as in the install folder.
use anyhow::{Context, Result, bail};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use zapret_core::bundle;

/// Payload paths (`/`-separated, relative to the install folder) and their sources.
fn files(root: &Path, app: &Path, service: &Path) -> Result<BTreeMap<String, PathBuf>> {
    let mut files = BTreeMap::new();
    files.insert("zapret-desktop.exe".to_owned(), app.to_path_buf());
    files.insert("runtime/zapretd.exe".to_owned(), service.to_path_buf());
    files.insert(
        "iolit.ico".to_owned(),
        root.join("src-tauri/icons/icon.ico"),
    );
    bundle::walk(
        &root.join("resources/runtime/windows"),
        "runtime/windows",
        &mut files,
    )?;
    bundle::walk(&root.join("licenses"), "licenses", &mut files)?;
    // GPLv3 requires a copy of the license with every binary we distribute.
    files.insert("LICENSE.txt".to_owned(), root.join("LICENSE"));
    if !root.join("LICENSE").is_file() {
        bail!("No LICENSE in {}", root.display());
    }
    for required in [
        "runtime/windows/strategies.json",
        "runtime/windows/bin/winws.exe",
    ] {
        if !files.contains_key(required) {
            bail!("No prepared runtime ({required}): run `zbuild prepare-windows` first");
        }
    }
    Ok(files)
}

/// Writes the deterministic `.tar.gz` that `zsetup` embeds at build time.
pub fn payload(root: &Path, app: &Path, service: &Path, out: &Path) -> Result<()> {
    for exe in [app, service] {
        if !exe.is_file() {
            bail!("Not a file: {}", exe.display());
        }
    }
    let files = files(root, app, service)?;
    let mut entries = Vec::new();
    for (path, source) in &files {
        let data = fs::read(source).with_context(|| source.display().to_string())?;
        entries.push((path.as_str(), data));
    }
    let bytes = bundle::pack_archive(entries.iter().map(|(p, d)| (*p, d.as_slice())))?;
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(out, &bytes).with_context(|| out.display().to_string())?;
    println!(
        "Installer payload: {} files, {} bytes -> {}",
        files.len(),
        bytes.len(),
        out.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn payload_mirrors_the_install_folder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for (path, data) in [
            ("resources/runtime/windows/strategies.json", "[]"),
            ("resources/runtime/windows/bin/winws.exe", "winws"),
            ("resources/runtime/windows/lists/list-general-user.txt", ""),
            ("src-tauri/icons/icon.ico", "icon"),
            ("licenses/WinDivert-LICENSE", "license"),
            ("LICENSE", "gpl"),
            ("app.exe", "app"),
            ("zapretd.exe", "service"),
        ] {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, data).unwrap();
        }
        let out = root.join("payload.tar.gz");
        payload(root, &root.join("app.exe"), &root.join("zapretd.exe"), &out).unwrap();
        let bytes = fs::read(&out).unwrap();
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes.as_slice()));
        let mut found = Vec::new();
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            let mut data = String::new();
            entry.read_to_string(&mut data).unwrap();
            found.push((entry.path().unwrap().display().to_string(), data));
        }
        assert_eq!(
            found,
            [
                ("LICENSE.txt", "gpl"),
                ("iolit.ico", "icon"),
                ("licenses/WinDivert-LICENSE", "license"),
                ("runtime/windows/bin/winws.exe", "winws"),
                ("runtime/windows/lists/list-general-user.txt", ""),
                ("runtime/windows/strategies.json", "[]"),
                ("runtime/zapretd.exe", "service"),
                ("zapret-desktop.exe", "app"),
            ]
            .map(|(p, d)| (p.to_owned(), d.to_owned()))
        );
        // Deterministic: the same inputs give the same bytes.
        payload(root, &root.join("app.exe"), &root.join("zapretd.exe"), &out).unwrap();
        assert_eq!(fs::read(&out).unwrap(), bytes);
        fs::remove_file(root.join("LICENSE")).unwrap();
        assert!(payload(root, &root.join("app.exe"), &root.join("zapretd.exe"), &out).is_err());
        fs::write(root.join("LICENSE"), "gpl").unwrap();
        fs::remove_file(root.join("resources/runtime/windows/bin/winws.exe")).unwrap();
        assert!(payload(root, &root.join("app.exe"), &root.join("zapretd.exe"), &out).is_err());
    }
}
