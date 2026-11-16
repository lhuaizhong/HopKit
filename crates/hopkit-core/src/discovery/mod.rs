//! Bounded discovery: active response evidence, cache observations and optional PTR are separate.
use crate::{runtime::*, scan, target, transport};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
mod arp;
pub mod interfaces;
pub(crate) mod names;
pub mod neighbors;
fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
fn bad(message: &str) -> ToolError {
    ToolError::new("INVALID_ARGUMENT", message)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub targets: Vec<String>,
    pub interface: String,
    pub methods: Vec<String>,
    pub ports: String,
    pub timeout_ms: u64,
    pub concurrency: usize,
    pub rate: u32,
    pub source: String,
    pub name_lookup: bool,
    pub dns_server: String,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            targets: vec![],
            interface: String::new(),
            methods: vec!["arp".into(), "icmp".into(), "tcp".into()],
            ports: "80,443".into(),
            timeout_ms: 500,
            concurrency: 32,
            rate: 100,
            source: String::new(),
            name_lookup: false,
            dns_server: String::new(),
        }
    }
}
/// No hostnames, implicit private networks or IPv6 prefix enumeration.
pub fn validate(c: &Config) -> Result<Vec<String>, ToolError> {
    if c.methods.is_empty()
        || c.methods.len() > 3
        || c.methods
            .iter()
            .any(|m| !["arp", "icmp", "tcp"].contains(&m.as_str()))
        || c.methods.iter().collect::<BTreeSet<_>>().len() != c.methods.len()
    {
        return Err(bad("Select unique arp/icmp/tcp methods"));
    }
    if !(100..=1000).contains(&c.timeout_ms)
        || c.interface.len() > 128
        || c.interface.chars().any(char::is_control)
    {
        return Err(bad("Discovery timeout 100..1000 ms; invalid interface"));
    }
    if c.dns_server.len() > 128
        || c.dns_server.chars().any(char::is_control)
        || c.name_lookup
            && target::literal_socket(&c.dns_server, 53)
                .or_else(|| c.dns_server.parse::<SocketAddr>().ok())
                .is_none_or(|s| s.port() == 0)
    {
        return Err(bad(
            "PTR requires an explicit numeric DNS server / PTR 查询须指定 DNS 服务器",
        ));
    }
    let ports = scan::ports(&c.ports)?;
    if ports.len() > 8 {
        return Err(bad("Discovery accepts at most 8 TCP ports"));
    }
    let base = scan::Config {
        targets: if c.targets.is_empty() {
            vec!["127.0.0.1".into()]
        } else {
            c.targets.clone()
        },
        ports: c.ports.clone(),
        timeout_ms: c.timeout_ms,
        concurrency: c.concurrency,
        rate: c.rate,
        source: c.source.clone(),
        ..Default::default()
    };
    let (hosts, _) = scan::validate(&base)?;
    if hosts.iter().any(|h| target::literal_socket(h, 0).is_none()) {
        return Err(bad(
            "Discovery requires numeric IPs, IPv4 CIDR or start-end; no hostname / 仅接受明确 IP 或 IPv4 地址段",
        ));
    }
    if c.targets.is_empty() {
        return Ok(vec![]);
    }
    let mut result = BTreeSet::new();
    for host in hosts {
        let addr = target::literal_socket(&host, 0).unwrap();
        if addr.ip().is_multicast()
            || addr.ip().is_unspecified()
            || addr.ip() == IpAddr::V4(std::net::Ipv4Addr::BROADCAST)
        {
            return Err(bad(
                "Unspecified, multicast and broadcast candidates are not supported",
            ));
        }
        if let SocketAddr::V6(v) = addr
            && v.ip().is_unicast_link_local()
            && v.scope_id() == 0
            && c.interface.is_empty()
        {
            return Err(bad(
                "IPv6 link-local candidate requires numeric scope or an interface",
            ));
        }
        result.insert(scan::address_text(addr));
    }
    Ok(result.into_iter().collect())
}
#[derive(Debug, Clone, Serialize)]
pub struct Observation {
    pub method: String,
    pub status: String,
    pub responded: bool,
    pub port: Option<u16>,
    pub elapsed_ms: f64,
    pub observed_at: String,
    pub source: String,
    pub local_endpoint: Option<String>,
    pub error: Option<ToolError>,
    /// Present only for a matching active ARP reply, never a next-hop/cache substitution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub neighbor: Option<neighbors::Neighbor>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Name {
    pub source: String,
    pub server: String,
    pub observed_at: String,
    pub names: Vec<String>,
    pub error: Option<ToolError>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Device {
    pub address: String,
    pub responded: bool,
    pub first_response_at: Option<String>,
    pub last_response_at: Option<String>,
    pub observations: Vec<Observation>,
    pub neighbors: Vec<neighbors::Neighbor>,
    pub name: Option<Name>,
    pub mac_change_hint: bool,
}
#[derive(Debug, Serialize)]
pub struct Stats {
    pub candidates: usize,
    pub responders: usize,
    pub unknown: usize,
    pub planned: usize,
    pub completed: usize,
    pub remaining: usize,
    pub duration_ms: u128,
}
#[derive(Serialize)]
pub struct ResultData {
    pub schema_version: u32,
    pub rows: Vec<Device>,
    pub stats: Stats,
    pub neighbor_sources: Vec<String>,
    pub errors: Vec<ToolError>,
    pub partial: bool,
    pub limitations: Vec<String>,
}
#[derive(Serialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum Event<'a> {
    DiscoveryDevice {
        device: &'a Device,
    },
    Stats {
        total: usize,
        observed: usize,
        remaining: usize,
    },
}
fn emit_event(event: Event<'_>, sink: &Sink<'_>) {
    sink(serde_json::to_value(event).unwrap());
}
fn merge_neighbors(rows: &mut BTreeMap<String, Device>, snapshot: neighbors::Snapshot) {
    for neighbor in snapshot.observations {
        if let Some(row) = rows.get_mut(&neighbor.address) {
            row.neighbors.push(neighbor);
            update_mac_hint(row);
        }
    }
}
fn update_mac_hint(row: &mut Device) {
    let mut links: BTreeMap<(u32, &str), BTreeSet<&str>> = BTreeMap::new();
    for n in &row.neighbors {
        if let Some(mac) = &n.mac {
            links
                .entry((n.interface_index, &n.interface))
                .or_default()
                .insert(mac);
        }
    }
    row.mac_change_hint = links.values().any(|m| m.len() > 1);
}
fn device(address: String) -> Device {
    Device {
        address,
        responded: false,
        first_response_at: None,
        last_response_at: None,
        observations: vec![],
        neighbors: vec![],
        name: None,
        mac_change_hint: false,
    }
}
fn record(row: &mut Device, obs: Observation) {
    if let Some(neighbor) = &obs.neighbor
        && neighbor.address == row.address
    {
        row.neighbors.push(neighbor.clone());
        update_mac_hint(row);
    }
    if obs.responded {
        row.responded = true;
        if row
            .first_response_at
            .as_ref()
            .is_none_or(|v| v > &obs.observed_at)
        {
            row.first_response_at = Some(obs.observed_at.clone());
        }
        if row
            .last_response_at
            .as_ref()
            .is_none_or(|v| v < &obs.observed_at)
        {
            row.last_response_at = Some(obs.observed_at.clone());
        }
    }
    row.observations.push(obs);
}
fn measure(
    addr: SocketAddr,
    method: &str,
    source: &str,
    config: &Config,
    c: &Cancel,
) -> Observation {
    let began = Instant::now();
    let result = if method == "tcp" {
        transport::connect(addr,source,c,began+Duration::from_millis(config.timeout_ms)).map(|s|json!({"status":"open","ok":true,"local":s.local_addr().ok().map(|a|a.to_string())}))
    } else {
        crate::diagnostics::discovery_echo(addr, source, config.timeout_ms as u32, c)
    };
    let (status, responded, local, error) = match result {
        Ok(v) => (
            v["status"].as_str().unwrap_or("unknown").to_string(),
            v["ok"] == true,
            v["local"].as_str().map(String::from),
            None,
        ),
        Err(e) => {
            let status = match e.code.as_str() {
                "CONNECTION_REFUSED" => "refused",
                "TIMEOUT" => "timeout",
                "CANCELLED" => "cancelled",
                "NETWORK_UNREACHABLE" => "unreachable",
                _ => "local_error",
            };
            (status.into(), e.code == "CONNECTION_REFUSED", None, Some(e))
        }
    };
    Observation {
        method: method.into(),
        status,
        responded,
        port: (method == "tcp").then_some(addr.port()),
        elapsed_ms: began.elapsed().as_secs_f64() * 1000.0,
        observed_at: now(),
        source: if source.is_empty() {
            "system_route".into()
        } else {
            source.into()
        },
        local_endpoint: local,
        error,
        neighbor: None,
    }
}
pub fn run(config: &Config, c: &Cancel, sink: &Sink<'_>) -> ToolResult {
    let arp = arp::Prober::default();
    run_with(
        config,
        c,
        sink,
        &|addr, method, source, cfg, cancel, permit| {
            if method == "arp" {
                arp.probe(addr, source, cfg, cancel, permit)
            } else {
                measure(addr, method, source, cfg, cancel)
            }
        },
        &neighbors::read,
    )
}
type Probe<'a> = dyn Fn(SocketAddr, &str, &str, &Config, &Cancel, Arc<crate::budget::Permit>) -> Observation
    + Sync
    + 'a;
