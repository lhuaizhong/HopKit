#[cfg(target_os = "macos")]
use crate::macos_icmp::icmp;
use crate::runtime::*;
#[cfg(not(target_os = "macos"))]
use serde_json::Value;
use serde_json::json;
#[cfg(windows)]
use std::net::IpAddr;
use std::{
    net::TcpStream,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[cfg(windows)]
fn icmp(ip: IpAddr, ttl: u8, timeout: u32, payload_size: usize) -> Result<Value, ToolError> {
    use windows_sys::Win32::{
        Foundation::{GetLastError, INVALID_HANDLE_VALUE},
        NetworkManagement::IpHelper::*,
    };
    unsafe {
        let handle = if ip.is_ipv4() {
            IcmpCreateFile()
        } else {
            Icmp6CreateFile()
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(ToolError::new(
                "PERMISSION_DENIED",
                format!("ICMP create: {}", GetLastError()),
            ));
        }
        let payload = vec![0x6e_u8; payload_size];
        let opts = IP_OPTION_INFORMATION {
            Ttl: ttl,
            Tos: 0,
            Flags: 0,
            OptionsSize: 0,
            OptionsData: std::ptr::null_mut(),
        };
        // Aligned storage for Windows reply structures and quoted data.
        let mut buf = vec![0_u64; (payload_size + 1024).div_ceil(8)];
        let start = Instant::now();
        let n = match ip {
            IpAddr::V4(v) => IcmpSendEcho2Ex(
                handle,
                std::ptr::null_mut(),
                None,
                std::ptr::null(),
                0,
                u32::from_ne_bytes(v.octets()),
                payload.as_ptr().cast(),
                payload.len() as u16,
                &opts,
                buf.as_mut_ptr().cast(),
                (buf.len() * 8) as u32,
                timeout,
            ),
            IpAddr::V6(v) => {
                use windows_sys::Win32::Networking::WinSock::*;
                let mut src: SOCKADDR_IN6 = std::mem::zeroed();
                src.sin6_family = AF_INET6;
                let mut dst: SOCKADDR_IN6 = std::mem::zeroed();
                dst.sin6_family = AF_INET6;
                dst.sin6_addr.u.Byte = v.octets();
                Icmp6SendEcho2(
                    handle,
                    std::ptr::null_mut(),
                    None,
                    std::ptr::null(),
                    &src,
                    &dst,
                    payload.as_ptr().cast(),
                    payload.len() as u16,
                    &opts,
                    buf.as_mut_ptr().cast(),
                    (buf.len() * 8) as u32,
                    timeout,
                )
            }
        };
        let err = GetLastError();
        IcmpCloseHandle(handle);
        if n == 0 {
            return Ok(
                json!({"ok":false,"status_code":err,"status":if err==11010{"timeout"}else{"icmp_error"},"rtt_ms":null,"address":ip.to_string()}),
            );
        }
        if ip.is_ipv4() {
            let reply = &*(buf.as_ptr() as *const ICMP_ECHO_REPLY);
            let from = std::net::Ipv4Addr::from(reply.Address.to_ne_bytes());
            Ok(
                json!({"ok":reply.Status==0,"responded":reply.Status!=11010,"status_code":reply.Status,"status":if reply.Status==0{"reply"}else if reply.Status==11010{"timeout"}else if reply.Status==11013||reply.Status==11014{"ttl_expired"}else if (11002..=11005).contains(&reply.Status){"unreachable"}else{"icmp_error"},"rtt_ms":reply.RoundTripTime,"address":from.to_string()}),
            )
        } else {
            let reply = &*(buf.as_ptr() as *const ICMPV6_ECHO_REPLY_LH);
            let from = std::net::Ipv6Addr::from(reply.Address.sin6_addr);
            Ok(
                json!({"ok":reply.Status==0,"responded":reply.Status!=11010,"status_code":reply.Status,"status":if reply.Status==0{"reply"}else if reply.Status==11010{"timeout"}else if reply.Status==11013||reply.Status==11014{"ttl_expired"}else if (11002..=11005).contains(&reply.Status){"unreachable"}else{"icmp_error"},"rtt_ms":start.elapsed().as_secs_f64()*1000.0,"address":from.to_string()}),
            )
        }
    }
}
#[cfg(not(any(windows, target_os = "macos")))]
fn icmp(_: std::net::SocketAddr, _: u8, _: u32, _: usize, _: &Cancel) -> Result<Value, ToolError> {
    Err(ToolError::new(
        "UNSUPPORTED_CAPABILITY",
        "ICMP is available on Windows and macOS",
    ))
}

pub fn probe(r: &Request, c: &Cancel, emit: &Sink<'_>) -> ToolResult {
    probe_with(r, c, emit, &mut network_probe, &mut |c, duration| {
        wait(c, duration)
    })
}

// The same loop is used by the platform adapters and deterministic fault-injection tests.
fn probe_with(
    r: &Request,
    c: &Cancel,
    emit: &Sink<'_>,
    measure: &mut impl FnMut(&Request, std::net::SocketAddr, u8, &Cancel) -> ToolResult,
    pause: &mut impl FnMut(&Cancel, Duration) -> Result<(), ToolError>,
) -> ToolResult {
    if r.tool == "tcping" && r.port == 0 {
        return Err(ToolError::new("INVALID_ARGUMENT", "TCP port is required"));
    }
    let mut stats = ProbeStats::new(&r.target, &r.tool);
    let execution = (|| -> Result<(), ToolError> {
        cancelled(c)?;
        let addr = resolve(&r.target, r.port, r.ipv6, c)?;
        stats.target = addr.ip().to_string();
        emit(
            json!({"event_type":"target","address":addr.ip().to_string(),"port":r.port,"payload_size_bytes":r.payload_size_bytes}),
        );
        let end = Instant::now() + Duration::from_secs(120);
        let limit = if r.tool == "trace" {
            u64::from(r.max_hops) * 3
        } else {
            u64::from(r.count)
        };
        while limit == 0 || stats.sent < limit {
            cancelled(c)?;
            if r.tool == "trace" && Instant::now() > end {
                break;
            }
            let start = Instant::now();
            let hop = if r.tool == "trace" {
                (stats.sent / 3 + 1) as u8
            } else {
                128
            };
            let timestamp = wall_ms();
            let elapsed_ms = stats.started.elapsed().as_secs_f64() * 1000.0;
            stats.sent += 1;
            let mut sample = match measure(r, addr, hop, c) {
                Ok(sample) => sample,
                Err(error) => {
                    // A cancelled call has no completed measurement. Keep it separate
                    // from timeout/loss and preserve all earlier completed probes.
                    stats.interrupted += 1;
                    return Err(error);
                }
            };
            // Synchronous adapters may return after Stop was requested. Do not
            // turn that cancelled in-flight call into a timeout or a success.
            if let Err(error) = cancelled(c) {
                stats.interrupted += 1;
                return Err(error);
            }
            sample["event_type"] = json!("sample");
            sample["seq"] = json!(stats.sent);
            sample["timestamp_ms"] = json!(timestamp);
            sample["elapsed_ms"] = json!(elapsed_ms);
            if r.tool == "ping" {
                sample["payload_size_bytes"] = json!(r.payload_size_bytes);
            }
            if r.tool == "trace" {
                sample["hop"] = json!(hop);
            }
            stats.record(&mut sample);
            emit(sample);
            let mut snapshot = stats.snapshot();
            snapshot["event_type"] = json!("stats");
            emit(snapshot);
            cancelled(c)?;
            if r.tool == "trace" {
                if stats.reached && stats.sent.is_multiple_of(3) {
                    break;
                }
            } else if limit == 0 || stats.sent < limit {
                pause(
                    c,
                    Duration::from_millis(r.interval_ms.max(50)).saturating_sub(start.elapsed()),
                )?;
            }
        }
        Ok(())
    })();
    let mut result = stats.snapshot();
    result["ended_at_ms"] = json!(wall_ms());
    result["samples"] = json!(stats.samples);
    if r.tool == "ping" {
        result["payload_size_bytes"] = json!(r.payload_size_bytes);
    }
    match execution {
        Ok(()) => Ok(result),
        Err(mut error) => {
            error.data = Some(result);
            Err(error)
        }
    }
}

pub(crate) fn network_probe(
    r: &Request,
    addr: std::net::SocketAddr,
    hop: u8,
    c: &Cancel,
) -> ToolResult {
    if r.tool == "tcping" {
        cancelled(c)?;
        let start = Instant::now();
        match TcpStream::connect_timeout(&addr, Duration::from_millis(r.timeout_ms)) {
            Ok(stream) => {
                drop(stream);
                Ok(
                    json!({"ok":true,"status":"connected","address":addr.ip().to_string(),"rtt_ms":start.elapsed().as_secs_f64()*1000.0}),
                )
            }
            Err(e) => Ok(
                json!({"ok":false,"status":if e.kind()==std::io::ErrorKind::ConnectionRefused{"refused"}else if is_timeout(&e){"timeout"}else{"connect_error"},"detail":e.to_string(),"rtt_ms":null,"address":addr.ip().to_string()}),
            ),
        }
    } else {
        let size = if r.tool == "ping" {
            r.payload_size_bytes as usize
        } else {
            32
        };
        #[cfg(windows)]
        {
            icmp(addr.ip(), hop, r.timeout_ms as u32, size)
        }
        #[cfg(not(windows))]
        {
            icmp(addr, hop, r.timeout_ms as u32, size, c)
        }
    }
}

fn wall_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

// Counters do not depend on either the retained samples or the consumer's event queue.
struct ProbeStats {
    target: String,
    tool: String,
    started: Instant,
    started_at: u128,
    sent: u64,
    completed: u64,
    received: u64,
    interrupted: u64,
    total: f64,
    minimum: f64,
    maximum: f64,
    reached: bool,
    hops: u64,
    timeouts: u64,
    failures: std::collections::BTreeMap<String, u64>,
    samples: Vec<serde_json::Value>,
}
impl ProbeStats {
    fn new(target: &str, tool: &str) -> Self {
        Self {
            target: target.into(),
            tool: tool.into(),
            started: Instant::now(),
            started_at: wall_ms(),
            sent: 0,
            completed: 0,
            received: 0,
            interrupted: 0,
            total: 0.0,
            minimum: f64::INFINITY,
            maximum: 0.0,
            reached: false,
            hops: 0,
            timeouts: 0,
            failures: Default::default(),
            samples: Vec::new(),
        }
    }
    fn record(&mut self, sample: &mut serde_json::Value) {
        self.completed += 1;
        let status = sample["status"].as_str().unwrap_or("unknown").to_owned();
        let reached = sample["ok"] == true;
        let responded = sample["responded"].as_bool().unwrap_or(
            reached
                || matches!(
                    status.as_str(),
                    "ttl_expired" | "unreachable" | "packet_too_big" | "parameter_problem"
                ),
        );
        sample["responded"] = json!(responded);
        sample["reached"] = json!(self.tool == "trace" && reached);
        if self.tool == "trace" {
            self.reached |= reached;
            self.hops = self.hops.max(sample["hop"].as_u64().unwrap_or(0));
        }
        if status == "timeout" {
            self.timeouts += 1;
        }
        if reached {
            self.received += 1;
        } else if self.tool != "trace" || status != "ttl_expired" {
            *self.failures.entry(status).or_default() += 1;
        }
        if (self.tool == "trace" && responded || reached)
            && let Some(rtt) = sample["rtt_ms"].as_f64()
        {
            self.total += rtt;
            self.minimum = self.minimum.min(rtt);
            self.maximum = self.maximum.max(rtt);
        }
        if self.samples.len() < 10000 {
            self.samples.push(sample.clone());
        }
    }
    fn snapshot(&self) -> serde_json::Value {
        let mut v = json!({"target":self.target,"sent":self.sent,"completed":self.completed,"interrupted_probes":self.interrupted,"counting_policy":"sent counts attempted calls; loss and RTT exclude interrupted calls","failure_counts":self.failures,"started_at_ms":self.started_at,"duration_ms":self.started.elapsed().as_secs_f64()*1000.0,"has_samples":self.completed>0,"samples_truncated":self.completed>10000,"sample_retention":"first_10000","statistics_scope":"entire_task"});
        if self.tool == "trace" {
            v["hops_probed"] = json!(self.hops);
            v["reached"] = json!(self.reached);
            v["timeout_probes"] = json!(self.timeouts);
            v["success"] = json!(self.reached);
            v["samples"] = json!(self.samples);
        } else {
            v["received"] = json!(self.received);
            v["lost"] = json!(self.completed - self.received);
            v["loss_percent"] = if self.completed > 0 {
                json!((self.completed - self.received) as f64 * 100.0 / self.completed as f64)
            } else {
                serde_json::Value::Null
            };
            v["success"] = json!(self.received > 0);
            v["rtt_min_ms"] = if self.received > 0 {
                json!(self.minimum)
            } else {
                serde_json::Value::Null
            };
            v["rtt_avg_ms"] = if self.received > 0 {
                json!(self.total / self.received as f64)
            } else {
                serde_json::Value::Null
            };
            v["rtt_max_ms"] = if self.received > 0 {
                json!(self.maximum)
            } else {
                serde_json::Value::Null
            };
        }
        v
    }
}

pub(crate) fn ntp_stamp(t: SystemTime) -> u64 {
    let d = t.duration_since(UNIX_EPOCH).unwrap_or_default();
    ((d.as_secs() + 2_208_988_800) << 32) | (((d.subsec_nanos() as u64) << 32) / 1_000_000_000)
}
fn seconds(v: u64) -> f64 {
    (v >> 32) as f64 + (v as u32) as f64 / 4294967296.0
}
pub fn ntp(r: &Request, c: &Cancel, emit: &Sink<'_>) -> ToolResult {
    if r.count > 20 {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "NTP supports 1..20 samples per task",
        ));
    }
    let addr = resolve(&r.target, if r.port == 0 { 123 } else { r.port }, r.ipv6, c)?;
    let socket = bind_udp(any_addr(addr.is_ipv6()))?;
    socket.connect(addr)?;
    let mut samples = Vec::new();
    for seq in 1..=r.count.min(20) {
        cancelled(c)?;
        let started = Instant::now();
        let wall = SystemTime::now();
        let t1 = ntp_stamp(wall);
        let mut packet = [0_u8; 48];
        packet[0] = 0x23;
        packet[40..48].copy_from_slice(&t1.to_be_bytes());
        socket.send(&packet)?;
        let end = Instant::now() + Duration::from_millis(r.timeout_ms);
        let mut buf = [0_u8; 512];
        let mut sample = json!({"seq":seq,"ok":false,"status":"timeout"});
        loop {
            cancelled(c)?;
            match socket.recv(&mut buf) {
                Ok(n) if n >= 48 => {
                    let t4 = ntp_stamp(SystemTime::now());
                    let origin = u64::from_be_bytes(buf[24..32].try_into().unwrap());
                    if origin != t1 {
                        continue;
                    }
                    let mode = buf[0] & 7;
                    let version = (buf[0] >> 3) & 7;
                    let stratum = buf[1];
                    if stratum == 0 {
                        return Err(ToolError::new(
                            "NTP_KOD",
                            format!("Server refused: {}", String::from_utf8_lossy(&buf[12..16])),
                        ));
                    }
                    let t2 = u64::from_be_bytes(buf[32..40].try_into().unwrap());
                    let t3 = u64::from_be_bytes(buf[40..48].try_into().unwrap());
                    let delay = (seconds(t4) - seconds(t1) - seconds(t3) + seconds(t2)) * 1000.0;
                    let offset = (seconds(t2) - seconds(t1) + seconds(t3) - seconds(t4)) * 500.0;
                    let jump = (SystemTime::now()
                        .duration_since(wall)
                        .unwrap_or_default()
                        .as_secs_f64()
                        - started.elapsed().as_secs_f64())
                    .abs()
                        > 0.2;
                    let valid = mode == 4
                        && (3..=4).contains(&version)
                        && buf[0] >> 6 != 3
                        && (1..=15).contains(&stratum)
                        && t2 != 0
                        && t3 != 0
                        && delay >= 0.0
                        && !jump;
                    sample = json!({"seq":seq,"ok":valid,"status":if valid{"reply"}else{"invalid_or_unsynchronized"},"stratum":stratum,"offset_ms":offset,"delay_ms":delay,"server":addr.to_string()});
                    break;
                }
                Ok(_) => {}
                Err(e) if is_timeout(&e) => {}
                Err(e) => return Err(e.into()),
            }
            if Instant::now() >= end {
                break;
            }
        }
        sample["event_type"] = json!("sample");
        emit(sample.clone());
        samples.push(sample);
        if seq < r.count.min(20) {
            wait(c, Duration::from_millis(r.interval_ms.max(100)))?;
        }
    }
    let valid: Vec<_> = samples.iter().filter(|s| s["ok"] == true).collect();
    let best = valid.iter().min_by(|a, b| {
        a["delay_ms"]
            .as_f64()
            .unwrap_or(f64::INFINITY)
            .total_cmp(&b["delay_ms"].as_f64().unwrap_or(f64::INFINITY))
    });
    Ok(
        json!({"success":!valid.is_empty(),"valid_samples":valid.len(),"best_sample":best,"samples":samples,"authenticated":false,"clock_changed":false}),
    )
}

