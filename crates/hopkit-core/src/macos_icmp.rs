//! Darwin's unprivileged ICMP datagram sockets. No shell, raw socket or helper.
use crate::runtime::*;
use serde_json::{Value, json};
use socket2::{Domain, Protocol, Socket, Type};
use std::{
    net::{IpAddr, SocketAddr, UdpSocket},
    time::{Duration, Instant},
};

fn checksum(bytes: &[u8]) -> u16 {
    let mut sum = bytes
        .chunks(2)
        .map(|p| u16::from_be_bytes([p[0], *p.get(1).unwrap_or(&0)]) as u32)
        .sum::<u32>();
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

// Validate each length before indexing. Darwin supplies an IPv4 header (whose
// ip_len is host endian), but strips the IPv6 header. Do not trust ip_len here.
fn v4_body(packet: &[u8]) -> Option<&[u8]> {
    let header = (*packet.first()? & 15) as usize * 4;
    if packet[0] >> 4 != 4 || header < 20 || packet.get(9) != Some(&1) {
        return None;
    }
    packet.get(header..)
}

#[derive(Debug, PartialEq)]
struct Reply {
    status: &'static str,
    code: u16,
    reported_mtu: Option<u32>,
}

fn parse(packet: &[u8], target: IpAddr, from: IpAddr, request: &[u8]) -> Option<Reply> {
    let v6 = target.is_ipv6();
    let body = if v6 { packet } else { v4_body(packet)? };
    if body.len() < 8 {
        return None;
    }
    if body[0] == if v6 { 129 } else { 0 } {
        // Sockets can receive other tasks' ICMP and our own IPv6 echo request.
        // Match the entire random payload as well as id/sequence and source.
        return (body[1] == 0 && from == target && body[4..] == request[4..]).then_some(Reply {
            status: "reply",
            code: 0,
            reported_mtu: None,
        });
    }
    let status = match (v6, body[0]) {
        (false, 11) | (true, 3) => "ttl_expired",
        (false, 3) if body[1] == 4 => "packet_too_big",
        (false, 3) | (true, 1) => "unreachable",
        (true, 2) => "packet_too_big",
        (false, 12) | (true, 4) => "parameter_problem",
        _ => return None,
    };
    let quoted = &body[8..];
    let echo = if let IpAddr::V4(ip) = target {
        if quoted.get(16..20)? != ip.octets() {
            return None;
        }
        v4_body(quoted)?
    } else if let IpAddr::V6(ip) = target {
        // Echo probes have no extension headers; reject unrelated quotations.
        if quoted.first()? >> 4 != 6
            || quoted.get(6) != Some(&58)
            || quoted.get(24..40)? != ip.octets()
        {
            return None;
        }
        quoted.get(40..)?
    } else {
        return None;
    };
    if echo.len() < 8
        || echo.len() > request.len()
        || echo[..2] != request[..2]
        || echo[4..] != request[4..echo.len()]
    {
        return None;
    }
    Some(Reply {
        status,
        code: u16::from_be_bytes([body[0], body[1]]),
        reported_mtu: if status == "packet_too_big" {
            Some(if v6 {
                u32::from_be_bytes(body[4..8].try_into().ok()?)
            } else {
                u16::from_be_bytes([body[6], body[7]]) as u32
            })
            .filter(|n| *n > 0)
        } else {
            None
        },
    })
}

fn echo_request(v6: bool, payload_size: usize) -> Vec<u8> {
    let nonce = uuid::Uuid::new_v4();
    let mut request = vec![0x6e; 8 + payload_size];
    request[..4].copy_from_slice(&[if v6 { 128 } else { 8 }, 0, 0, 0]);
    request[4..8].copy_from_slice(&nonce.as_bytes()[..4]);
    let random_len = payload_size.min(16);
    request[8..8 + random_len].copy_from_slice(&nonce.as_bytes()[..random_len]);
    if !v6 {
        let sum = checksum(&request);
        request[2..4].copy_from_slice(&sum.to_be_bytes());
    }
    request
}

pub(crate) fn icmp(
    addr: SocketAddr,
    ttl: u8,
    timeout: u32,
    payload_size: usize,
    cancel: &Cancel,
) -> Result<Value, ToolError> {
    icmp_bound(addr, ttl, timeout, payload_size, cancel, "")
}
pub(crate) fn icmp_bound(
    addr: SocketAddr,
    ttl: u8,
    timeout: u32,
    payload_size: usize,
    cancel: &Cancel,
    source: &str,
) -> Result<Value, ToolError> {
    icmp_options(addr, ttl, timeout, payload_size, cancel, source, false)
}
pub(crate) fn pmtu_echo(
    addr: SocketAddr,
    timeout: u32,
    payload: usize,
    c: &Cancel,
    source: &str,
) -> ToolResult {
    icmp_options(addr, 128, timeout, payload, c, source, true)
}
fn icmp_options(
    addr: SocketAddr,
    ttl: u8,
    timeout: u32,
    payload_size: usize,
    cancel: &Cancel,
    source: &str,
    dont_fragment: bool,
) -> Result<Value, ToolError> {
    let v6 = addr.is_ipv6();
    let socket = Socket::new(
        Domain::for_address(addr),
        Type::DGRAM,
        Some(if v6 {
            Protocol::ICMPV6
        } else {
            Protocol::ICMPV4
        }),
    )?;
    // Darwin's default datagram buffers can be smaller than a large probe.
    socket.set_send_buffer_size((payload_size + 128).max(8192))?;
    socket.set_recv_buffer_size(((payload_size + 128) * 2).max(8192))?;
    #[cfg(target_os = "macos")]
    if dont_fragment {
        use std::os::fd::AsRawFd;
        let (level, option, value): (i32, i32, i32) = if v6 {
            (libc::IPPROTO_IPV6, libc::IPV6_DONTFRAG, 1)
        } else {
            (libc::IPPROTO_IP, libc::IP_DONTFRAG, 1)
        };
        if unsafe {
            libc::setsockopt(
                socket.as_raw_fd(),
                level,
                option,
                (&value as *const i32).cast(),
                std::mem::size_of_val(&value) as _,
            )
        } < 0
        {
            return Err(ToolError::new(
                "UNSUPPORTED_CAPABILITY",
                format!(
                    "Cannot enforce no fragmentation: {}",
                    std::io::Error::last_os_error()
                ),
            ));
        }
    }
    #[cfg(not(target_os = "macos"))]
    if dont_fragment {
        return Err(ToolError::new(
            "UNSUPPORTED_CAPABILITY",
            "Darwin adapter unavailable",
        ));
    }
    if v6 {
        socket.set_unicast_hops_v6(ttl as u32)?;
    } else {
        socket.set_ttl_v4(ttl as u32)?;
    }
    if !source.is_empty() {
        let local = crate::target::literal_socket(source, 0)
            .ok_or_else(|| ToolError::new("INVALID_ARGUMENT", "Invalid source"))?;
        socket
            .bind(&local.into())
            .map_err(|e| ToolError::new("SOURCE_BIND_FAILED", e.to_string()))?;
    }
    let socket: UdpSocket = socket.into();
    // Short reads make cancellation independent of the configured probe timeout.
    socket.set_read_timeout(Some(Duration::from_millis(50)))?;
    let request = echo_request(v6, payload_size);
    // ICMPv6's pseudo-header checksum is supplied by the Darwin kernel.
    let start = Instant::now();
    cancelled(cancel)?;
    let destination = SocketAddr::new(addr.ip(), 0);
    // Keep the scope id for IPv6 link-local destinations.
    let destination = match (destination, addr) {
        (SocketAddr::V6(mut d), SocketAddr::V6(a)) => {
            d.set_scope_id(a.scope_id());
            SocketAddr::V6(d)
        }
        _ => destination,
    };
    if let Err(e) = socket.send_to(&request, destination) {
        #[cfg(unix)]
        if dont_fragment && e.raw_os_error() == Some(libc::EMSGSIZE) {
            return Ok(
                json!({"status":"local_too_big","status_code":e.raw_os_error(),"address":null,"reported_mtu":null}),
            );
        }
        return Err(e.into());
    }
    let budget = Duration::from_millis(timeout as u64);
    // Include room for the longest IPv4 header, or a quoted ICMP error.
    let mut buf = vec![0_u8; (request.len() + 60).max(2048)];
    loop {
        cancelled(cancel)?;
        if start.elapsed() >= budget {
            return Ok(
                json!({"ok":false,"status":"timeout","status_code":null,"rtt_ms":null,"address":addr.ip().to_string()}),
            );
        }
        match socket.recv_from(&mut buf) {
            Ok((n, peer)) => {
                if let Some(reply) = parse(&buf[..n], addr.ip(), peer.ip(), &request) {
                    return Ok(
                        json!({"ok":reply.status=="reply","status":reply.status,"status_code":reply.code,"reported_mtu":reply.reported_mtu,"rtt_ms":start.elapsed().as_secs_f64()*1000.0,"address":peer.ip().to_string()}),
                    );
                }
            }
            Err(e) if is_timeout(&e) || e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_payload_sizes_and_complete_reply_matching() {
        for size in [0, 1, 15, 16, 32, 1472, 4096, 65500] {
            for v6 in [false, true] {
                let request = echo_request(v6, size);
                assert_eq!(request.len(), size + 8);
                if !v6 {
                    assert_eq!(checksum(&request), 0);
                }
                let target = if v6 { "::1" } else { "192.0.2.1" }.parse().unwrap();
                let mut reply = request.clone();
                reply[0] = if v6 { 129 } else { 0 };
                let packet = if v6 { reply.clone() } else { ip4(&reply) };
                assert_eq!(
                    parse(&packet, target, target, &request).unwrap().status,
                    "reply"
                );
                // A truncated echo must not count as a response to a large probe.
                assert!(parse(&packet[..packet.len() - 1], target, target, &request).is_none());
                if size > 0 {
                    *reply.last_mut().unwrap() ^= 1;
                    let packet = if v6 { reply } else { ip4(&reply) };
                    assert!(parse(&packet, target, target, &request).is_none());
                }
            }
        }
    }
    fn ip4(body: &[u8]) -> Vec<u8> {
        let mut p = vec![0; 20];
        p[0] = 0x45;
        p[9] = 1;
        p[16..20].copy_from_slice(&[192, 0, 2, 1]);
        p.extend_from_slice(body);
        p
    }
    #[test]
    fn correlates_echo_and_quoted_ttl_errors() {
        let target = "192.0.2.1".parse().unwrap();
        let request = [8, 0, 0, 0, 1, 2, 3, 4, 5, 6];
        let mut echo = request;
        echo[0] = 0;
        assert_eq!(
            parse(&ip4(&echo), target, target, &request).unwrap().status,
            "reply"
        );
        echo[8] = 9;
        assert!(parse(&ip4(&echo), target, target, &request).is_none());
        let mut ttl = vec![11, 0, 0, 0, 0, 0, 0, 0];
        ttl.extend(ip4(&request[..8]));
        assert_eq!(
            parse(&ip4(&ttl), target, "192.0.2.254".parse().unwrap(), &request)
                .unwrap()
                .status,
            "ttl_expired"
        );
        *ttl.last_mut().unwrap() ^= 1;
        assert!(parse(&ip4(&ttl), target, target, &request).is_none());
    }
    #[test]
    fn ipv6_ignores_own_request_and_other_tasks() {
        let target = "::1".parse().unwrap();
        let request = [128, 0, 0, 0, 1, 2, 3, 4, 5, 6];
        assert!(parse(&request, target, target, &request).is_none());
        let mut reply = request;
        reply[0] = 129;
        assert_eq!(
            parse(&reply, target, target, &request).unwrap().status,
            "reply"
        );
        reply[5] = 9;
        assert!(parse(&reply, target, target, &request).is_none());
    }
    #[test]
    fn malformed_packets_are_ignored() {
        let request = [8, 0, 0, 0, 1, 2, 3, 4];
        for n in 0..160 {
            for byte in [0, 0x45, 0x6f, 0xff] {
                let _ = parse(
                    &vec![byte; n],
                    "127.0.0.1".parse().unwrap(),
                    "127.0.0.1".parse().unwrap(),
                    &request,
                );
                let _ = parse(
                    &vec![byte; n],
                    "::1".parse().unwrap(),
                    "::1".parse().unwrap(),
                    &request,
                );
            }
        }
        assert_eq!(checksum(&[0, 1, 0xf2, 3, 0xf4, 0xf5, 0xf6, 0xf7]), 0x220d);
    }
    #[test]
    fn pmtu_errors_preserve_mtu_and_require_matching_quoted_probe() {
        let target = "192.0.2.1".parse().unwrap();
        let request = echo_request(false, 1472);
        let mut body = vec![3, 4, 0, 0, 0, 0, 5, 120]; // 1400
        body.extend(ip4(&request[..8]));
        let reply = parse(
            &ip4(&body),
            target,
            "192.0.2.254".parse().unwrap(),
            &request,
        )
        .unwrap();
        assert_eq!(reply.status, "packet_too_big");
        assert_eq!(reply.reported_mtu, Some(1400));
        *body.last_mut().unwrap() ^= 1;
        assert!(parse(&ip4(&body), target, target, &request).is_none());
        let target: IpAddr = "2001:db8::1".parse().unwrap();
        let request = echo_request(true, 1452);
        let mut body = vec![2, 0, 0, 0, 0, 0, 5, 120];
        let mut quoted = vec![0; 40];
        quoted[0] = 0x60;
        quoted[6] = 58;
        if let IpAddr::V6(a) = target {
            quoted[24..40].copy_from_slice(&a.octets());
        }
        body.extend(quoted);
        body.extend(&request[..8]);
        let reply = parse(&body, target, "2001:db8::ff".parse().unwrap(), &request).unwrap();
        assert_eq!(reply.status, "packet_too_big");
        assert_eq!(reply.reported_mtu, Some(1400));
        body[32] ^= 1;
        assert!(parse(&body, target, target, &request).is_none());
    }
    #[test]
    fn quoted_errors_match_family_target_id_sequence_and_available_payload() {
        for v6 in [false, true] {
            let target = if v6 { "2001:db8::1" } else { "192.0.2.1" }
                .parse()
                .unwrap();
            let request = echo_request(v6, 32);
            let quoted = |echo: &[u8]| {
                if v6 {
                    let mut p = vec![0; 40];
                    p[0] = 0x60;
                    p[6] = 58;
                    if let IpAddr::V6(a) = target {
                        p[24..40].copy_from_slice(&a.octets());
                    }
                    p.extend(echo);
                    p
                } else {
                    ip4(echo)
                }
            };
            let error = |echo: &[u8]| {
                let mut body = vec![if v6 { 3 } else { 11 }, 0, 0, 0, 0, 0, 0, 0];
                body.extend(quoted(echo));
                if v6 { body } else { ip4(&body) }
            };
            assert!(parse(&error(&request[..8]), target, target, &request).is_some());
            assert!(parse(&error(&request), target, target, &request).is_some());
            for i in [4, 6, 8, 20] {
                let mut other = request.clone();
                other[i] ^= 1;
                assert!(parse(&error(&other), target, target, &request).is_none());
            }
            assert!(
                parse(
                    &error(&request),
                    if v6 { "2001:db8::2" } else { "192.0.2.2" }
                        .parse()
                        .unwrap(),
                    target,
                    &request
                )
                .is_none()
            );
        }
    }
}
