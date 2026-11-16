//! Minimal WebSocket client for Telegram's gateway: binary frames only.
//!
//! Ported from Flowseal/tg-ws-proxy (MIT), `proxy/raw_websocket.py`.
use anyhow::{Result, bail};
use base64::Engine as _;
use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, Waker},
};
use tokio::{
    io::{
        AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt,
        BufReader, ReadHalf, WriteHalf,
    },
    sync::Mutex,
};

pub trait Io: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> Io for T {}
/// TLS in production, plain TCP in tests.
pub type Stream = Box<dyn Io>;

const CONTINUATION: u8 = 0x0;
const TEXT: u8 = 0x1;
const BINARY: u8 = 0x2;
const CLOSE: u8 = 0x8;
const PING: u8 = 0x9;
const PONG: u8 = 0xa;
/// Largest message accepted from the gateway.
const MAX_MESSAGE: usize = 16 * 1024 * 1024;
const MAX_RESPONSE_HEAD: usize = 8 * 1024;

pub struct Reader {
    stream: BufReader<ReadHalf<Stream>>,
    /// Pongs and the close reply go out from the reading side.
    writer: Writer,
    fragments: Vec<u8>,
    closed: bool,
}

#[derive(Clone)]
pub struct Writer(Arc<Mutex<WriteHalf<Stream>>>);

/// Upgrades `stream`; fails on anything but `101 Switching Protocols`.
pub async fn connect(stream: Stream, host: &str, path: &str) -> Result<(Reader, Writer)> {
    let (read, mut write) = tokio::io::split(stream);
    let mut key = [0u8; 16];
    getrandom::fill(&mut key)?;
    let key = base64::engine::general_purpose::STANDARD.encode(key);
    let request = format!(
        "GET {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: {key}\r\n\
         Sec-WebSocket-Version: 13\r\n\
         Sec-WebSocket-Protocol: binary\r\n\r\n"
    );
    write.write_all(request.as_bytes()).await?;
    let mut stream = BufReader::new(read);
    let mut head = Vec::new();
    let mut status = None;
    loop {
        let start = head.len();
        let read = (&mut stream)
            .take((MAX_RESPONSE_HEAD - start) as u64)
            .read_until(b'\n', &mut head)
            .await?;
        let line = &head[start..];
        if read == 0 || !line.ends_with(b"\n") {
            bail!("Шлюз Telegram не ответил на запрос WebSocket");
        }
        if status.is_none() {
            let line = String::from_utf8_lossy(line);
            status = Some(
                line.split_whitespace()
                    .nth(1)
                    .and_then(|code| code.parse::<u16>().ok())
                    .unwrap_or(0),
            );
        }
        if line == b"\r\n" || line == b"\n" {
            break;
        }
    }
    match status {
        Some(101) => {}
        code => bail!("Шлюз Telegram ответил HTTP {}", code.unwrap_or(0)),
    }
    let writer = Writer(Arc::new(Mutex::new(write)));
    Ok((
        Reader {
            stream,
            writer: writer.clone(),
            fragments: Vec::new(),
            closed: false,
        },
        writer,
    ))
}

impl Reader {
    /// The next binary message; `None` once the gateway closed the connection.
    pub async fn recv(&mut self) -> Result<Option<Vec<u8>>> {
        while !self.closed {
            let (opcode, fin, payload) = self.frame().await?;
            match opcode {
                CLOSE => {
                    self.closed = true;
                    let code = &payload[..payload.len().min(2)];
                    let _ = self.writer.control(CLOSE, code).await;
                }
                PING => {
                    let _ = self.writer.control(PONG, &payload).await;
                }
                CONTINUATION | TEXT | BINARY => {
                    if fin && self.fragments.is_empty() {
                        return Ok(Some(payload));
                    }
                    if self.fragments.len() + payload.len() > MAX_MESSAGE {
                        bail!("Слишком большое сообщение от шлюза Telegram");
                    }
                    self.fragments.extend_from_slice(&payload);
                    if fin {
                        return Ok(Some(std::mem::take(&mut self.fragments)));
                    }
                }
                _ => {}
            }
        }
        Ok(None)
    }

