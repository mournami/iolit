use anyhow::{Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signer, SigningKey};
use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::{Arc, Mutex as SyncMutex},
};
use tokio::sync::Mutex;
use zapret_core::{
    Capabilities, ProbeResult, Strategy,
    bundle::{self, FileHash, MANIFEST_SCHEMA, Manifest, SignedManifest},
};
use zapretd::{
    engine::{Engine, EngineState},
    operation::OperationManager,
    selector::{Backend, Probes},
    service::Controller,
    updater::{Fetch, Outcome, Store, Updater},
};

/// Test-only seed; never used for real bundles.
const TEST_ONLY_SEED: [u8; 32] = [0x5a; 32];
const SOURCE: &str = "https://mirror.test/";

#[derive(Clone, Default)]
struct TestEngine {
    running: bool,
    started: Arc<SyncMutex<Vec<PathBuf>>>,
}
impl Engine for TestEngine {
    const PLATFORM: &'static str = "test";
    const NAME: &'static str = "test engine";
    const CAPABILITIES: Capabilities = Capabilities {
        tcp: true,
        udp: true,
        ipv6: false,
        packet_injection: false,
    };
    async fn start(&mut self, _: &Strategy, root: &Path, _: &Path) -> Result<()> {
        // A bundle carrying `fail` simulates an engine that does not start.
        if root.join("fail").exists() {
            bail!("engine did not start");
        }
        self.started.lock().unwrap().push(root.to_path_buf());
        self.running = true;
        Ok(())
    }
    fn stop(&mut self) -> Result<()> {
        self.running = false;
        Ok(())
    }
    fn is_ready(&mut self) -> Result<EngineState> {
        Ok(EngineState {
            running: self.running,
            uptime_secs: 0,
            warnings: vec![],
        })
    }
    fn cleanup(&mut self) -> Result<()> {
        self.stop()
    }
}

#[derive(Default)]
struct FakeFetch(SyncMutex<HashMap<String, Vec<u8>>>);
impl FakeFetch {
    fn put(&self, name: &str, bytes: Vec<u8>) {
        self.0
            .lock()
            .unwrap()
            .insert(format!("{SOURCE}{name}"), bytes);
    }
    fn remove(&self, name: &str) {
        self.0.lock().unwrap().remove(&format!("{SOURCE}{name}"));
    }
}
/// The updater owns its fetcher; tests keep a second handle to change responses.
struct SharedFetch(Arc<FakeFetch>);
impl Fetch for SharedFetch {
    async fn get(&self, url: &str, _: usize) -> Result<Vec<u8>> {
        match self.0.0.lock().unwrap().get(url) {
            Some(bytes) => Ok(bytes.clone()),
            None => bail!("connection reset"),
        }
    }
}

/// Each round returns two results, the first `n` of them passing.
#[derive(Default)]
struct ScriptedProbes(SyncMutex<VecDeque<usize>>);
impl Probes for ScriptedProbes {
    async fn run(&self) -> Vec<ProbeResult> {
        let n = self
            .0
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected probes");
        (0..2)
            .map(|i| ProbeResult {
                id: format!("s{i}"),
                name: format!("s{i}"),
                protocol: "TCP".into(),
                ok: i < n,
                latency_ms: 0,
                detail: String::new(),
                warning: None,
            })
            .collect()
    }
}

fn catalog(ids: &[&str]) -> Vec<u8> {
    serde_json::to_vec(
        &ids.iter()
            .map(|id| Strategy {
                id: (*id).into(),
                name: (*id).into(),
                description: String::new(),
                supported: true,
                arguments: vec!["--test".into()],
            })
            .collect::<Vec<_>>(),
    )
    .unwrap()
}

