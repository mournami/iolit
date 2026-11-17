//! Local MTProto proxy for Telegram Desktop. The client connects to
//! `127.0.0.1`, the proxy re-encrypts its traffic and carries it over a
//! WebSocket to Telegram's own gateway, or plainly to the datacenter when the
//! gateway is not an option. No third-party server takes part.
//!
//! A Rust port of the core of Flowseal/tg-ws-proxy (MIT), see
//! `licenses/THIRD-PARTY.md`. Independent of the packet engine: it has its own
//! switch and keeps running while the bypass is off.
pub mod mtproto;
pub mod upstream;
pub mod ws;

use anyhow::{Context, Result};
use ctr::cipher::StreamCipher;
use mtproto::{Ciphers, INIT_LEN, SECRET_LEN, Splitter, Transport};
use std::{
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{Mutex, Semaphore},
    task::{JoinHandle, JoinSet},
    time::timeout,
};
use upstream::{Pool, Routes};
use zapret_core::TelegramProxy;

/// The port tg-ws-proxy uses, so its instructions and links stay valid.
pub const PORT: u16 = 1443;
/// "1" while the user keeps the proxy on.
const ENABLED: &str = "telegram";
/// 32 hex characters, created once: the link saved in Telegram stays valid.
const SECRET: &str = "telegram-secret";
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_CLIENTS: usize = 512;
/// Read from a client that failed the handshake before closing, so that the
/// answer does not tell a wrong secret from a slow server.
const BAD_HANDSHAKE_DRAIN: usize = 64 * 1024;

#[derive(Default)]
struct Stats {
    active: AtomicU32,
    websocket: AtomicU32,
    direct: AtomicU32,
    failed_handshake: AtomicU32,
    failed_secret: AtomicU32,
    failed_route: AtomicU32,
}

/// How a session ended.
enum Outcome {
    /// Reached Telegram and was carried until one side closed.
    Served,
    /// The client closed or stayed silent before sending its 64-byte init.
    NoHandshake,
    /// The init was not made with this proxy's secret, or named no datacenter.
    WrongSecret,
    /// Neither the gateway nor the datacenter could be reached.
    NoRoute,
}

struct Running {
    task: JoinHandle<()>,
    address: SocketAddr,
    secret: [u8; SECRET_LEN],
    stats: Arc<Stats>,
    pool: Arc<Pool>,
}
impl Drop for Running {
    fn drop(&mut self) {
        // Aborting the listener drops its sessions with it.
        self.task.abort();
        self.pool.close();
    }
}

#[derive(Default)]
struct State {
    running: Option<Running>,
    error: Option<String>,
}

pub struct Manager {
    data: PathBuf,
    listen: SocketAddr,
    /// `None`: Telegram's addresses, prepared on the first start.
    routes: Mutex<Option<Arc<Routes>>>,
    state: Mutex<State>,
}

impl Manager {
    pub fn new(data: PathBuf) -> Self {
        Self {
            data,
            listen: SocketAddr::from((Ipv4Addr::LOCALHOST, PORT)),
            routes: Mutex::new(None),
            state: Mutex::default(),
        }
    }

    /// Other addresses, for tests.
    pub fn with_routes(data: PathBuf, listen: SocketAddr, routes: Routes) -> Self {
        Self {
            data,
            listen,
            routes: Mutex::new(Some(Arc::new(routes))),
            state: Mutex::default(),
        }
    }

    fn enabled(&self) -> bool {
        std::fs::read(self.data.join(ENABLED)).is_ok_and(|v| v == b"1")
    }

    /// Service start: bring the proxy back if the user left it on. A failure
    /// stays visible in the status and does not stop the service.
    pub async fn restore(&self) {
        if self.enabled() {
            let state = &mut *self.state.lock().await;
            state.error = self.start(state).await.err().map(|e| format!("{e:#}"));
        }
    }

    pub async fn set(&self, enabled: bool) -> Result<()> {
        let state = &mut *self.state.lock().await;
        state.error = None;
        if enabled {
            self.start(state).await?;
            std::fs::write(self.data.join(ENABLED), b"1")?;
        } else {
            let persisted = std::fs::write(self.data.join(ENABLED), b"0");
            state.running = None;
            persisted.context("Прокси остановлен, но сохранить настройку не удалось")?;
        }
        Ok(())
    }

