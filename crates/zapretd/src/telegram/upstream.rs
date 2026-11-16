//! Connections to Telegram: the WebSocket gateway with a small pool of ready
//! connections, and the plain TCP route used when the gateway is not an option.
//!
//! Addresses and the order of attempts come from Flowseal/tg-ws-proxy (MIT),
//! `proxy/utils.py` and `proxy/pool.py`. Its Cloudflare routes are left out on
//! purpose: this proxy talks to Telegram's own servers only.
use super::ws::{self, Stream};
use anyhow::{Context, Result};
use rustls::{
    ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme,
    client::{
        WebPkiServerVerifier,
        danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    },
    pki_types::{CertificateDer, ServerName, UnixTime},
};
use std::{
    collections::{HashMap, VecDeque},
    net::{Ipv4Addr, SocketAddr},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    net::TcpStream,
    time::{Instant, timeout},
};
use tokio_rustls::TlsConnector;

/// Ready connections kept per datacenter and kind while Telegram is active.
const POOL_SIZE: usize = 4;
/// The gateway drops a connection that stays silent longer.
const POOL_MAX_AGE: Duration = Duration::from_secs(120);
const GATEWAY_TIMEOUT: Duration = Duration::from_secs(8);
const DIRECT_TIMEOUT: Duration = Duration::from_secs(10);
/// After a failed attempt the route is skipped for this long, doubling up to
/// the maximum, so that a blocked route does not delay every new connection.
const GATEWAY_BACKOFF: (Duration, Duration) = (Duration::from_secs(1), Duration::from_secs(60));
const DIRECT_BACKOFF: (Duration, Duration) = (Duration::from_secs(30), Duration::from_secs(600));
/// Server name sent instead of Telegram's when the real one does not get
/// through; the certificate is still checked against Telegram's name.
const FRONT_NAME: &str = "sprinthost.ru";
const FRONT_CERTIFICATE_NAME: &str = "web.telegram.org";

pub type Connection = (ws::Reader, ws::Writer);

pub struct Routes {
    /// Datacenters reachable through the WebSocket gateway, and its address.
    pub gateways: HashMap<u16, SocketAddr>,
    pub direct: HashMap<u16, SocketAddr>,
    pub direct_test: HashMap<u16, SocketAddr>,
    /// `None` only in tests: WebSocket over plain TCP.
    pub tls: Option<Tls>,
}

pub struct Tls {
    direct: TlsConnector,
    fronted: TlsConnector,
}

fn addresses(list: &[(u16, [u8; 4])]) -> HashMap<u16, SocketAddr> {
    list.iter()
        .map(|(dc, ip)| (*dc, SocketAddr::from((Ipv4Addr::from(*ip), 443))))
        .collect()
}

impl Routes {
    /// Telegram's production addresses.
    pub fn telegram() -> Result<Self> {
        Ok(Self {
            gateways: addresses(&[(2, [149, 154, 167, 220]), (4, [149, 154, 167, 220])]),
            direct: addresses(&[
                (1, [149, 154, 175, 50]),
                (2, [149, 154, 167, 51]),
                (3, [149, 154, 175, 100]),
                (4, [149, 154, 167, 91]),
                (5, [149, 154, 171, 5]),
                (203, [91, 105, 192, 100]),
            ]),
            direct_test: addresses(&[
                (1, [149, 154, 175, 10]),
                (2, [149, 154, 167, 40]),
                (3, [149, 154, 175, 117]),
            ]),
            tls: Some(Tls::new()?),
        })
    }

    /// One attempt: TCP, TLS and the WebSocket upgrade.
    async fn gateway(
        &self,
        address: SocketAddr,
        domain: &str,
        path: &str,
        fronted: bool,
    ) -> Result<Connection> {
        let tcp = TcpStream::connect(address).await?;
        tcp.set_nodelay(true)?;
        let stream: Stream = match &self.tls {
            Some(tls) => {
                let (connector, name) = if fronted {
                    (&tls.fronted, FRONT_NAME)
                } else {
                    (&tls.direct, domain)
                };
                let name = ServerName::try_from(name.to_owned())?;
                Box::new(connector.connect(name, tcp).await?)
            }
            None => Box::new(tcp),
        };
        ws::connect(stream, domain, path).await
    }
}

