//! Golden inputs are data: read as text by the strategy compiler, never executed.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
use zapret_core::{Strategy, compiler, strategy};

#[derive(Deserialize)]
struct Source {
    version: String,
    archive_sha256: String,
    sources: Vec<BatchSource>,
}
#[derive(Deserialize)]
struct BatchSource {
    id: String,
    file: String,
    sha256: String,
}
#[derive(Deserialize)]
struct Arguments {
    id: String,
    arguments: Vec<String>,
}
fn digest(bytes: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn portable(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .trim_start_matches("//?/")
        .to_owned()
}

#[test]
fn pinned_flowseal_catalog_and_final_winws_arguments_match() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let golden = project.join("contracts/golden/flowseal-1.10.4");
    let source: Source =
        serde_json::from_slice(&fs::read(golden.join("source.json")).unwrap()).unwrap();
    let lock: serde_json::Value =
        serde_json::from_slice(&fs::read(project.join("resources/upstream.lock.json")).unwrap())
            .unwrap();
    assert_eq!(source.version, lock["flowseal"]["version"]);
    assert_eq!(source.archive_sha256, lock["flowseal"]["sha256"]);
    let expected: Vec<Strategy> =
        serde_json::from_slice(&fs::read(golden.join("strategies.json")).unwrap()).unwrap();
    let argv: Vec<Arguments> =
        serde_json::from_slice(&fs::read(golden.join("winws-argv.json")).unwrap()).unwrap();
    assert_eq!(source.sources.len(), 22);
    assert_eq!(expected.len(), source.sources.len());
    assert_eq!(argv.len(), expected.len());
    let batches: Vec<_> = source
        .sources
        .iter()
        .map(|input| {
            (
                input.file.as_str(),
                fs::read(golden.join(&input.file)).unwrap(),
            )
        })
        .collect();
    for ((input, strategy), (_, bytes)) in source.sources.iter().zip(&expected).zip(&batches) {
        assert_eq!(input.id, strategy.id);
        assert_eq!(digest(bytes), input.sha256);
    }
    // The Rust compiler must reproduce the pinned catalog byte for byte.
    let compiled = compiler::compile_catalog(
        batches
            .iter()
            .map(|(name, bytes)| (*name, bytes.as_slice())),
    )
    .unwrap();
    assert_eq!(
        compiler::catalog_json(&compiled).unwrap(),
        fs::read_to_string(golden.join("strategies.json")).unwrap()
    );
    let dir = tempfile::tempdir().unwrap();
    let bundle = dir.path().join("bundle with spaces");
    let data = dir.path().join("service data");
    fs::create_dir_all(&bundle).unwrap();
    fs::create_dir_all(&data).unwrap();
    fs::copy(
        golden.join("strategies.json"),
        bundle.join("strategies.json"),
    )
    .unwrap();
    // Resolve only paths referenced by the fixed JSON. No engine payload is executed.
    for s in &expected {
        for arg in &s.arguments {
            if let Some((_, relative)) = arg.split_once("=@") {
                let path = bundle.join(relative);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, []).unwrap();
            }
        }
    }
    let loaded = strategy::load_catalog(&bundle).unwrap();
    assert_eq!(
        serde_json::to_value(&loaded).unwrap(),
        serde_json::to_value(&expected).unwrap()
    );
    let root_path = portable(&bundle.canonicalize().unwrap());
    let data_path = portable(&data);
    for (s, expected) in loaded.iter().zip(argv) {
        assert_eq!(s.id, expected.id);
        let args =
            strategy::resolve_winws_arguments(s, &bundle, &data.join("user-domains.txt")).unwrap();
        let normalized: Vec<_> = args
            .iter()
            .map(|arg| {
                assert!(
                    !arg.contains(r"\\?\"),
                    "Cygwin cannot receive verbatim Windows paths"
                );
                arg.replace('\\', "/")
                    .replace(&root_path, "$BUNDLE")
                    .replace(&data_path, "$DATA")
            })
            .collect();
        assert_eq!(normalized, expected.arguments, "{}", s.id);
    }
}
