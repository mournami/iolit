use crate::ProbeResult;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;

pub async fn run() -> Vec<ProbeResult> {
    let targets = [
        (
            "internet",
            "Интернет",
            "https://www.gstatic.com/generate_204",
        ),
        ("youtube", "YouTube", "https://www.youtube.com/"),
        (
            "discord",
            "Discord API",
            "https://discord.com/api/v10/gateway",
        ),
        (
            "cloudflare",
            "Cloudflare",
            "https://www.cloudflare.com/cdn-cgi/trace",
        ),
    ];
    let mut tasks = tokio::task::JoinSet::new();
    for (id, name, url) in targets {
        tasks.spawn(http_probe(id, name, url));
    }
    tasks.spawn(stun_probe());
    let mut results = Vec::new();
    while let Some(result) = tasks.join_next().await {
        if let Ok(result) = result {
            results.push(result);
        }
    }
    results.sort_by(|a, b| a.id.cmp(&b.id));
    results
}

async fn http_probe(id: &str, name: &str, url: &str) -> ProbeResult {
    let start = Instant::now();
    let result = async {
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(9))
            .connect_timeout(Duration::from_secs(5))
            .user_agent("ZapretDesktop/0.1 network-check")
            .build()?;
        let mut response = client.get(url).send().await?;
        let status = response.status();
        if !status.is_success() {
            anyhow::bail!(
                "Сервис ответил HTTP {}. Полная работа не подтверждена.",
                status.as_u16()
            );
        }
        let mut bytes = 0;
        while let Some(chunk) = response.chunk().await? {
            bytes += chunk.len();
            if bytes >= 65536 {
                break;
            }
        }
        Ok::<_, anyhow::Error>(format!("TLS и HTTP работают · получено {bytes} байт"))
    }
    .await;
    ProbeResult {
        id: id.into(),
        name: name.into(),
        protocol: "HTTPS / TCP".into(),
        ok: result.is_ok(),
        latency_ms: start.elapsed().as_millis() as u64,
        detail: result.unwrap_or_else(|e| failure_detail(&e)),
    }
}

/// Failure text with the underlying cause. reqwest's own message ("error
/// sending request for url") hides resets, timeouts, DNS and TLS errors.
/// OS and TLS error texts carry no IP addresses; the URL is a fixed target.
fn failure_detail(error: &anyhow::Error) -> String {
    let mut causes: Vec<String> = Vec::new();
    for cause in error.chain() {
        let text = cause.to_string();
        if causes.last() != Some(&text) {
            causes.push(text);
        }
    }
    // The top-level request message only repeats the URL of this check.
    if causes.len() > 1 && error.downcast_ref::<reqwest::Error>().is_some() {
        causes.remove(0);
    }
    let causes = causes.join(": ");
    let causes = causes.trim_end_matches('.');
    match network_cause(error) {
        Some(summary) => {
            format!("Проверка не прошла: {summary} ({causes}). Причина блокировки не установлена.")
        }
        None => format!("Проверка не прошла: {causes}. Причина блокировки не установлена."),
    }
}

fn network_cause(error: &anyhow::Error) -> Option<&'static str> {
    use std::io::ErrorKind;
    if error
        .downcast_ref::<reqwest::Error>()
        .is_some_and(reqwest::Error::is_timeout)
    {
        return Some("сервер не ответил вовремя");
    }
    for cause in error.chain() {
        if let Some(io) = cause.downcast_ref::<std::io::Error>() {
            match io.kind() {
                ErrorKind::ConnectionReset => return Some("соединение сброшено"),
                ErrorKind::ConnectionAborted => return Some("соединение прервано"),
                ErrorKind::ConnectionRefused => return Some("соединение отклонено"),
                ErrorKind::TimedOut => return Some("сервер не ответил вовремя"),
                ErrorKind::UnexpectedEof => return Some("соединение закрыто до ответа"),
                _ => {}
            }
        }
        let text = cause.to_string().to_ascii_lowercase();
        if text.contains("dns error") {
            return Some("не удалось получить адрес сервера (DNS)");
        }
        if text.contains("certificate") {
            return Some("сертификат сервера не прошёл проверку");
        }
    }
    None
}

async fn stun_probe() -> ProbeResult {
    let start = Instant::now();
    let result = tokio::time::timeout(Duration::from_secs(6), async {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        socket.connect("stun.l.google.com:19302").await?;
        let mut message = [0u8; 20];
        message[1] = 1;
        message[4..8].copy_from_slice(&[0x21, 0x12, 0xa4, 0x42]);
        // Transaction IDs must be unpredictable enough to reject stale/unrelated responses.
        getrandom::fill(&mut message[8..20]).map_err(|e| std::io::Error::other(e.to_string()))?;
        socket.send(&message).await?;
        let mut buf = [0u8; 2048];
        let n = socket.recv(&mut buf).await?;
        if n < 20 || buf[..2] != [1, 1] || buf[4..20] != message[4..20] {
            return Err(std::io::Error::other("Неверный ответ STUN"));
        }
        Ok::<_, std::io::Error>(
            "Получен ответ STUN. Это подтверждает UDP, но не голосовой сеанс Discord.".to_string(),
        )
    })
    .await;
    let (ok, detail) = match result {
        Ok(Ok(s)) => (true, s),
        Ok(Err(e)) => (false, e.to_string()),
        Err(_) => (
            false,
            "Нет ответа STUN за 6 секунд. Причина не установлена.".into(),
        ),
    };
    ProbeResult {
        id: "udp".into(),
        name: "UDP / STUN".into(),
        protocol: "UDP".into(),
        ok,
        latency_ms: start.elapsed().as_millis() as u64,
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Error, ErrorKind};

    #[test]
    fn failure_detail_names_the_underlying_cause() {
        let reset = anyhow::Error::new(Error::new(ErrorKind::ConnectionReset, "reset by peer"))
            .context("client error (Connect)");
        assert_eq!(
            failure_detail(&reset),
            "Проверка не прошла: соединение сброшено (client error (Connect): reset by peer). \
             Причина блокировки не установлена."
        );
        let eof = anyhow::Error::new(Error::new(ErrorKind::UnexpectedEof, "tls handshake eof"));
        assert!(failure_detail(&eof).contains("соединение закрыто до ответа"));
        let tls = anyhow::Error::new(Error::new(
            ErrorKind::InvalidData,
            "invalid peer certificate: UnknownIssuer",
        ));
        assert!(failure_detail(&tls).contains("сертификат сервера не прошёл проверку"));
        let status = anyhow::anyhow!("Сервис ответил HTTP 403. Полная работа не подтверждена.");
        assert_eq!(
            failure_detail(&status),
            "Проверка не прошла: Сервис ответил HTTP 403. Полная работа не подтверждена. \
             Причина блокировки не установлена."
        );
    }
}
