//! The Telegram proxy end to end: a client that speaks like Telegram Desktop,
//! the proxy, and stand-ins for Telegram's WebSocket gateway and datacenter.
use ctr::cipher::StreamCipher;
use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddr},
    path::Path,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::mpsc,
};
use zapretd::telegram::{
    Manager,
    mtproto::{self, Cipher, SECRET_LEN, Transport},
    upstream::Routes,
};

fn local() -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, 0))
}

fn routes(gateways: &[(u16, SocketAddr)], direct: &[(u16, SocketAddr)]) -> Routes {
    Routes {
        gateways: gateways.iter().copied().collect(),
        direct: direct.iter().copied().collect(),
        direct_test: HashMap::new(),
        tls: None,
    }
}

/// What the stand-in gateway saw of one connection.
#[derive(Debug, PartialEq)]
enum Seen {
    Request(String),
    Init(Transport, i16),
    /// A decrypted message, one per WebSocket frame.
    Message(Vec<u8>),
}

/// A WebSocket gateway without TLS: answers the upgrade, reads the relay init
/// from the first frame and sends every later message back reversed.
async fn gateway() -> (SocketAddr, mpsc::UnboundedReceiver<Seen>) {
    let listener = TcpListener::bind(local()).await.unwrap();
    let address = listener.local_addr().unwrap();
    let (seen, events) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let seen = seen.clone();
            tokio::spawn(async move {
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    match stream.read_u8().await {
                        Ok(byte) => request.push(byte),
                        // A pooled connection nobody used.
                        Err(_) => return,
                    }
                }
                let first = String::from_utf8_lossy(&request);
                let first: Vec<_> = first.lines().take(2).collect();
                stream
                    .write_all(b"HTTP/1.1 101 Switching Protocols\r\n\r\n")
                    .await
                    .unwrap();
                let mut ciphers: Option<(Cipher, Cipher)> = None;
                loop {
                    let Ok(head) = stream.read_u16().await else {
                        return;
                    };
                    assert_eq!(head >> 8, 0x82, "one final binary frame per message");
                    assert_ne!(head & 0x80, 0, "a client masks its frames");
                    let len = match head & 0x7f {
                        126 => stream.read_u16().await.unwrap() as usize,
                        127 => stream.read_u64().await.unwrap() as usize,
                        len => len as usize,
                    };
                    let mut mask = [0u8; 4];
                    stream.read_exact(&mut mask).await.unwrap();
                    let mut payload = vec![0u8; len];
                    stream.read_exact(&mut payload).await.unwrap();
                    for (i, byte) in payload.iter_mut().enumerate() {
                        *byte ^= mask[i % 4];
                    }
                    let Some((receive, send)) = &mut ciphers else {
                        let init: [u8; 64] = payload.try_into().expect("64-byte relay init");
                        let (transport, dc, receive, send) = mtproto::server_side(&init).unwrap();
                        seen.send(Seen::Request(first.join(" | "))).unwrap();
                        seen.send(Seen::Init(transport, dc)).unwrap();
                        ciphers = Some((receive, send));
                        continue;
                    };
                    receive.apply_keystream(&mut payload);
                    seen.send(Seen::Message(payload.clone())).unwrap();
                    payload.reverse();
                    send.apply_keystream(&mut payload);
                    let mut frame = vec![0x82, 126];
                    frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
                    frame.extend_from_slice(&payload);
                    stream.write_all(&frame).await.unwrap();
                }
            });
        }
    });
    (address, events)
}

/// A datacenter on plain TCP: reads the relay init, then echoes reversed.
async fn datacenter() -> (SocketAddr, mpsc::UnboundedReceiver<Seen>) {
    let listener = TcpListener::bind(local()).await.unwrap();
    let address = listener.local_addr().unwrap();
    let (seen, events) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let seen = seen.clone();
            tokio::spawn(async move {
                let mut init = [0u8; 64];
                stream.read_exact(&mut init).await.unwrap();
                let (transport, dc, mut receive, mut send) = mtproto::server_side(&init).unwrap();
                seen.send(Seen::Init(transport, dc)).unwrap();
                let mut buffer = [0u8; 4096];
                loop {
                    let n = stream.read(&mut buffer).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    let mut data = buffer[..n].to_vec();
                    receive.apply_keystream(&mut data);
                    seen.send(Seen::Message(data.clone())).unwrap();
                    data.reverse();
                    send.apply_keystream(&mut data);
                    stream.write_all(&data).await.unwrap();
                }
            });
        }
    });
    (address, events)
}