/// Signed manifest and archive with the given files.
fn build(version: &str, files: &[(&str, Vec<u8>)], key: &SigningKey) -> (Vec<u8>, Vec<u8>) {
    let archive = bundle::pack_archive(files.iter().map(|(p, d)| (*p, d.as_slice()))).unwrap();
    let manifest = Manifest {
        schema: MANIFEST_SCHEMA,
        version: version.into(),
        flowseal: "1.10.5".into(),
        archive: FileHash {
            path: format!("bundle-{version}.tar.gz"),
            sha256: bundle::sha256_hex(&archive),
        },
        files: files
            .iter()
            .map(|(p, d)| FileHash {
                path: (*p).into(),
                sha256: bundle::sha256_hex(d),
            })
            .collect(),
    };
    let payload = serde_json::to_vec(&manifest).unwrap();
    let envelope = SignedManifest {
        payload: STANDARD.encode(&payload),
        signature: STANDARD.encode(key.sign(&payload).to_bytes()),
    };
    (serde_json::to_vec(&envelope).unwrap(), archive)
}

fn good(version: &str) -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("windows/strategies.json", catalog(&["general", "alt"])),
        (
            "windows/bin/winws.exe",
            format!("engine {version}").into_bytes(),
        ),
        ("licenses/Flowseal-MIT.txt", b"MIT".to_vec()),
    ]
}

struct Fixture {
    dir: tempfile::TempDir,
    controller: Arc<Mutex<Controller<TestEngine>>>,
    operations: Arc<OperationManager>,
    fetch: Arc<FakeFetch>,
    probes: Arc<ScriptedProbes>,
    started: Arc<SyncMutex<Vec<PathBuf>>>,
    updater: Arc<Updater<TestEngine, SharedFetch, ScriptedProbes>>,
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let installed = dir.path().join("install/windows");
        std::fs::create_dir_all(&installed).unwrap();
        std::fs::write(installed.join("strategies.json"), catalog(&["alt"])).unwrap();
        let data = dir.path().join("data");
        let engine = TestEngine::default();
        let started = engine.started.clone();
        let controller = Controller::new(installed, data.clone(), engine).unwrap();
        let controller = Arc::new(Mutex::new(controller));
        Backend::enable(&mut *controller.lock().await)
            .await
            .unwrap();
        let operations = Arc::new(OperationManager::new(data));
        let fetch = Arc::new(FakeFetch::default());
        let probes = Arc::new(ScriptedProbes::default());
        let updater = Arc::new(Updater::new(
            controller.clone(),
            operations.clone(),
            Fixture::store_in(dir.path()),
            SharedFetch(fetch.clone()),
            probes.clone(),
            *SigningKey::from_bytes(&TEST_ONLY_SEED)
                .verifying_key()
                .as_bytes(),
            vec![SOURCE.into()],
        ));
        Self {
            dir,
            controller,
            operations,
            fetch,
            probes,
            started,
            updater,
        }
    }
    fn store_in(dir: &Path) -> Store {
        Store::new(dir.join("install/bundles"), dir.join("install/windows"))
    }
    fn store(&self) -> Store {
        Fixture::store_in(self.dir.path())
    }
    fn installed(&self) -> PathBuf {
        self.dir.path().join("install/windows")
    }
    fn bundles(&self) -> PathBuf {
        self.dir.path().join("install/bundles")
    }
    fn publish(&self, version: &str, files: &[(&str, Vec<u8>)]) {
        let (envelope, archive) = build(version, files, &SigningKey::from_bytes(&TEST_ONLY_SEED));
        self.fetch.put("manifest.json", envelope);
        self.fetch.put(&format!("bundle-{version}.tar.gz"), archive);
    }
    fn probes(&self, rounds: &[usize]) {
        self.probes.0.lock().unwrap().extend(rounds);
    }
    async fn root(&self) -> PathBuf {
        self.controller.lock().await.root().to_path_buf()
    }
    async fn running(&self) -> bool {
        Backend::status(&mut *self.controller.lock().await)
            .unwrap()
            .running
    }
    fn bundle_dirs(&self) -> Vec<String> {
        let mut names: Vec<_> = std::fs::read_dir(self.bundles())
            .map(|entries| {
                entries
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| n != "state.json")
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }
}

