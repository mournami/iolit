//! Build-time bundle preparation. Replaces the Windows part of
//! `scripts/prepare_engines.py`; never executes batch files or upstream installers.
mod installer;
mod release;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use zapret_core::{bundle::extract_tar_gz, compiler};

const USAGE: &str = "usage:
  zbuild prepare-windows [--root DIR] [--flowseal-url URL --flowseal-sha256 HEX]
  zbuild bundle --version V --flowseal TAG [--root DIR] [--out DIR]
  zbuild sign --manifest FILE --out FILE       (key from ZAPRET_BUNDLE_SIGNING_KEY)
  zbuild public-key                            (key from ZAPRET_BUNDLE_SIGNING_KEY)
  zbuild verify --manifest FILE --archive FILE --public-key BASE64
  zbuild changes --base DIR --new DIR
  zbuild installer-payload --app EXE --service EXE --out FILE [--root DIR]";
/// service.bat creates these at first launch; we never run batch files.
const USER_LISTS: [&str; 3] = [
    "list-general-user.txt",
    "list-exclude-user.txt",
    "ipset-exclude-user.txt",
];

struct Options(HashMap<String, String>);

impl Options {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self> {
        let mut map = HashMap::new();
        while let Some(flag) = args.next() {
            let key = flag.strip_prefix("--").context(USAGE)?;
            let value = args.next().context(USAGE)?;
            if map.insert(key.to_owned(), value).is_some() {
                bail!("--{key} given twice");
            }
        }
        Ok(Self(map))
    }
    fn only(&self, allowed: &[&str]) -> Result<()> {
        match self.0.keys().find(|k| !allowed.contains(&k.as_str())) {
            Some(key) => bail!("Unknown option --{key}\n{USAGE}"),
            None => Ok(()),
        }
    }
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }
    fn required(&self, key: &str) -> Result<&str> {
        self.get(key)
            .with_context(|| format!("Missing --{key}\n{USAGE}"))
    }
    fn root(&self) -> Result<PathBuf> {
        Ok(match self.get("root") {
            Some(root) => root.into(),
            None => std::env::current_dir()?,
        })
    }
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let command = args.next().context(USAGE)?;
    let opts = Options::parse(args)?;
    match command.as_str() {
        "prepare-windows" => {
            opts.only(&["root", "flowseal-url", "flowseal-sha256"])?;
            let source = match (opts.get("flowseal-url"), opts.get("flowseal-sha256")) {
                (Some(url), Some(sha)) => Some((url, sha)),
                (None, None) => None,
                _ => bail!("--flowseal-url and --flowseal-sha256 go together"),
            };
            prepare_windows(&opts.root()?, source)
        }
        "bundle" => {
            opts.only(&["root", "version", "flowseal", "out"])?;
            let root = opts.root()?;
            let out = opts
                .get("out")
                .map(PathBuf::from)
                .unwrap_or_else(|| root.join("work/bundle"));
            release::bundle(
                &root,
                opts.required("version")?,
                opts.required("flowseal")?,
                &out,
            )
            .map(drop)
        }
        "sign" => {
            opts.only(&["manifest", "out"])?;
            let key = release::signing_key_from_env()?;
            release::sign_file(
                Path::new(opts.required("manifest")?),
                Path::new(opts.required("out")?),
                &key,
            )
        }
        "public-key" => {
            opts.only(&[])?;
            println!("{}", release::public_key(&release::signing_key_from_env()?));
            Ok(())
        }
        "verify" => {
            opts.only(&["manifest", "archive", "public-key"])?;
            release::verify_files_cli(
                Path::new(opts.required("manifest")?),
                Path::new(opts.required("archive")?),
                opts.required("public-key")?,
            )
        }
        "changes" => {
            opts.only(&["base", "new"])?;
            let notes = release::changes(
                Path::new(opts.required("base")?),
                Path::new(opts.required("new")?),
            )?;
            print!("{notes}");
            Ok(())
        }
        "installer-payload" => {
            opts.only(&["root", "app", "service", "out"])?;
            installer::payload(
                &opts.root()?,
                Path::new(opts.required("app")?),
                Path::new(opts.required("service")?),
                Path::new(opts.required("out")?),
            )
        }
        _ => bail!("{USAGE}"),
    }
}

/// Imports the pinned Flowseal release, or `source` (URL and SHA-256) when the
/// bundle workflow checks a newer one. The target folder is rebuilt from scratch.
fn prepare_windows(root: &Path, source: Option<(&str, &str)>) -> Result<()> {
    let lock: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("resources/upstream.lock.json"))?)?;
    let (url, digest) = match source {
        Some(source) => source,
        None => (
            lock["flowseal"]["url"]
                .as_str()
                .context("lock: flowseal.url")?,
            lock["flowseal"]["sha256"]
                .as_str()
                .context("lock: flowseal.sha256")?,
        ),
    };
    let digest = digest.to_ascii_lowercase();
    if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("Flowseal SHA-256 must be 64 hex characters");
    }
    let work = root.join("work");
    let runtime = root.join("resources/runtime");
    let target = runtime.join("windows");

    // One cache file per archive digest, so a pinned and a newer release never collide.
    let archive = work.join(format!("flowseal-{}.tar.gz", &digest[..16]));
    download(url, &archive, &digest)?;
    let extracted = work.join("flowseal");
    if extracted.exists() {
        fs::remove_dir_all(&extracted)?;
    }
    extract_tar_gz(&fs::read(&archive)?, &extracted)?;
    let source = single_release_root(&extracted)?;

    // Files from an earlier import must not leak into a newer bundle.
    if target.exists() {
        fs::remove_dir_all(&target)?;
    }
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