/// Checks the certificate against a fixed name, whatever name was sent.
#[derive(Debug)]
struct FixedName {
    inner: Arc<WebPkiServerVerifier>,
    name: ServerName<'static>,
}
impl ServerCertVerifier for FixedName {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _sent: &ServerName<'_>,
        ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        self.inner
            .verify_server_cert(end_entity, intermediates, &self.name, ocsp, now)
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

impl Tls {
    fn new() -> Result<Self> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let mut roots = RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let roots = Arc::new(roots);
        let direct = ClientConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()?
            .with_root_certificates(roots.clone())
            .with_no_client_auth();
        let verifier = FixedName {
            inner: WebPkiServerVerifier::builder_with_provider(roots, provider.clone())
                .build()
                .context("Не удалось подготовить проверку сертификатов")?,
            name: ServerName::try_from(FRONT_CERTIFICATE_NAME)?,
        };
        let fronted = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(verifier))
            .with_no_client_auth();
        Ok(Self {
            direct: TlsConnector::from(Arc::new(direct)),
            fronted: TlsConnector::from(Arc::new(fronted)),
        })
    }
}

/// Gateway names for a datacenter; media connections have their own first.
fn gateway_domains(dc: u16, media: bool) -> Vec<String> {
    let dc = if dc == 203 { 2 } else { dc };
    let main = format!("kws{dc}.web.telegram.org");
    if media {
        vec![format!("kws{dc}-1.web.telegram.org"), main]
    } else {
        vec![main]
    }
}

/// Datacenter, media, test.
type Key = (u16, bool, bool);

#[derive(Default)]
struct Slot {
    ready: VecDeque<(Connection, Instant)>,
    refilling: bool,
    backoff: Backoff,
}

#[derive(Default)]
struct Backoff {
    failures: u32,
    until: Option<Instant>,
}
impl Backoff {
    fn active(&self) -> bool {
        self.until.is_some_and(|until| Instant::now() < until)
    }
    fn failed(&mut self, (first, max): (Duration, Duration)) {
        let delay = first.saturating_mul(1 << self.failures.min(16)).min(max);
        self.failures += 1;
        self.until = Some(Instant::now() + delay);
    }
    fn succeeded(&mut self) {
        *self = Self::default();
    }
}

pub struct Pool {
    routes: Arc<Routes>,
    slots: Mutex<HashMap<Key, Slot>>,
    direct: Mutex<HashMap<SocketAddr, Backoff>>,
    /// The fronted attempt worked last time: try it first.
    fronting_first: AtomicBool,
    /// The proxy was turned off: nothing new is kept ready.
    closed: AtomicBool,
}

impl Pool {
    pub fn new(routes: Arc<Routes>) -> Arc<Self> {
        Arc::new(Self {
            routes,
            slots: Mutex::default(),
            direct: Mutex::default(),
            fronting_first: AtomicBool::new(false),
            closed: AtomicBool::new(false),
        })
    }

    /// Drops the ready connections; refills still running add nothing.
    pub fn close(&self) {
        self.closed.store(true, Ordering::Relaxed);
        self.slots.lock().expect("pool lock").clear();
    }

    /// A gateway connection for the datacenter: a ready one when there is,
    /// else a new one. `None` when the datacenter has no gateway or the
    /// gateway does not answer; the caller then goes directly.
    pub async fn gateway(self: &Arc<Self>, dc: u16, media: bool, test: bool) -> Option<Connection> {
        let address = *self.routes.gateways.get(&dc)?;
        let key = (dc, media, test);
        let (ready, refill, backing_off) = {
            let mut slots = self.slots.lock().expect("pool lock");
            let slot = slots.entry(key).or_default();
            let mut ready = None;
            while let Some((mut connection, created)) = slot.ready.pop_front() {
                if created.elapsed() < POOL_MAX_AGE && !connection.0.is_dead() {
                    ready = Some(connection);
                    break;
                }
            }
            let refill = !slot.refilling && !slot.backoff.active();
            slot.refilling |= refill;
            (ready, refill, slot.backoff.active())
        };
        if refill {
            tokio::spawn(self.clone().refill(key, address));
        }
        if ready.is_some() || backing_off {
            return ready;
        }
        let connection = self.connect(key, address).await;
        self.note(key, connection.is_some());
        connection
    }

    fn note(&self, key: Key, connected: bool) {
        let mut slots = self.slots.lock().expect("pool lock");
        let backoff = &mut slots.entry(key).or_default().backoff;
        if connected {
            backoff.succeeded();
        } else {
            backoff.failed(GATEWAY_BACKOFF);
        }
    }

