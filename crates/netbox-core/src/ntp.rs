//! Temporary, unauthenticated NTP server using the host's system clock.
use crate::runtime::*;
use serde_json::{Value, json};
use std::{
    net::SocketAddr,
    time::{Duration, Instant, SystemTime},
};

pub fn run(r: &Request, c: &Cancel, emit: &Sink<'_>) -> ToolResult {
    match r.action.as_str() {
        "" | "query" => crate::diagnostics::ntp(r, c, emit),
        "serve" => serve(r, c, emit),
        _ => Err(ToolError::new(
            "INVALID_ARGUMENT",
            "NTP action must be query or serve / NTP 操作须为 query 或 serve",
        )),
    }
}

fn reply(request: &[u8], received: SystemTime, stratum: u8) -> Option<[u8; 48]> {
    // Only the basic, unauthenticated client/server exchange is supported.
    // Do not answer control/private modes, extensions, or authenticated packets.
    if request.len() != 48 || request[0] & 7 != 3 {
        return None;
    }
    let version = (request[0] >> 3) & 7;
    if !(3..=4).contains(&version) {
        return None;
    }
    let mut packet = [0_u8; 48];
    packet[0] = (version << 3) | 4;
    packet[1] = stratum;
    packet[2] = (request[2] as i8).clamp(4, 17) as u8;
    packet[3] = (-10_i8) as u8; // Conservative system-clock precision: about 1 ms.
    packet[8..12].copy_from_slice(&(1_u32 << 16).to_be_bytes()); // 1 s dispersion.
    // Identify a local reference, without claiming an external upstream server.
    packet[12..16].copy_from_slice(if stratum == 1 {
        b"LOCL"
    } else {
        &[127, 127, 1, 0]
    });
    let stamp = crate::diagnostics::ntp_stamp(received).to_be_bytes();
    packet[16..24].copy_from_slice(&stamp);
    packet[24..32].copy_from_slice(&request[40..48]);
    packet[32..40].copy_from_slice(&stamp);
    Some(packet)
}

#[derive(Default)]
struct Counters {
    received: u64,
    replied: u64,
    denied: u64,
    invalid: u64,
    send_errors: u64,
}
impl Counters {
    fn event(&self, event_type: &str) -> Value {
        json!({"event_type":event_type,"received":self.received,"replied":self.replied,
            "denied":self.denied,"invalid":self.invalid,"send_errors":self.send_errors})
    }
}

fn serve(r: &Request, c: &Cancel, emit: &Sink<'_>) -> ToolResult {
    if !(1..=15).contains(&r.ntp_stratum) {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "NTP server stratum must be 1..15 / NTP 服务层级须为 1–15",
        ));
    }
    let nets = crate::syslog::sources(r)?;
    let socket = bind_udp(SocketAddr::new(
        r.listen.parse().expect("validated listen IP"),
        if r.port == 0 { 123 } else { r.port },
    ))?;
    let listen = socket.local_addr()?.to_string();
    emit(
        json!({"event_type":"ready","listen":listen,"transport":"udp",
        "stratum":r.ntp_stratum,"clock_source":"system","authenticated":false,
        "clock_changed":false}),
    );
    let began = Instant::now();
    let mut last_stat = Instant::now();
    let mut preview = Instant::now();
    let mut counters = Counters::default();
    let mut buf = [0_u8; 65535];
    loop {
        let expired =
            r.duration_secs > 0 && began.elapsed() >= Duration::from_secs(r.duration_secs);
        if cancelled(c).is_err() || expired {
            emit(counters.event("stopped"));
            if expired {
                let mut result = counters.event("stopped");
                result.as_object_mut().unwrap().remove("event_type");
                result["success"] = json!(true);
                result["listen"] = json!(listen);
                result["stratum"] = json!(r.ntp_stratum);
                result["clock_source"] = json!("system");
                result["authenticated"] = json!(false);
                result["clock_changed"] = json!(false);
                return Ok(result);
            }
            return Err(ToolError::new(
                "CANCELLED",
                "NTP server stopped / NTP 服务已停止",
            ));
        }
        match socket.recv_from(&mut buf) {
            Ok((n, peer)) => {
                let received = SystemTime::now();
                counters.received += 1;
                if !crate::syslog::allowed(peer.ip(), &nets) {
                    counters.denied += 1;
                } else if let Some(mut packet) = reply(&buf[..n], received, r.ntp_stratum) {
                    // Stamp immediately before sending, after validation and serialization.
                    packet[40..48].copy_from_slice(
                        &crate::diagnostics::ntp_stamp(SystemTime::now()).to_be_bytes(),
                    );
                    match socket.send_to(&packet, peer) {
                        Ok(_) => {
                            counters.replied += 1;
                            if counters.replied <= 10
                                || preview.elapsed() >= Duration::from_millis(50)
                            {
                                emit(json!({"event_type":"request","seq":counters.replied,
                                    "ok":true,"status":"replied","source":peer.to_string(),
                                    "version":(packet[0] >> 3) & 7,"stratum":r.ntp_stratum,
                                    "received_at":chrono::DateTime::<chrono::Utc>::from(received).to_rfc3339()}));
                                preview = Instant::now();
                            }
                        }
                        // A failed reply to one client must not stop other clients.
                        Err(_) => counters.send_errors += 1,
                    }
                } else {
                    counters.invalid += 1;
                }
            }
            Err(e) if is_timeout(&e) => {}
            Err(e) => return Err(e.into()),
        }
        if last_stat.elapsed() >= Duration::from_secs(1) {
            emit(counters.event("stats"));
            last_stat = Instant::now();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::UNIX_EPOCH;

    #[test]
    fn server_reply_preserves_version_and_origin_and_encodes_local_clock() {
        let now = UNIX_EPOCH + Duration::new(1_700_000_000, 500_000_000);
        for version in [3, 4] {
            let mut request = [0; 48];
            request[0] = (version << 3) | 3;
            request[2] = 6;
            request[40..48].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
            let response = reply(&request, now, 10).unwrap();
            assert_eq!(response[0], (version << 3) | 4);
            assert_eq!(response[1], 10);
            assert_eq!(response[2], 6);
            assert_eq!(&response[12..16], &[127, 127, 1, 0]);
            assert_eq!(&response[24..32], &request[40..48]);
            let expected = ((1_700_000_000_u64 + 2_208_988_800) << 32) | 0x8000_0000;
            assert_eq!(&response[16..24], &expected.to_be_bytes());
            assert_eq!(&response[32..40], &expected.to_be_bytes());
        }
    }

    #[test]
    fn server_ignores_unsupported_packets() {
        for (size, header) in [
            (0, 0),
            (47, 0x23),
            (49, 0x23),
            (68, 0x23),
            (48, 0x13),
            (48, 0x2b),
            (48, 0x24),
            (48, 0x26),
            (48, 0x27),
        ] {
            let mut packet = vec![0; size];
            if size > 0 {
                packet[0] = header;
            }
            assert!(reply(&packet, SystemTime::now(), 10).is_none());
        }
    }
}