async fn started(data: &Path, routes: Routes) -> (Manager, SocketAddr, [u8; SECRET_LEN]) {
    let manager = Manager::with_routes(data.to_owned(), local(), routes);
    manager.set(true).await.unwrap();
    let status = manager.status().await;
    let link = status.own_link().expect("a link of the proxy's own form");
    let port: u16 = link
        .split("port=")
        .nth(1)
        .unwrap()
        .split('&')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let mut secret = [0u8; SECRET_LEN];
    hex::decode_to_slice(link.rsplit("secret=dd").next().unwrap(), &mut secret).unwrap();
    (
        manager,
        SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
        secret,
    )
}

fn intermediate(payload: &[u8]) -> Vec<u8> {
    let mut packet = (payload.len() as u32).to_le_bytes().to_vec();
    packet.extend_from_slice(payload);
    packet
}

async fn next(events: &mut mpsc::UnboundedReceiver<Seen>) -> Seen {
    tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("the stand-in server saw nothing")
        .unwrap()
}

#[tokio::test]
async fn client_reaches_the_gateway_with_one_packet_per_frame() {
    let dir = tempfile::tempdir().unwrap();
    let (gateway, mut events) = gateway().await;
    let (manager, proxy, secret) = started(dir.path(), routes(&[(4, gateway)], &[])).await;

    let mut client = TcpStream::connect(proxy).await.unwrap();
    let (init, mut send, mut receive) = mtproto::client_init(&secret, Transport::Padded, -4);
    client.write_all(&init).await.unwrap();
    // Two packets in one write and a third in two: the gateway must get three
    // frames, each a whole packet.
    let packets = [
        intermediate(&[1; 40]),
        intermediate(&[2; 300]),
        intermediate(&[3; 16]),
    ];
    let mut stream = packets.concat();
    send.apply_keystream(&mut stream);
    let (first, rest) = stream.split_at(packets[0].len() + packets[1].len() + 7);
    client.write_all(first).await.unwrap();
    client.flush().await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    client.write_all(rest).await.unwrap();

    assert_eq!(
        next(&mut events).await,
        Seen::Request("GET /apiws HTTP/1.1 | Host: kws4-1.web.telegram.org".into())
    );
    assert_eq!(next(&mut events).await, Seen::Init(Transport::Padded, -4));
    for packet in &packets {
        assert_eq!(next(&mut events).await, Seen::Message(packet.clone()));
    }

    // The gateway's answers come back decrypted for this client.
    let mut expected: Vec<u8> = Vec::new();
    for packet in &packets {
        expected.extend(packet.iter().rev());
    }
    let mut answer = vec![0u8; expected.len()];
    client.read_exact(&mut answer).await.unwrap();
    receive.apply_keystream(&mut answer);
    assert_eq!(answer, expected);

    let status = manager.status().await;
    assert_eq!(
        (
            status.active,
            status.websocket,
            status.direct,
            status.failed
        ),
        (1, 1, 0, 0)
    );
    drop(client);
}

#[tokio::test]
async fn datacenter_without_a_gateway_is_reached_directly() {
    let dir = tempfile::tempdir().unwrap();
    let (direct, mut events) = datacenter().await;
    let (manager, proxy, secret) = started(dir.path(), routes(&[], &[(5, direct)])).await;

    let mut client = TcpStream::connect(proxy).await.unwrap();
    let (init, mut send, mut receive) = mtproto::client_init(&secret, Transport::Abridged, 5);
    client.write_all(&init).await.unwrap();
    let mut message = b"straight to the datacenter".to_vec();
    send.apply_keystream(&mut message);
    client.write_all(&message).await.unwrap();

    assert_eq!(next(&mut events).await, Seen::Init(Transport::Abridged, 5));
    assert_eq!(
        next(&mut events).await,
        Seen::Message(b"straight to the datacenter".to_vec())
    );
    let mut answer = vec![0u8; message.len()];
    client.read_exact(&mut answer).await.unwrap();
    receive.apply_keystream(&mut answer);
    answer.reverse();
    assert_eq!(answer, b"straight to the datacenter");
    let status = manager.status().await;
    assert_eq!((status.websocket, status.direct), (0, 1));
}

#[tokio::test]
async fn wrong_secret_and_unknown_datacenter_never_reach_telegram() {
    let dir = tempfile::tempdir().unwrap();
    let (gateway, mut events) = gateway().await;
    let (manager, proxy, secret) = started(dir.path(), routes(&[(2, gateway)], &[])).await;

    let mut stranger = TcpStream::connect(proxy).await.unwrap();
    let (init, ..) = mtproto::client_init(&[0; SECRET_LEN], Transport::Padded, 2);
    stranger.write_all(&init).await.unwrap();
    stranger.shutdown().await.unwrap();
    assert_eq!(stranger.read(&mut [0u8; 16]).await.unwrap(), 0);

    // The right secret, but a datacenter with no route at all.
    let mut client = TcpStream::connect(proxy).await.unwrap();
    let (init, ..) = mtproto::client_init(&secret, Transport::Padded, 9);
    client.write_all(&init).await.unwrap();
    assert_eq!(client.read(&mut [0u8; 16]).await.unwrap(), 0);

    assert!(events.try_recv().is_err());
    // The counters follow the closed sockets by a moment.
    let mut status = manager.status().await;
    for _ in 0..100 {
        if status.failed == 2 && status.active == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
        status = manager.status().await;
    }
    assert_eq!((status.active, status.websocket, status.failed), (0, 0, 2));
}

