//! Secure DNS: a public resolver over HTTPS on the computer's network adapters.
//!
//! Some providers drop plain DNS queries for blocked names, so YouTube has no
//! address even when the bypass works; browsers cope with their own secure
//! DNS, other programs do not. This sets what Windows 11 offers in Settings →
//! Network → DNS server assignment: the resolver's IPv4 and IPv6 addresses
//! with "DNS over HTTPS: on (automatic template)" and no fallback to plain
//! text. The previous settings of every adapter are saved first and turning
//! it off restores them exactly. Only the user's command changes anything.
//!
//! Without fallback a resolver the network blocks leaves the computer with no
//! DNS at all, so every resolver is checked after it is applied: Cloudflare,
//! then Google, then Quad9. If none answers, the saved settings come back.
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::{path::Path, time::Duration};
use zapret_core::api::SecureDns;

/// Previous settings, kept in the admin-only data folder while enabled.
const BACKUP: &str = "secure-dns.json";
/// How long one resolver gets to answer after it is applied. Three resolvers
/// must fit in the 20 s request timeout.
const VERIFY: Duration = Duration::from_secs(4);
/// Names checked through the system resolver; any answer proves DNS works.
pub const CHECK_NAMES: [&str; 2] = ["www.youtube.com", "www.microsoft.com"];

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Backup {
    pub adapters: Vec<AdapterBackup>,
    /// Resolver in use; absent while it is being checked.
    #[serde(default)]
    pub provider: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdapterBackup {
    /// `{GUID}` as Windows names the adapter.
    pub guid: String,
    pub name: String,
    pub ipv4: FamilySettings,
    pub ipv6: FamilySettings,
}

/// Static servers (empty: from DHCP) and per-server DNS-over-HTTPS settings.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FamilySettings {
    pub servers: String,
    pub doh: Vec<DohServer>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DohServer {
    pub index: u32,
    pub template: Option<String>,
    pub flags: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Adapter {
    pub guid: String,
    pub name: String,
}

/// Resolvers whose DoH templates Windows 11 knows, so "automatic template"
/// works without typing a URL.
pub struct Provider {
    pub name: &'static str,
    pub ipv4: &'static str,
    pub ipv6: &'static str,
}

pub const PROVIDERS: [Provider; 3] = [
    Provider {
        name: "Cloudflare",
        ipv4: "1.1.1.1,1.0.0.1",
        ipv6: "2606:4700:4700::1111,2606:4700:4700::1001",
    },
    Provider {
        name: "Google",
        ipv4: "8.8.8.8,8.8.4.4",
        ipv6: "2001:4860:4860::8888,2001:4860:4860::8844",
    },
    Provider {
        name: "Quad9",
        ipv4: "9.9.9.9,149.112.112.112",
        ipv6: "2620:fe::fe,2620:fe::9",
    },
];

/// What the operating system has to offer; faked in tests.
pub trait Dns {
    fn supported(&self) -> bool;
    fn adapters(&self) -> Result<Vec<Adapter>>;
    fn read(&self, adapter: &Adapter) -> Result<AdapterBackup>;
    fn apply(&self, adapter: &Adapter, provider: &Provider) -> Result<()>;
    fn restore(&self, saved: &AdapterBackup) -> Result<()>;
    /// Whether one of `CHECK_NAMES` resolves through the system resolver
    /// within `wait`, skipping its cache.
    fn resolves(&self, wait: Duration) -> bool;
}

pub fn system() -> impl Dns {
    imp::System
}

pub fn state(data: &Path) -> SecureDns {
    let backup = std::fs::read(data.join(BACKUP))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Backup>(&bytes).ok());
    SecureDns {
        supported: imp::System.supported(),
        enabled: data.join(BACKUP).exists(),
        provider: backup.and_then(|b| b.provider),
    }
}

pub fn set(data: &Path, enabled: bool) -> Result<()> {
    if enabled {
        enable(&system(), data)
    } else {
        disable(&system(), data)
    }
}

fn read_backup(data: &Path) -> Result<Backup> {
    match std::fs::read(data.join(BACKUP)) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Backup::default()),
        Err(e) => Err(e.into()),
    }
}

