//! Active IPv4 ARP. Subnet membership AND the current on-link route must agree.
//! Cached neighbors are deliberately never used as active response evidence.
use super::*;
use std::{net::Ipv4Addr, sync::OnceLock};
#[cfg(target_os = "macos")]
mod macos;

#[derive(Default)]
pub(super) struct Prober {
    interfaces: OnceLock<Result<Vec<interfaces::Interface>, ToolError>>,
    #[cfg(target_os = "macos")]
    denied: Mutex<BTreeMap<u32, ToolError>>,
}
fn eligible(i: &interfaces::Interface, dest: Ipv4Addr, source: &str, cfg: &Config) -> bool {
    let Ok(local) = i.address.parse::<Ipv4Addr>() else {
        return false;
    };
    if i.index == 0
        || i.prefix == 0
        || i.prefix > 32
        || local.is_loopback()
        || dest.is_loopback()
        || dest == local
        || !cfg.interface.is_empty()
            && cfg.interface != i.name
            && cfg.interface != i.index.to_string()
        || !source.is_empty() && source != i.address
    {
        return false;
    }
    let mask = u32::MAX << (32 - i.prefix);
    let network = u32::from(local) & mask;
    let host = u32::from(dest);
    host & mask == network && (i.prefix >= 31 || host != network && host != (network | !mask))
}
impl Prober {
    pub fn probe(
        &self,
        addr: SocketAddr,
        source: &str,
        cfg: &Config,
        c: &Cancel,
        _permit: Arc<crate::budget::Permit>,
    ) -> Observation {
        let began = Instant::now();
        let deadline = began + Duration::from_millis(cfg.timeout_ms);
        let mut local = source.to_string();
        let result = (|| -> Result<Option<neighbors::Neighbor>, ToolError> {
            cancelled(c)?;
            let IpAddr::V4(ip) = addr.ip() else {
                return Ok(None);
            };
            if ip.is_loopback() {
                return Ok(None);
            }
            let ifaces = self
                .interfaces
                .get_or_init(|| interfaces::list(c))
                .as_ref()
                .map_err(Clone::clone)?;
            let candidates: Vec<_> = ifaces
                .iter()
                .filter(|i| eligible(i, ip, source, cfg))
                .collect();
            if candidates.is_empty() {
                return Ok(None);
            }
            #[cfg(target_os = "macos")]
            {
                let Some(index) = macos::on_link(ip, deadline, c)? else {
                    return Ok(None);
                };
                let Some(iface) = candidates.iter().find(|i| i.index == index) else {
                    return Ok(None);
                };
                local = iface.address.clone();
                if let Some(error) = self.denied.lock().unwrap().get(&index) {
                    return Err(error.clone());
                }
                let result = macos::probe(ip, iface, deadline, c);
                if let Err(e) = &result
                    && e.code == "PERMISSION_DENIED"
                {
                    self.denied.lock().unwrap().insert(index, e.clone());
                }
                result.map(Some)
            }
            #[cfg(windows)]
            {
                let Some(index) = windows_route(ip, source)? else {
                    return Ok(None);
                };
                let Some(iface) = candidates.iter().find(|i| i.index == index) else {
                    return Ok(None);
                };
                local = iface.address.clone();
                windows_probe(ip, iface, deadline, c, _permit).map(Some)
            }
            #[cfg(not(any(target_os = "macos", windows)))]
            {
                let _ = (candidates, deadline);
                Err(ToolError::new(
                    "UNSUPPORTED_CAPABILITY",
                    "Active ARP supports Windows/macOS",
                ))
            }
        })();
        let (status, neighbor, error) = match result {
            Ok(Some(n)) => ("reply", Some(n), None),
            Ok(None) => ("skipped_not_on_link", None, None),
            Err(e) => (
                match e.code.as_str() {
                    "TIMEOUT" => "timeout",
                    "CANCELLED" => "cancelled",
                    _ => "local_error",
                },
                None,
                Some(e),
            ),
        };
        Observation {
            method: "arp".into(),
            status: status.into(),
            responded: neighbor.is_some(),
            port: None,
            elapsed_ms: began.elapsed().as_secs_f64() * 1000.0,
            observed_at: now(),
            source: local,
            local_endpoint: None,
            error,
            neighbor,
        }
    }
}
fn reply(
    ip: Ipv4Addr,
    iface: &interfaces::Interface,
    raw_mac: &[u8],
    adapter: &str,
) -> Result<neighbors::Neighbor, ToolError> {
    let text = raw_mac
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(":");
    let mac = neighbors::mac(&text).ok_or_else(|| {
        ToolError::new(
            "LOCAL_PROBE_FAILED",
            "ARP returned an invalid or redacted MAC",
        )
    })?;
    Ok(neighbors::Neighbor {
        address: ip.to_string(),
        interface: iface.name.clone(),
        interface_index: iface.index,
        mac: Some(mac),
        state: "reply".into(),
        source: adapter.into(),
        observed_at: now(),
        phase: "active".into(),
    })
}