#[tokio::test]
async fn update_switches_bundle_and_restarts_engine() {
    let f = Fixture::new().await;
    let domains = f.dir.path().join("data/user-domains.txt");
    std::fs::write(&domains, "example.com\n").unwrap();
    f.publish("2026.11.12.1000", &good("2026.11.12.1000"));
    f.probes(&[1, 2]);

    assert_eq!(
        f.updater.check().await.unwrap(),
        Outcome::Updated("2026.11.12.1000".into())
    );
    let root = f.bundles().join("2026.11.12.1000/windows");
    assert_eq!(f.root().await, root);
    assert!(f.running().await);
    assert_eq!(f.started.lock().unwrap().last(), Some(&root));
    assert_eq!(std::fs::read_to_string(&domains).unwrap(), "example.com\n");
    let state = f.store().state();
    assert_eq!(state.current.as_deref(), Some("2026.11.12.1000"));
    assert_eq!(state.flowseal.as_deref(), Some("1.10.5"));
    assert!(state.last_update.is_some() && state.error.is_none());
    let bundle = f.controller.lock().await.bundle.clone();
    assert_eq!(bundle.version.as_deref(), Some("2026.11.12.1000"));

    // The same manifest again changes nothing.
    assert_eq!(f.updater.check().await.unwrap(), Outcome::UpToDate);
    // The service restarts on the verified downloaded bundle.
    let key = SigningKey::from_bytes(&TEST_ONLY_SEED)
        .verifying_key()
        .to_bytes();
    let (start_root, _, warning) = f.store().startup(Some(&key));
    assert_eq!(start_root, root);
    assert!(warning.is_none());
}

#[tokio::test]
async fn bad_signature_is_rejected_without_changes() {
    let f = Fixture::new().await;
    let (envelope, archive) = build(
        "2026.11.12.1000",
        &good("x"),
        &SigningKey::from_bytes(&[1; 32]),
    );
    f.fetch.put("manifest.json", envelope);
    f.fetch.put("bundle-2026.11.12.1000.tar.gz", archive);

    let err = f.updater.check().await.unwrap_err();
    assert!(
        format!("{err:#}").contains("Подпись набора недействительна"),
        "{err:#}"
    );
    assert_eq!(f.root().await, f.installed());
    assert!(f.bundle_dirs().is_empty());
    assert!(f.store().state().error.is_some());
    assert!(f.controller.lock().await.bundle.error.is_some());
}

#[tokio::test]
async fn bad_archive_hash_is_rejected() {
    let f = Fixture::new().await;
    f.publish("2026.11.12.1000", &good("x"));
    let (_, other) = build(
        "2026.11.12.1000",
        &good("tampered"),
        &SigningKey::from_bytes(&TEST_ONLY_SEED),
    );
    f.fetch.put("bundle-2026.11.12.1000.tar.gz", other);

    let err = f.updater.check().await.unwrap_err();
    assert!(
        format!("{err:#}").contains("Контрольная сумма архива"),
        "{err:#}"
    );
    assert_eq!(f.root().await, f.installed());
    assert!(f.bundle_dirs().is_empty());
}

#[tokio::test]
async fn interrupted_download_keeps_current_bundle() {
    let f = Fixture::new().await;
    f.publish("2026.11.12.1000", &good("x"));
    f.fetch.remove("bundle-2026.11.12.1000.tar.gz");
    assert!(f.updater.check().await.is_err());

    // A truncated archive fails the hash check before extraction.
    let (_, archive) = build(
        "2026.11.12.1000",
        &good("x"),
        &SigningKey::from_bytes(&TEST_ONLY_SEED),
    );
    f.fetch.put(
        "bundle-2026.11.12.1000.tar.gz",
        archive[..archive.len() / 2].to_vec(),
    );
    assert!(f.updater.check().await.is_err());
    assert_eq!(f.root().await, f.installed());
    assert!(f.running().await);
    assert!(f.bundle_dirs().is_empty());
    assert!(f.store().state().current.is_none());
}