    async fn frame(&mut self) -> Result<(u8, bool, Vec<u8>)> {
        let mut head = [0u8; 2];
        self.stream.read_exact(&mut head).await?;
        let len = match head[1] & 0x7f {
            126 => self.stream.read_u16().await? as u64,
            127 => self.stream.read_u64().await?,
            len => len as u64,
        };
        if len > MAX_MESSAGE as u64 {
            bail!("Слишком большой кадр от шлюза Telegram");
        }
        let mut mask = [0u8; 4];
        if head[1] & 0x80 != 0 {
            self.stream.read_exact(&mut mask).await?;
        }
        let mut payload = vec![0u8; len as usize];
        self.stream.read_exact(&mut payload).await?;
        apply_mask(&mut payload, mask);
        Ok((head[0] & 0x0f, head[0] & 0x80 != 0, payload))
    }

    /// Whether an idle connection was closed or broke meanwhile. Never waits;
    /// data that already arrived stays for `recv`.
    pub fn is_dead(&mut self) -> bool {
        let mut context = Context::from_waker(Waker::noop());
        match Pin::new(&mut self.stream).poll_fill_buf(&mut context) {
            Poll::Ready(Ok(buffered)) => buffered.is_empty(),
            Poll::Ready(Err(_)) => true,
            Poll::Pending => self.closed,
        }
    }
}

impl Writer {
    pub async fn send(&self, data: &[u8]) -> Result<()> {
        self.write(&frame(BINARY, data)?).await
    }

    /// Several messages in one write.
    pub async fn send_batch(&self, parts: &[Vec<u8>]) -> Result<()> {
        let mut out = Vec::with_capacity(parts.iter().map(|p| p.len() + 14).sum());
        for part in parts {
            out.extend_from_slice(&frame(BINARY, part)?);
        }
        self.write(&out).await
    }

    async fn control(&self, opcode: u8, payload: &[u8]) -> Result<()> {
        // Control frames carry at most 125 bytes.
        self.write(&frame(opcode, &payload[..payload.len().min(125)])?)
            .await
    }

    async fn write(&self, bytes: &[u8]) -> Result<()> {
        let mut stream = self.0.lock().await;
        stream.write_all(bytes).await?;
        stream.flush().await?;
        Ok(())
    }

    pub async fn close(&self) {
        let _ = self.control(CLOSE, &[]).await;
        let _ = self.0.lock().await.shutdown().await;
    }
}

fn apply_mask(data: &mut [u8], mask: [u8; 4]) {
    if mask != [0; 4] {
        for (i, byte) in data.iter_mut().enumerate() {
            *byte ^= mask[i % 4];
        }
    }
}

