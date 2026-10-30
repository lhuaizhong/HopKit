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
                json!({"ok":reply.Status==0,"status_code":reply.Status,"status":if reply.Status==0{"reply"}else if reply.Status==11013{"ttl_expired"}else{"icmp_error"},"rtt_ms":reply.RoundTripTime,"address":from.to_string()}),
            )
        } else {
            let reply = &*(buf.as_ptr() as *const ICMPV6_ECHO_REPLY_LH);
            let from = std::net::Ipv6Addr::from(reply.Address.sin6_addr);
            Ok(
                json!({"ok":reply.Status==0,"status_code":reply.Status,"status":if reply.Status==0{"reply"}else if reply.Status==11013{"ttl_expired"}else{"icmp_error"},"rtt_ms":start.elapsed().as_secs_f64()*1000.0,"address":from.to_string()}),
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
    if r.tool == "tcping" && r.port == 0 {
        return Err(ToolError::new("INVALID_ARGUMENT", "TCP port is required"));
    }
    let addr = resolve(&r.target, r.port, r.ipv6, c)?;
    let payload_size = if r.tool == "ping" {
        r.payload_size_bytes as usize
    } else {
        32
    };
    let mut target = json!({"event_type":"target","address":addr.ip().to_string(),"port":r.port});
    if r.tool == "ping" {
        target["payload_size_bytes"] = json!(payload_size);
    }
    emit(target);
    let mut sent = 0_u32;
    let mut received = 0_u32;
    let mut total = 0.0_f64;
    let mut minimum = f64::INFINITY;
    let mut maximum = 0.0_f64;
    let mut reached = false;
    let mut samples = Vec::new();
    let end = Instant::now() + Duration::from_secs(120);
    let limit = if r.tool == "trace" {
        r.max_hops as u32 * 3
    } else {
        r.count
    };
    while limit == 0 || sent < limit {
        cancelled(c)?;
        if r.tool == "trace" && Instant::now() > end {
            break;
        }
        let start = Instant::now();
        let hop = if r.tool == "trace" {
            (sent / 3 + 1) as u8
        } else {
            128
        };
        let mut sample = if r.tool == "tcping" {
            match TcpStream::connect_timeout(&addr, Duration::from_millis(r.timeout_ms)) {
                Ok(stream) => {
                    drop(stream);
                    json!({"ok":true,"status":"connected","address":addr.ip().to_string(),"rtt_ms":start.elapsed().as_secs_f64()*1000.0})
                }
                Err(e) => {
                    json!({"ok":false,"status":if e.kind()==std::io::ErrorKind::ConnectionRefused{"refused"}else if is_timeout(&e){"timeout"}else{"connect_error"},"detail":e.to_string(),"rtt_ms":null,"address":addr.ip().to_string()})
                }
            }
        } else {
            #[cfg(windows)]
            {
                icmp(addr.ip(), hop, r.timeout_ms as u32, payload_size)?
            }
            #[cfg(not(windows))]
            {
                icmp(addr, hop, r.timeout_ms as u32, payload_size, c)?
            }
        };
        sent += 1;
        sample["event_type"] = json!("sample");
        sample["seq"] = json!(sent);
        if r.tool == "ping" {
            sample["payload_size_bytes"] = json!(payload_size);
        }
        if r.tool == "trace" {
            sample["hop"] = json!(hop);
        }
        if sample["ok"] == true {
            received += 1;
            let v = sample["rtt_ms"].as_f64().unwrap_or(0.0);
            total += v;
            minimum = minimum.min(v);
            maximum = maximum.max(v);
            if r.tool == "trace" {
                reached = true;
            }
        }
        emit(sample.clone());
        if samples.len() < 10000 {
            samples.push(sample);
        }
        if r.tool == "trace" {
            if reached && sent.is_multiple_of(3) {
                break;
            }
        } else if limit == 0 || sent < limit {
            wait(
                c,
                Duration::from_millis(r.interval_ms.max(50)).saturating_sub(start.elapsed()),
            )?;
        }
    }
    let mut result = json!({"target":addr.ip().to_string(),"sent":sent,"received":received,"lost":sent-received,"loss_percent":(sent-received)as f64*100.0/sent.max(1)as f64,"rtt_min_ms":if received>0{Some(minimum)}else{None},"rtt_avg_ms":if received>0{Some(total/received as f64)}else{None},"rtt_max_ms":if received>0{Some(maximum)}else{None},"reached":reached,"success":if r.tool=="trace"{reached}else{received>0},"samples":samples,"samples_truncated":sent>10000});
    if r.tool == "ping" {
        result["payload_size_bytes"] = json!(payload_size);
    }
    Ok(result)
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
