//! Evidence-based ICMP PMTU bounds. Silence never reduces a size boundary.
#[cfg(test)]
mod tests;
use crate::{runtime::*, scan::shared_scheduler, target};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub target: String,
    pub family: String,
    pub source: String,
    pub min_ip_bytes: u32,
    pub max_ip_bytes: u32,
    pub retries: u8,
    pub timeout_ms: u64,
    pub total_timeout_ms: u64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            target: "127.0.0.1".into(),
            family: "ipv4".into(),
            source: String::new(),
            min_ip_bytes: 576,
            max_ip_bytes: 1500,
            retries: 1,
            timeout_ms: 1000,
            total_timeout_ms: 30000,
        }
    }
}
pub fn validate(c: &Config) -> Result<AddressFamily, ToolError> {
    let family = match c.family.as_str() {
        "ipv4" => AddressFamily::Ipv4,
        "ipv6" => AddressFamily::Ipv6,
        _ => return Err(bad()),
    };
    let t = target::parse(&c.target)?;
    if !matches!(t.kind.as_str(), "ip" | "hostname")
        || t.port.is_some()
        || c.target.len() > 253
        || c.min_ip_bytes
            < if family == AddressFamily::Ipv6 {
                1280
            } else {
                68
            }
        || c.min_ip_bytes > c.max_ip_bytes
        || c.max_ip_bytes > 65535
        || c.retries > 3
        || !(100..=1000).contains(&c.timeout_ms)
        || !(1000..=120000).contains(&c.total_timeout_ms)
    {
        return Err(bad());
    }
    if let Some(a) = target::literal_socket(&c.target, 0) {
        family.validate(a.ip())?;
    }
    if !c.source.is_empty() {
        family.validate(target::literal_socket(&c.source, 0).ok_or_else(bad)?.ip())?;
    }
    Ok(family)
}
fn bad() -> ToolError {
    ToolError::new(
        "INVALID_ARGUMENT",
        "Invalid PMTU target/family/size/time budget / 路径 MTU 参数无效",
    )
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Probe {
    pub seq: u32,
    pub observed_at: String,
    pub phase: String,
    pub ip_bytes: u32,
    pub ip_header_bytes: u32,
    pub icmp_header_bytes: u32,
    pub payload_bytes: u32,
    pub status: String,
    pub status_code: Option<u64>,
    pub responder: Option<String>,
    pub reported_mtu: Option<u32>,
    pub rtt_ms: Option<f64>,
}
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Stats {
    pub attempts: u32,
    pub replies: u32,
    pub too_big: u32,
    pub unknown: u32,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct ResultData {
    pub schema_version: u32,
    pub target: String,
    pub resolved_target: Option<String>,
    pub source: String,
    pub local_endpoint: Option<String>,
    pub family: String,
    pub adapter: String,
    pub fragmentation_control: String,
    pub started_at: String,
    pub ended_at: String,
    pub duration_ms: u64,
    pub confirmed_lower: Option<u32>,
    /// Smallest explicitly rejected IP length, NOT an inclusive supported MTU.
    pub confirmed_upper: Option<u32>,
    pub reported_mtu: Option<u32>,
    pub probes: Vec<Probe>,
    pub stats: Stats,
    pub unknown_reasons: Vec<String>,
    pub diagnostic_outcome: String,
    pub success: bool,
    pub partial: bool,
    pub limitations: Vec<String>,
}
#[derive(Serialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum Event<'a> {
    PmtuProbe {
        event_seq: u64,
        probe: &'a Probe,
    },
    PmtuSnapshot {
        event_seq: u64,
        snapshot: &'a ResultData,
    },
}
fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
#[derive(PartialEq, Debug)]
enum Verdict {
    Reply,
    TooBig,
    Unknown,
}
struct Search<'a> {
    config: &'a Config,
    cancel: &'a Cancel,
    emit: &'a Sink<'a>,
    start: Instant,
    result: ResultData,
    event_seq: u64,
}
impl Search<'_> {
    fn reason(&mut self, reason: &str) {
        if !self.result.unknown_reasons.iter().any(|r| r == reason) {
            self.result.unknown_reasons.push(reason.into());
        }
    }
    fn snapshot(&mut self) {
        self.result.ended_at = now();
        self.result.duration_ms = self.start.elapsed().as_millis() as u64;
        self.event_seq += 1;
        (self.emit)(
            serde_json::to_value(Event::PmtuSnapshot {
                event_seq: self.event_seq,
                snapshot: &self.result,
            })
            .unwrap(),
        );
    }
    fn candidate(
        &mut self,
        size: u32,
        phase: &str,
        addr: SocketAddr,
        measure: &mut impl FnMut(SocketAddr, u32, u32, &Cancel) -> ToolResult,
    ) -> Result<Verdict, ToolError> {
        let scheduler = shared_scheduler().session(20);
        let mut replies = 0;
        let mut too_big = false;
        for _ in 0..u32::from(self.config.retries) + 2 {
            cancelled(self.cancel)?;
            wait(self.cancel, Duration::from_millis(50))?;
            let _permit = scheduler.acquire(self.cancel)?;
            let remaining = Duration::from_millis(self.config.total_timeout_ms)
                .saturating_sub(self.start.elapsed())
                .as_millis() as u64;
            if remaining == 0 || self.result.probes.len() >= 256 {
                return Err(ToolError::new(
                    "TIMEOUT",
                    "PMTU total deadline / 路径 MTU 总期限到达",
                ));
            }
            let header = if addr.is_ipv6() { 40 } else { 20 };
            let v = measure(
                addr,
                size - header - 8,
                self.config.timeout_ms.min(remaining) as u32,
                self.cancel,
            )?;
            if let Some(local) = v["local_endpoint"].as_str() {
                self.result.local_endpoint = Some(local.into());
            }
            let status = v["status"].as_str().unwrap_or("unknown").to_owned();
            let probe = Probe {
                seq: self.result.probes.len() as u32 + 1,
                observed_at: now(),
                phase: phase.into(),
                ip_bytes: size,
                ip_header_bytes: header,
                icmp_header_bytes: 8,
                payload_bytes: size - header - 8,
                status: status.clone(),
                status_code: v["status_code"].as_u64(),
                responder: if matches!(status.as_str(), "reply" | "packet_too_big") {
                    v["address"].as_str().map(str::to_owned)
                } else {
                    None
                },
                reported_mtu: v["reported_mtu"]
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .filter(|n| *n > 0),
                rtt_ms: v["rtt_ms"].as_f64(),
            };
            self.result.stats.attempts += 1;
            match status.as_str() {
                "reply" => {
                    replies += 1;
                    self.result.stats.replies += 1;
                }
                "packet_too_big" | "local_too_big" => {
                    too_big = true;
                    self.result.stats.too_big += 1;
                    if probe.reported_mtu.is_some() {
                        self.result.reported_mtu = probe.reported_mtu;
                    }
                }
                _ => {
                    self.result.stats.unknown += 1;
                    self.reason(&status);
                }
            }
            self.event_seq += 1;
            (self.emit)(
                serde_json::to_value(Event::PmtuProbe {
                    event_seq: self.event_seq,
                    probe: &probe,
                })
                .unwrap(),
            );
            self.result.probes.push(probe);
            self.snapshot();
            cancelled(self.cancel)?;
            // Require two replies; a correlated explicit error establishes a
            // rejected size, which is checked again during final verification.
            if replies >= 2 || too_big {
                break;
            }
        }
        if replies > 0 && too_big {
            self.reason("inconsistent_path");
            return Ok(Verdict::Unknown);
        }
        Ok(if replies >= 2 {
            Verdict::Reply
        } else if too_big {
            Verdict::TooBig
        } else {
            Verdict::Unknown
        })
    }
    fn search(
        &mut self,
        addr: SocketAddr,
        measure: &mut impl FnMut(SocketAddr, u32, u32, &Cancel) -> ToolResult,
    ) -> Result<(), ToolError> {
        let min = self.config.min_ip_bytes;
        let max = self.config.max_ip_bytes;
        match self.candidate(min, "baseline", addr, measure)? {
            Verdict::Reply => self.result.confirmed_lower = Some(min),
            Verdict::TooBig => {
                self.result.confirmed_upper = Some(min);
                self.reason("baseline_too_big");
                return Ok(());
            }
            Verdict::Unknown => {
                self.reason("baseline_unconfirmed");
                return Ok(());
            }
        }
        if max > min {
            match self.candidate(max, "upper", addr, measure)? {
                Verdict::Reply => self.result.confirmed_lower = Some(max),
                Verdict::TooBig => self.result.confirmed_upper = Some(max),
                Verdict::Unknown => {
                    self.reason("upper_unconfirmed");
                    return Ok(());
                }
            }
        }
        while let (Some(lo), Some(hi)) = (self.result.confirmed_lower, self.result.confirmed_upper)
        {
            if hi <= lo + 1 {
                break;
            }
            // Reported MTU is a hint, never itself a confirmed successful size.
            let size = self
                .result
                .reported_mtu
                .filter(|m| *m > lo && *m < hi)
                .unwrap_or(lo + (hi - lo) / 2);
            match self.candidate(size, "search", addr, measure)? {
                Verdict::Reply => self.result.confirmed_lower = Some(size),
                Verdict::TooBig => self.result.confirmed_upper = Some(size),
                Verdict::Unknown => {
                    self.reason("search_unconfirmed");
                    return Ok(());
                }
            }
        }
        let lo = self.result.confirmed_lower.unwrap();
        if self.candidate(lo, "verify_lower", addr, measure)? != Verdict::Reply {
            self.result.confirmed_lower = None;
            self.result.confirmed_upper = None;
            self.reason("path_changed_or_verification_unknown");
            return Ok(());
        }
        if let Some(hi) = self.result.confirmed_upper
            && self.candidate(hi, "verify_upper", addr, measure)? != Verdict::TooBig
        {
            self.result.confirmed_upper = None;
            self.reason("path_changed_or_verification_unknown");
            return Ok(());
        }
        self.result.success = true;
        self.result.partial = false;
        self.result.diagnostic_outcome = if self.result.confirmed_upper.is_some() {
            "bounded"
        } else {
            "at_least"
        }
        .into();
        Ok(())
    }
}
pub fn run(config: &Config, c: &Cancel, emit: &Sink<'_>) -> ToolResult {
    // UDP connect performs route selection without sending a datagram. Bind the
    // ICMP probe to that concrete source so the reported local IP is enforced.
    let mut chosen_source = None;
    run_with(config, c, emit, &mut |addr, payload, timeout, c| {
        #[cfg(windows)]
        if addr.is_ipv6() {
            return crate::diagnostics::pmtu_echo(addr, &config.source, timeout, payload as usize, c);
        }
        if chosen_source.is_none() {
            let socket = socket2::Socket::new(
                socket2::Domain::for_address(addr),
                socket2::Type::DGRAM,
                Some(socket2::Protocol::UDP),
            )?;
            if !config.source.is_empty() {
                socket
                    .bind(
                        &target::literal_socket(&config.source, 0)
                            .ok_or_else(bad)?
                            .into(),
                    )
                    .map_err(|e| ToolError::new("SOURCE_BIND_FAILED", e.to_string()))?;
            }
            let mut route = addr;
            route.set_port(9);
            socket.connect(&route.into())?;
            let local = socket.local_addr()?.as_socket().ok_or_else(bad)?;
            chosen_source = Some(match local {
                SocketAddr::V6(a) if a.scope_id() > 0 => format!("{}%{}", a.ip(), a.scope_id()),
                _ => local.ip().to_string(),
            });
        }
        let source = chosen_source.as_ref().unwrap();
        let mut value = crate::diagnostics::pmtu_echo(addr, source, timeout, payload as usize, c)?;
        value["local_endpoint"] = json!(source);
        Ok(value)
    })
}
fn run_with(
    config: &Config,
    c: &Cancel,
    emit: &Sink<'_>,
    measure: &mut impl FnMut(SocketAddr, u32, u32, &Cancel) -> ToolResult,
) -> ToolResult {
    let family = validate(config)?;
    let mut s = Search {
        config, cancel: c, emit, start: Instant::now(), event_seq: 0,
        result: ResultData {
            schema_version: 1,
            target: config.target.clone(), resolved_target: None,
            source: config.source.clone(), local_endpoint: None, family: config.family.clone(),
            adapter: if cfg!(windows) {"windows-iphelper-df"} else if cfg!(target_os="macos") {"darwin-datagram-dontfrag"} else if cfg!(target_os="linux") {"linux-ping-pmtudisc-probe"} else {"unavailable"}.into(),
            fragmentation_control: if family == AddressFamily::Ipv4 {"ipv4_df"} else {"ipv6_dontfrag"}.into(),
            started_at: now(), ended_at: now(), duration_ms: 0,
            confirmed_lower: None, confirmed_upper: None, reported_mtu: None,
            probes: vec![], stats: Stats::default(), unknown_reasons: vec![],
            diagnostic_outcome: "unknown".into(), success: false, partial: true,
            limitations: [
                "ICMP observation only; not a TCP/UDP payload guarantee. Upper is the smallest explicitly rejected IP length, not a supported MTU.",
                "Two replies confirm each accepted length. Timeout/unreachable cannot establish a too-large boundary. Local too-big evidence may reflect OS/interface/cache limits.",
                "Source IP binding does not fix an interface. The reported local IP is explicitly bound after OS route selection. Route changes/ECMP may invalidate the interval; verification is a finite observation window.",
                "Windows IPv6 PMTU is unsupported until source fragmentation control is verified; no ordinary-ping fallback. Windows does not expose next-hop MTU through this adapter. Other platforms require native wire acceptance.",
            ].into_iter().map(str::to_owned).collect(),
        },
    };
    let execution = (|| {
        cancelled(c)?;
        let parsed = target::parse(&config.target)?;
        let addr = if let Some(addr) = target::literal_socket(&parsed.host, 0) {
            addr
        } else {
            resolve_all_with_timeout(
                &parsed.host,
                0,
                family,
                c,
                Duration::from_millis(config.total_timeout_ms).min(Duration::from_secs(5)),
            )?
            .into_iter()
            .next()
            .ok_or_else(|| ToolError::new("DNS_ERROR", "No matching address"))?
        };
        s.result.resolved_target = Some(addr.to_string());
        family.validate(addr.ip())?;
        s.search(addr, measure)
    })();
    if let Err(ref e) = execution {
        s.reason(&e.code);
        if e.code == "UNSUPPORTED_CAPABILITY" {
            s.result.fragmentation_control = "unavailable".into();
        }
    }
    s.snapshot();
    let data = serde_json::to_value(&s.result).unwrap();
    match execution {
        Ok(()) => Ok(data),
        Err(mut e) => {
            e.data = Some(data);
            Err(e)
        }
    }
}