/// One final frame, masked as a client must.
fn frame(opcode: u8, data: &[u8]) -> Result<Vec<u8>> {
    let mut mask = [0u8; 4];
    getrandom::fill(&mut mask)?;
    let mut out = Vec::with_capacity(data.len() + 14);
    out.push(0x80 | opcode);
    match data.len() {
        len @ 0..126 => out.push(0x80 | len as u8),
        len @ 126..65536 => {
            out.push(0x80 | 126);
            out.extend_from_slice(&(len as u16).to_be_bytes());
        }
        len => {
            out.push(0x80 | 127);
            out.extend_from_slice(&(len as u64).to_be_bytes());
        }
    }
    out.extend_from_slice(&mask);
    let start = out.len();
    out.extend_from_slice(data);
    apply_mask(&mut out[start..], mask);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    /// The gateway's end of an upgraded connection: reads the request, answers
    /// with `status`.
    async fn accept(mut server: tokio::io::DuplexStream, status: &str) -> tokio::io::DuplexStream {
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            request.push(server.read_u8().await.unwrap());
        }
        let request = String::from_utf8(request).unwrap();
        assert!(request.starts_with("GET /apiws HTTP/1.1\r\nHost: kws2.web.telegram.org\r\n"));
        assert!(request.contains("Sec-WebSocket-Protocol: binary\r\n"));
        server
            .write_all(format!("HTTP/1.1 {status}\r\nUpgrade: websocket\r\n\r\n").as_bytes())
            .await
            .unwrap();
        server
    }

    #[tokio::test]
    async fn messages_pings_fragments_and_close() {
        let (client, server) = duplex(1 << 20);
        let gateway = tokio::spawn(async move {
            let mut server = accept(server, "101 Switching Protocols").await;
            // Unmasked frames, as a server sends: a message, a ping, a message
            // in two fragments, a 300-byte message, close.
            server.write_all(&[0x82, 3, 1, 2, 3]).await.unwrap();
            server.write_all(&[0x89, 2, 7, 7]).await.unwrap();
            server.write_all(&[0x02, 2, 4, 5]).await.unwrap();
            server.write_all(&[0x80, 1, 6]).await.unwrap();
            server.write_all(&[0x82, 126, 1, 44]).await.unwrap();
            server.write_all(&[9; 300]).await.unwrap();
            server.write_all(&[0x88, 2, 3, 232]).await.unwrap();
            let mut received = Vec::new();
            server.read_to_end(&mut received).await.unwrap();
            received
        });
        let (mut reader, writer) = connect(Box::new(client), "kws2.web.telegram.org", "/apiws")
            .await
            .unwrap();
        writer.send(b"hello").await.unwrap();
        assert_eq!(reader.recv().await.unwrap(), Some(vec![1, 2, 3]));
        assert_eq!(reader.recv().await.unwrap(), Some(vec![4, 5, 6]));
        assert_eq!(reader.recv().await.unwrap(), Some(vec![9; 300]));
        assert_eq!(reader.recv().await.unwrap(), None);
        writer.close().await;
        drop(reader);

        // What the gateway got: the masked message, the pong, the close reply.
        let received = gateway.await.unwrap();
        let unmask = |frame: &[u8]| -> Vec<u8> {
            let mask = [frame[2], frame[3], frame[4], frame[5]];
            let mut payload = frame[6..].to_vec();
            apply_mask(&mut payload, mask);
            payload
        };
        assert_eq!(received[..2], [0x82, 0x80 | 5]);
        assert_eq!(unmask(&received[..11]), b"hello");
        assert_eq!(received[11..13], [0x8a, 0x80 | 2]);
        assert_eq!(unmask(&received[11..19]), [7, 7]);
        assert_eq!(received[19..21], [0x88, 0x80 | 2]);
        assert_eq!(unmask(&received[19..27]), [3, 232]);
    }

    #[tokio::test]
    async fn redirect_is_an_error_and_dead_connection_is_seen() {
        let (client, server) = duplex(4096);
        tokio::spawn(accept(server, "302 Found"));
        let error = connect(Box::new(client), "kws2.web.telegram.org", "/apiws")
            .await
            .err()
            .unwrap();
        assert!(error.to_string().contains("HTTP 302"));

        let (client, server) = duplex(4096);
        let gateway = tokio::spawn(accept(server, "101 Switching Protocols"));
        let (mut reader, _writer) = connect(Box::new(client), "kws2.web.telegram.org", "/apiws")
            .await
            .unwrap();
        let server = gateway.await.unwrap();
        assert!(!reader.is_dead());
        drop(server);
        tokio::task::yield_now().await;
        assert!(reader.is_dead());
    }

    #[test]
    fn long_frames_use_the_extended_length() {
        let short = frame(BINARY, &[1; 125]).unwrap();
        assert_eq!((short[1], short.len()), (0x80 | 125, 2 + 4 + 125));
        let medium = frame(BINARY, &[1; 126]).unwrap();
        assert_eq!(medium[1..4], [0x80 | 126, 0, 126]);
        let long = frame(BINARY, &[1; 65536]).unwrap();
        assert_eq!(long[1], 0x80 | 127);
        assert_eq!(long[2..10], 65536u64.to_be_bytes());
        assert_eq!(long.len(), 10 + 4 + 65536);
    }
}
