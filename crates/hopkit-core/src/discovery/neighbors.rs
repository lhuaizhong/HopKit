//! Read-only neighbor observations; cache presence never proves a current response.
use super::*;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Neighbor {
    pub address: String,
    pub interface: String,
    pub interface_index: u32,
    pub mac: Option<String>,
    pub state: String,
    pub source: String,
    pub observed_at: String,
    pub phase: String,
}
#[derive(Debug, Default, Serialize)]
pub struct Snapshot {
    pub observations: Vec<Neighbor>,
    pub errors: Vec<ToolError>,
}
pub(super) fn mac(text: &str) -> Option<String> {
    let bytes: Option<Vec<_>> = text
        .split(':')
        .map(|p| u8::from_str_radix(p, 16).ok())
        .collect();
    let bytes = bytes.filter(|b| {
        b.len() == 6
            && b.iter().any(|v| *v != 0)
            && b[0] & 1 == 0
            && b.as_slice() != [2, 0, 0, 0, 0, 0]
    })?;
    Some(
        bytes
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(":"),
    )
}
#[cfg(unix)]
fn index(name: &str) -> u32 {
    std::ffi::CString::new(name)
        .ok()
        .map(|name| unsafe { libc::if_nametoindex(name.as_ptr()) })
        .unwrap_or(0)
}
#[cfg(all(test, not(unix)))]
fn index(name: &str) -> u32 {
    name.parse().unwrap_or(0)
}
#[cfg(any(target_os = "macos", test))]
fn parse_macos(text: &str, v6: bool, phase: &str) -> Vec<Neighbor> {
    text.lines()
        .filter_map(|line| {
            let f: Vec<_> = line.split_whitespace().collect();
            let (address, raw_mac, iface, state) = if v6 {
                if f.len() < 5 || f[0] == "Neighbor" {
                    return None;
                }
                (
                    f[0].to_string(),
                    f[1],
                    f[2],
                    match f[4] {
                        "R" => "reachable",
                        "S" => "stale",
                        "D" => "delay",
                        "P" => "probe",
                        "N" => "incomplete",
                        _ => "unknown",
                    },
                )
            } else {
                let at = f.iter().position(|v| *v == "at")?;
                let on = f.iter().position(|v| *v == "on")?;
                (
                    f.get(at - 1)?.trim_matches(['(', ')']).to_string(),
                    *f.get(at + 1)?,
                    *f.get(on + 1)?,
                    if line.contains("(incomplete)") {
                        "incomplete"
                    } else if line.contains("permanent") {
                        "permanent"
                    } else {
                        "cached_age_unknown"
                    },
                )
            };
            let idx = index(iface);
            let raw = address.split('%').next()?;
            let ip = raw.parse::<IpAddr>().ok()?;
            if ip.is_multicast() || ip.is_unspecified() {
                return None;
            }
            let address = if matches!(ip,IpAddr::V6(v) if v.is_unicast_link_local()) {
                format!("{ip}%{idx}")
            } else {
                ip.to_string()
            };
            Some(Neighbor {
                address,
                interface: iface.into(),
                interface_index: idx,
                mac: mac(raw_mac),
                state: state.into(),
                source: if v6 { "macos-ndp" } else { "macos-arp" }.into(),
                observed_at: now(),
                phase: phase.into(),
            })
        })
        .collect()
}
pub fn read(c: &Cancel, phase: &str) -> Snapshot {
    let mut result = Snapshot::default();
    #[cfg(target_os = "macos")]
    for (program, v6) in [("/usr/sbin/arp", false), ("/usr/sbin/ndp", true)] {
        match crate::process::capture(
            std::process::Command::new(program)
                .arg("-an")
                .env("LC_ALL", "C"),
            c,
            Duration::from_secs(5),
            512 * 1024,
        ) {
            Ok(out) if out.code == 0 => {
                if out.stdout.contains("02:00:00:00:00:00") || out.stdout.contains("2:0:0:0:0:0") {
                    result.errors.push(ToolError::new("PARTIAL_CAPABILITY","Neighbor output contains placeholder MACs; addresses may be redacted or omitted by OS policy / 系统返回占位 MAC，邻居可能被隐藏"));
                }
                result
                    .observations
                    .extend(parse_macos(&out.stdout, v6, phase));
            }
            Ok(_) => result.errors.push(ToolError::new(
                "SYSTEM_QUERY_FAILED",
                format!("{program}: query failed"),
            )),
            Err(e) => result.errors.push(e),
        }
    }
    #[cfg(windows)]
    match windows_bounded(c, phase) {
        Ok(rows) => result.observations = rows,
        Err(e) => result.errors.push(e),
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = (c, phase);
        result.errors.push(ToolError::new("UNSUPPORTED_CAPABILITY","Neighbor adapter supports Windows/macOS; active probes remain available / 邻居查询支持 Windows/macOS"));
    }
    if result.observations.len() > 16384 {
        result.observations.truncate(16384);
        result.errors.push(ToolError::new(
            "RESOURCE_EXHAUSTED",
            "Neighbor table truncated at 16384 observations",
        ));
    }
    result
}
#[cfg(windows)]
fn windows_bounded(c: &Cancel, phase: &str) -> Result<Vec<Neighbor>, ToolError> {
    use std::sync::{OnceLock, mpsc};
    static BUDGET: OnceLock<crate::budget::ConnectionBudget> = OnceLock::new();
    let permit = BUDGET
        .get_or_init(|| crate::budget::ConnectionBudget::new(2))
        .try_acquire()
        .ok_or_else(|| {
            ToolError::new(
                "RESOURCE_EXHAUSTED",
                "Neighbor workers busy; OS calls may still be finishing",
            )
        })?;
    let (tx, rx) = mpsc::sync_channel(1);
    let cancel = c.clone();
    let phase = phase.to_string();
    std::thread::Builder::new()
        .name("hopkit-neighbors".into())
        .spawn(move || {
            let _permit = permit;
            let _ = tx.send(windows_read(&cancel, &phase));
        })
        .map_err(|e| ToolError::new("RESOURCE_EXHAUSTED", e.to_string()))?;
    let began = Instant::now();
    loop {
        cancelled(c)?;
        if began.elapsed() >= Duration::from_secs(5) {
            return Err(ToolError::new(
                "TIMEOUT",
                "Neighbor query exceeded 5s; OS worker may finish later",
            ));
        }
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(result) => return result,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => {
                return Err(ToolError::new(
                    "SYSTEM_QUERY_FAILED",
                    "Neighbor worker failed",
                ));
            }
        }
    }
}
#[cfg(windows)]
fn windows_read(c: &Cancel, phase: &str) -> Result<Vec<Neighbor>, ToolError> {
    use windows_sys::Win32::{NetworkManagement::IpHelper::*, Networking::WinSock::*};
    cancelled(c)?;
    unsafe {
        let mut table = std::ptr::null_mut();
        let code = GetIpNetTable2(AF_UNSPEC, &mut table);
        if code != 0 {
            return Err(ToolError::new(
                "SYSTEM_QUERY_FAILED",
                format!("GetIpNetTable2: {code}"),
            ));
        }
        struct Guard(*mut MIB_IPNET_TABLE2);
        impl Drop for Guard {
            fn drop(&mut self) {
                unsafe {
                    FreeMibTable(self.0.cast());
                }
            }
        }
        let _guard = Guard(table);
        if table.is_null() {
            return Err(ToolError::new("SYSTEM_QUERY_FAILED", "Null neighbor table"));
        }
        if (*table).NumEntries > 16384 {
            return Err(ToolError::new(
                "RESOURCE_EXHAUSTED",
                "Neighbor table exceeds 16384 entries",
            ));
        }
        let rows = std::slice::from_raw_parts(
            std::ptr::addr_of!((*table).Table).cast::<MIB_IPNET_ROW2>(),
            (*table).NumEntries as usize,
        );
        let mut result = Vec::new();
        for row in rows {
            cancelled(c)?;
            let ip = match row.Address.si_family {
                AF_INET => IpAddr::V4(std::net::Ipv4Addr::from(
                    row.Address.Ipv4.sin_addr.S_un.S_addr.to_ne_bytes(),
                )),
                AF_INET6 => IpAddr::V6(std::net::Ipv6Addr::from(row.Address.Ipv6.sin6_addr.u.Byte)),
                _ => continue,
            };
            if ip.is_multicast() || ip.is_unspecified() {
                continue;
            }
            let address = if matches!(ip,IpAddr::V6(v) if v.is_unicast_link_local()) {
                format!("{ip}%{}", row.InterfaceIndex)
            } else {
                ip.to_string()
            };
            let raw_mac = row.PhysicalAddress[..(row.PhysicalAddressLength as usize).min(32)]
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(":");
            let state = match row.State {
                0 => "unreachable",
                1 => "incomplete",
                2 => "probe",
                3 => "delay",
                4 => "stale",
                5 => "reachable",
                6 => "permanent",
                _ => "unknown",
            };
            result.push(Neighbor {
                address,
                interface: row.InterfaceIndex.to_string(),
                interface_index: row.InterfaceIndex,
                mac: mac(&raw_mac),
                state: state.into(),
                source: "windows-iphelper".into(),
                observed_at: now(),
                phase: phase.into(),
            });
        }
        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_cache_without_inventing_liveness_or_vendor() {
        let rows = parse_macos(
            "? (192.0.2.1) at a:b:c:d:e:f on en0 ifscope [ethernet]\n? (192.0.2.2) at (incomplete) on en0 ifscope [ethernet]\n? (224.0.0.251) at 1:0:5e:0:0:fb on en0",
            false,
            "before",
        );
        assert!(mac("02:00:00:00:00:00").is_none());
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].mac.as_deref(), Some("0a:0b:0c:0d:0e:0f"));
        assert_eq!(rows[0].state, "cached_age_unknown");
        assert!(rows[1].mac.is_none());
        let rows = parse_macos(
            "fe80::1%lo0 2:0:0:0:0:1 lo0 expired S\n2001:db8::1 (incomplete) en0 expired N",
            true,
            "after",
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].state, "stale");
        assert!(rows[0].address.contains('%'));
        assert!(rows[1].mac.is_none());
    }
}