fn write_backup(data: &Path, backup: &Backup) -> Result<()> {
    std::fs::create_dir_all(data)?;
    let fresh = data.join(format!("{BACKUP}.new"));
    std::fs::write(&fresh, serde_json::to_vec_pretty(backup)?)?;
    std::fs::rename(fresh, data.join(BACKUP))?;
    Ok(())
}

pub fn enable(dns: &impl Dns, data: &Path) -> Result<()> {
    if !dns.supported() {
        bail!(
            "Защищённый DNS для всей системы есть только в Windows 11. В Windows 10 включите \
             безопасный DNS в браузере."
        );
    }
    let mut backup = read_backup(data)?;
    let adapters = dns.adapters()?;
    if adapters.is_empty() {
        bail!("Не найдено активное сетевое подключение");
    }
    // Saved before anything changes; an adapter already switched keeps its
    // original backup, so enabling twice never loses the user's settings.
    for adapter in &adapters {
        if !backup.adapters.iter().any(|b| b.guid == adapter.guid) {
            backup.adapters.push(dns.read(adapter)?);
        }
    }
    backup.provider = None;
    write_backup(data, &backup)?;
    let mut tried = Vec::new();
    for provider in &PROVIDERS {
        let applied = adapters
            .iter()
            .try_for_each(|adapter| dns.apply(adapter, provider));
        match applied {
            Ok(()) if dns.resolves(VERIFY) => {
                backup.provider = Some(provider.name.into());
                return write_backup(data, &backup);
            }
            Ok(()) => tried.push(format!("{} не ответил", provider.name)),
            Err(e) => tried.push(format!("{}: {e:#}", provider.name)),
        }
    }
    let restored = disable(dns, data);
    let tried = tried.join("; ");
    match restored {
        Ok(()) => bail!(
            "Защищённый DNS не заработал в этой сети ({tried}). Прежние настройки DNS \
             возвращены."
        ),
        Err(e) => bail!("Защищённый DNS не заработал ({tried}). {e:#}"),
    }
}