    async fn start(&self, state: &mut State) -> Result<()> {
        if state.running.is_some() {
            return Ok(());
        }
        let routes = {
            let mut routes = self.routes.lock().await;
            match &*routes {
                Some(routes) => routes.clone(),
                None => routes.insert(Arc::new(Routes::telegram()?)).clone(),
            }
        };
        let secret = self.secret()?;
        let listener = TcpListener::bind(self.listen).await.with_context(|| {
            format!(
                "Порт {} на этом компьютере занят. Если запущен TG WS Proxy, закройте его",
                self.listen.port()
            )
        })?;
        let address = listener.local_addr()?;
        let stats = Arc::new(Stats::default());
        let pool = Pool::new(routes);
        state.running = Some(Running {
            task: tokio::spawn(serve(listener, secret, pool.clone(), stats.clone())),
            address,
            secret,
            stats,
            pool,
        });
        Ok(())
    }

    /// The saved secret, or a new one.
    fn secret(&self) -> Result<[u8; SECRET_LEN]> {
        let file = self.data.join(SECRET);
        let saved = std::fs::read_to_string(&file).unwrap_or_default();
        let mut secret = [0u8; SECRET_LEN];
        if hex::decode_to_slice(saved.trim(), &mut secret).is_ok() {
            return Ok(secret);
        }
        getrandom::fill(&mut secret).context("Нет источника случайных чисел")?;
        std::fs::write(&file, hex::encode(secret))?;
        Ok(secret)
    }

    pub async fn status(&self) -> TelegramProxy {
        let state = self.state.lock().await;
        let count = |counter: &AtomicU32| counter.load(Ordering::Relaxed);
        match &state.running {
            Some(running) => TelegramProxy {
                enabled: true,
                running: true,
                failed: count(&running.stats.failed_handshake)
                    + count(&running.stats.failed_secret)
                    + count(&running.stats.failed_route),
                failed_handshake: count(&running.stats.failed_handshake),
                failed_secret: count(&running.stats.failed_secret),
                failed_route: count(&running.stats.failed_route),
                link: Some(format!(
                    "tg://proxy?server={}&port={}&secret=dd{}",
                    running.address.ip(),
                    running.address.port(),
                    hex::encode(running.secret)
                )),
                active: count(&running.stats.active),
                websocket: count(&running.stats.websocket),
                direct: count(&running.stats.direct),
                error: None,
            },
            None => TelegramProxy {
                enabled: self.enabled(),
                error: state.error.clone(),
                ..Default::default()
            },
        }
    }
}

async fn serve(
    listener: TcpListener,
    secret: [u8; SECRET_LEN],
    pool: Arc<Pool>,
    stats: Arc<Stats>,
) {
    let limit = Arc::new(Semaphore::new(MAX_CLIENTS));
    let mut sessions = JoinSet::new();
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let Ok((client, _)) = accepted else {
                    // Out of sockets or a reset before accept: not a reason to stop.
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                };
                // Over the limit the connection is closed at once.
                if let Ok(permit) = limit.clone().try_acquire_owned() {
                    let (pool, stats) = (pool.clone(), stats.clone());
                    sessions.spawn(async move {
                        let _permit = permit;
                        stats.active.fetch_add(1, Ordering::Relaxed);
                        let outcome = session(client, &secret, &pool, &stats).await;
                        stats.active.fetch_sub(1, Ordering::Relaxed);
                        let counter = match outcome {
                            Outcome::Served => None,
                            Outcome::NoHandshake => Some(&stats.failed_handshake),
                            Outcome::WrongSecret => Some(&stats.failed_secret),
                            Outcome::NoRoute => Some(&stats.failed_route),
                        };
                        if let Some(counter) = counter {
                            counter.fetch_add(1, Ordering::Relaxed);
                        }
                    });
                }
            }
            Some(_) = sessions.join_next() => {}
        }
    }
}