#[cfg(windows)]
fn sockaddr(ip: Ipv4Addr) -> windows_sys::Win32::Networking::WinSock::SOCKADDR_INET {
    use windows_sys::Win32::Networking::WinSock::*;
    let mut addr = SOCKADDR_INET::default();
    addr.Ipv4.sin_family = AF_INET;
    addr.Ipv4.sin_addr.S_un.S_addr = u32::from_ne_bytes(ip.octets());
    addr
}
#[cfg(windows)]
fn windows_route(ip: Ipv4Addr, source: &str) -> Result<Option<u32>, ToolError> {
    use windows_sys::Win32::{NetworkManagement::IpHelper::*, Networking::WinSock::*};
    let dest = sockaddr(ip);
    let src = source.parse::<Ipv4Addr>().ok().map(sockaddr);
    let mut route = MIB_IPFORWARD_ROW2::default();
    let mut best_source = SOCKADDR_INET::default();
    let code = unsafe {
        GetBestRoute2(
            std::ptr::null(),
            0,
            src.as_ref().map_or(std::ptr::null(), |s| s),
            &dest,
            0,
            &mut route,
            &mut best_source,
        )
    };
    if code != 0 {
        return Err(ToolError::new(
            "SYSTEM_QUERY_FAILED",
            format!("GetBestRoute2: {code}"),
        ));
    }
    let direct = !route.Loopback
        && unsafe {
            route.NextHop.si_family == AF_INET && route.NextHop.Ipv4.sin_addr.S_un.S_addr == 0
        };
    if !direct {
        return Ok(None);
    }
    let mut link = MIB_IF_ROW2::default();
    link.InterfaceIndex = route.InterfaceIndex;
    let code = unsafe { GetIfEntry2(&mut link) };
    if code != 0 {
        return Err(ToolError::new(
            "SYSTEM_QUERY_FAILED",
            format!("GetIfEntry2: {code}"),
        ));
    }
    // Ethernet, IEEE 802.11, or VLAN: IP-only tunnels cannot exchange Ethernet ARP.
    Ok(matches!(link.Type, 6 | 71 | 135).then_some(route.InterfaceIndex))
}
#[cfg(windows)]
fn windows_probe(
    ip: Ipv4Addr,
    iface: &interfaces::Interface,
    deadline: Instant,
    c: &Cancel,
    shared_permit: Arc<crate::budget::Permit>,
) -> Result<neighbors::Neighbor, ToolError> {
    // SendARP is synchronous. Bound outstanding native calls globally, retain their permit
    // until return, and let the task cancel without waiting on a stuck OS resolver.
    use std::sync::mpsc;
    use windows_sys::Win32::NetworkManagement::IpHelper::SendARP;
    static BUDGET: OnceLock<crate::budget::ConnectionBudget> = OnceLock::new();
    let budget = BUDGET.get_or_init(|| crate::budget::ConnectionBudget::new(32));
    let permit = loop {
        cancelled(c)?;
        if Instant::now() >= deadline {
            return Err(ToolError::new("TIMEOUT", "ARP admission timeout"));
        }
        if let Some(permit) = budget.try_acquire() {
            break permit;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let src = iface
        .address
        .parse::<Ipv4Addr>()
        .map_err(|_| bad("ARP source must be IPv4"))?;
    let iface = iface.clone();
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("hopkit-arp".into())
        .spawn(move || {
            let _permit = permit;
            let _shared_permit = shared_permit;
            let mut mac = [0u32; 2];
            let mut len = 8;
            let code = unsafe {
                SendARP(
                    u32::from_ne_bytes(ip.octets()),
                    u32::from_ne_bytes(src.octets()),
                    mac.as_mut_ptr().cast(),
                    &mut len,
                )
            };
            let result = if code == 0 && len == 6 {
                let raw = unsafe { std::slice::from_raw_parts(mac.as_ptr().cast::<u8>(), 6) };
                reply(ip, &iface, raw, "windows-sendarp")
            } else {
                Err(ToolError::new(
                    if code == 5 {
                        "PERMISSION_DENIED"
                    } else if [31, 67, 1168].contains(&code) {
                        "TIMEOUT"
                    } else {
                        "LOCAL_PROBE_FAILED"
                    },
                    format!("SendARP: {code}"),
                ))
            };
            let _ = tx.send(result);
        })
        .map_err(|e| ToolError::new("RESOURCE_EXHAUSTED", e.to_string()))?;
    loop {
        cancelled(c)?;
        let wait = deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(25));
        if wait.is_zero() {
            return Err(ToolError::new("TIMEOUT", "ARP reply timeout"));
        }
        match rx.recv_timeout(wait) {
            Ok(r) => return r,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => return Err(ToolError::new("LOCAL_PROBE_FAILED", "ARP worker stopped")),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_directed_local_ipv4_hosts_are_eligible() {
        let i = interfaces::Interface {
            name: "en0".into(),
            index: 7,
            address: "192.0.2.20".into(),
            prefix: 24,
        };
        let cfg = Config::default();
        for ip in ["192.0.2.1", "192.0.2.254"] {
            assert!(eligible(&i, ip.parse().unwrap(), "", &cfg));
        }
        for ip in [
            "198.51.100.1",
            "192.0.2.0",
            "192.0.2.255",
            "192.0.2.20",
            "127.0.0.1",
        ] {
            assert!(!eligible(&i, ip.parse().unwrap(), "", &cfg), "{ip}");
        }
        assert!(!eligible(
            &i,
            "192.0.2.1".parse().unwrap(),
            "192.0.2.21",
            &cfg
        ));
        assert!(!eligible(
            &i,
            "192.0.2.1".parse().unwrap(),
            "",
            &Config {
                interface: "en1".into(),
                ..cfg.clone()
            }
        ));
        let p2p = interfaces::Interface {
            address: "192.0.2.0".into(),
            prefix: 31,
            ..i.clone()
        };
        assert!(eligible(&p2p, "192.0.2.1".parse().unwrap(), "", &cfg));
        assert!(!eligible(
            &interfaces::Interface { prefix: 0, ..i },
            "203.0.113.1".parse().unwrap(),
            "",
            &cfg
        ));
    }
}
