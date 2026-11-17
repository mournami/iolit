use anyhow::{Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signer, SigningKey};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex as SyncMutex},
    time::Duration,
};
use zapret_core::{
    app_update::{AppManifest, installer_name},
    bundle::SignedManifest,
};
use zapretd::{app_update::AppUpdater, operation::OperationManager, updater::Fetch};

/// Test-only seed; never used for real releases.
const TEST_ONLY_SEED: [u8; 32] = [0x6b; 32];
const SOURCE: &str = "https://mirror.test/";

#[derive(Default)]
struct FakeFetch(SyncMutex<HashMap<String, Vec<u8>>>);
impl FakeFetch {
    fn put(&self, name: &str, bytes: Vec<u8>) {
        self.0
            .lock()
            .unwrap()
            .insert(format!("{SOURCE}{name}"), bytes);
    }
    fn clear(&self) {
        self.0.lock().unwrap().clear();
    }
}
struct SharedFetch(Arc<FakeFetch>);
impl Fetch for SharedFetch {
    async fn get(&self, url: &str, _: usize) -> Result<Vec<u8>> {
        match self.0.0.lock().unwrap().get(url) {
            Some(bytes) => Ok(bytes.clone()),
            None => bail!("connection reset"),
        }
    }
}

static LAUNCHED: SyncMutex<Vec<PathBuf>> = SyncMutex::new(Vec::new());
fn record_launch(path: &Path) -> Result<()> {
    LAUNCHED.lock().unwrap().push(path.to_path_buf());
    Ok(())
}
fn launched(dir: &Path) -> usize {
    LAUNCHED
        .lock()
        .unwrap()
        .iter()
        .filter(|p| p.starts_with(dir))
        .count()
}

fn signed(manifest: &AppManifest, key: &SigningKey) -> Vec<u8> {
    let payload = serde_json::to_vec(manifest).unwrap();
    serde_json::to_vec(&SignedManifest {
        payload: STANDARD.encode(&payload),
        signature: STANDARD.encode(key.sign(&payload).to_bytes()),
    })
    .unwrap()
}

struct Fixture {
    dir: tempfile::TempDir,
    operations: Arc<OperationManager>,
    fetch: Arc<FakeFetch>,
    updater: AppUpdater<SharedFetch>,
}

impl Fixture {
    fn new(current: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let operations = Arc::new(OperationManager::new(dir.path().join("data")));
        let fetch = Arc::new(FakeFetch::default());
        let updater = AppUpdater::new(
            operations.clone(),
            dir.path().join("updates"),
            SharedFetch(fetch.clone()),
            SigningKey::from_bytes(&TEST_ONLY_SEED)
                .verifying_key()
                .to_bytes(),
            vec![SOURCE.into()],
            current,
        )
        .with_launcher(record_launch, Duration::from_millis(10));
        Self {
            dir,
            operations,
            fetch,
            updater,
        }
    }
    fn publish(&self, version: &str, installer: &[u8]) {
        let manifest = AppManifest::new(version, installer).unwrap();
        self.fetch.put(
            "app-manifest.json",
            signed(&manifest, &SigningKey::from_bytes(&TEST_ONLY_SEED)),
        );
        self.fetch.put(&installer_name(version), installer.to_vec());
    }
    fn updates(&self) -> PathBuf {
        self.dir.path().join("updates")
    }
}