#[tokio::test]
async fn switch_and_secret_survive_a_restart_and_off_closes_the_port() {
    let dir = tempfile::tempdir().unwrap();
    let fresh = Manager::with_routes(dir.path().to_owned(), local(), routes(&[], &[]));
    // A fresh installation: off, nothing listens, no link.
    fresh.restore().await;
    let status = fresh.status().await;
    assert!(!status.enabled && !status.running && status.link.is_none());

    let (first, _, secret) = started(dir.path(), routes(&[], &[])).await;
    drop(first);
    // The service restarts: the proxy comes back with the same secret.
    let again = Manager::with_routes(dir.path().to_owned(), local(), routes(&[], &[]));
    again.restore().await;
    let status = again.status().await;
    assert!(status.enabled && status.running);
    assert!(status.link.unwrap().ends_with(&hex::encode(secret)));

    // A busy port is reported, not fatal, and the switch stays on.
    let holder = TcpListener::bind(local()).await.unwrap();
    let taken = holder.local_addr().unwrap();
    let busy = Manager::with_routes(dir.path().to_owned(), taken, routes(&[], &[]));
    busy.restore().await;
    let status = busy.status().await;
    assert!(status.enabled && !status.running);
    assert!(status.error.unwrap().contains("занят"));

    again.set(false).await.unwrap();
    let status = again.status().await;
    assert!(!status.enabled && !status.running && status.link.is_none());
    assert_eq!(std::fs::read(dir.path().join("telegram")).unwrap(), b"0");
}

/// Needs the internet and reaches Telegram: `cargo test -p zapretd --test
/// telegram -- --ignored`. Sends the first, unencrypted step of a key
/// exchange (`req_pq_multi`) through the proxy and expects Telegram's `resPQ`
/// with the same nonce: proof that a real datacenter understood the relayed
/// stream and that its answer came back readable.
#[tokio::test]
#[ignore = "connects to Telegram's WebSocket gateway"]
async fn live_telegram_answers_a_key_exchange_through_the_proxy() {
    let dir = tempfile::tempdir().unwrap();
    for dc in [2i16, 4, -2] {
        let (manager, proxy, secret) = started(dir.path(), Routes::telegram().unwrap()).await;
        let mut client = TcpStream::connect(proxy).await.unwrap();
        let (init, mut send, mut receive) =
            mtproto::client_init(&secret, Transport::Intermediate, dc);
        client.write_all(&init).await.unwrap();

        let nonce: [u8; 16] = std::array::from_fn(|i| 0xa0 + i as u8);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap();
        let message_id = (now.as_secs() << 32) | (u64::from(now.subsec_nanos()) & !3);
        let mut message = vec![0u8; 8];
        message.extend_from_slice(&message_id.to_le_bytes());
        message.extend_from_slice(&20u32.to_le_bytes());
        message.extend_from_slice(&0xbe7e8ef1u32.to_le_bytes());
        message.extend_from_slice(&nonce);
        let mut packet = intermediate(&message);
        send.apply_keystream(&mut packet);
        client.write_all(&packet).await.unwrap();

        let read = async {
            let mut length = [0u8; 4];
            client.read_exact(&mut length).await.unwrap();
            receive.apply_keystream(&mut length);
            let mut answer = vec![0u8; u32::from_le_bytes(length) as usize];
            client.read_exact(&mut answer).await.unwrap();
            receive.apply_keystream(&mut answer);
            answer
        };
        let answer = tokio::time::timeout(Duration::from_secs(20), read)
            .await
            .unwrap_or_else(|_| panic!("DC {dc}: no answer from Telegram"));
        assert_eq!(
            answer[20..24],
            0x05162463u32.to_le_bytes(),
            "DC {dc}: resPQ"
        );
        assert_eq!(answer[24..40], nonce, "DC {dc}: our nonce");
        let status = manager.status().await;
        assert_eq!((status.websocket, status.direct), (1, 0), "DC {dc}: route");
        println!(
            "DC {dc}: resPQ through the WebSocket gateway, {} bytes",
            answer.len()
        );
    }
}