    /// Tops the slot up, then drops what nobody took before it went stale, so
    /// nothing stays open while Telegram is idle.
    async fn refill(self: Arc<Self>, key: Key, address: SocketAddr) {
        let missing = {
            let slots = self.slots.lock().expect("pool lock");
            POOL_SIZE.saturating_sub(slots.get(&key).map_or(0, |slot| slot.ready.len()))
        };
        let mut attempts = tokio::task::JoinSet::new();
        for _ in 0..missing {
            let pool = self.clone();
            attempts.spawn(async move { pool.connect(key, address).await });
        }
        let mut connected = 0;
        while let Some(attempt) = attempts.join_next().await {
            if let Ok(Some(connection)) = attempt {
                connected += 1;
                if self.closed.load(Ordering::Relaxed) {
                    return;
                }
                let mut slots = self.slots.lock().expect("pool lock");
                let ready = &mut slots.entry(key).or_default().ready;
                ready.push_back((connection, Instant::now()));
            }
        }
        if missing > 0 {
            self.note(key, connected > 0);
        }
        self.slots
            .lock()
            .expect("pool lock")
            .entry(key)
            .or_default()
            .refilling = false;
        tokio::time::sleep(POOL_MAX_AGE).await;
        let stale: Vec<_> = {
            let mut slots = self.slots.lock().expect("pool lock");
            let ready = &mut slots.entry(key).or_default().ready;
            let fresh = ready.partition_point(|(_, created)| created.elapsed() >= POOL_MAX_AGE);
            ready.drain(..fresh).collect()
        };
        for ((_, writer), _) in stale {
            writer.close().await;
        }
    }

    /// Every name and both ways of naming the server, as upstream does.
    async fn connect(&self, (dc, media, test): Key, address: SocketAddr) -> Option<Connection> {
        let path = if test { "/apiws_test" } else { "/apiws" };
        let fronting_first = self.fronting_first.load(Ordering::Relaxed);
        for domain in gateway_domains(dc, media) {
            for fronted in [fronting_first, !fronting_first] {
                if fronted && self.routes.tls.is_none() {
                    continue;
                }
                let attempt = self.routes.gateway(address, &domain, path, fronted);
                if let Ok(Ok(connection)) = timeout(GATEWAY_TIMEOUT, attempt).await {
                    self.fronting_first.store(fronted, Ordering::Relaxed);
                    return Some(connection);
                }
            }
        }
        None
    }

    /// Plain TCP to the datacenter itself.
    pub async fn direct(&self, dc: u16, test: bool) -> Option<TcpStream> {
        let table = if test {
            &self.routes.direct_test
        } else {
            &self.routes.direct
        };
        let address = *table.get(&dc)?;
        if self
            .direct
            .lock()
            .expect("direct lock")
            .get(&address)
            .is_some_and(Backoff::active)
        {
            return None;
        }
        let stream = match timeout(DIRECT_TIMEOUT, TcpStream::connect(address)).await {
            Ok(Ok(stream)) => stream.set_nodelay(true).is_ok().then_some(stream),
            _ => None,
        };
        let mut direct = self.direct.lock().expect("direct lock");
        let backoff = direct.entry(address).or_default();
        if stream.is_some() {
            backoff.succeeded();
        } else {
            backoff.failed(DIRECT_BACKOFF);
        }
        stream
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_connections_try_their_own_gateway_name_first() {
        assert_eq!(gateway_domains(2, false), ["kws2.web.telegram.org"]);
        assert_eq!(
            gateway_domains(4, true),
            ["kws4-1.web.telegram.org", "kws4.web.telegram.org"]
        );
        assert_eq!(gateway_domains(203, false), ["kws2.web.telegram.org"]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_failed_route_is_skipped_for_a_growing_time() {
        let mut backoff = Backoff::default();
        assert!(!backoff.active());
        backoff.failed(GATEWAY_BACKOFF);
        assert!(backoff.active());
        tokio::time::advance(Duration::from_secs(1)).await;
        assert!(!backoff.active());
        for _ in 0..20 {
            backoff.failed(GATEWAY_BACKOFF);
        }
        tokio::time::advance(Duration::from_secs(59)).await;
        assert!(backoff.active());
        tokio::time::advance(Duration::from_secs(1)).await;
        assert!(!backoff.active());
        backoff.succeeded();
        assert_eq!(backoff.failures, 0);
    }

    /// Needs the internet and reaches Telegram: `cargo test -p zapretd
    /// telegram -- --ignored`.
    #[tokio::test]
    #[ignore = "connects to Telegram's WebSocket gateway"]
    async fn live_gateway_answers_under_both_server_names() {
        let routes = Routes::telegram().unwrap();
        let address = routes.gateways[&2];
        for fronted in [false, true] {
            routes
                .gateway(address, "kws2.web.telegram.org", "/apiws", fronted)
                .await
                .unwrap_or_else(|e| panic!("fronted={fronted}: {e:#}"));
        }
    }
}