#[tokio::test]
async fn newer_version_is_downloaded_once_and_offered() {
    let f = Fixture::new("0.1.0");
    f.publish("0.2.0", b"setup 0.2.0");
    assert_eq!(f.updater.check().await.unwrap().as_deref(), Some("0.2.0"));
    let app = f.operations.app.lock().await.clone();
    assert_eq!(app.available.as_deref(), Some("0.2.0"));
    assert!(app.last_check.is_some() && app.error.is_none());
    assert_eq!(
        std::fs::read(f.updates().join("Iolit-Setup.exe")).unwrap(),
        b"setup 0.2.0"
    );
    // Only the manifest is fetched again; a network error keeps the offer.
    f.fetch.clear();
    f.publish("0.2.0", b"setup 0.2.0");
    f.fetch
        .0
        .lock()
        .unwrap()
        .remove(&format!("{SOURCE}{}", installer_name("0.2.0")));
    assert_eq!(f.updater.check().await.unwrap().as_deref(), Some("0.2.0"));
    f.fetch.clear();
    assert!(f.updater.check().await.is_err());
    let app = f.operations.app.lock().await.clone();
    assert_eq!(app.available.as_deref(), Some("0.2.0"));
    assert!(app.error.is_some());
}

#[tokio::test]
async fn same_or_older_version_is_not_offered_and_files_go() {
    let f = Fixture::new("0.1.0");
    f.publish("0.2.0", b"setup");
    f.updater.check().await.unwrap();
    let f2 = AppUpdater::new(
        f.operations.clone(),
        f.updates(),
        SharedFetch(f.fetch.clone()),
        SigningKey::from_bytes(&TEST_ONLY_SEED)
            .verifying_key()
            .to_bytes(),
        vec![SOURCE.into()],
        "0.2.0",
    );
    // The installer a version before the rename left behind goes too.
    std::fs::write(f.updates().join("Zapret-Desktop-Setup.exe"), b"old").unwrap();
    // After the update the same release is current: nothing to offer.
    f2.startup().await;
    assert!(!f.updates().join("Zapret-Desktop-Setup.exe").exists());
    assert!(f.operations.app.lock().await.available.is_none());
    assert!(!f.updates().join("Iolit-Setup.exe").exists());
    assert_eq!(f2.check().await.unwrap(), None);
}

#[tokio::test]
async fn bad_signature_or_hash_writes_nothing() {
    let f = Fixture::new("0.1.0");
    let manifest = AppManifest::new("0.2.0", b"setup").unwrap();
    f.fetch.put(
        "app-manifest.json",
        signed(&manifest, &SigningKey::from_bytes(&[1; 32])),
    );
    f.fetch.put(&installer_name("0.2.0"), b"setup".to_vec());
    let err = f.updater.check().await.unwrap_err();
    assert!(format!("{err:#}").contains("Подпись"), "{err:#}");

    f.publish("0.2.0", b"setup");
    f.fetch.put(&installer_name("0.2.0"), b"tampered".to_vec());
    let err = f.updater.check().await.unwrap_err();
    assert!(format!("{err:#}").contains("Контрольная сумма"), "{err:#}");
    assert!(!f.updates().join("Iolit-Setup.exe").exists());
    assert!(f.operations.app.lock().await.available.is_none());
}

#[tokio::test]
async fn install_runs_the_verified_installer_only() {
    let f = Fixture::new("0.1.0");
    assert!(f.updater.install().await.is_err(), "nothing downloaded");
    f.publish("0.2.0", b"setup");
    f.updater.check().await.unwrap();

    {
        let _selection = f.operations.gate.lock().await;
        assert!(f.updater.install().await.is_err());
    }
    assert_eq!(launched(&f.updates()), 0);

    // The installer would stop the service; here it does not, so the wait
    // ends with an error and the offer stays.
    let err = f.updater.install().await.unwrap_err();
    assert!(format!("{err:#}").contains("не установилось"), "{err:#}");
    assert_eq!(launched(&f.updates()), 1);
    let app = f.operations.app.lock().await.clone();
    assert!(!app.installing && app.available.is_some() && app.error.is_some());

    // A file changed on disk is never started.
    std::fs::write(f.updates().join("Iolit-Setup.exe"), b"evil").unwrap();
    assert!(f.updater.install().await.is_err());
    assert_eq!(launched(&f.updates()), 1);
    assert!(f.operations.app.lock().await.available.is_none());
}