#[tokio::test]
async fn worse_probes_roll_back_and_the_version_is_not_retried() {
    let f = Fixture::new().await;
    f.publish("2026.11.12.1000", &good("x"));
    f.probes(&[2, 1]);

    assert_eq!(
        f.updater.check().await.unwrap(),
        Outcome::RolledBack("2026.11.12.1000".into())
    );
    assert_eq!(f.root().await, f.installed());
    assert!(f.running().await);
    assert_eq!(f.started.lock().unwrap().last(), Some(&f.installed()));
    let state = f.store().state();
    assert!(state.current.is_none());
    assert_eq!(state.rejected, ["2026.11.12.1000"]);
    assert!(state.error.unwrap().contains("ухудшил"));
    // No probes are queued: a retry would panic in ScriptedProbes.
    assert_eq!(f.updater.check().await.unwrap(), Outcome::UpToDate);
}

#[tokio::test]
async fn engine_start_failure_restores_previous_bundle() {
    let f = Fixture::new().await;
    let mut files = good("x");
    files.push(("windows/fail", b"1".to_vec()));
    f.publish("2026.11.12.1000", &files);
    f.probes(&[2]);

    assert!(f.updater.check().await.is_err());
    assert_eq!(f.root().await, f.installed());
    assert!(f.running().await);
    assert_eq!(f.store().state().rejected, ["2026.11.12.1000"]);
}

#[tokio::test]
async fn tampered_bundle_on_disk_falls_back_to_installer_bundle() {
    let f = Fixture::new().await;
    f.publish("2026.11.12.1000", &good("x"));
    f.probes(&[2, 2]);
    f.updater.check().await.unwrap();
    std::fs::write(
        f.bundles().join("2026.11.12.1000/windows/bin/winws.exe"),
        b"evil",
    )
    .unwrap();

    let key = SigningKey::from_bytes(&TEST_ONLY_SEED)
        .verifying_key()
        .to_bytes();
    let (root, status, warning) = f.store().startup(Some(&key));
    assert_eq!(root, f.installed());
    assert!(status.version.is_none());
    assert!(warning.unwrap().contains("не прошёл проверку"));
    assert!(status.error.unwrap().contains("не прошёл проверку"));
    assert!(f.store().state().current.is_none());
    // A build without a public key never uses downloaded bundles.
    let (root, status, _) = f.store().startup(None);
    assert_eq!(root, f.installed());
    assert!(!status.updates_enabled);
}

#[tokio::test]
async fn keeps_current_and_previous_bundles_only() {
    let f = Fixture::new().await;
    for (i, version) in ["2026.11.12.1000", "2026.11.12.1100", "2026.11.12.1200"]
        .iter()
        .enumerate()
    {
        f.publish(version, &good(version));
        f.probes(&[2, 2]);
        assert_eq!(
            f.updater.check().await.unwrap(),
            Outcome::Updated((*version).into()),
            "{i}"
        );
    }
    assert_eq!(
        f.bundle_dirs(),
        [
            "2026.11.12.1100",
            "2026.11.12.1100.manifest.json",
            "2026.11.12.1200",
            "2026.11.12.1200.manifest.json",
        ]
    );
    assert_eq!(
        f.store().state().previous.as_deref(),
        Some("2026.11.12.1100")
    );
}

#[tokio::test]
async fn busy_gate_defers_the_switch_and_reuses_the_download() {
    let f = Fixture::new().await;
    f.publish("2026.11.12.1000", &good("x"));
    {
        let _selection = f.operations.gate.lock().await;
        assert_eq!(f.updater.check().await.unwrap(), Outcome::Busy);
    }
    assert_eq!(f.root().await, f.installed());
    // The verified copy on disk is reused; no second download happens.
    f.fetch.remove("bundle-2026.11.12.1000.tar.gz");
    f.probes(&[2, 2]);
    assert_eq!(
        f.updater.check().await.unwrap(),
        Outcome::Updated("2026.11.12.1000".into())
    );
}

#[test]
fn updater_loop_can_run_on_the_service_runtime() {
    fn assert_send<T: Send + 'static>(_: T) {}
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let f = rt.block_on(Fixture::new());
    assert_send(f.updater.clone().run());
}