/// One Telegram connection from the handshake to the end.
async fn session(
    mut client: TcpStream,
    secret: &[u8; SECRET_LEN],
    pool: &Arc<Pool>,
    stats: &Stats,
) -> Outcome {
    let _ = client.set_nodelay(true);
    let mut init = [0u8; INIT_LEN];
    if !matches!(
        timeout(HANDSHAKE_TIMEOUT, client.read_exact(&mut init)).await,
        Ok(Ok(_))
    ) {
        return Outcome::NoHandshake;
    }
    let Some(hello) = mtproto::parse_client_init(&init, secret) else {
        let mut sink = vec![0u8; BAD_HANDSHAKE_DRAIN];
        let _ = timeout(HANDSHAKE_TIMEOUT, async {
            let mut left = BAD_HANDSHAKE_DRAIN;
            while left > 0 {
                match client.read(&mut sink[..left]).await {
                    Ok(n) if n > 0 => left -= n,
                    _ => break,
                }
            }
        })
        .await;
        return Outcome::WrongSecret;
    };
    let relay = mtproto::relay_init(hello.transport, hello.dc_index());
    let ciphers = mtproto::ciphers(&hello, secret, &relay);
    if let Some((reader, writer)) = pool.gateway(hello.dc, hello.media, hello.test).await
        && writer.send(&relay).await.is_ok()
    {
        stats.websocket.fetch_add(1, Ordering::Relaxed);
        bridge_websocket(client, reader, writer, ciphers, hello.transport).await;
        return Outcome::Served;
    }
    if let Some(mut telegram) = pool.direct(hello.dc, hello.test).await
        && telegram.write_all(&relay).await.is_ok()
    {
        stats.direct.fetch_add(1, Ordering::Relaxed);
        bridge_direct(client, telegram, ciphers).await;
        return Outcome::Served;
    }
    Outcome::NoRoute
}

/// Client TCP to gateway WebSocket and back, re-encrypting both ways, until
/// either side ends.
async fn bridge_websocket(
    client: TcpStream,
    mut reader: ws::Reader,
    writer: ws::Writer,
    ciphers: Ciphers,
    transport: Transport,
) {
    let (mut from_client, mut to_client) = client.into_split();
    let Ciphers {
        from_client: mut client_in,
        to_client: mut client_out,
        to_telegram: mut telegram_out,
        from_telegram: mut telegram_in,
    } = ciphers;
    let up = async {
        let mut splitter = Splitter::new(transport);
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            let n = from_client.read(&mut buffer).await?;
            if n == 0 {
                if let Some(tail) = splitter.flush() {
                    writer.send(&tail).await?;
                }
                return anyhow::Ok(());
            }
            let mut plain = buffer[..n].to_vec();
            client_in.apply_keystream(&mut plain);
            let mut cipher = plain.clone();
            telegram_out.apply_keystream(&mut cipher);
            let parts = splitter.split(&cipher, &plain);
            if !parts.is_empty() {
                writer.send_batch(&parts).await?;
            }
        }
    };
    let down = async {
        while let Some(mut data) = reader.recv().await? {
            telegram_in.apply_keystream(&mut data);
            client_out.apply_keystream(&mut data);
            to_client.write_all(&data).await?;
        }
        anyhow::Ok(())
    };
    let _ = tokio::select! { ended = up => ended, ended = down => ended };
    writer.close().await;
}

/// The same over plain TCP to the datacenter.
async fn bridge_direct(client: TcpStream, telegram: TcpStream, ciphers: Ciphers) {
    let (mut from_client, mut to_client) = client.into_split();
    let (mut from_telegram, mut to_telegram) = telegram.into_split();
    let Ciphers {
        from_client: mut client_in,
        to_client: mut client_out,
        to_telegram: mut telegram_out,
        from_telegram: mut telegram_in,
    } = ciphers;
    let up = async {
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            let n = from_client.read(&mut buffer).await?;
            if n == 0 {
                return anyhow::Ok(());
            }
            client_in.apply_keystream(&mut buffer[..n]);
            telegram_out.apply_keystream(&mut buffer[..n]);
            to_telegram.write_all(&buffer[..n]).await?;
        }
    };
    let down = async {
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            let n = from_telegram.read(&mut buffer).await?;
            if n == 0 {
                return anyhow::Ok(());
            }
            telegram_in.apply_keystream(&mut buffer[..n]);
            client_out.apply_keystream(&mut buffer[..n]);
            to_client.write_all(&buffer[..n]).await?;
        }
    };
    let _ = tokio::select! { ended = up => ended, ended = down => ended };
}
