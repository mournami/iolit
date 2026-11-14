//! Secure DNS: Cloudflare over HTTPS on the computer's network adapters.
//!
//! Some providers drop plain DNS queries for blocked names, so YouTube has no
//! address even when the bypass works; browsers cope with their own secure
//! DNS, other programs do not. This sets what Windows 11 offers in Settings →
//! Network → DNS server assignment: 1.1.1.1 / 1.0.0.1 (and Cloudflare IPv6)
//! with "DNS over HTTPS: on (automatic template)" and no fallback to plain
//! text. The previous settings of every adapter are saved first and turning
//! it off restores them exactly. Only the user's command changes anything.
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use zapret_core::api::SecureDns;

/// Previous settings, kept in the admin-only data folder while enabled.
const BACKUP: &str = "secure-dns.json";

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Backup {
    pub adapters: Vec<AdapterBackup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

pub fn state(data: &Path) -> SecureDns {
    SecureDns {
        supported: imp::supported(),
        enabled: data.join(BACKUP).exists(),
    }
}

pub fn set(data: &Path, enabled: bool) -> Result<()> {
    if enabled { enable(data) } else { disable(data) }
}

fn read_backup(data: &Path) -> Result<Backup> {
    match std::fs::read(data.join(BACKUP)) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Backup::default()),
        Err(e) => Err(e.into()),
    }
}

fn enable(data: &Path) -> Result<()> {
    if !imp::supported() {
        anyhow::bail!(
            "Защищённый DNS для всей системы есть только в Windows 11. В Windows 10 включите \
             безопасный DNS в браузере."
        );
    }
    let mut backup = read_backup(data)?;
    let adapters = imp::adapters()?;
    if adapters.is_empty() {
        anyhow::bail!("Не найдено активное сетевое подключение");
    }
    // Saved before anything changes; an adapter already switched keeps its
    // original backup, so enabling twice never loses the user's settings.
    for adapter in &adapters {
        if !backup.adapters.iter().any(|b| b.guid == adapter.guid) {
            backup.adapters.push(imp::read(adapter)?);
        }
    }
    std::fs::create_dir_all(data)?;
    std::fs::write(data.join(BACKUP), serde_json::to_vec_pretty(&backup)?)?;
    for adapter in &adapters {
        imp::apply_secure(adapter)?;
    }
    Ok(())
}

fn disable(data: &Path) -> Result<()> {
    let backup = read_backup(data)?;
    let mut failed = Vec::new();
    for saved in &backup.adapters {
        if let Err(e) = imp::restore(saved) {
            failed.push(format!("{}: {e:#}", saved.name));
        }
    }
    if !failed.is_empty() {
        anyhow::bail!("Не удалось вернуть настройки DNS: {}", failed.join("; "));
    }
    match std::fs::remove_file(data.join(BACKUP)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

/// Cloudflare servers; Windows 11 knows their DoH templates.
pub const IPV4_SERVERS: &str = "1.1.1.1,1.0.0.1";
pub const IPV6_SERVERS: &str = "2606:4700:4700::1111,2606:4700:4700::1001";

#[cfg(not(windows))]
mod imp {
    use super::AdapterBackup;
    use anyhow::{Result, bail};

    pub struct Adapter {
        pub guid: String,
    }
    pub fn supported() -> bool {
        false
    }
    pub fn adapters() -> Result<Vec<Adapter>> {
        Ok(Vec::new())
    }
    pub fn read(_: &Adapter) -> Result<AdapterBackup> {
        bail!("Только Windows")
    }
    pub fn apply_secure(_: &Adapter) -> Result<()> {
        bail!("Только Windows")
    }
    pub fn restore(_: &AdapterBackup) -> Result<()> {
        bail!("Только Windows")
    }
}

#[cfg(windows)]
mod imp {
    use super::{AdapterBackup, DohServer, FamilySettings, IPV4_SERVERS, IPV6_SERVERS};
    use anyhow::{Context, Result, bail};
    use windows_sys::{
        Win32::{
            NetworkManagement::{IpHelper::*, Ndis::IfOperStatusUp},
            System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW},
        },
        core::GUID,
    };

    pub struct Adapter {
        pub guid: String,
        name: String,
        id: GUID,
    }

    /// Per-adapter DNS over HTTPS appeared in Windows 11 (build 22000).
    pub fn supported() -> bool {
        build().is_some_and(|b| b >= 22000)
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
    pub fn adapters() -> Result<Vec<Adapter>> {
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
            let Some(id) = parse_guid(&guid) else {
                continue;
            };
            let friendly = unsafe { pwstr(a.FriendlyName) };
            adapters.push(Adapter {
                guid,
                name: if friendly.is_empty() {
                    description
                } else {
                    friendly
                },
                id,
            });
        }
        Ok(adapters)
    }

    pub fn read(adapter: &Adapter) -> Result<AdapterBackup> {
        Ok(AdapterBackup {
            guid: adapter.guid.clone(),
            name: adapter.name.clone(),
            ipv4: read_family(adapter.id, false)?,
            ipv6: read_family(adapter.id, true)?,
        })
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

    pub fn apply_secure(adapter: &Adapter) -> Result<()> {
        for (ipv6, servers) in [(false, IPV4_SERVERS), (true, IPV6_SERVERS)] {
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
            write_family(adapter.id, ipv6, &settings)
                .with_context(|| format!("{}, IPv{}", adapter.name, if ipv6 { 6 } else { 4 }))?;
        }
        Ok(())
    }

    pub fn restore(saved: &AdapterBackup) -> Result<()> {
        let id = parse_guid(&saved.guid).context("неверный идентификатор адаптера")?;
        write_family(id, false, &saved.ipv4)?;
        write_family(id, true, &saved.ipv6)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_without_backup_and_off_outside_windows() {
        let dir = std::env::temp_dir().join(format!("zapret-dns-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let state = state(&dir);
        assert!(!state.enabled);
        #[cfg(not(windows))]
        {
            assert!(!state.supported);
            assert!(set(&dir, true).is_err());
            // Turning off with nothing saved is a no-op.
            assert!(set(&dir, false).is_ok());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
