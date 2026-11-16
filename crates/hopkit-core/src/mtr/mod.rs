//! Sequential per-TTL ICMP observations. No queued rounds or inferred physical path.
mod names;
use crate::{runtime::*, scan::shared_scheduler, target};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use serde_json::json;
use std::{
    collections::VecDeque,
    net::SocketAddr,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const HISTORY: usize = 256;
const RESPONDERS: usize = 16;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub ptr_server: String,
    #[serde(serialize_with = "crate::target::serialize_target")]
    pub target: String,
    pub family: String,
    pub source: String,
    pub cycles: u32,
    pub duration_secs: u64,
    pub interval_ms: u64,
    pub timeout_ms: u64,
    pub max_hops: u8,
    pub rate: u32,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            ptr_server: String::new(),
            target: "127.0.0.1".into(),
            family: "auto".into(),
            source: String::new(),
            cycles: 100,
            duration_secs: 0,
            interval_ms: 1000,
            timeout_ms: 1000,
            max_hops: 30,
            rate: 20,
        }
    }
}
pub fn validate(c: &Config) -> Result<AddressFamily, ToolError> {
    if !c.ptr_server.is_empty()
        && (c.ptr_server.len() > 128
            || target::literal_socket(&c.ptr_server, 53)
                .or_else(|| c.ptr_server.parse::<SocketAddr>().ok())
                .is_none())
    {
        return Err(bad());
    }
    let family = match c.family.as_str() {
        "auto" => AddressFamily::Auto,
        "ipv4" => AddressFamily::Ipv4,
        "ipv6" => AddressFamily::Ipv6,
        _ => return Err(bad()),
    };
    if c.target.is_empty()
        || c.target.len() > 253
        || c.target
            .chars()
            .any(|ch| ch.is_control() || ch.is_whitespace())
        || c.target.contains('/')
        || c.target.contains('@')
        || c.cycles > 1_000_000
        || c.duration_secs > 86400
        || !(100..=60000).contains(&c.interval_ms)
        || !(100..=1000).contains(&c.timeout_ms)
        || !(1..=64).contains(&c.max_hops)
        || !(1..=100).contains(&c.rate)
    {
        return Err(bad());
    }
    if let Some(a) = target::literal_socket(&c.target, 0) {
        family.validate(a.ip())?;
    } else if !c
        .target
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_'))
    {
        return Err(bad());
    }
    if !c.source.is_empty() {
        let a = target::literal_socket(&c.source, 0).ok_or_else(bad)?;
        family.validate(a.ip())?;
    }
    Ok(family)
}
fn bad() -> ToolError {
    ToolError::new(
        "INVALID_ARGUMENT",
        "MTR: target IP/hostname; cycles 0..1000000, duration 0..86400 s, interval 100..60000 ms, timeout 100..1000 ms, hops 1..64, rate 1..100/s / MTR 参数超出范围",
    )
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct RttStats {
    pub count: u64,
    pub last_ms: Option<f64>,
    pub min_ms: Option<f64>,
    pub avg_ms: Option<f64>,
    pub max_ms: Option<f64>,
    pub jitter_ms: Option<f64>,
    pub jitter_pairs: u64,
}
impl RttStats {
    fn record(&mut self, rtt: f64, previous: Option<f64>) {
        self.count += 1;
        self.last_ms = Some(rtt);
        self.min_ms = Some(self.min_ms.map_or(rtt, |n| n.min(rtt)));
        self.max_ms = Some(self.max_ms.map_or(rtt, |n| n.max(rtt)));
        let avg = self.avg_ms.unwrap_or(0.);
        self.avg_ms = Some(avg + (rtt - avg) / self.count as f64);
        if let Some(prev) = previous {
            self.jitter_pairs += 1;
            let avg = self.jitter_ms.unwrap_or(0.);
            self.jitter_ms = Some(avg + ((rtt - prev).abs() - avg) / self.jitter_pairs as f64);
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Responder {
    pub address: String,
    pub responses: u64,
    pub rtt: RttStats,
}
#[derive(Clone, Debug, Serialize)]
pub struct Hop {
    pub ttl: u8,
    pub sent: u64,
    pub responses: u64,
    pub timeouts: u64,
    pub interrupted: u64,
    pub no_response_percent: Option<f64>,
    pub rtt: RttStats,
    pub responders: Vec<Responder>,
    pub untracked_responses: u64,
    pub last_status: String,
    #[serde(skip)]
    previous: Option<(String, f64)>,
    #[serde(skip)]
    last_address: Option<String>,
}
impl Hop {
    fn new(ttl: u8) -> Self {
        Self {
            ttl,
            sent: 0,
            responses: 0,
            timeouts: 0,
            interrupted: 0,
            no_response_percent: None,
            rtt: RttStats::default(),
            responders: vec![],
            untracked_responses: 0,
            last_status: String::new(),
            previous: None,
            last_address: None,
        }
    }
    fn record(&mut self, sample: &Sample) -> Option<String> {
        self.last_status = sample.status.clone();
        let mut changed = None;
        if let (Some(address), Some(rtt)) = (&sample.address, sample.rtt_ms) {
            self.responses += 1;
            if self.last_address.as_ref().is_some_and(|a| a != address) {
                changed = self.last_address.clone();
            }
            self.last_address = Some(address.clone());
            let prev = self
                .previous
                .as_ref()
                .filter(|(a, _)| a == address)
                .map(|(_, v)| *v);
            self.rtt.record(rtt, prev);
            if !self.responders.iter().any(|r| &r.address == address)
                && self.responders.len() < RESPONDERS
            {
                self.responders.push(Responder {
                    address: address.clone(),
                    responses: 0,
                    rtt: RttStats::default(),
                });
            }
            if let Some(row) = self.responders.iter_mut().find(|r| &r.address == address) {
                row.responses += 1;
                row.rtt.record(rtt, prev);
            } else {
                self.untracked_responses += 1;
            }
            self.previous = Some((address.clone(), rtt));
        } else {
            self.timeouts += 1;
            self.previous = None;
        }
        let completed = self.responses + self.timeouts;
        self.no_response_percent =
            (completed > 0).then(|| self.timeouts as f64 * 100. / completed as f64);
        changed
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Sample {
    pub seq: u64,
    pub cycle: u64,
    pub ttl: u8,
    pub observed_at: String,
    pub elapsed_ms: u64,
    pub status: String,
    pub address: Option<String>,
    pub rtt_ms: Option<f64>,
    pub target_reached: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct Change {
    pub observed_at: String,
    pub cycle: u64,
    pub ttl: u8,
    pub previous: String,
    pub address: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct Gap {
    pub observed_at: String,
    pub elapsed_ms: u64,
    pub duration_ms: u64,
    pub reason: &'static str,
}
#[derive(Clone, Debug, Serialize)]
pub struct Stats {
    pub cycles_started: u64,
    pub cycles_completed: u64,
    pub target_reached_cycles: u64,
    pub target_reach_percent: Option<f64>,
    pub sent: u64,
    pub responses: u64,
    pub timeouts: u64,
    pub interrupted: u64,
    pub gaps: u64,
    pub path_changes: u64,
}
#[derive(Clone, Debug, Serialize)]
pub struct ResultData {
    pub names: Vec<names::Name>,
    pub names_dropped: u64,
    pub schema_version: u32,
    pub target: String,
    pub resolved_target: Option<String>,
    pub source: String,
    pub started_at: String,
    pub ended_at: String,
    pub duration_ms: u64,
    pub stats: Stats,
    pub hops: Vec<Hop>,
    pub samples: VecDeque<Sample>,
    pub path_changes: VecDeque<Change>,
    pub gaps: VecDeque<Gap>,
    pub samples_dropped: u64,
    pub changes_dropped: u64,
    pub gaps_dropped: u64,
    pub target_outcome: &'static str,
    pub success: bool,
    pub partial: bool,
    pub limitations: Vec<&'static str>,
}
#[derive(Serialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum Event<'a> {
    MtrSample { sample: &'a Sample },
    MtrSnapshot { snapshot: &'a ResultData },
}
fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
fn wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn push<T>(queue: &mut VecDeque<T>, value: T, dropped: &mut u64) {
    if queue.len() == HISTORY {
        queue.pop_front();
        *dropped += 1;
    }
    queue.push_back(value);
}
// Compare both clocks: suspend may or may not advance Instant on a given OS.
fn gap_ms(monotonic_ms: u64, wall_delta: i128, expected_ms: u64) -> u64 {
    let actual = monotonic_ms.max(wall_delta.max(0).min(u64::MAX as i128) as u64);
    if actual > expected_ms.saturating_add(2000) {
        actual.saturating_sub(expected_ms)
    } else {
        0
    }
}
impl ResultData {
    fn new(c: &Config) -> Self {
        Self {
            names: vec![],
            names_dropped: 0,
            schema_version: 1,
            target: c.target.clone(),
            resolved_target: None,
            source: c.source.clone(),
            started_at: now(),
            ended_at: now(),
            duration_ms: 0,
            stats: Stats {
                cycles_started: 0,
                cycles_completed: 0,
                target_reached_cycles: 0,
                target_reach_percent: None,
                sent: 0,
                responses: 0,
                timeouts: 0,
                interrupted: 0,
                gaps: 0,
                path_changes: 0,
            },
            hops: vec![],
            samples: VecDeque::new(),
            path_changes: VecDeque::new(),
            gaps: VecDeque::new(),
            samples_dropped: 0,
            changes_dropped: 0,
            gaps_dropped: 0,
            target_outcome: "unknown",
            success: false,
            partial: false,
            limitations: vec![
                "Intermediate no-response is not forwarding loss. RTT differences are not link latency.",
                "Multiple responders are observations, not a proven unique physical path. ECMP, asymmetric routing and ICMP rate limiting apply; no Paris traceroute.",
                "ICMP only; one outstanding probe per task. Windows matching is delegated to IP Helper; Unix matches family, target, id/sequence and quoted fields.",
                "Source IP binding only; no fixed-interface guarantee. Optional PTR uses explicit resolver over separate OS-routed UDP, 300ms timeout, no TCP fallback; not ICMP evidence.",
                "Gap detection is heuristic (wall/monotonic excess >2s); physical sleep and interface/VPN changes need platform acceptance.",
                "Last 256 samples/changes/gaps, at most 16 responders per TTL. PTR at most 256 addresses and 4 names each. Counters remain cumulative; preview may be incomplete.",
            ],
        }
    }
    fn gap(&mut self, duration_ms: u64, elapsed_ms: u64) {
        if duration_ms == 0 {
            return;
        }
        self.stats.gaps += 1;
        push(
            &mut self.gaps,
            Gap {
                observed_at: now(),
                elapsed_ms,
                duration_ms,
                reason: "suspend_or_scheduler_delay",
            },
            &mut self.gaps_dropped,
        );
        for hop in &mut self.hops {
            hop.previous = None;
        }
    }
    fn snapshot(&mut self, start: Instant, emit: &Sink<'_>) {
        self.ended_at = now();
        self.duration_ms = start.elapsed().as_millis() as u64;
        self.stats.target_reach_percent = (self.stats.cycles_completed > 0).then(|| {
            self.stats.target_reached_cycles as f64 * 100. / self.stats.cycles_completed as f64
        });
        emit(serde_json::to_value(Event::MtrSnapshot { snapshot: self }).unwrap());
    }
}
pub fn run(config: &Config, c: &Cancel, emit: &Sink<'_>) -> ToolResult {
    let sequence = std::sync::atomic::AtomicU64::new(0);
    let ordered = |mut value: serde_json::Value| {
        value["event_seq"] =
            serde_json::json!(sequence.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1);
        emit(value);
    };
    run_with(config, c, &ordered, &mut |addr, ttl, c| {
        crate::diagnostics::mtr_echo(addr, &config.source, ttl, config.timeout_ms as u32, c)
    })
}
fn run_with(
    config: &Config,
    c: &Cancel,
    emit: &Sink<'_>,
    probe: &mut impl FnMut(SocketAddr, u8, &Cancel) -> ToolResult,
) -> ToolResult {
    let mut family = validate(config)?;
    if family == AddressFamily::Auto && !config.source.is_empty() {
        family = if target::literal_socket(&config.source, 0).unwrap().is_ipv4() {
            AddressFamily::Ipv4
        } else {
            AddressFamily::Ipv6
        };
    }
    let started = Instant::now();
    let mut result = ResultData::new(config);
    let mut names =
        (!config.ptr_server.is_empty()).then(|| names::Names::new(config.ptr_server.clone()));
    let execution = (|| -> Result<(), ToolError> {
        cancelled(c)?;
        let addr = resolve(&config.target, 0, family, c)?;
        if !config.source.is_empty() {
            let source = target::literal_socket(&config.source, 0).unwrap();
            if source.is_ipv6() != addr.is_ipv6() {
                return Err(bad());
            }
        }
        result.resolved_target = Some(addr.to_string());
        let scheduler = shared_scheduler().session(config.rate);
        let mut last_snapshot = Instant::now();
        while config.cycles == 0 || result.stats.cycles_completed < u64::from(config.cycles) {
            cancelled(c)?;
            if config.duration_secs > 0
                && started.elapsed() >= Duration::from_secs(config.duration_secs)
            {
                break;
            }
            let round = Instant::now();
            result.stats.cycles_started += 1;
            let mut reached = false;
            let mut complete = true;
            for ttl in 1..=config.max_hops {
                cancelled(c)?;
                if config.duration_secs > 0
                    && started.elapsed() >= Duration::from_secs(config.duration_secs)
                {
                    complete = false;
                    break;
                }
                let before_admit = Instant::now();
                let wall = wall_ms();
                let _permit = scheduler.acquire(c)?;
                result.gap(
                    gap_ms(
                        before_admit.elapsed().as_millis() as u64,
                        wall_ms() as i128 - wall as i128,
                        1000,
                    ),
                    started.elapsed().as_millis() as u64,
                );
                if config.duration_secs > 0
                    && started.elapsed() >= Duration::from_secs(config.duration_secs)
                {
                    complete = false;
                    break;
                }
                if result.hops.len() < ttl as usize {
                    result.hops.push(Hop::new(ttl));
                }
                result.hops[ttl as usize - 1].sent += 1;
                result.stats.sent += 1;
                let before = Instant::now();
                let wall = wall_ms();
                let measured = probe(addr, ttl, c);
                let gap = gap_ms(
                    before.elapsed().as_millis() as u64,
                    wall_ms() as i128 - wall as i128,
                    config.timeout_ms,
                );
                if gap > 0 || cancelled(c).is_err() || measured.is_err() {
                    result.hops[ttl as usize - 1].interrupted += 1;
                    result.hops[ttl as usize - 1].previous = None;
                    result.stats.interrupted += 1;
                    result.gap(gap, started.elapsed().as_millis() as u64);
                    cancelled(c)?;
                    measured?;
                    complete = false;
                    break;
                }
                let v = measured?;
                let status = v["status"].as_str().unwrap_or("timeout");
                let address = v["address"]
                    .as_str()
                    .filter(|_| status != "timeout")
                    .map(str::to_owned);
                let rtt = v["rtt_ms"]
                    .as_f64()
                    .filter(|n| n.is_finite() && *n >= 0.)
                    .filter(|_| address.is_some());
                let hit = status == "reply"
                    && address
                        .as_deref()
                        .and_then(|a| a.parse::<std::net::IpAddr>().ok())
                        == Some(addr.ip())
                    && rtt.is_some();
                let sample = Sample {
                    seq: result.stats.sent,
                    cycle: result.stats.cycles_started,
                    ttl,
                    observed_at: now(),
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    status: status.into(),
                    address: address.filter(|_| rtt.is_some()),
                    rtt_ms: rtt,
                    target_reached: hit,
                };
                if let Some(names) = &mut names {
                    names.drain();
                    if let Some(address) = &sample.address {
                        names.schedule(address);
                    }
                    result.names = names.rows.values().cloned().collect();
                    result.names_dropped = names.dropped;
                }
                if let Some(prev) = result.hops[ttl as usize - 1].record(&sample) {
                    result.stats.path_changes += 1;
                    push(
                        &mut result.path_changes,
                        Change {
                            observed_at: sample.observed_at.clone(),
                            cycle: sample.cycle,
                            ttl,
                            previous: prev,
                            address: sample.address.clone().unwrap(),
                        },
                        &mut result.changes_dropped,
                    );
                }
                if rtt.is_some() {
                    result.stats.responses += 1;
                } else {
                    result.stats.timeouts += 1;
                }
                emit(serde_json::to_value(Event::MtrSample { sample: &sample }).unwrap());
                push(&mut result.samples, sample, &mut result.samples_dropped);
                if last_snapshot.elapsed() >= Duration::from_secs(1) {
                    result.snapshot(started, emit);
                    last_snapshot = Instant::now();
                }
                if hit {
                    reached = true;
                    break;
                }
            }
            if complete {
                result.stats.cycles_completed += 1;
                if reached {
                    result.stats.target_reached_cycles += 1;
                }
            }
            if reached {
                result.target_outcome = "reached";
                result.success = true;
            }
            // A shorter route can leave later TTLs unobserved for this cycle.
            // Do not join jitter across those missing observations.
            let visited = result
                .samples
                .back()
                .filter(|s| s.cycle == result.stats.cycles_started)
                .map_or(0, |s| s.ttl as usize);
            for hop in result.hops.iter_mut().skip(visited) {
                hop.previous = None;
            }
            cancelled(c)?;
            if last_snapshot.elapsed() >= Duration::from_millis(250) {
                result.snapshot(started, emit);
                last_snapshot = Instant::now();
            }
            if config.cycles > 0 && result.stats.cycles_completed >= u64::from(config.cycles) {
                break;
            }
            let sleep = Duration::from_millis(config.interval_ms).saturating_sub(round.elapsed());
            let sleep = if config.duration_secs > 0 {
                sleep.min(
                    Duration::from_secs(config.duration_secs).saturating_sub(started.elapsed()),
                )
            } else {
                sleep
            };
            let before = Instant::now();
            let wall = wall_ms();
            wait(c, sleep)?;
            result.gap(
                gap_ms(
                    before.elapsed().as_millis() as u64,
                    wall_ms() as i128 - wall as i128,
                    sleep.as_millis() as u64,
                ),
                started.elapsed().as_millis() as u64,
            );
        }
        Ok(())
    })();
    if let Some(names) = &mut names {
        names.finish();
        result.names = names.rows.values().cloned().collect();
        result.names_dropped = names.dropped;
    }
    result.partial =
        execution.is_err() || result.stats.cycles_started != result.stats.cycles_completed;
    if !result.success && result.stats.cycles_completed > 0 {
        result.target_outcome = "not_reached";
    }
    result.snapshot(started, emit);
    let value = serde_json::to_value(result).unwrap();
    match execution {
        Ok(()) => Ok(value),
        Err(mut e) => {
            e.data = Some(value);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    fn sample(address: Option<&str>, rtt: Option<f64>) -> Sample {
        Sample {
            seq: 1,
            cycle: 1,
            ttl: 1,
            observed_at: now(),
            elapsed_ms: 0,
            status: if address.is_some() {
                "ttl_expired"
            } else {
                "timeout"
            }
            .into(),
            address: address.map(str::to_owned),
            rtt_ms: rtt,
            target_reached: false,
        }
    }
    #[test]
    fn jitter_breaks_at_timeout_change_and_gap() {
        let mut h = Hop::new(1);
        h.record(&sample(Some("192.0.2.1"), Some(10.)));
        h.record(&sample(Some("192.0.2.1"), Some(16.)));
        assert_eq!(h.rtt.jitter_ms, Some(6.));
        h.record(&sample(None, None));
        h.record(&sample(Some("192.0.2.1"), Some(90.)));
        h.record(&sample(Some("192.0.2.2"), Some(5.)));
        assert_eq!(h.rtt.jitter_pairs, 1);
        h.previous = None;
        h.record(&sample(Some("192.0.2.2"), Some(80.)));
        assert_eq!(h.rtt.jitter_pairs, 1);
        assert_eq!(h.rtt.count, 5);
        assert_eq!(h.no_response_percent, Some(100. / 6.));
    }
    #[test]
    fn bounded_responders_keep_total_and_histories() {
        let mut h = Hop::new(1);
        for n in 0..100 {
            h.record(&sample(Some(&format!("192.0.2.{n}")), Some(n as f64)));
        }
        assert_eq!(h.responses, 100);
        assert_eq!(h.responders.len(), 16);
        assert_eq!(h.untracked_responses, 84);
        let mut q = VecDeque::new();
        let mut dropped = 0;
        for n in 0..10000 {
            push(&mut q, n, &mut dropped);
        }
        assert_eq!(q.len(), 256);
        assert_eq!(dropped, 9744);
    }
    #[test]
    fn sleep_clocks_do_not_create_timeouts() {
        assert_eq!(gap_ms(100, 30000, 100), 29900);
        assert_eq!(gap_ms(30000, 100, 100), 29900);
        assert_eq!(gap_ms(100, -10000, 100), 0);
        assert_eq!(gap_ms(1000, 1000, 1000), 0);
    }
    #[test]
    fn missing_middle_multiple_responders_and_target_are_separate() {
        let c = Arc::new(AtomicBool::new(false));
        let config = Config {
            cycles: 2,
            interval_ms: 100,
            rate: 100,
            max_hops: 4,
            ..Default::default()
        };
        let mut i = 0;
        let v=run_with(&config,&c,&|_|{},&mut |_,ttl,_| {i+=1;Ok(match ttl {1=>json!({"status":"timeout"}),2=>json!({"status":"ttl_expired","address":if i<4{"192.0.2.1"}else{"192.0.2.2"},"rtt_ms":2}),_=>json!({"status":"reply","address":"127.0.0.1","rtt_ms":5})})}).unwrap();
        assert_eq!(v["stats"]["target_reach_percent"], 100.);
        assert_eq!(v["hops"][0]["no_response_percent"], 100.);
        assert_eq!(v["stats"]["sent"], 6);
        assert_eq!(v["stats"]["path_changes"], 1);
        assert_eq!(v["hops"].as_array().unwrap().len(), 3);
    }
    #[test]
    fn cancelled_inflight_is_not_loss() {
        let c = Arc::new(AtomicBool::new(false));
        let v = run_with(&Config::default(), &c, &|_| {}, &mut |_, _, c| {
            c.store(true, Ordering::Relaxed);
            Ok(json!({"status":"timeout"}))
        })
        .unwrap_err();
        assert_eq!(v.code, "CANCELLED");
        let d = v.data.unwrap();
        assert_eq!(d["stats"]["sent"], 1);
        assert_eq!(d["stats"]["timeouts"], 0);
        assert_eq!(d["stats"]["interrupted"], 1);
        assert!(d["hops"][0]["no_response_percent"].is_null());
    }
    #[test]
    fn local_permission_error_keeps_partial_without_loss() {
        let c = Arc::new(AtomicBool::new(false));
        let e = run_with(&Config::default(), &c, &|_| {}, &mut |_, _, _| {
            Err(ToolError::new("PERMISSION_DENIED", "denied"))
        })
        .unwrap_err();
        assert_eq!(e.code, "PERMISSION_DENIED");
        let d = e.data.unwrap();
        assert_eq!(d["stats"]["timeouts"], 0);
        assert_eq!(d["stats"]["interrupted"], 1);
        assert_eq!(d["target_outcome"], "unknown");
    }
    #[test]
    fn slow_probe_produces_gap_and_no_fabricated_timeout() {
        let c = Arc::new(AtomicBool::new(false));
        let mut first = true;
        let config = Config {
            cycles: 1,
            timeout_ms: 100,
            interval_ms: 100,
            rate: 100,
            max_hops: 1,
            ..Default::default()
        };
        let v = run_with(&config, &c, &|_| {}, &mut |_, _, _| {
            if first {
                first = false;
                std::thread::sleep(Duration::from_millis(2200));
            }
            Ok(json!({"status":"reply","address":"127.0.0.1","rtt_ms":1}))
        })
        .unwrap();
        assert_eq!(v["stats"]["gaps"], 1);
        assert_eq!(v["stats"]["interrupted"], 1);
        assert_eq!(v["stats"]["timeouts"], 0);
        assert_eq!(v["stats"]["cycles_started"], 2);
        assert_eq!(v["stats"]["cycles_completed"], 1);
        assert_eq!(v["samples"].as_array().unwrap().len(), 1);
    }
    #[test]
    fn shared_admission_limits_all_ttls_even_when_round_is_slow() {
        let c = Arc::new(AtomicBool::new(false));
        let config = Config {
            cycles: 1,
            max_hops: 4,
            rate: 10,
            ..Default::default()
        };
        let start = Instant::now();
        let v = run_with(&config, &c, &|_| {}, &mut |_, _, _| {
            Ok(json!({"status":"timeout"}))
        })
        .unwrap();
        assert!(start.elapsed() >= Duration::from_millis(290));
        assert_eq!(v["stats"]["sent"], 4);
        assert_eq!(v["target_outcome"], "not_reached");
        assert_eq!(v["success"], false);
    }
    #[test]
    fn stop_during_last_sample_is_cancelled_with_retained_response() {
        let c = Arc::new(AtomicBool::new(false));
        let config = Config {
            cycles: 1,
            max_hops: 1,
            ..Default::default()
        };
        let e = run_with(
            &config,
            &c,
            &|v| {
                if v["event_type"] == "mtr_sample" {
                    c.store(true, Ordering::Relaxed);
                }
            },
            &mut |_, _, _| Ok(json!({"status":"reply","address":"127.0.0.1","rtt_ms":1})),
        )
        .unwrap_err();
        assert_eq!(e.code, "CANCELLED");
        let d = e.data.unwrap();
        assert_eq!(d["stats"]["responses"], 1);
        assert_eq!(d["stats"]["timeouts"], 0);
        assert_eq!(d["stats"]["cycles_completed"], 1);
    }
}
