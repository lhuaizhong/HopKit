//! TCP connect scanning: bounded workers, fixed resolution and process-wide fair admission.
use crate::{
    budget::{ConnectionBudget, Limits, Permit},
    runtime::*,
    subnet, target, transport,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    net::{IpAddr, SocketAddr},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub targets: Vec<String>,
    pub ports: String,
    pub timeout_ms: u64,
    pub concurrency: usize,
    pub rate: u32,
    pub family: String,
    pub source: String,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            targets: vec!["127.0.0.1".into()],
            ports: "22,80,443,3389".into(),
            timeout_ms: 1000,
            concurrency: 32,
            rate: 100,
            family: "auto".into(),
            source: String::new(),
        }
    }
}
fn bad(s: &str) -> ToolError {
    ToolError::new("INVALID_ARGUMENT", s)
}
pub fn ports(input: &str) -> Result<Vec<u16>, ToolError> {
    if input.len() > 65536 {
        return Err(bad("Port list too large"));
    }
    let mut ports = BTreeSet::new();
    for item in input.split(',') {
        let item = item.trim();
        let (start, end) = if let Some((a, b)) = item.split_once('-') {
            (a.trim(), b.trim())
        } else {
            (item, item)
        };
        let a = start
            .parse::<u16>()
            .map_err(|_| bad("Use ports 1..65535 and ascending ranges / 端口格式无效"))?;
        let b = end.parse::<u16>().map_err(|_| bad("Invalid port range"))?;
        if a == 0 || a > b {
            return Err(bad("Ports must be 1..65535; ranges ascending"));
        }
        ports.extend(a..=b);
    }
    Ok(ports.into_iter().collect())
}
fn family(config: &Config) -> Result<AddressFamily, ToolError> {
    match config.family.as_str() {
        "auto" => Ok(AddressFamily::Auto),
        "ipv4" => Ok(AddressFamily::Ipv4),
        "ipv6" => Ok(AddressFamily::Ipv6),
        _ => Err(bad("Invalid address family")),
    }
}
/// Expand only bounded numeric IPv4 ranges. IPv6 requires explicit addresses (scopes supported).
pub fn validate(config: &Config) -> Result<(Vec<String>, Vec<u16>), ToolError> {
    let f = family(config)?;
    if config.targets.is_empty()
        || config.targets.len() > 4096
        || !(100..=10000).contains(&config.timeout_ms)
        || config.rate == 0
        || config.rate > 100
    {
        return Err(bad("Targets 1..4096; timeout 100..10000 ms; rate 1..100/s"));
    }
    if !config.source.is_empty() {
        let source = target::literal_socket(&config.source, 0)
            .ok_or_else(|| bad("Source must be a numeric local IP"))?;
        f.validate(source.ip())?;
    }
    let ports = ports(&config.ports)?;
    let mut hosts = BTreeSet::new();
    for input in &config.targets {
        let input = input.trim();
        if input.is_empty() || input.len() > 253 || input.chars().any(|c| c.is_control()) {
            return Err(bad("Invalid scan target"));
        }
        let range = if input.contains('/') {
            let net = subnet::parse(input)?;
            let (a, b, bits) = subnet::bounds(net);
            if bits != 32 {
                return Err(bad(
                    "IPv6 scans require explicit addresses; CIDR enumeration disabled",
                ));
            }
            let normal = net.prefix_len() < 31;
            Some((a + u128::from(normal), b - u128::from(normal)))
        } else if let Some((a, b)) = input.split_once('-').filter(|(a, b)| {
            a.parse::<std::net::Ipv4Addr>().is_ok() && b.parse::<std::net::Ipv4Addr>().is_ok()
        }) {
            Some((
                u32::from(a.parse::<std::net::Ipv4Addr>().unwrap()) as u128,
                u32::from(b.parse::<std::net::Ipv4Addr>().unwrap()) as u128,
            ))
        } else {
            None
        };
        if let Some((a, b)) = range {
            if a > b || b - a + 1 > 4096 {
                return Err(bad("IPv4 range exceeds 4096 candidates or is reversed"));
            }
            f.validate(IpAddr::V4(std::net::Ipv4Addr::from(a as u32)))?;
            for n in a..=b {
                hosts.insert(std::net::Ipv4Addr::from(n as u32).to_string());
            }
        } else if let Some(addr) = target::literal_socket(input, 0) {
            f.validate(addr.ip())?;
            hosts.insert(input.to_string());
        } else {
            if !input
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
            {
                return Err(bad(
                    "Use a hostname, numeric IP, IPv4 CIDR or IPv4 start-end / 目标格式无效",
                ));
            }
            hosts.insert(input.to_string());
        }
        Limits::default().validate_batch(hosts.len(), ports.len(), config.concurrency)?;
    }
    Ok((hosts.into_iter().collect(), ports))
}
struct Admission {
    queue: VecDeque<u64>,
    waiting: BTreeMap<u64, usize>,
    next: BTreeMap<u64, Instant>,
    global_next: Instant,
    serial: u64,
}
pub struct Scheduler {
    state: Mutex<Admission>,
    connections: ConnectionBudget,
}
impl Default for Scheduler {
    fn default() -> Self {
        Self {
            state: Mutex::new(Admission {
                queue: VecDeque::new(),
                waiting: BTreeMap::new(),
                next: BTreeMap::new(),
                global_next: Instant::now(),
                serial: 0,
            }),
            connections: ConnectionBudget::new(128),
        }
    }
}
struct Session<'a> {
    scheduler: &'a Scheduler,
    id: u64,
    rate: u32,
}
impl Scheduler {
    fn session(&self, rate: u32) -> Session<'_> {
        let mut s = self.state.lock().unwrap();
        s.serial += 1;
        let id = s.serial;
        s.queue.push_back(id);
        s.waiting.insert(id, 0);
        s.next.insert(id, Instant::now());
        Session {
            scheduler: self,
            id,
            rate,
        }
    }
}
impl Session<'_> {
    fn acquire(&self, c: &Cancel) -> Result<Permit, ToolError> {
        {
            let mut s = self.scheduler.state.lock().unwrap();
            *s.waiting.get_mut(&self.id).unwrap() += 1;
        }
        loop {
            let mut s = self.scheduler.state.lock().unwrap();
            if cancelled(c).is_err() {
                *s.waiting.get_mut(&self.id).unwrap() -= 1;
                return Err(ToolError::new("CANCELLED", "Scan stopped / 扫描已停止"));
            }
            let now = Instant::now();
            let turn = s
                .queue
                .iter()
                .copied()
                .find(|id| s.waiting[id] > 0 && s.next[id] <= now);
            if now >= s.global_next && turn == Some(self.id) {
                if let Some(permit) = self.scheduler.connections.try_acquire() {
                    *s.waiting.get_mut(&self.id).unwrap() -= 1;
                    s.next.insert(
                        self.id,
                        now + Duration::from_secs_f64(1.0 / self.rate as f64),
                    );
                    s.global_next = now + Duration::from_millis(5);
                    s.queue.retain(|id| *id != self.id);
                    s.queue.push_back(self.id);
                    return Ok(permit);
                }
            }
            drop(s);
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
impl Drop for Session<'_> {
    fn drop(&mut self) {
        let mut s = self.scheduler.state.lock().unwrap();
        s.queue.retain(|id| *id != self.id);
        s.waiting.remove(&self.id);
        s.next.remove(&self.id);
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct Row {
    pub address: String,
    pub port: u16,
    pub status: String,
    pub elapsed_ms: f64,
    pub local_endpoint: Option<String>,
    pub remote_endpoint: String,
    pub error: Option<ToolError>,
}
#[derive(Debug, Serialize)]
pub struct Stats {
    pub total: usize,
    pub completed: usize,
    pub remaining: usize,
    pub interrupted: usize,
    pub counts: BTreeMap<String, usize>,
    pub duration_ms: u128,
}
#[derive(Serialize)]
pub struct ResultData {
    pub schema_version: u32,
    pub resolution_complete: bool,
    pub candidates: Vec<String>,
    pub ports: Vec<u16>,
    pub rows: Vec<Row>,
    pub stats: Stats,
    pub partial: bool,
    pub notes: Vec<&'static str>,
}
#[derive(Serialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum Event<'a> {
    ScanTarget {
        input: &'a str,
        candidates: &'a [String],
    },
    ScanRow {
        row: &'a Row,
    },
    Stats {
        total: usize,
        observed: usize,
        remaining: usize,
    },
}
impl Event<'_> {
    fn emit(self, sink: &Sink<'_>) {
        sink(serde_json::to_value(self).expect("Scan events contain serializable bounded fields"));
    }
}
fn address_text(addr: SocketAddr) -> String {
    match addr {
        SocketAddr::V6(v) if v.scope_id() != 0 => format!("{}%{}", v.ip(), v.scope_id()),
        _ => addr.ip().to_string(),
    }
}

fn probe(address: SocketAddr, config: &Config, c: &Cancel) -> Row {
    let began = Instant::now();
    let result = transport::connect(
        address,
        &config.source,
        c,
        began + Duration::from_millis(config.timeout_ms),
    );
    probe_result(address, began, result)
}
fn probe_result(
    address: SocketAddr,
    began: Instant,
    result: Result<std::net::TcpStream, ToolError>,
) -> Row {
    let (status, local, error) = match result {
        Ok(stream) => (
            "open",
            stream.local_addr().ok().map(|a| a.to_string()),
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
            (status, None, Some(e))
        }
    };
    Row {
        address: address_text(address),
        port: address.port(),
        status: status.into(),
        elapsed_ms: began.elapsed().as_secs_f64() * 1000.0,
        local_endpoint: local,
        remote_endpoint: address.to_string(),
        error,
    }
}
pub fn run(config: &Config, c: &Cancel, emit: &Sink<'_>) -> ToolResult {
    static SHARED: OnceLock<Scheduler> = OnceLock::new();
    run_with(
        config,
        c,
        emit,
        SHARED.get_or_init(Scheduler::default),
        &probe,
    )
}
fn run_with(
    config: &Config,
    c: &Cancel,
    emit: &Sink<'_>,
    scheduler: &Scheduler,
    probe: &(dyn Fn(SocketAddr, &Config, &Cancel) -> Row + Sync),
) -> ToolResult {
    let began = Instant::now();
    let (hosts, ports) = validate(config)?;
    let f = family(config)?;
    let mut addresses = BTreeSet::new();
    for host in hosts {
        let resolved = (|| {
            cancelled(c)?;
            if let Some(addr) = target::literal_socket(&host, 0) {
                Ok(vec![addr])
            } else {
                resolve_all(&host, 0, f, c)
            }
        })().map_err(|mut e| {
            if e.code == "CANCELLED" {
                e.data = Some(json!({
                    "schema_version":1,"resolution_complete":false,
                    "candidates":addresses.iter().copied().map(address_text).collect::<Vec<_>>(),
                    "ports":ports,"rows":[],"partial":true,
                    "stats":{"total":null,"completed":0,"remaining":null,"interrupted":0,
                        "counts":{},"duration_ms":began.elapsed().as_millis()},
                    "notes":["Stopped during resolution; total and remaining are unknown. No probes started."]
                }));
            }
            e
        })?;
        let mut candidates = Vec::new();
        for mut addr in resolved {
            addr.set_port(0);
            f.validate(addr.ip())?;
            addresses.insert(addr);
            candidates.push(address_text(addr));
            if addresses.len() > 4096 {
                return Err(bad("Resolved candidate count exceeds 4096"));
            }
        }
        Event::ScanTarget {
            input: &host,
            candidates: &candidates,
        }
        .emit(emit);
    }
    Limits::default().validate_batch(addresses.len(), ports.len(), config.concurrency)?;
    if !config.source.is_empty() {
        let source = target::literal_socket(&config.source, 0).unwrap();
        if addresses.iter().any(|a| a.is_ipv4() != source.is_ipv4()) {
            return Err(bad(
                "Source and all candidates must have matching address families",
            ));
        }
    }
    let addresses: Vec<_> = addresses.into_iter().collect();
    let total = addresses.len() * ports.len();
    let work = Mutex::new((0usize, Vec::<Row>::with_capacity(total)));
    let session = scheduler.session(config.rate);
    std::thread::scope(|scope| {
        for _ in 0..config.concurrency.min(total) {
            let work = &work;
            let ports = &ports;
            let addresses = &addresses;
            let session = &session;
            scope.spawn(move || {
                loop {
                    let job = {
                        let mut w = work.lock().unwrap();
                        if c.load(std::sync::atomic::Ordering::Relaxed) || w.0 >= total {
                            break;
                        }
                        let job = w.0;
                        w.0 += 1;
                        job
                    };
                    let Ok(_permit) = session.acquire(c) else {
                        break;
                    };
                    if cancelled(c).is_err() {
                        break;
                    }
                    let mut addr = addresses[job / ports.len()];
                    addr.set_port(ports[job % ports.len()]);
                    let row = probe(addr, config, c);
                    let mut w = work.lock().unwrap();
                    w.1.push(row.clone());
                    Event::ScanRow { row: &row }.emit(emit);
                    if w.1.len() % 10 == 0 || w.1.len() == total {
                        Event::Stats {
                            total,
                            observed: w.1.len(),
                            remaining: total - w.1.len(),
                        }
                        .emit(emit);
                    }
                }
            });
        }
    });
    let (_, mut rows) = work.into_inner().unwrap();
    rows.sort_by(|a, b| (&a.address, a.port).cmp(&(&b.address, b.port)));
    let interrupted = rows.iter().filter(|r| r.status == "cancelled").count();
    let completed = rows.len() - interrupted;
    let mut counts = BTreeMap::new();
    for row in &rows {
        *counts.entry(row.status.clone()).or_insert(0) += 1;
    }
    let data = ResultData {
        schema_version: 1,
        resolution_complete: true,
        candidates: addresses.iter().copied().map(address_text).collect(),
        ports,
        rows,
        stats: Stats {
            total,
            completed,
            remaining: total - completed,
            interrupted,
            counts,
            duration_ms: began.elapsed().as_millis(),
        },
        partial: completed < total,
        notes: vec![
            "TCP connect only; port labels are common uses, not detected service identities.",
            "Timeout is not proof of filtering; refusal may originate from a middlebox.",
            "IPv4 CIDRs below /31 exclude network and broadcast; IPv6 requires explicit candidates.",
        ],
    };
    let value = serde_json::to_value(data)
        .map_err(|_| ToolError::new("INTERNAL_ERROR", "Cannot encode scan result"))?;
    if c.load(std::sync::atomic::Ordering::Relaxed) {
        let mut e = ToolError::new(
            "CANCELLED",
            "Scan stopped; partial results retained / 扫描已停止，保留部分结果",
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
    #[test]
    fn cancellation_before_resolution_has_explicit_unknown_totals() {
        let c = Arc::new(AtomicBool::new(true));
        let e = run(&Config::default(), &c, &|_| {}).unwrap_err();
        assert_eq!(e.code, "CANCELLED");
        let d = e.data.unwrap();
        assert_eq!(d["resolution_complete"], false);
        assert_eq!(d["partial"], true);
        assert!(d["stats"]["total"].is_null());
        assert_eq!(d["stats"]["completed"], 0);
        assert_eq!(d["rows"].as_array().unwrap().len(), 0);
    }
    #[test]
    fn validation_before_resolution() {
        assert_eq!(ports("80,80,79-81,65535").unwrap(), vec![79, 80, 81, 65535]);
        for p in ["", "0", "65536", "4-2", "80,"] {
            assert!(ports(p).is_err());
        }
        for target in ["0.0.0.0/0", "::/64", "1.2.3.4-1.2.3.3"] {
            assert!(
                validate(&Config {
                    targets: vec![target.into()],
                    ..Default::default()
                })
                .is_err()
            );
        }
        assert_eq!(
            validate(&Config {
                targets: vec!["192.0.2.0/31".into(), "192.0.2.1".into()],
                ..Default::default()
            })
            .unwrap()
            .0
            .len(),
            2
        );
    }
    #[test]
    fn controlled_outcomes_and_cancellation() {
        let c = Arc::new(AtomicBool::new(false));
        let config = Config {
            ports: "1-20".into(),
            rate: 100,
            concurrency: 1,
            ..Default::default()
        };
        let result = run_with(&config, &c, &|_| {}, &Scheduler::default(), &|a, _, c| {
            if a.port() == 3 {
                c.store(true, Ordering::Relaxed);
            }
            let code = match a.port() {
                1 => "TIMEOUT",
                2 => "NETWORK_UNREACHABLE",
                _ => "CANCELLED",
            };
            probe_result(
                a,
                Instant::now(),
                Err(ToolError::new(code, "Controlled transport error")),
            )
        })
        .unwrap_err();
        assert_eq!(result.code, "CANCELLED");
        let data = result.data.unwrap();
        assert!(data["stats"]["remaining"].as_u64().unwrap() > 0);
        assert_eq!(data["stats"]["counts"]["timeout"], 1);
        assert_eq!(data["stats"]["counts"]["unreachable"], 1);
        assert_eq!(data["stats"]["interrupted"], 1);
        assert_eq!(data["stats"]["completed"], 2);
        assert!(
            data["rows"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["status"] == "timeout")
        );
    }
}

#[cfg(test)]
mod scheduler_tests {
    use super::*;
    use std::sync::{
        Arc, Barrier,
        atomic::{AtomicBool, Ordering},
    };
    #[test]
    fn four_tasks_share_rate_and_each_makes_progress() {
        let scheduler = Scheduler::default();
        let barrier = Barrier::new(4);
        let starts = Mutex::new(Vec::new());
        let c = Arc::new(AtomicBool::new(false));
        std::thread::scope(|scope| {
            for task in 0..4 {
                let scheduler = &scheduler;
                let barrier = &barrier;
                let starts = &starts;
                let c = &c;
                scope.spawn(move || {
                    let session = scheduler.session(100);
                    barrier.wait();
                    for _ in 0..8 {
                        let _p = session.acquire(c).unwrap();
                        starts.lock().unwrap().push((task, Instant::now()));
                    }
                });
            }
        });
        let starts = starts.into_inner().unwrap();
        assert_eq!(starts.len(), 32);
        for task in 0..4 {
            assert_eq!(starts.iter().filter(|(id, _)| *id == task).count(), 8);
            assert!(starts.iter().position(|(id, _)| *id == task).unwrap() < 12);
        }
        let mut times: Vec<_> = starts.iter().map(|(_, t)| *t).collect();
        times.sort();
        for pair in times.windows(2) {
            assert!(pair[1].duration_since(pair[0]) >= Duration::from_millis(4));
        }
        let session = scheduler.session(1);
        drop(session.acquire(&c).unwrap());
        c.store(true, Ordering::Relaxed);
        let begin = Instant::now();
        assert!(session.acquire(&c).is_err());
        assert!(begin.elapsed() < Duration::from_millis(100));
        assert_eq!(
            scheduler
                .state
                .lock()
                .unwrap()
                .waiting
                .values()
                .sum::<usize>(),
            0
        );
    }
}