fn run_with(
    config: &Config,
    c: &Cancel,
    sink: &Sink<'_>,
    probe: &Probe<'_>,
    read: &dyn Fn(&Cancel, &str) -> neighbors::Snapshot,
) -> ToolResult {
    let began = Instant::now();
    let hosts = validate(config)?;
    let interfaces = if !config.interface.is_empty() {
        interfaces::list(c)?
    } else {
        vec![]
    };
    let selected: Vec<_> = interfaces
        .iter()
        .filter(|i| i.name == config.interface || i.index.to_string() == config.interface)
        .collect();
    if !config.interface.is_empty() && selected.is_empty() {
        return Err(bad(
            "Interface not found or has no active IP / 接口不存在或没有活动 IP",
        ));
    }
    let mut snapshot = read(c, "before");
    if !selected.is_empty() {
        snapshot
            .observations
            .retain(|n| selected.iter().any(|i| i.index == n.interface_index));
    }
    let mut errors = snapshot.errors.clone();
    let mut candidates: BTreeSet<String> = hosts.into_iter().collect();
    if config.targets.is_empty() {
        // Cache-only view: no implicit probes. The UI must explicitly copy a suggested network.
        candidates.extend(
            snapshot
                .observations
                .iter()
                .filter(|n| {
                    config.interface.is_empty()
                        || selected.iter().any(|i| i.index == n.interface_index)
                })
                .map(|n| n.address.clone()),
        );
    }
    if candidates.len() > 4096 {
        return Err(bad(
            "Neighbor candidates exceed 4096; select an interface or explicit targets",
        ));
    }
    let mut rows = BTreeMap::new();
    let mut jobs = Vec::new();
    let ports = scan::ports(&config.ports)?;
    for address in candidates {
        let mut addr =
            target::literal_socket(&address, 0).ok_or_else(|| bad("Invalid cached candidate"))?;
        if let SocketAddr::V6(ref mut v) = addr
            && v.ip().is_unicast_link_local()
            && let Some(i) = selected.first()
        {
            if v.scope_id() != 0 && v.scope_id() != i.index {
                return Err(bad("IPv6 scope does not match interface"));
            }
            v.set_scope_id(i.index);
        }
        let source = if !config.source.is_empty() {
            if !selected.is_empty()
                && !selected.iter().any(|i| {
                    target::literal_socket(&i.address, 0)
                        == target::literal_socket(&config.source, 0)
                })
            {
                return Err(bad("Source must belong to the selected interface"));
            }
            config.source.clone()
        } else if !selected.is_empty() {
            let family: Vec<_> = selected
                .iter()
                .filter(|i| {
                    target::literal_socket(&i.address, 0).is_some_and(|a| {
                        a.is_ipv4() == addr.is_ipv4()
                            && (a.ip().is_loopback() == addr.ip().is_loopback())
                            && (matches!(a.ip(),IpAddr::V6(v) if v.is_unicast_link_local())
                                == matches!(addr.ip(),IpAddr::V6(v) if v.is_unicast_link_local()))
                    })
                })
                .collect();
            family
                .iter()
                .find(|i| interfaces::contains(i, addr.ip()))
                .or_else(|| family.first())
                .ok_or_else(|| bad("Selected interface has no matching address family"))?
                .address
                .clone()
        } else {
            String::new()
        };
        if let Some(src) = target::literal_socket(&source, 0)
            && src.is_ipv4() != addr.is_ipv4()
        {
            return Err(bad("Source and candidates must use the same family"));
        }
        let key = scan::address_text(addr);
        if rows.contains_key(&key) {
            continue;
        }
        rows.insert(key.clone(), device(key.clone()));
        if !config.targets.is_empty() {
            for method in &config.methods {
                if method != "tcp" {
                    jobs.push((key.clone(), addr, method.clone(), source.clone()));
                } else {
                    for port in &ports {
                        let mut dest = addr;
                        dest.set_port(*port);
                        jobs.push((key.clone(), dest, method.clone(), source.clone()));
                    }
                }
            }
        }
    }
    let mut planned = jobs.len();
    if planned > 65536 {
        return Err(bad("Discovery probe budget exceeds 65536"));
    }
    let mut neighbor_sources: Vec<_> = snapshot
        .observations
        .iter()
        .map(|n| n.source.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    merge_neighbors(&mut rows, snapshot);
    for row in rows.values() {
        if !row.neighbors.is_empty() {
            emit_event(Event::DiscoveryDevice { device: row }, sink);
        }
    }
    let session = scan::shared_scheduler().session(config.rate);
    let mut completed = 0;
    for round in 0..2 {
        if round == 1 {
            if cancelled(c).is_err() {
                break;
            }
            // The first packet may be lost while the OS resolves a neighbor or the
            // link wakes up. Retry only unanswered ICMP timeouts, once, through the
            // same admission/rate budget. Never promote cache/route presence to a reply.
            jobs.retain(|(key, _, method, _)| {
                method == "icmp"
                    && !rows[key].responded
                    && rows[key]
                        .observations
                        .iter()
                        .any(|o| o.method == "icmp" && o.status == "timeout")
            });
            if jobs.is_empty() {
                break;
            }
            planned += jobs.len();
            emit_event(
                Event::Stats {
                    total: planned,
                    observed: completed,
                    remaining: planned - completed,
                },
                sink,
            );
        }
        let state = Mutex::new((0usize, completed, rows));
        std::thread::scope(|scope| {
            for _ in 0..config.concurrency.min(jobs.len()) {
                let state = &state;
                let jobs = &jobs;
                let session = &session;
                scope.spawn(move || {
                    loop {
                        let job = {
                            let mut state = state.lock().unwrap();
                            if cancelled(c).is_err() || state.0 >= jobs.len() {
                                break;
                            }
                            let job = state.0;
                            state.0 += 1;
                            job
                        };
                        let Ok(permit) = session.acquire(c) else {
                            break;
                        };
                        let (key, addr, method, source) = &jobs[job];
                        let obs = probe(*addr, method, source, config, c, Arc::new(permit));
                        let completed = obs.status != "cancelled";
                        let mut state = state.lock().unwrap();
                        state.1 += usize::from(completed);
                        record(state.2.get_mut(key).unwrap(), obs);
                        emit_event(
                            Event::DiscoveryDevice {
                                device: &state.2[key],
                            },
                            sink,
                        );
                        if state.1 % 10 == 0 || state.1 == planned {
                            emit_event(
                                Event::Stats {
                                    total: planned,
                                    observed: state.1,
                                    remaining: planned - state.1,
                                },
                                sink,
                            );
                        }
                    }
                });
            }
        });
        let (_, progress, devices) = state.into_inner().unwrap();
        completed = progress;
        rows = devices;
    }
    // Active ARP failures must remain visible even when their nonresponsive rows are hidden.
    for row in rows.values() {
        for obs in &row.observations {
            if obs.method == "arp"
                && obs.status == "local_error"
                && let Some(e) = &obs.error
                && !errors
                    .iter()
                    .any(|known| known.code == e.code && known.message == e.message)
            {
                errors.push(e.clone());
            }
        }
    }
    if cancelled(c).is_ok() && !config.targets.is_empty() {
        let mut after = read(c, "after");
        if !selected.is_empty() {
            after
                .observations
                .retain(|n| selected.iter().any(|i| i.index == n.interface_index));
        }
        errors.extend(after.errors.clone());
        merge_neighbors(&mut rows, after);
    }
    if config.name_lookup && cancelled(c).is_ok() {
        let queue: Vec<_> = rows
            .values()
            .filter(|r| r.responded)
            .map(|r| r.address.clone())
            .collect();
        let work = Mutex::new((0, &mut rows));
        std::thread::scope(|scope| {
            for _ in 0..4.min(queue.len()) {
                let work = &work;
                let queue = &queue;
                let session = &session;
                scope.spawn(move || {
                    loop {
                        let address = {
                            let mut w = work.lock().unwrap();
                            if cancelled(c).is_err() || w.0 >= queue.len() {
                                break;
                            }
                            let a = queue[w.0].clone();
                            w.0 += 1;
                            a
                        };
                        let Ok(_permit) = session.acquire(c) else {
                            break;
                        };
                        let result =
                            names::lookup(&address, &config.dns_server, config.timeout_ms, c);
                        let (names, error) = match result {
                            Ok(n) => (n, None),
                            Err(e) => (vec![], Some(e)),
                        };
                        let mut w = work.lock().unwrap();
                        let row = w.1.get_mut(&address).unwrap();
                        row.name = Some(Name {
                            source: "dns_ptr".into(),
                            server: config.dns_server.clone(),
                            observed_at: now(),
                            names,
                            error,
                        });
                        emit_event(Event::DiscoveryDevice { device: row }, sink);
                    }
                });
            }
        });
    }
    for row in rows.values() {
        emit_event(Event::DiscoveryDevice { device: row }, sink);
        for neighbor in &row.neighbors {
            if !neighbor_sources.contains(&neighbor.source) {
                neighbor_sources.push(neighbor.source.clone());
            }
        }
    }
    let responders = rows.values().filter(|r| r.responded).count();
    let candidates = rows.len();
    let local_failure = planned > 0
        && rows
            .values()
            .flat_map(|r| &r.observations)
            .any(|o| o.status == "local_error");
    let name_incomplete = config.name_lookup
        && rows
            .values()
            .any(|r| r.responded && r.name.as_ref().is_none_or(|n| n.error.is_some()));
    let partial = cancelled(c).is_err()
        || completed < planned
        || !errors.is_empty()
        || local_failure
        || name_incomplete;
    let mut rows: Vec<_> = rows.into_values().collect();
    rows.sort_by_key(|r| target::literal_socket(&r.address, 0));
    let result=ResultData{schema_version:1,rows,stats:Stats{candidates,responders,unknown:candidates-responders,planned,completed,remaining:planned-completed,duration_ms:began.elapsed().as_millis()},neighbor_sources,errors,partial,limitations:vec![
        "TCP refusal is response evidence and may originate from a middlebox. / TCP 拒绝可能来自中间设备。".into(),
        "No response means unknown, never an offline device or an available IP. / 未响应为未知，不代表离线或空闲 IP。".into(),
        "The OS may omit or redact neighbor entries. Placeholder MACs are unknown; an empty cache is not an empty LAN. / 系统可能隐藏邻居，占位 MAC 为未知，空缓存不代表空网络。".into(),
        "Neighbor MAC/state is a cache observation for that exact address and link, never the routed target's next-hop MAC; cache age may be unknown. / 邻居缓存不代表本次响应。".into(),
        "Interface selection binds a source IP and scopes IPv6; fixed-interface routing is not enforced. PTR uses the system route to the explicit resolver. / 接口选择用于源 IP 与 scope，不强制固定接口路由。".into(),
        "ARP is IPv4 only: both a directly connected interface subnet and an on-link route are required. Routed/VPN targets are skipped. macOS requires BPF access; Windows uses SendARP. No active ND or RFC5227 probes. / ARP 仅限 IPv4 直连网段且实际路由不经网关；macOS 需要 BPF 权限。".into(),
        "MAC change is only a clue: Proxy ARP, VRRP and interface changes are possible. / MAC 变化仅为线索。".into(),
        "No bundled OUI dataset; vendor unknown, including local/random MACs. / 无离线 OUI 数据，厂商未知。".into()]};
    let value = serde_json::to_value(result).unwrap();
    if planned == 0
        && value["rows"].as_array().unwrap().is_empty()
        && !value["errors"].as_array().unwrap().is_empty()
        && cancelled(c).is_ok()
    {
        let mut e = ToolError::new(
            "UNSUPPORTED_CAPABILITY",
            "Neighbor query failed; inspect adapter errors / 邻居查询失败，请查看适配器错误",
        );
        e.data = Some(value);
        return Err(e);
    }
    if cancelled(c).is_err() {
        let mut e = ToolError::new(
            "CANCELLED",
            "Discovery stopped; partial results retained / 发现已停止，保留部分结果",
        );
        e.data = Some(value);
        Err(e)
    } else if local_failure && responders == 0 {
        let mut e = ToolError::new(
            "LOCAL_PROBE_FAILED",
            "Discovery probes encountered local errors; inspect evidence / 本机探测失败，请查看证据",
        );
        e.data = Some(value);
        Err(e)
    } else {
        Ok(value)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    fn cancel() -> Cancel {
        Arc::new(AtomicBool::new(false))
    }
    fn cached(address: &str, mac: &str, index: u32, phase: &str) -> neighbors::Neighbor {
        neighbors::Neighbor {
            address: address.into(),
            mac: Some(mac.into()),
            interface: "fixture".into(),
            interface_index: index,
            state: "stale".into(),
            source: "fixture-cache".into(),
            observed_at: now(),
            phase: phase.into(),
        }
    }
    fn fake(
        _: SocketAddr,
        method: &str,
        _: &str,
        _: &Config,
        _: &Cancel,
        _: Arc<crate::budget::Permit>,
    ) -> Observation {
        Observation {
            method: method.into(),
            status: if method == "tcp" {
                "refused"
            } else {
                "timeout"
            }
            .into(),
            responded: method == "tcp",
            port: (method == "tcp").then_some(80),
            elapsed_ms: 2.0,
            observed_at: now(),
            source: "fixture".into(),
            local_endpoint: None,
            error: None,
            neighbor: None,
        }
    }
    #[test]
    fn candidate_edges_and_limits() {
        let mut c = Config {
            targets: vec![
                "192.0.2.0/31".into(),
                "192.0.2.1/32".into(),
                "192.0.2.8-192.0.2.9".into(),
                "2001:db8::1".into(),
                "2001:0db8::1".into(),
            ],
            ..Default::default()
        };
        assert_eq!(validate(&c).unwrap().len(), 5);
        for target in [
            "::/64",
            "0.0.0.0/0",
            "192.0.2.2-192.0.2.1",
            "255.255.255.255-0.0.0.1",
            "localhost",
            "224.0.0.1",
            "0.0.0.0",
            "fe80::1",
        ] {
            c.targets = vec![target.into()];
            assert!(validate(&c).is_err(), "{target}");
        }
        c.targets = vec!["fe80::1%1".into(), "fe80::1%2".into()];
        assert_eq!(validate(&c).unwrap().len(), 2);
        c.ports = "1-9".into();
        assert!(validate(&c).is_err());
        c.ports = "80".into();
        c.methods = vec!["tcp".into(), "tcp".into()];
        assert!(validate(&c).is_err());
    }
    #[test]
    fn subnet_gateway_gets_one_bounded_retry_and_never_uses_cache_as_a_reply() {
        let cfg = Config {
            targets: vec!["192.168.10.8/24".into()],
            methods: vec!["icmp".into()],
            ..Default::default()
        };
        let calls = Mutex::new(BTreeMap::<String, usize>::new());
        let result = run_with(
            &cfg,
            &cancel(),
            &|_| {},
            &|addr, m, s, c, t, permit| {
                let mut o = fake(addr, m, s, c, t, permit);
                let ip = addr.ip().to_string();
                let mut calls = calls.lock().unwrap();
                let n = calls.entry(ip.clone()).or_default();
                *n += 1;
                if ip == "192.168.10.1" && *n == 2 {
                    o.status = "reply".into();
                    o.responded = true;
                }
                o
            },
            &|_, phase| neighbors::Snapshot {
                observations: vec![cached("192.168.10.2", "02:00:00:00:00:02", 7, phase)],
                errors: vec![],
            },
        )
        .unwrap();
        let rows = result["rows"].as_array().unwrap();
        let gateway = rows
            .iter()
            .find(|r| r["address"] == "192.168.10.1")
            .unwrap();
        assert_eq!(gateway["responded"], true);
        assert_eq!(gateway["observations"].as_array().unwrap().len(), 2);
        assert_eq!(
            rows.iter()
                .find(|r| r["address"] == "192.168.10.2")
                .unwrap()["responded"],
            false
        );
        assert_eq!(result["stats"]["candidates"], 254);
        assert_eq!(result["stats"]["completed"], 508);
        assert_eq!(result["stats"]["planned"], 508);
        assert_eq!(result["stats"]["responders"], 1);
        assert!(calls.lock().unwrap().values().all(|n| *n == 2));
    }
    #[test]
    fn arp_reply_discovers_ping_blocked_host_and_preserves_its_exact_mac() {
        let config = Config {
            targets: vec![
                "192.0.2.100".into(),
                "192.0.2.10".into(),
                "192.0.2.2".into(),
            ],
            methods: vec!["arp".into(), "icmp".into()],
            ..Default::default()
        };
        let result = run_with(
            &config,
            &cancel(),
            &|_| {},
            &|addr, m, s, c, t, permit| {
                let mut o = fake(addr, m, s, c, t, permit);
                if m == "arp" && addr.ip().to_string() == "192.0.2.10" {
                    o.status = "reply".into();
                    o.responded = true;
                    let mut n = cached("192.0.2.10", "02:00:00:00:00:10", 7, "active");
                    n.source = "fixture-active-arp".into();
                    o.neighbor = Some(n);
                }
                o
            },
            &|_, _| neighbors::Snapshot::default(),
        )
        .unwrap();
        let rows = result["rows"].as_array().unwrap();
        assert_eq!(
            rows.iter()
                .map(|r| r["address"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["192.0.2.2", "192.0.2.10", "192.0.2.100"]
        );
        assert_eq!(result["stats"]["responders"], 1);
        assert_eq!(result["stats"]["unknown"], 2);
        assert_eq!(rows[1]["neighbors"][0]["mac"], "02:00:00:00:00:10");
        assert!(rows[0]["neighbors"].as_array().unwrap().is_empty());
        assert!(rows[2]["neighbors"].as_array().unwrap().is_empty());
        assert!(
            result["neighbor_sources"]
                .as_array()
                .unwrap()
                .contains(&json!("fixture-active-arp"))
        );
    }
    #[test]
    fn tcp_response_cache_and_mac_changes_stay_separate() {
        let config = Config {
            targets: vec!["192.0.2.1-192.0.2.3".into()],
            ports: "80".into(),
            concurrency: 1,
            methods: vec!["icmp".into(), "tcp".into()],
            ..Default::default()
        };
        let result = run_with(&config, &cancel(), &|_| {}, &fake, &|_, phase| {
            neighbors::Snapshot {
                observations: vec![
                    cached(
                        "192.0.2.1",
                        if phase == "before" {
                            "02:00:00:00:00:01"
                        } else {
                            "02:00:00:00:00:02"
                        },
                        7,
                        phase,
                    ),
                    cached(
                        "192.0.2.2",
                        if phase == "before" {
                            "02:00:00:00:00:01"
                        } else {
                            "02:00:00:00:00:02"
                        },
                        if phase == "before" { 7 } else { 8 },
                        phase,
                    ),
                    cached("192.0.2.254", "02:00:00:00:00:ff", 7, phase),
                ],
                errors: vec![],
            }
        })
        .unwrap();
        assert_eq!(result["stats"]["responders"], 3);
        assert_eq!(result["stats"]["completed"], 6);
        let rows = result["rows"].as_array().unwrap();
        assert_eq!(rows[0]["mac_change_hint"], true);
        assert_eq!(rows[1]["mac_change_hint"], false);
        assert_eq!(rows[2]["neighbors"].as_array().unwrap().len(), 0);
        assert_eq!(rows[0]["observations"][0]["method"], "icmp");
        assert_eq!(rows[0]["observations"][0]["responded"], false);
        assert_eq!(rows[0]["observations"][1]["status"], "refused");
        assert!(rows[0]["first_response_at"].is_string());
        let cache = run_with(
            &Config::default(),
            &cancel(),
            &|_| {},
            &|_, _, _, _, _, _| panic!("cache view probed"),
            &|_, phase| neighbors::Snapshot {
                observations: vec![cached("192.0.2.1", "02:00:00:00:00:01", 7, phase)],
                errors: vec![],
            },
        )
        .unwrap();
        assert_eq!(cache["stats"]["responders"], 0);
        assert_eq!(cache["stats"]["unknown"], 1);
        assert_eq!(cache["stats"]["planned"], 0);
        assert!(cache["rows"][0]["first_response_at"].is_null());
    }
    #[test]
    fn cancelled_probe_retains_unknown_and_partial_results() {
        let c = cancel();
        let config = Config {
            targets: vec!["192.0.2.0/31".into()],
            methods: vec!["tcp".into()],
            ports: "80,443".into(),
            concurrency: 1,
            ..Default::default()
        };
        let result = run_with(
            &config,
            &c,
            &|_| {},
            &|addr, m, s, config, c, permit| {
                let o = fake(addr, m, s, config, c, permit);
                c.store(true, Ordering::Relaxed);
                o
            },
            &|_, _| neighbors::Snapshot::default(),
        )
        .unwrap_err();
        assert_eq!(result.code, "CANCELLED");
        let d = result.data.unwrap();
        assert_eq!(d["stats"]["planned"], 4);
        assert_eq!(d["stats"]["completed"], 1);
        assert_eq!(d["stats"]["remaining"], 3);
        assert_eq!(d["stats"]["responders"], 1);
        assert_eq!(d["partial"], true);
    }
    #[test]
    fn permission_error_is_not_packet_loss_or_empty_success() {
        let config = Config {
            targets: vec!["192.0.2.1".into()],
            methods: vec!["icmp".into()],
            ..Default::default()
        };
        let e = run_with(
            &config,
            &cancel(),
            &|_| {},
            &|a, m, s, c, t, permit| {
                let mut o = fake(a, m, s, c, t, permit);
                o.status = "local_error".into();
                o.error = Some(ToolError::new("PERMISSION_DENIED", "fixture denied"));
                o
            },
            &|_, _| neighbors::Snapshot::default(),
        )
        .unwrap_err();
        assert_eq!(e.code, "LOCAL_PROBE_FAILED");
        assert_eq!(
            e.data.unwrap()["rows"][0]["observations"][0]["error"]["code"],
            "PERMISSION_DENIED"
        );
    }
    #[test]
    fn schedule_4096_candidates_without_duplicates_or_missing_results() {
        let config = Config {
            targets: vec!["198.18.0.0-198.18.15.255".into()],
            methods: vec!["tcp".into()],
            ports: "80".into(),
            ..Default::default()
        };
        let d = run_with(&config, &cancel(), &|_| {}, &fake, &|_, _| {
            neighbors::Snapshot::default()
        })
        .unwrap();
        assert_eq!(d["stats"]["candidates"], 4096);
        assert_eq!(d["stats"]["completed"], 4096);
        assert_eq!(d["stats"]["remaining"], 0);
        let rows = d["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 4096);
        assert!(
            rows.iter()
                .all(|r| r["observations"].as_array().unwrap().len() == 1)
        );
    }
}
