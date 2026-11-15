//! Optional bounded UDP PTR phase. No implicit public resolver, TCP fallback or vendor lookup.
use super::*;
pub fn lookup(
    address: &str,
    server: &str,
    timeout: u64,
    c: &Cancel,
) -> Result<Vec<String>, ToolError> {
    let ip = crate::target::literal_socket(address, 0).unwrap().ip();
    let question = match ip {
        IpAddr::V4(ip) => format!(
            "{}.in-addr.arpa",
            ip.octets()
                .iter()
                .rev()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(".")
        ),
        IpAddr::V6(ip) => format!(
            "{}.ip6.arpa",
            format!("{:032x}", u128::from(ip))
                .chars()
                .rev()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(".")
        ),
    };
    let server = crate::target::literal_socket(server, 53)
        .or_else(|| server.parse::<SocketAddr>().ok())
        .ok_or_else(|| bad("PTR server must be numeric IP or IP:port"))?;
    let id = uuid::Uuid::new_v4().as_u128() as u16;
    let mut packet = vec![0; 12];
    packet[..2].copy_from_slice(&id.to_be_bytes());
    packet[2] = 1;
    packet[5] = 1;
    for label in question.split('.') {
        packet.push(label.len() as u8);
        packet.extend_from_slice(label.as_bytes());
    }
    packet.extend_from_slice(&[0, 0, 12, 0, 1]);
    let socket = bind_udp(any_addr(server.is_ipv6()))?;
    socket.connect(server)?;
    cancelled(c)?;
    socket.send(&packet)?;
    let began = Instant::now();
    let mut buf = [0; 4096];
    loop {
        cancelled(c)?;
        if began.elapsed() >= Duration::from_millis(timeout) {
            return Err(ToolError::new("TIMEOUT", "PTR lookup timed out"));
        }
        match socket.recv(&mut buf) {
            Ok(n) if n >= 12 && buf[..2] == id.to_be_bytes() => {
                if buf[2] & 2 != 0 {
                    return Err(ToolError::new(
                        "RESOURCE_EXHAUSTED",
                        "PTR UDP answer truncated; no TCP fallback in discovery",
                    ));
                }
                let data = crate::dns::parse(&buf[..n], id, &question, 12)?;
                if data["rcode"] != 0 {
                    return Err(ToolError::new(
                        "DNS_RESPONSE",
                        format!("PTR DNS rcode={}", data["rcode"]),
                    ));
                }
                return Ok(data["records"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|r| r["type"] == 12 && r["section"] == "answer")
                    .filter_map(|r| r["value"].as_str())
                    .take(16)
                    .map(|s| s.chars().take(253).collect())
                    .collect());
            }
            Ok(_) => {}
            Err(e) if is_timeout(&e) => {}
            Err(e) => return Err(e.into()),
        }
    }
}