pub fn disable(dns: &impl Dns, data: &Path) -> Result<()> {
    let backup = read_backup(data)?;
    let mut failed = Vec::new();
    for saved in &backup.adapters {
        if let Err(e) = dns.restore(saved) {
            failed.push(format!("{}: {e:#}", saved.name));
        }
    }
    if !failed.is_empty() {
        bail!(
            "Не удалось вернуть настройки DNS: {}. Проверьте их в «Параметрах» → «Сеть и \
             Интернет».",
            failed.join("; ")
        );
    }
    match std::fs::remove_file(data.join(BACKUP)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{Adapter, AdapterBackup, Dns, Provider};
    use anyhow::{Result, bail};
    use std::time::Duration;

    pub struct System;
    impl Dns for System {
        fn supported(&self) -> bool {
            false
        }
        fn adapters(&self) -> Result<Vec<Adapter>> {
            Ok(Vec::new())
        }
        fn read(&self, _: &Adapter) -> Result<AdapterBackup> {
            bail!("Только Windows")
        }
        fn apply(&self, _: &Adapter, _: &Provider) -> Result<()> {
            bail!("Только Windows")
        }
        fn restore(&self, _: &AdapterBackup) -> Result<()> {
            bail!("Только Windows")
        }
        fn resolves(&self, _: Duration) -> bool {
            false
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::{Adapter, AdapterBackup, CHECK_NAMES, Dns, DohServer, FamilySettings, Provider};
    use anyhow::{Context, Result, bail};
    use std::time::{Duration, Instant};
    use windows_sys::{
        Win32::{
            NetworkManagement::{Dns::*, IpHelper::*, Ndis::IfOperStatusUp},
            System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW},
        },
        core::GUID,
    };

    pub struct System;
    impl Dns for System {
        /// Per-adapter DNS over HTTPS appeared in Windows 11 (build 22000).
        fn supported(&self) -> bool {
            build().is_some_and(|b| b >= 22000)
        }
        fn adapters(&self) -> Result<Vec<Adapter>> {
            adapters()
        }
        fn read(&self, adapter: &Adapter) -> Result<AdapterBackup> {
            let id = parse_guid(&adapter.guid).context("неверный идентификатор адаптера")?;
            Ok(AdapterBackup {
                guid: adapter.guid.clone(),
                name: adapter.name.clone(),
                ipv4: read_family(id, false)?,
                ipv6: read_family(id, true)?,
            })
        }
        fn apply(&self, adapter: &Adapter, provider: &Provider) -> Result<()> {
            let id = parse_guid(&adapter.guid).context("неверный идентификатор адаптера")?;
            for (ipv6, servers) in [(false, provider.ipv4), (true, provider.ipv6)] {
                let doh = (0..servers.split(',').count() as u32)
                    .map(|index| DohServer {
                        index,
                        template: None,
                        flags: DNS_DOH_SERVER_SETTINGS_ENABLE_AUTO as u64,
                    })
                    .collect::<Vec<_>>();
                let settings = FamilySettings {
                    servers: servers.into(),
                    doh,
                };
                write_family(id, ipv6, &settings).with_context(|| {
                    format!("{}, IPv{}", adapter.name, if ipv6 { 6 } else { 4 })
                })?;
            }
            Ok(())
        }
        fn restore(&self, saved: &AdapterBackup) -> Result<()> {
            let id = parse_guid(&saved.guid).context("неверный идентификатор адаптера")?;
            write_family(id, false, &saved.ipv4)?;
            write_family(id, true, &saved.ipv6)
        }
        fn resolves(&self, wait: Duration) -> bool {
            resolves(&CHECK_NAMES, wait)
        }
    }

    /// Asks the DNS Client service, which applies the adapters' DoH settings,
    /// bypassing its cache and the hosts file. New settings take a moment, so
    /// the query repeats until `wait` runs out.
    pub fn resolves(names: &[&str], wait: Duration) -> bool {
        let deadline = Instant::now() + wait;
        loop {
            if names.iter().any(|name| query(name)) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    }

    fn query(name: &str) -> bool {
        let name = wide(name);
        let mut records: *mut DNS_RECORDA = std::ptr::null_mut();
        let status = unsafe {
            DnsQuery_W(
                name.as_ptr(),
                DNS_TYPE_A,
                DNS_QUERY_BYPASS_CACHE | DNS_QUERY_NO_HOSTS_FILE,
                std::ptr::null_mut(),
                &mut records,
                std::ptr::null_mut(),
            )
        };
        if !records.is_null() {
            unsafe { DnsFree(records as *const _, DnsFreeRecordList) };
        }
        status == 0
    }

    fn build() -> Option<u32> {
        let key = wide(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");
        let value = wide("CurrentBuildNumber");
        let mut buffer = [0u16; 32];
        let mut size = std::mem::size_of_val(&buffer) as u32;
        let status = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buffer.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if status != 0 {
            return None;
        }
        String::from_utf16_lossy(&buffer)
            .trim_end_matches('\0')
            .parse()
            .ok()
    }

    /// Connected Ethernet and Wi-Fi adapters with a default gateway. VPN and
    /// virtual adapters keep their own DNS.
    fn adapters() -> Result<Vec<Adapter>> {
        const ETHERNET: u32 = 6;
        const WIFI: u32 = 71;
        let flags = GAA_FLAG_INCLUDE_GATEWAYS | GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST;
        let mut size = 16 * 1024u32;
        let mut buffer: Vec<u64>;
        loop {
            buffer = vec![0u64; size as usize / 8 + 1];
            let status = unsafe {
                GetAdaptersAddresses(
                    0,
                    flags,
                    std::ptr::null(),
                    buffer.as_mut_ptr().cast(),
                    &mut size,
                )
            };
            match status {
                0 => break,
                111 => continue, // ERROR_BUFFER_OVERFLOW: size was updated
                232 => return Ok(Vec::new()), // ERROR_NO_DATA
                e => bail!("Список сетевых адаптеров недоступен (код {e})"),
            }
        }
        let mut adapters = Vec::new();
        let mut next = buffer.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
        while let Some(a) = unsafe { next.as_ref() } {
            next = a.Next;
            if a.OperStatus != IfOperStatusUp
                || !(a.IfType == ETHERNET || a.IfType == WIFI)
                || a.FirstGatewayAddress.is_null()
            {
                continue;
            }
            let description = unsafe { pwstr(a.Description) };
            let lower = description.to_lowercase();
            if [
                "vpn",
                "tap-",
                "wireguard",
                "wintun",
                "virtual",
                "hyper-v",
                "vmware",
                "virtualbox",
            ]
            .iter()
            .any(|word| lower.contains(word))
            {
                continue;
            }
            let guid = unsafe { std::ffi::CStr::from_ptr(a.AdapterName.cast()) }
                .to_string_lossy()
                .into_owned();
            if parse_guid(&guid).is_none() {
                continue;
            }
            let friendly = unsafe { pwstr(a.FriendlyName) };
            adapters.push(Adapter {
                guid,
                name: if friendly.is_empty() {
                    description
                } else {
                    friendly
                },
            });
        }
        Ok(adapters)
    }

    /// Adapter names with the raw bytes of their first gateway address: a
    /// new Wi-Fi network changes the gateway even on the same adapter.
    pub fn signature() -> Option<String> {
        let flags = GAA_FLAG_INCLUDE_GATEWAYS | GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST;
        let mut size = 16 * 1024u32;
        let mut buffer: Vec<u64>;
        loop {
            buffer = vec![0u64; size as usize / 8 + 1];
            let status = unsafe {
                GetAdaptersAddresses(
                    0,
                    flags,
                    std::ptr::null(),
                    buffer.as_mut_ptr().cast(),
                    &mut size,
                )
            };
            match status {
                0 => break,
                111 => continue,
                232 => return Some(String::new()),
                _ => return None,
            }
        }
        let mut parts = Vec::new();
        let mut next = buffer.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
        while let Some(a) = unsafe { next.as_ref() } {
            next = a.Next;
            if a.OperStatus != IfOperStatusUp {
                continue;
            }
            let name = unsafe { std::ffi::CStr::from_ptr(a.AdapterName.cast()) }
                .to_string_lossy()
                .into_owned();
            let gateway = unsafe { a.FirstGatewayAddress.as_ref() }
                .and_then(|g| {
                    let address = g.Address;
                    (!address.lpSockaddr.is_null()).then(|| unsafe {
                        std::slice::from_raw_parts(
                            address.lpSockaddr as *const u8,
                            address.iSockaddrLength as usize,
                        )
                        .to_vec()
                    })
                })
                .unwrap_or_default();
            parts.push(format!("{name}={gateway:02x?}"));
        }
        parts.sort();
        Some(parts.join(";"))
    }

    fn read_family(id: GUID, ipv6: bool) -> Result<FamilySettings> {
        let mut settings: DNS_INTERFACE_SETTINGS3 = unsafe { std::mem::zeroed() };
        settings.Version = DNS_INTERFACE_SETTINGS_VERSION3;
        if ipv6 {
            settings.Flags = DNS_SETTING_IPV6 as u64;
        }
        let status = unsafe {
            GetInterfaceDnsSettings(id, (&mut settings as *mut DNS_INTERFACE_SETTINGS3).cast())
        };
        if status != 0 {
            bail!("Не удалось прочитать настройки DNS (код {status})");
        }
        let servers = unsafe { pwstr(settings.NameServer) };
        let mut doh = Vec::new();
        for i in 0..settings.cServerProperties as usize {
            let property = unsafe { &*settings.ServerProperties.add(i) };
            if property.Type != DnsServerDohProperty {
                continue;
            }
            let Some(value) = (unsafe { property.Property.DohSettings.as_ref() }) else {
                continue;
            };
            let template = unsafe { pwstr(value.Template) };
            doh.push(DohServer {
                index: property.ServerIndex,
                template: (!template.is_empty()).then_some(template),
                flags: value.Flags,
            });
        }
        unsafe { FreeInterfaceDnsSettings((&mut settings as *mut DNS_INTERFACE_SETTINGS3).cast()) };
        Ok(FamilySettings { servers, doh })
    }

    fn write_family(id: GUID, ipv6: bool, wanted: &FamilySettings) -> Result<()> {
        // Everything the pointers below refer to lives until the call returns.
        let mut servers = wide(&wanted.servers);
        let mut templates: Vec<Option<Vec<u16>>> = wanted
            .doh
            .iter()
            .map(|d| d.template.as_deref().map(wide))
            .collect();
        let mut doh: Vec<DNS_DOH_SERVER_SETTINGS> = wanted
            .doh
            .iter()
            .zip(templates.iter_mut())
            .map(|(d, t)| DNS_DOH_SERVER_SETTINGS {
                Template: t.as_mut().map_or(std::ptr::null_mut(), |t| t.as_mut_ptr()),
                Flags: d.flags,
            })
            .collect();
        let mut properties: Vec<DNS_SERVER_PROPERTY> = wanted
            .doh
            .iter()
            .zip(doh.iter_mut())
            .map(|(d, s)| DNS_SERVER_PROPERTY {
                Version: DNS_SERVER_PROPERTY_VERSION1,
                ServerIndex: d.index,
                Type: DnsServerDohProperty,
                Property: DNS_SERVER_PROPERTY_TYPES { DohSettings: s },
            })
            .collect();
        let mut settings: DNS_INTERFACE_SETTINGS3 = unsafe { std::mem::zeroed() };
        settings.Version = DNS_INTERFACE_SETTINGS_VERSION3;
        settings.Flags = (DNS_SETTING_NAMESERVER | DNS_SETTING_DOH) as u64;
        if ipv6 {
            settings.Flags |= DNS_SETTING_IPV6 as u64;
        }
        // An empty list returns the adapter to servers from DHCP.
        settings.NameServer = servers.as_mut_ptr();
        settings.cServerProperties = properties.len() as u32;
        settings.ServerProperties = if properties.is_empty() {
            std::ptr::null_mut()
        } else {
            properties.as_mut_ptr()
        };
        let status = unsafe {
            SetInterfaceDnsSettings(id, (&settings as *const DNS_INTERFACE_SETTINGS3).cast())
        };
        if status != 0 {
            bail!("Windows не применила настройки DNS (код {status})");
        }
        Ok(())
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }

    unsafe fn pwstr(text: *const u16) -> String {
        if text.is_null() {
            return String::new();
        }
        let mut len = 0;
        while unsafe { *text.add(len) } != 0 {
            len += 1;
        }
        String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, len) })
    }

    fn parse_guid(text: &str) -> Option<GUID> {
        let hex: String = text
            .trim_matches(|c| c == '{' || c == '}')
            .chars()
            .filter(|c| *c != '-')
            .collect();
        (hex.len() == 32)
            .then(|| u128::from_str_radix(&hex, 16).ok())
            .flatten()
            .map(GUID::from_u128)
    }
}

/// Connected adapters and their gateways, for noticing a network change.
/// Kept in memory only; `None` where it cannot be read.
pub fn network_signature() -> Option<String> {
    #[cfg(windows)]
    return imp::signature();
    #[cfg(not(windows))]
    None
}

/// Whether the system resolver finds `name` now; for Doctor. `None` where
/// the check does not exist.
pub fn system_resolves(name: &str, wait: Duration) -> Option<bool> {
    #[cfg(windows)]
    return Some(imp::resolves(&[name], wait));
    #[cfg(not(windows))]
    {
        let _ = (name, wait);
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// One adapter; DNS works only once a resolver in `working` is applied.
    struct Fake {
        working: Vec<&'static str>,
        current: RefCell<String>,
        log: RefCell<Vec<String>>,
        restore_fails: bool,
    }
    impl Fake {
        fn new(working: &[&'static str]) -> Self {
            Self {
                working: working.to_vec(),
                current: RefCell::new("dhcp".into()),
                log: RefCell::new(Vec::new()),
                restore_fails: false,
            }
        }
    }
    impl Dns for Fake {
        fn supported(&self) -> bool {
            true
        }
        fn adapters(&self) -> Result<Vec<Adapter>> {
            Ok(vec![Adapter {
                guid: "{A}".into(),
                name: "Wi-Fi".into(),
            }])
        }
        fn read(&self, adapter: &Adapter) -> Result<AdapterBackup> {
            Ok(AdapterBackup {
                guid: adapter.guid.clone(),
                name: adapter.name.clone(),
                ipv4: FamilySettings {
                    servers: self.current.borrow().clone(),
                    doh: vec![],
                },
                ipv6: FamilySettings::default(),
            })
        }
        fn apply(&self, _: &Adapter, provider: &Provider) -> Result<()> {
            self.log.borrow_mut().push(provider.name.into());
            *self.current.borrow_mut() = provider.name.into();
            Ok(())
        }
        fn restore(&self, saved: &AdapterBackup) -> Result<()> {
            if self.restore_fails {
                bail!("занято");
            }
            *self.current.borrow_mut() = saved.ipv4.servers.clone();
            Ok(())
        }
        fn resolves(&self, _: Duration) -> bool {
            self.working.contains(&self.current.borrow().as_str())
        }
    }

    fn dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("zapret-dns-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn first_resolver_that_answers_is_kept() {
        let data = dir("fallback");
        let dns = Fake::new(&["Google", "Quad9"]);
        enable(&dns, &data).unwrap();
        assert_eq!(*dns.log.borrow(), ["Cloudflare", "Google"]);
        let backup = read_backup(&data).unwrap();
        assert_eq!(backup.provider.as_deref(), Some("Google"));
        assert_eq!(backup.adapters[0].ipv4.servers, "dhcp");
        // Enabling again keeps the original settings, not Google's.
        enable(&dns, &data).unwrap();
        assert_eq!(read_backup(&data).unwrap().adapters[0].ipv4.servers, "dhcp");
        disable(&dns, &data).unwrap();
        assert_eq!(*dns.current.borrow(), "dhcp");
        assert!(!data.join(BACKUP).exists());
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn no_answer_restores_the_previous_settings() {
        let data = dir("none");
        let dns = Fake::new(&[]);
        let error = enable(&dns, &data).unwrap_err().to_string();
        assert!(
            error.contains("Прежние настройки DNS возвращены"),
            "{error}"
        );
        assert!(error.contains("Quad9 не ответил"), "{error}");
        assert_eq!(*dns.current.borrow(), "dhcp");
        assert!(!data.join(BACKUP).exists());
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn failed_restore_keeps_the_backup() {
        let data = dir("stuck");
        let mut dns = Fake::new(&[]);
        dns.restore_fails = true;
        let error = enable(&dns, &data).unwrap_err().to_string();
        assert!(error.contains("Не удалось вернуть"), "{error}");
        // The saved settings stay so "off" or uninstall can retry.
        assert!(data.join(BACKUP).exists());
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn state_reports_the_provider() {
        let data = dir("state");
        assert!(!state(&data).enabled);
        let backup = Backup {
            adapters: vec![],
            provider: Some("Quad9".into()),
        };
        write_backup(&data, &backup).unwrap();
        let state = state(&data);
        assert!(state.enabled);
        assert_eq!(state.provider.as_deref(), Some("Quad9"));
        // Old backups written before the provider field still load.
        std::fs::write(data.join(BACKUP), br#"{"adapters":[]}"#).unwrap();
        assert_eq!(read_backup(&data).unwrap().provider, None);
        let _ = std::fs::remove_dir_all(&data);
    }

    #[cfg(not(windows))]
    #[test]
    fn off_outside_windows() {
        let data = dir("other");
        assert!(!state(&data).supported);
        assert!(set(&data, true).is_err());
        // Turning off with nothing saved is a no-op.
        assert!(set(&data, false).is_ok());
        let _ = std::fs::remove_dir_all(&data);
    }
}