#[cfg(test)]
mod trust_tests {
    use super::*;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };

    fn cancel() -> Cancel {
        Arc::new(AtomicBool::new(false))
    }
    fn fixture(seq: u64) -> serde_json::Value {
        if seq.is_multiple_of(5) {
            json!({"ok":false,"status":if seq.is_multiple_of(10){"refused"}else{"timeout"},"rtt_ms":null,"address":"127.0.0.1"})
        } else {
            json!({"ok":true,"status":"connected","rtt_ms":([1.0,30.0,100.0,500.0][((seq-1)%5)as usize]),"address":"127.0.0.1"})
        }
    }
    #[test]
    fn thousand_probes_survive_preview_eviction_and_final_json() {
        let request = Request {
            tool: "tcping".into(),
            port: 443,
            count: 1200,
            ..Request::default()
        };
        let c = cancel();
        let preview = Mutex::new(std::collections::VecDeque::new());
        let latest = Mutex::new(serde_json::Value::Null);
        let mut count = 0;
        let value = probe_with(
            &request,
            &c,
            &|event| {
                if event["event_type"] == "stats" {
                    *latest.lock().unwrap() = event.clone();
                }
                let mut queue = preview.lock().unwrap();
                queue.push_back(event);
                if queue.len() > 150 {
                    queue.pop_front();
                }
            },
            &mut |_, _, _, _| {
                count += 1;
                Ok(fixture(count))
            },
            &mut |_, _| Ok(()),
        )
        .unwrap();
        assert_eq!(count, 1200);
        for stats in [&value, &*latest.lock().unwrap()] {
            assert_eq!(stats["sent"], 1200);
            assert_eq!(stats["received"], 960);
            assert_eq!(stats["lost"], 240);
            assert_eq!(stats["failure_counts"]["timeout"], 120);
            assert_eq!(stats["failure_counts"]["refused"], 120);
            assert_eq!(stats["loss_percent"], 20.0);
            assert_eq!(stats["rtt_min_ms"], 1.0);
            assert_eq!(stats["rtt_avg_ms"], 157.75);
            assert_eq!(stats["rtt_max_ms"], 500.0);
        }
        assert_eq!(preview.lock().unwrap().len(), 150);
        assert_eq!(value["samples"].as_array().unwrap().len(), 1200);
        let final_json = envelope("fixture-tcp", "tcping", Ok(value));
        assert_eq!(final_json["status"], "succeeded");
        if let Ok(path) = std::env::var("HOPKIT_TRUST_FIXTURE") {
            std::fs::write(path, serde_json::to_vec_pretty(&final_json).unwrap()).unwrap();
        }
    }
    #[test]
    fn retention_is_bounded_without_losing_counts_or_event_semantics() {
        let mut stats = ProbeStats::new("127.0.0.1", "tcping");
        for seq in 1..=12500 {
            let mut sample = fixture(seq);
            stats.sent += 1;
            stats.record(&mut sample);
            assert!(sample["responded"].is_boolean());
        }
        assert_eq!(stats.samples.len(), 10000);
        assert_eq!(stats.snapshot()["sent"], 12500);
        assert_eq!(stats.snapshot()["received"], 10000);
        assert_eq!(stats.snapshot()["samples_truncated"], true);
    }
    #[test]
    fn three_hops_are_not_ping_loss_and_keep_multiple_responders() {
        let r = Request {
            tool: "trace".into(),
            max_hops: 3,
            ..Request::default()
        };
        let mut n = 0;
        let value=probe_with(&r,&cancel(),&|_|{},&mut |_,_,hop,_|{
            n+=1;
            Ok(json!({"ok":hop==3,"status":if hop==3{"reply"}else{"ttl_expired"},"rtt_ms":hop as f64*10.0,"address":format!("192.0.2.{}",n)}))
        },&mut |_,_|panic!("trace must not wait between hops")).unwrap();
        assert_eq!(n, 9);
        assert_eq!(value["hops_probed"], 3);
        assert_eq!(value["reached"], true);
        assert_eq!(value["timeout_probes"], 0);
        assert!(value.get("loss_percent").is_none());
        assert_eq!(value["failure_counts"], json!({}));
        let probes = value["samples"].as_array().unwrap();
        assert_eq!(probes[0]["ok"], false);
        assert_eq!(probes[0]["responded"], true);
        assert_eq!(probes[0]["reached"], false);
        assert_eq!(probes[8]["reached"], true);
        assert_ne!(probes[0]["address"], probes[1]["address"]);
    }
    #[test]
    fn trace_partial_timeouts_and_unreachable_do_not_imply_target_reached() {
        let r = Request {
            tool: "trace".into(),
            max_hops: 2,
            ..Request::default()
        };
        let mut n = 0;
        let value=probe_with(&r,&cancel(),&|_|{},&mut |_,_,_,_|{
            n+=1;let status=["ttl_expired","timeout","unreachable"][(n-1)%3];
            Ok(json!({"ok":false,"status":status,"rtt_ms":if status=="timeout"{None}else{Some(30)},"address":"192.0.2.1"}))
        },&mut |_,_|Ok(())).unwrap();
        assert_eq!(value["timeout_probes"], 2);
        assert_eq!(value["reached"], false);
        assert_eq!(
            value["failure_counts"],
            json!({"timeout":2,"unreachable":2})
        );
        assert_eq!(value["samples"][2]["responded"], true);
    }
    #[test]
    fn cancellation_before_first_probe_has_explicit_empty_partial_result() {
        let c = cancel();
        c.store(true, Ordering::Relaxed);
        let value = envelope("early", "ping", probe(&Request::default(), &c, &|_| {}));
        assert_eq!(value["status"], "cancelled");
        assert_eq!(value["error"]["code"], "CANCELLED");
        assert_eq!(value["data"]["sent"], 0);
        assert_eq!(value["data"]["has_samples"], false);
        assert!(value["data"]["loss_percent"].is_null());
        assert!(value["data"]["ended_at_ms"].is_number());
    }
    #[test]
    fn cancellation_in_wait_and_network_keep_prior_samples() {
        let r = Request {
            tool: "tcping".into(),
            port: 443,
            count: 10,
            ..Request::default()
        };
        for in_network in [false, true] {
            let c = cancel();
            let mut n = 0;
            let result = probe_with(
                &r,
                &c,
                &|_| {},
                &mut |_, _, _, c| {
                    n += 1;
                    if in_network && n == 2 {
                        c.store(true, Ordering::Relaxed);
                        cancelled(c)?;
                    }
                    Ok(fixture(1))
                },
                &mut |c, _| {
                    if !in_network {
                        c.store(true, Ordering::Relaxed);
                    }
                    cancelled(c)
                },
            );
            let value = envelope("partial", "tcping", result);
            assert_eq!(value["status"], "cancelled");
            assert_eq!(value["data"]["sent"], if in_network { 2 } else { 1 });
            assert_eq!(value["data"]["completed"], 1);
            assert_eq!(value["data"]["received"], 1);
            assert_eq!(value["data"]["samples"].as_array().unwrap().len(), 1);
            assert_eq!(value["data"]["interrupted_probes"], u64::from(in_network));
        }
    }
    #[test]
    fn synchronous_adapter_returning_after_stop_is_interrupted_not_timeout() {
        let c = cancel();
        let r = Request {
            count: 4,
            ..Request::default()
        };
        let value = envelope(
            "in-flight",
            "ping",
            probe_with(
                &r,
                &c,
                &|_| {},
                &mut |_, _, _, c| {
                    c.store(true, Ordering::Relaxed);
                    Ok(json!({"ok":false,"status":"timeout","rtt_ms":null}))
                },
                &mut |_, _| Ok(()),
            ),
        );
        assert_eq!(value["status"], "cancelled");
        assert_eq!(value["data"]["sent"], 1);
        assert_eq!(value["data"]["completed"], 0);
        assert_eq!(value["data"]["interrupted_probes"], 1);
        assert_eq!(value["data"]["lost"], 0);
        assert_eq!(value["data"]["failure_counts"], json!({}));
    }
    #[test]
    fn cancellation_during_last_sample_does_not_report_complete() {
        let c = cancel();
        let r = Request {
            count: 1,
            ..Request::default()
        };
        let value = envelope(
            "last",
            "ping",
            probe_with(
                &r,
                &c,
                &|event| {
                    if event["event_type"] == "sample" {
                        c.store(true, Ordering::Relaxed);
                    }
                },
                &mut |_, _, _, _| Ok(fixture(1)),
                &mut |_, _| Ok(()),
            ),
        );
        assert_eq!(value["status"], "cancelled");
        assert_eq!(value["data"]["sent"], 1);
    }
}
