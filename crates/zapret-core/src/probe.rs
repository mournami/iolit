use crate::{
    ProbeResult,
    targets::{Check, TargetList},
};
use std::{
    net::{IpAddr, SocketAddr},
    time::{Duration, Instant},
};
use tokio::net::UdpSocket;

/// Runs every check of `targets` in parallel; results sorted by id.
pub async fn run(targets: &TargetList) -> Vec<ProbeResult> {
    let mut tasks = tokio::task::JoinSet::new();
    for target in &targets.targets {
        let (id, name) = (target.id.clone(), target.name.clone());
        match target.check.clone() {
            Check::Https { url } => {
                tasks.spawn(async move { http_probe(&id, &name, &url).await });
            }
            Check::Stun { host, port } => {
                tasks.spawn(async move { stun_probe(&id, &name, &host, port).await });
            }
        }
    }
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
    let mut warning = None;
    let mut result = fetch(url, None).await;
    // A DNS failure says nothing about DPI: ask a public resolver over HTTPS
    // and check the service by that address, so strategy selection measures
    // the bypass. The DNS problem is reported separately.
    if let Err(error) = &result
        && is_dns_failure(error)
        && let Some(host) = reqwest::Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_owned))
        && let Some(addresses) = resolve_over_https(&host).await
    {
        result = match fetch(url, Some((&host, &addresses))).await {
            Ok(detail) => {
                warning = Some(DNS_WARNING.to_owned());
                Ok(format!("{detail} · адрес получен через защищённый DNS"))
            }
            Err(e) => Err(e.context("системный DNS не знает адрес; по адресу из защищённого DNS")),
        };
    }
    ProbeResult {
        id: id.into(),
        name: name.into(),
        protocol: "HTTPS / TCP".into(),
        ok: result.is_ok(),
        latency_ms: start.elapsed().as_millis() as u64,
        detail: result.unwrap_or_else(|e| failure_detail(&e)),
        warning,
    }
}

const DNS_WARNING: &str = "DNS этого компьютера не знает адрес сервиса, обычно его блокирует \
    провайдер. Обход работает, но браузер может не открыть сайт, пока не включён защищённый DNS.";

async fn fetch(url: &str, resolved: Option<(&str, &[SocketAddr])>) -> anyhow::Result<String> {
    // A DPI block usually shows as a stalled TLS handshake, so every failing
    // check waits for the timeout; selection runs these once per strategy and
    // must stay under 3 minutes. 64 KB arrive well within 7 s on a working line.
    let mut client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(7))
        .connect_timeout(Duration::from_secs(4))
        .user_agent("ZapretDesktop/0.1 network-check");
    if let Some((host, addresses)) = resolved {
        client = client.resolve_to_addrs(host, addresses);
    }
    let mut response = client.build()?.get(url).send().await?;
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
    Ok(format!("TLS и HTTP работают · получено {bytes} байт"))
}

fn is_dns_failure(error: &anyhow::Error) -> bool {
    error
        .chain()
        .any(|cause| cause.to_string().to_ascii_lowercase().contains("dns error"))
}

/// DNS-over-HTTPS JSON API of public resolvers, reached by IP address so the
/// lookup itself needs no DNS. Only the fixed check host is sent.
const DOH_RESOLVERS: [&str; 2] = ["https://1.1.1.1/dns-query", "https://8.8.8.8/resolve"];

async fn resolve_over_https(host: &str) -> Option<Vec<SocketAddr>> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .ok()?;
    for resolver in DOH_RESOLVERS {
        let Ok(response) = client
            .get(resolver)
            .query(&[("name", host), ("type", "A")])
            .header("accept", "application/dns-json")
            .send()
            .await
        else {
            continue;
        };
        let Ok(text) = response.text().await else {
            continue;
        };
        let addresses = parse_doh_answer(&text);
        if !addresses.is_empty() {
            return Some(addresses);
        }
    }
    None
}

fn parse_doh_answer(text: &str) -> Vec<SocketAddr> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    value["Answer"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|record| record["type"] == 1)
        .filter_map(|record| record["data"].as_str()?.parse::<IpAddr>().ok())
        .map(|ip| SocketAddr::new(ip, 443))
        .collect()
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

async fn stun_probe(id: &str, name: &str, host: &str, port: u16) -> ProbeResult {
    let start = Instant::now();
    let result = tokio::time::timeout(Duration::from_secs(6), async {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        socket.connect((host, port)).await?;
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
        id: id.into(),
        name: name.into(),
        protocol: "UDP".into(),
        ok,
        latency_ms: start.elapsed().as_millis() as u64,
        detail,
        warning: None,
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
        assert!(is_dns_failure(
            &anyhow::anyhow!("dns error: host unknown").context("client error (Connect)")
        ));
        assert!(!is_dns_failure(&reset));
        assert_eq!(
            failure_detail(&status),
            "Проверка не прошла: Сервис ответил HTTP 403. Полная работа не подтверждена. \
             Причина блокировки не установлена."
        );
    }

    #[test]
    fn doh_answer_keeps_ipv4_records_only() {
        let answer = r#"{"Status":0,"Answer":[
            {"name":"www.youtube.com","type":5,"data":"youtube-ui.l.google.com."},
            {"name":"youtube-ui.l.google.com","type":1,"data":"192.0.2.10"},
            {"name":"youtube-ui.l.google.com","type":1,"data":"192.0.2.11"}]}"#;
        assert_eq!(
            parse_doh_answer(answer),
            vec![
                "192.0.2.10:443".parse::<SocketAddr>().unwrap(),
                "192.0.2.11:443".parse().unwrap()
            ]
        );
        assert!(parse_doh_answer(r#"{"Status":3}"#).is_empty());
        assert!(parse_doh_answer("not json").is_empty());
    }
}
