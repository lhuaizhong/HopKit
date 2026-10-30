use crate::runtime::*;
use serde_json::json;
use std::{
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream},
    time::{Duration, Instant},
};

fn u16_at(b: &[u8], p: usize) -> Result<u16, ToolError> {
    let s = b
        .get(p..p + 2)
        .ok_or_else(|| ToolError::new("PROTOCOL_ERROR", "Truncated DNS packet"))?;
    Ok(u16::from_be_bytes([s[0], s[1]]))
}
fn name(b: &[u8], position: &mut usize) -> Result<String, ToolError> {
    let mut p = *position;
    let mut labels = Vec::new();
    let mut jumped = false;
    let mut total = 0;
    for _ in 0..128 {
        let n = *b
            .get(p)
            .ok_or_else(|| ToolError::new("PROTOCOL_ERROR", "DNS name out of bounds"))?;
        if n & 0xc0 == 0xc0 {
            let offset = (((n as usize) & 0x3f) << 8)
                | *b.get(p + 1)
                    .ok_or_else(|| ToolError::new("PROTOCOL_ERROR", "DNS pointer truncated"))?
                    as usize;
            if !jumped {
                *position = p + 2;
                jumped = true;
            }
            p = offset;
            continue;
        }
        if n & 0xc0 != 0 || n > 63 {
            return Err(ToolError::new("PROTOCOL_ERROR", "Invalid DNS label"));
        }
        p += 1;
        if n == 0 {
            if !jumped {
                *position = p;
            }
            return Ok(if labels.is_empty() {
                ".".into()
            } else {
                labels.join(".")
            });
        }
        let label = b
            .get(p..p + n as usize)
            .ok_or_else(|| ToolError::new("PROTOCOL_ERROR", "DNS label truncated"))?;
        total += n as usize + 1;
        if total > 255 {
            return Err(ToolError::new("PROTOCOL_ERROR", "DNS name too long"));
        }
        labels.push(String::from_utf8_lossy(label).into_owned());
        p += n as usize;
    }
    Err(ToolError::new("PROTOCOL_ERROR", "DNS pointer cycle"))
}
fn type_id(s: &str) -> Result<u16, ToolError> {
    match s.to_uppercase().as_str() {
        "A" => Ok(1),
        "NS" => Ok(2),
        "CNAME" => Ok(5),
        "SOA" => Ok(6),
        "PTR" => Ok(12),
        "MX" => Ok(15),
        "TXT" => Ok(16),
        "AAAA" => Ok(28),
        _ => Err(ToolError::new(
            "INVALID_ARGUMENT",
            "A, AAAA, CNAME, MX, TXT, NS, PTR, SOA supported",
        )),
    }
}
pub fn parse(b: &[u8], id: u16, expected: &str, qtype: u16) -> ToolResult {
    if b.len() < 12 || u16_at(b, 0)? != id || b[2] & 0x80 == 0 {
        return Err(ToolError::new("PROTOCOL_ERROR", "Invalid DNS response"));
    }
    let mut p = 12;
    let qd = u16_at(b, 4)?;
    if qd != 1 {
        return Err(ToolError::new(
            "PROTOCOL_ERROR",
            "Unexpected question count",
        ));
    }
    let question = name(b, &mut p)?;
    let kind = u16_at(b, p)?;
    let class = u16_at(b, p + 2)?;
    p += 4;
    if !question
        .trim_end_matches('.')
        .eq_ignore_ascii_case(expected.trim_end_matches('.'))
        || kind != qtype
        || class != 1
    {
        return Err(ToolError::new("PROTOCOL_ERROR", "DNS question mismatch"));
    }
    let mut records = Vec::new();
    for (section, num) in [
        ("answer", u16_at(b, 6)?),
        ("authority", u16_at(b, 8)?),
        ("additional", u16_at(b, 10)?),
    ] {
        for _ in 0..num {
            let owner = name(b, &mut p)?;
            let typ = u16_at(b, p)?;
            let rrclass = u16_at(b, p + 2)?;
            let ttl = u32::from_be_bytes(
                b.get(p + 4..p + 8)
                    .ok_or_else(|| ToolError::new("PROTOCOL_ERROR", "Truncated RR"))?
                    .try_into()
                    .unwrap(),
            );
            let len = u16_at(b, p + 8)? as usize;
            p += 10;
            let end = p
                .checked_add(len)
                .filter(|e| *e <= b.len())
                .ok_or_else(|| ToolError::new("PROTOCOL_ERROR", "RDATA truncated"))?;
            let d = &b[p..end];
            let value = match typ {
                1 if len == 4 => Ipv4Addr::new(d[0], d[1], d[2], d[3]).to_string(),
                28 if len == 16 => Ipv6Addr::from(<[u8; 16]>::try_from(d).unwrap()).to_string(),
                2 | 5 | 12 => {
                    let mut x = p;
                    let result = name(b, &mut x)?;
                    if x > end {
                        return Err(ToolError::new("PROTOCOL_ERROR", "Name beyond RDATA"));
                    }
                    result
                }
                15 if len >= 3 => {
                    let mut x = p + 2;
                    let n = name(b, &mut x)?;
                    if x > end {
                        return Err(ToolError::new("PROTOCOL_ERROR", "MX beyond RDATA"));
                    }
                    format!("{} {}", u16_at(b, p)?, n)
                }
                6 => {
                    let mut x = p;
                    let mname = name(b, &mut x)?;
                    let rname = name(b, &mut x)?;
                    if x + 20 != end {
                        return Err(ToolError::new("PROTOCOL_ERROR", "Invalid SOA length"));
                    }
                    let mut numbers = Vec::new();
                    for _ in 0..5 {
                        numbers.push(u32::from_be_bytes(b[x..x + 4].try_into().unwrap()));
                        x += 4;
                    }
                    format!(
                        "{} {} {} {} {} {} {}",
                        mname, rname, numbers[0], numbers[1], numbers[2], numbers[3], numbers[4]
                    )
                }
                16 => {
                    let mut x = 0;
                    let mut parts = Vec::new();
                    while x < len {
                        let l = d[x] as usize;
                        x += 1;
                        let part = d
                            .get(x..x + l)
                            .ok_or_else(|| ToolError::new("PROTOCOL_ERROR", "TXT truncated"))?;
                        parts.push(String::from_utf8_lossy(part).into_owned());
                        x += l;
                    }
                    parts.join(" ")
                }
                _ => d.iter().map(|x| format!("{x:02x}")).collect::<String>(),
            };
            records.push(json!({"section":section,"name":owner,"type":typ,"class":rrclass,"ttl":ttl,"value":value}));
            p = end;
        }
    }
    Ok(
        json!({"source":"direct","rcode":b[3]&15,"truncated":b[2]&2!=0,"records":records,"success":b[3]&15==0}),
    )
}
pub fn query(r: &Request, c: &Cancel, emit: &Sink<'_>) -> ToolResult {
    if r.dns_server.is_empty() {
        if !r.record_type.is_empty() && r.record_type != "A" {
            return Err(ToolError::new(
                "INVALID_ARGUMENT",
                "RR queries require an explicit DNS server",
            ));
        }
        let addrs = resolve_all(&r.target, 0, r.ipv6, c)?;
        let mut ips = Vec::new();
        for addr in addrs {
            let ip = addr.ip().to_string();
            if !ips.contains(&ip) {
                ips.push(ip);
            }
        }
        let result =
            json!({"source":"system","addresses":ips,"ttl":null,"rcode":null,"success":true});
        emit(json!({"event_type":"result","data":result}));
        return Ok(result);
    }
    let server = r
        .dns_server
        .parse::<SocketAddr>()
        .or_else(|_| {
            r.dns_server
                .parse::<IpAddr>()
                .map(|ip| SocketAddr::new(ip, if r.port == 0 { 53 } else { r.port }))
        })
        .map_err(|_| ToolError::new("INVALID_ARGUMENT", "DNS server must be IP or IP:port"))?;
    let typ = type_id(&r.record_type)?;
    let id = (uuid::Uuid::new_v4().as_u128() & 65535) as u16;
    let mut packet = vec![0; 12];
    packet[0..2].copy_from_slice(&id.to_be_bytes());
    packet[2] = 1;
    packet[5] = 1;
    let host = r.target.trim_end_matches('.');
    if host.is_empty() || host.len() > 253 {
        return Err(ToolError::new("INVALID_ARGUMENT", "Invalid DNS name"));
    }
    for label in host.split('.') {
        if label.is_empty() || label.len() > 63 || !label.is_ascii() {
            return Err(ToolError::new(
                "INVALID_ARGUMENT",
                "Use ASCII/Punycode DNS labels of 1..63 bytes",
            ));
        }
        packet.push(label.len() as u8);
        packet.extend_from_slice(label.as_bytes());
    }
    packet.push(0);
    packet.extend_from_slice(&typ.to_be_bytes());
    packet.extend_from_slice(&1u16.to_be_bytes());
    let socket = bind_udp(any_addr(server.is_ipv6()))?;
    socket.connect(server)?;
    socket.send(&packet)?;
    let start = Instant::now();
    let mut buf = vec![0; 65535];
    let mut n = loop {
        cancelled(c)?;
        match socket.recv(&mut buf) {
            Ok(n) if n >= 2 && buf[0..2] == id.to_be_bytes() => break n,
            Ok(_) => {}
            Err(e) if is_timeout(&e) => {}
            Err(e) => return Err(e.into()),
        }
        if start.elapsed() >= Duration::from_millis(r.timeout_ms) {
            return Err(ToolError::new("TIMEOUT", "DNS timeout"));
        }
    };
    let mut transport = "udp";
    if n >= 12 && buf[2] & 2 != 0 {
        cancelled(c)?;
        let timeout = Duration::from_millis(r.timeout_ms);
        let mut stream = TcpStream::connect_timeout(&server, timeout)?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        stream.write_all(&(packet.len() as u16).to_be_bytes())?;
        stream.write_all(&packet)?;
        let mut len = [0; 2];
        stream.read_exact(&mut len)?;
        n = u16::from_be_bytes(len) as usize;
        stream.read_exact(&mut buf[..n])?;
        transport = "tcp";
    }
    let mut result = parse(&buf[..n], id, host, typ)?;
    result["server"] = json!(server.to_string());
    result["transport"] = json!(transport);
    result["elapsed_ms"] = json!(start.elapsed().as_millis());
    emit(json!({"event_type":"result","data":result}));
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pointer_cycle_is_rejected() {
        let b = [0xc0, 0];
        assert!(name(&b, &mut 0).is_err());
    }
    #[test]
    fn malformed_packet_never_panics() {
        for len in 0..32 {
            assert!(parse(&vec![0; len], 7, "test", 1).is_err());
        }
    }
}
