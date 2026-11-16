//! Read-only endpoint snapshots. Four OS tables are observations over an interval, not atomic state.
use crate::runtime::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    net::{IpAddr, SocketAddr},
    sync::{OnceLock, mpsc},
    time::{Duration, Instant},
};
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub protocol: String,
    pub family: String,
    pub port: Option<u16>,
    pub pid: Option<u32>,
    pub state: String,
    pub address: String,
    pub process: String,
    pub listening: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct Endpoint {
    pub protocol: String,
    pub family: String,
    pub local_endpoint: String,
    pub remote_endpoint: Option<String>,
    pub state: Option<String>,
    pub pid: Option<u32>,
    pub process: Option<String>,
}
#[derive(Default)]
struct Snapshot {
    rows: Vec<Endpoint>,
    errors: Vec<ToolError>,
}
pub fn validate(config: &Config) -> Result<(), ToolError> {
    if !matches!(config.protocol.as_str(), "" | "tcp" | "udp")
        || !matches!(config.family.as_str(), "" | "auto" | "ipv4" | "ipv6")
        || config.port == Some(0)
        || config.state.len() > 32
        || config.address.len() > 128
        || config.process.len() > 256
    {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "Invalid endpoint filter",
        ));
    }
    Ok(())
}
fn matches(row: &Endpoint, c: &Config) -> bool {
    (c.protocol.is_empty() || row.protocol == c.protocol)
        && (c.family.is_empty() || c.family == "auto" || row.family == c.family)
        && c.pid.is_none_or(|p| row.pid == Some(p))
        && c.port.is_none_or(|p| {
            [Some(&row.local_endpoint), row.remote_endpoint.as_ref()]
                .into_iter()
                .flatten()
                .any(|s| s.parse::<SocketAddr>().is_ok_and(|a| a.port() == p))
        })
        && (!c.listening || row.protocol == "udp" || row.state.as_deref() == Some("listen"))
        && (c.state.is_empty() || row.state.as_deref() == Some(c.state.as_str()))
        && (c.address.is_empty()
            || row.local_endpoint.contains(&c.address)
            || row
                .remote_endpoint
                .as_ref()
                .is_some_and(|s| s.contains(&c.address)))
        && (c.process.is_empty()
            || row
                .process
                .as_ref()
                .is_some_and(|s| s.to_lowercase().contains(&c.process.to_lowercase())))
}
pub fn run(config: &Config, c: &Cancel, _emit: &Sink<'_>) -> ToolResult {
    validate(config)?;
    cancelled(c)?;
    static BUDGET: OnceLock<crate::budget::ConnectionBudget> = OnceLock::new();
    let permit = BUDGET
        .get_or_init(|| crate::budget::ConnectionBudget::new(2))
        .try_acquire()
        .ok_or_else(|| {
            ToolError::new(
                "RESOURCE_EXHAUSTED",
                "Endpoint workers busy; retry after current query",
            )
        })?;
    let began = Instant::now();
    let started = chrono::Utc::now().to_rfc3339();
    let (tx, rx) = mpsc::sync_channel(1);
    let cancel = c.clone();
    std::thread::Builder::new()
        .name("hopkit-endpoints".into())
        .spawn(move || {
            let _permit = permit;
            let _ = tx.send(read(&cancel));
        })
        .map_err(|e| ToolError::new("RESOURCE_EXHAUSTED", e.to_string()))?;
    let result = loop {
        cancelled(c)?;
        if began.elapsed() > Duration::from_secs(5) {
            return Err(ToolError::new(
                "TIMEOUT",
                "Endpoint query exceeded 5s; bounded OS worker may finish later",
            ));
        }
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(r) => break r,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => {
                return Err(ToolError::new(
                    "SYSTEM_QUERY_FAILED",
                    "Endpoint worker stopped",
                ));
            }
        }
    }?;
    let total = result.rows.len();
    let rows: Vec<_> = result
        .rows
        .into_iter()
        .filter(|r| matches(r, config))
        .collect();
    let limitations = if cfg!(windows) {
        vec![
            "OS tables are queried sequentially; process names may be unknown after exit or under access restrictions. No process paths are collected.",
        ]
    } else {
        vec![
            "Process/endpoint visibility is partial under OS permissions; empty results do not prove absence. No process paths are collected.",
        ]
    };
    Ok(
        json!({"schema_version":1,"query_started_at":started,"query_ended_at":chrono::Utc::now().to_rfc3339(),"duration_ms":began.elapsed().as_millis(),"rows":rows,"observed_endpoints":total,"partial":!result.errors.is_empty()||!cfg!(windows),"partial_errors":result.errors,"adapter":if cfg!(windows){"windows-iphelper"}else if cfg!(target_os="macos"){"macos-lsof"}else{"linux-proc-net"},"process_visibility":if cfg!(windows){"best_effort"}else{"partial"},"limitations":limitations}),
    )
}
fn row(
    protocol: &str,
    local: SocketAddr,
    remote: Option<SocketAddr>,
    state: Option<String>,
    pid: Option<u32>,
    process: Option<String>,
) -> Endpoint {
    Endpoint {
        protocol: protocol.into(),
        family: if local.is_ipv4() { "ipv4" } else { "ipv6" }.into(),
        local_endpoint: local.to_string(),
        remote_endpoint: remote.map(|a| a.to_string()),
        state,
        pid,
        process,
    }
}
fn tcp_state(n: u32) -> String {
    match n {
        1 => "closed",
        2 => "listen",
        3 => "syn_sent",
        4 => "syn_received",
        5 => "established",
        6 => "fin_wait_1",
        7 => "fin_wait_2",
        8 => "close_wait",
        9 => "closing",
        10 => "last_ack",
        11 => "time_wait",
        12 => "delete_tcb",
        _ => "unknown",
    }
    .into()
}
#[cfg(windows)]
fn read(c: &Cancel) -> Result<Snapshot, ToolError> {
    use windows_sys::Win32::{
        Foundation::*, NetworkManagement::IpHelper::*, Networking::WinSock::*,
        System::Diagnostics::ToolHelp::*,
    };
    let mut result = Snapshot::default();
    let mut names = std::collections::BTreeMap::new();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot != INVALID_HANDLE_VALUE {
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            if Process32FirstW(snapshot, &mut entry) != 0 {
                loop {
                    let n = entry
                        .szExeFile
                        .iter()
                        .position(|c| *c == 0)
                        .unwrap_or(entry.szExeFile.len());
                    names.insert(
                        entry.th32ProcessID,
                        String::from_utf16_lossy(&entry.szExeFile[..n]),
                    );
                    if Process32NextW(snapshot, &mut entry) == 0 {
                        break;
                    }
                }
            }
            CloseHandle(snapshot);
        } else {
            result.errors.push(ToolError::new(
                "PARTIAL_CAPABILITY",
                "Process names unavailable",
            ));
        }
    }
    for (tcp, v6) in [(true, false), (true, true), (false, false), (false, true)] {
        cancelled(c)?;
        let previous_errors = result.errors.len();
        // DWORD-aligned storage, a hard byte cap, bounded retries for table growth.
        let mut size = 0u32;
        let af = if v6 { AF_INET6 } else { AF_INET } as u32;
        let query = |ptr: *mut std::ffi::c_void, size: &mut u32| unsafe {
            if tcp {
                GetExtendedTcpTable(ptr, size, 1, af, TCP_TABLE_OWNER_PID_ALL, 0)
            } else {
                GetExtendedUdpTable(ptr, size, 1, af, UDP_TABLE_OWNER_PID, 0)
            }
        };
        let initial = query(std::ptr::null_mut(), &mut size);
        if initial != 0 && initial != 122 {
            result.errors.push(ToolError::new(
                if initial == 5 {
                    "PERMISSION_DENIED"
                } else {
                    "SYSTEM_QUERY_FAILED"
                },
                format!("IP Helper table sizing {tcp}/{v6}: {initial}"),
            ));
            continue;
        }
        let mut success = false;
        for _ in 0..3 {
            if !(4..=8 * 1024 * 1024).contains(&size) {
                result.errors.push(ToolError::new(
                    "RESOURCE_EXHAUSTED",
                    "OS endpoint table exceeds byte budget",
                ));
                break;
            }
            let mut buffer = vec![0u32; (size as usize).div_ceil(4)];
            let code = query(buffer.as_mut_ptr().cast(), &mut size);
            if code == 122 {
                continue;
            }
            if code != 0 {
                result.errors.push(ToolError::new(
                    "SYSTEM_QUERY_FAILED",
                    format!("IP Helper {tcp}/{v6}: {code}"),
                ));
                break;
            }
            let count = buffer[0] as usize;
            let row_size = match (tcp, v6) {
                (true, false) => std::mem::size_of::<MIB_TCPROW_OWNER_PID>(),
                (true, true) => std::mem::size_of::<MIB_TCP6ROW_OWNER_PID>(),
                (false, false) => std::mem::size_of::<MIB_UDPROW_OWNER_PID>(),
                (false, true) => std::mem::size_of::<MIB_UDP6ROW_OWNER_PID>(),
            };
            if count > 65536 || 4 + count * row_size > buffer.len() * 4 {
                result.errors.push(ToolError::new(
                    "RESOURCE_EXHAUSTED",
                    "OS endpoint count exceeds budget",
                ));
                break;
            }
            for i in 0..count {
                cancelled(c)?;
                let ptr = unsafe { buffer.as_ptr().cast::<u8>().add(4 + i * row_size) };
                let port = |n: u32| u16::from_be(n as u16);
                let v4 = |n: u32, p: u32| {
                    SocketAddr::new(
                        IpAddr::V4(std::net::Ipv4Addr::from(n.to_ne_bytes())),
                        port(p),
                    )
                };
                let v6addr = |a: [u8; 16], p: u32, scope: u32| {
                    SocketAddr::V6(std::net::SocketAddrV6::new(
                        std::net::Ipv6Addr::from(a),
                        port(p),
                        0,
                        scope,
                    ))
                };
                let (local, remote, state, pid) = unsafe {
                    match (tcp, v6) {
                        (true, false) => {
                            let r = std::ptr::read_unaligned(ptr.cast::<MIB_TCPROW_OWNER_PID>());
                            (
                                v4(r.dwLocalAddr, r.dwLocalPort),
                                if r.dwState == 2 {
                                    None
                                } else {
                                    Some(v4(r.dwRemoteAddr, r.dwRemotePort))
                                },
                                Some(tcp_state(r.dwState)),
                                r.dwOwningPid,
                            )
                        }
                        (true, true) => {
                            let r = std::ptr::read_unaligned(ptr.cast::<MIB_TCP6ROW_OWNER_PID>());
                            (
                                v6addr(r.ucLocalAddr, r.dwLocalPort, r.dwLocalScopeId),
                                if r.dwState == 2 {
                                    None
                                } else {
                                    Some(v6addr(r.ucRemoteAddr, r.dwRemotePort, r.dwRemoteScopeId))
                                },
                                Some(tcp_state(r.dwState)),
                                r.dwOwningPid,
                            )
                        }
                        (false, false) => {
                            let r = std::ptr::read_unaligned(ptr.cast::<MIB_UDPROW_OWNER_PID>());
                            (v4(r.dwLocalAddr, r.dwLocalPort), None, None, r.dwOwningPid)
                        }
                        (false, true) => {
                            let r = std::ptr::read_unaligned(ptr.cast::<MIB_UDP6ROW_OWNER_PID>());
                            (
                                v6addr(r.ucLocalAddr, r.dwLocalPort, r.dwLocalScopeId),
                                None,
                                None,
                                r.dwOwningPid,
                            )
                        }
                    }
                };
                result.rows.push(row(
                    if tcp { "tcp" } else { "udp" },
                    local,
                    remote,
                    state,
                    Some(pid),
                    names.get(&pid).cloned(),
                ));
            }
            success = true;
            break;
        }
        if !success && result.errors.len() == previous_errors {
            result.errors.push(ToolError::new(
                "SYSTEM_QUERY_FAILED",
                "Endpoint table changed during query; retry",
            ));
        }
    }
    if result.rows.is_empty() && !result.errors.is_empty() {
        return Err(result.errors.remove(0));
    }
    Ok(result)
}
#[cfg(any(target_os = "macos", test))]
fn parse_lsof(text: &str) -> Vec<Endpoint> {
    let mut rows = Vec::new();
    let mut pid = None;
    let mut name = None;
    let mut protocol = String::new();
    let mut endpoint = None;
    let mut state = None;
    let mut family = String::new();
    let flush = |rows: &mut Vec<Endpoint>,
                 protocol: &str,
                 endpoint: &Option<String>,
                 state: &Option<String>,
                 pid: Option<u32>,
                 name: &Option<String>,
                 family: &str| {
        if let Some(text) = endpoint {
            let mut parts = text.split("->");
            let address = |s: &str| {
                s.strip_prefix("*:")
                    .map(|port| {
                        if family == "IPv6" {
                            format!("[::]:{port}")
                        } else {
                            format!("0.0.0.0:{port}")
                        }
                    })
                    .unwrap_or_else(|| s.to_string())
                    .parse::<SocketAddr>()
                    .ok()
            };
            if let Some(local) = parts.next().and_then(address) {
                rows.push(row(
                    protocol,
                    local,
                    parts.next().and_then(|s| s.parse().ok()),
                    state.clone(),
                    pid,
                    name.clone(),
                ));
            }
        }
    };
    for line in text.lines() {
        let Some((tag, value)) = line.split_at_checked(1) else {
            continue;
        };
        match tag {
            "p" | "f" => {
                flush(&mut rows, &protocol, &endpoint, &state, pid, &name, &family);
                endpoint = None;
                state = None;
                protocol.clear();
                family.clear();
                if tag == "p" {
                    pid = value.parse().ok();
                    name = None;
                }
            }
            "c" => name = Some(value.chars().take(256).collect()),
            "P" => protocol = value.trim().to_lowercase(),
            "t" => family = value.to_string(),
            "n" => endpoint = Some(value.to_string()),
            "T" if value.starts_with("ST=") => {
                state = Some(value[3..].to_lowercase().replace('-', "_"))
            }
            _ => {}
        }
    }
    flush(&mut rows, &protocol, &endpoint, &state, pid, &name, &family);
    rows
}
#[cfg(target_os = "macos")]
fn read(c: &Cancel) -> Result<Snapshot, ToolError> {
    let output = crate::process::capture(
        std::process::Command::new("/usr/sbin/lsof")
            .args(["-nP", "-i", "-FpcfPtTn"])
            .env("LC_ALL", "C"),
        c,
        Duration::from_secs(4),
        4 * 1024 * 1024,
    )?;
    if output.code > 1 {
        return Err(ToolError::new(
            "SYSTEM_QUERY_FAILED",
            "lsof endpoint query failed",
        ));
    }
    Ok(Snapshot {
        rows: parse_lsof(&output.stdout),
        errors: vec![ToolError::new(
            "PARTIAL_CAPABILITY",
            "lsof visibility is limited by OS permissions; hidden processes cannot be excluded",
        )],
    })
}
#[cfg(target_os = "linux")]
fn read(c: &Cancel) -> Result<Snapshot, ToolError> {
    let mut result = Snapshot::default();
    for (file, protocol, v6) in [
        ("tcp", "tcp", false),
        ("tcp6", "tcp", true),
        ("udp", "udp", false),
        ("udp6", "udp", true),
    ] {
        cancelled(c)?;
        let mut data = String::new();
        use std::io::Read;
        match std::fs::File::open(format!("/proc/net/{file}"))
            .and_then(|f| f.take(4 * 1024 * 1024 + 1).read_to_string(&mut data))
        {
            Ok(_) if data.len() <= 4 * 1024 * 1024 => {}
            Ok(_) => {
                result.errors.push(ToolError::new(
                    "RESOURCE_EXHAUSTED",
                    "/proc endpoint table exceeds budget",
                ));
                continue;
            }
            Err(e) => {
                result.errors.push(e.into());
                continue;
            }
        }
        let addr = |s: &str| -> Option<SocketAddr> {
            let (a, p) = s.split_once(':')?;
            let p = u16::from_str_radix(p, 16).ok()?;
            let ip = if v6 {
                let mut bytes = [0u8; 16];
                for (i, chunk) in bytes.chunks_mut(4).enumerate() {
                    chunk.copy_from_slice(
                        &u32::from_str_radix(a.get(i * 8..i * 8 + 8)?, 16)
                            .ok()?
                            .to_ne_bytes(),
                    );
                }
                IpAddr::V6(bytes.into())
            } else {
                IpAddr::V4(u32::from_str_radix(a, 16).ok()?.to_ne_bytes().into())
            };
            Some(SocketAddr::new(ip, p))
        };
        for line in data.lines().skip(1) {
            let f: Vec<_> = line.split_whitespace().collect();
            if f.len() < 4 {
                continue;
            }
            if let Some(local) = addr(f[1]) {
                let state = if protocol == "tcp" {
                    Some(
                        match f[3] {
                            "01" => "established",
                            "02" => "syn_sent",
                            "03" => "syn_received",
                            "04" => "fin_wait_1",
                            "05" => "fin_wait_2",
                            "06" => "time_wait",
                            "07" => "closed",
                            "08" => "close_wait",
                            "09" => "last_ack",
                            "0A" => "listen",
                            "0B" => "closing",
                            _ => "unknown",
                        }
                        .into(),
                    )
                } else {
                    None
                };
                result.rows.push(row(
                    protocol,
                    local,
                    addr(f[2]).filter(|a| a.port() != 0),
                    state,
                    None,
                    None,
                ));
            }
        }
    }
    Ok(result)
}
#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
fn read(_: &Cancel) -> Result<Snapshot, ToolError> {
    Err(ToolError::new(
        "UNSUPPORTED_CAPABILITY",
        "Endpoint adapter unavailable",
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_structured_lsof_without_udp_tcp_state() {
        let rows = parse_lsof(
            "p42\ncpeer\nf5\nP TCP\nn127.0.0.1:1234\nTST=LISTEN\nf6\nPUDP\nn[::1]:9000\n",
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].pid, Some(42));
        assert!(rows[1].state.is_none());
        let rows =
            parse_lsof("p7\ncpeer\nf3\ntIPv6\nPTCP\nn*:443\nTST=LISTEN\nf4\ntIPv4\nPUDP\nn*:53\n");
        assert_eq!(rows[0].local_endpoint, "[::]:443");
        assert_eq!(rows[1].local_endpoint, "0.0.0.0:53");
        let rows =
            parse_lsof("p7\ncpeer\nf3\ntIPv6\nPTCP\nn*:443\nTST=LISTEN\nf4\ntIPv4\nPUDP\nn*:53\n");
        assert_eq!(rows[0].local_endpoint, "[::]:443");
        assert_eq!(rows[1].local_endpoint, "0.0.0.0:53");
    }
}
