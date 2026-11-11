//! Build-time bundle preparation. Replaces the Windows part of
//! `scripts/prepare_engines.py`; never executes batch files or upstream installers.
use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};
use zapret_core::compiler;

const USAGE: &str = "usage: zbuild prepare-windows [--root <project dir>]";
/// service.bat creates these at first launch; we never run batch files.
const USER_LISTS: [&str; 3] = [
    "list-general-user.txt",
    "list-exclude-user.txt",
    "ipset-exclude-user.txt",
];

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let command = args.next().context(USAGE)?;
    let mut root = std::env::current_dir()?;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => root = args.next().context(USAGE)?.into(),
            _ => bail!("{USAGE}"),
        }
    }
    match command.as_str() {
        "prepare-windows" => prepare_windows(&root),
        _ => bail!("{USAGE}"),
    }
}

fn prepare_windows(root: &Path) -> Result<()> {
    let lock: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("resources/upstream.lock.json"))?)?;
    let url = lock["flowseal"]["url"]
        .as_str()
        .context("lock: flowseal.url")?;
    let digest = lock["flowseal"]["sha256"]
        .as_str()
        .context("lock: flowseal.sha256")?;
    let work = root.join("work");
    let runtime = root.join("resources/runtime");
    let target = runtime.join("windows");

    let archive = work.join("flowseal.tar.gz");
    download(url, &archive, digest)?;
    let extracted = work.join("flowseal");
    if extracted.exists() {
        fs::remove_dir_all(&extracted)?;
    }
    extract_tar_gz(&fs::read(&archive)?, &extracted)?;
    let source = single_release_root(&extracted)?;

    fs::create_dir_all(&target)?;
    for name in ["bin", "lists"] {
        copy_tree(&source.join(name), &target.join(name))?;
    }
    for name in USER_LISTS {
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(target.join("lists").join(name))?;
    }

    let mut batches = Vec::new();
    for entry in fs::read_dir(&source)? {
        let entry = entry?;
        let name = entry.file_name().into_string().ok().unwrap_or_default();
        if name.starts_with("general") && name.ends_with(".bat") && entry.file_type()?.is_file() {
            batches.push((name, fs::read(entry.path())?));
        }
    }
    let catalog = compiler::compile_catalog(
        batches
            .iter()
            .map(|(name, bytes)| (name.as_str(), bytes.as_slice())),
    )?;
    for strategy in &catalog {
        for arg in &strategy.arguments {
            if let Some((_, file)) = arg.split_once("=@")
                && !target.join(file).is_file()
            {
                bail!("Missing payload: {arg}");
            }
        }
    }
    fs::write(
        target.join("strategies.json"),
        compiler::catalog_json(&catalog)?,
    )?;
    fs::copy(
        root.join("licenses/Flowseal-MIT.txt"),
        target.join("LICENSE-flowseal.txt"),
    )?;
    copy_tree(&root.join("licenses"), &runtime.join("licenses"))?;
    println!("Imported {} pinned Windows strategies", catalog.len());
    Ok(())
}

/// Reuse a cached archive if present; the digest is always checked.
fn download(url: &str, path: &Path, digest: &str) -> Result<()> {
    if !path.exists() {
        fs::create_dir_all(path.parent().context("archive path")?)?;
        let bytes = reqwest::blocking::Client::builder()
            .user_agent("ZapretDesktop-build")
            .build()?
            .get(url)
            .send()?
            .error_for_status()?
            .bytes()?;
        fs::write(path, &bytes)?;
    }
    let actual = hex::encode(Sha256::digest(fs::read(path)?));
    if actual != digest {
        bail!("SHA256 mismatch: {}", path.display());
    }
    Ok(())
}

/// Extract only regular files and directories that stay inside `dest`.
fn extract_tar_gz(bytes: &[u8], dest: &Path) -> Result<()> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        if path.as_os_str().is_empty()
            || path
                .components()
                .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
        {
            bail!("Unsafe archive path: {}", path.display());
        }
        let out = dest.join(&path);
        match entry.header().entry_type() {
            tar::EntryType::Directory => fs::create_dir_all(&out)?,
            tar::EntryType::Regular => {
                fs::create_dir_all(out.parent().context("archive file parent")?)?;
                let mut data = Vec::new();
                entry.read_to_end(&mut data)?;
                fs::write(&out, data)?;
            }
            // pax/GNU metadata headers carry no files of their own.
            tar::EntryType::XGlobalHeader | tar::EntryType::XHeader => {}
            other => bail!("Unsupported archive entry {other:?}: {}", path.display()),
        }
    }
    Ok(())
}

fn single_release_root(extracted: &Path) -> Result<PathBuf> {
    let mut found = Vec::new();
    find_files(extracted, "general.bat", &mut found)?;
    match found.as_slice() {
        [one] => Ok(one.parent().context("release root")?.to_path_buf()),
        _ => bail!("Unexpected flowseal archive structure"),
    }
}

fn find_files(dir: &Path, name: &str, found: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            find_files(&entry.path(), name, found)?;
        } else if kind.is_file() && entry.file_name() == name {
            found.push(entry.path());
        }
    }
    Ok(())
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from).with_context(|| from.display().to_string())? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let dest = to.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&entry.path(), &dest)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), dest)?;
        } else {
            bail!("Unsupported file type: {}", entry.path().display());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn archive(build: impl FnOnce(&mut tar::Builder<Vec<u8>>)) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        build(&mut builder);
        let tar = builder.into_inner().unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), Default::default());
        std::io::Write::write_all(&mut gz, &tar).unwrap();
        gz.finish().unwrap()
    }

    fn file(builder: &mut tar::Builder<Vec<u8>>, path: &str, data: &[u8]) {
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_entry_type(tar::EntryType::Regular);
        // Write the raw name so unsafe paths reach the extractor unchanged.
        let name = &mut header.as_old_mut().name;
        name[..path.len()].copy_from_slice(path.as_bytes());
        header.set_cksum();
        builder.append(&header, data).unwrap();
    }

    #[test]
    fn extracts_regular_files_and_finds_release_root() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = archive(|b| {
            file(b, "release/general.bat", b"x");
            file(b, "release/bin/a.bin", b"y");
        });
        extract_tar_gz(&bytes, dir.path()).unwrap();
        assert_eq!(
            single_release_root(dir.path()).unwrap(),
            dir.path().join("release")
        );
        assert_eq!(
            fs::read(dir.path().join("release/bin/a.bin")).unwrap(),
            b"y"
        );
    }

    #[test]
    fn rejects_traversal_absolute_paths_and_links() {
        for path in ["../outside", "/etc/outside", "a/../../outside"] {
            let dir = tempfile::tempdir().unwrap();
            let bytes = archive(|b| file(b, path, b"x"));
            assert!(extract_tar_gz(&bytes, dir.path()).is_err(), "{path}");
        }
        let dir = tempfile::tempdir().unwrap();
        let bytes = archive(|b| {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_size(0);
            b.append_link(&mut header, "release/link", "/etc/passwd")
                .unwrap();
        });
        assert!(extract_tar_gz(&bytes, dir.path()).is_err());
    }
}
