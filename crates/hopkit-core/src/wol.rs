use crate::runtime::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    net::{Ipv4Addr, SocketAddr},
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub mac: String,
    pub destination: String,
    pub source: String,
    pub port: u16,
    pub count: u32,
    pub interval_ms: u64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            mac: String::new(),
            destination: "255.255.255.255".into(),
            source: String::new(),
            port: 9,
            count: 3,
            interval_ms: 200,
        }
    }
}
pub fn packet(mac: &str) -> Result<([u8; 102], String), ToolError> {
    let compact = mac.replace([':', '-', '.'], "");
    if compact.len() != 12 || !compact.is_ascii() {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "MAC requires six hexadecimal octets / MAC 须为六字节十六进制",
        ));
    }
    let mut bytes = [0u8; 6];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = u8::from_str_radix(&compact[i * 2..i * 2 + 2], 16)
            .map_err(|_| ToolError::new("INVALID_ARGUMENT", "Invalid MAC"))?;
    }
    if bytes[0] & 1 != 0 || bytes == [0; 6] {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "Target MAC must be nonzero unicast / 目标 MAC 须为非零单播地址",
        ));
    }
    let mut result = [0xff; 102];
    for part in result[6..].chunks_mut(6) {
        part.copy_from_slice(&bytes);
    }
    Ok((
        result,
        bytes
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(":"),
    ))
}
pub fn validate(config: &Config) -> Result<(), ToolError> {
    packet(&config.mac)?;
    let dest: Ipv4Addr = config
        .destination
        .parse()
        .map_err(|_| ToolError::new("INVALID_ARGUMENT", "Numeric IPv4 destination required"))?;
    if dest.is_unspecified()
        || dest.is_multicast()
        || config.port == 0
        || !(1..=100).contains(&config.count)
        || !(10..=60000).contains(&config.interval_ms)
    {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "WOL: port 1..65535, count 1..100, interval 10..60000 ms; IPv4 unicast/broadcast destination",
        ));
    }
    if !config.source.is_empty() && config.source.parse::<Ipv4Addr>().is_err() {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "WOL source must be IPv4",
        ));
    }
    Ok(())
}
pub fn run(config: &Config, c: &Cancel, emit: &Sink<'_>) -> ToolResult {
    validate(config)?;
    cancelled(c)?;
    let (packet, mac) = packet(&config.mac)?;
    let local: Ipv4Addr = if config.source.is_empty() {
        Ipv4Addr::UNSPECIFIED
    } else {
        config.source.parse().unwrap()
    };
    let socket = bind_udp(SocketAddr::from((local, 0)))
        .map_err(|e| ToolError::new("SOURCE_BIND_FAILED", e.message))?;
    socket.set_broadcast(true)?;
    let dest: SocketAddr =
        SocketAddr::from((config.destination.parse::<Ipv4Addr>().unwrap(), config.port));
    // Connecting UDP selects the actual source while retaining broadcast permission.
    socket.connect(dest)?;
    let actual_local_address = socket.local_addr()?.to_string();
    let began = Instant::now();
    let mut sent = 0;
    let mut attempts = 0;
    let mut errors = Vec::new();
    for index in 0..config.count {
        if cancelled(c).is_err() {
            break;
        }
        attempts += 1;
        match socket.send(&packet) {
            Ok(102) => sent += 1,
            Ok(_) => errors.push(ToolError::new("IO_ERROR", "Incomplete UDP send")),
            Err(e) => errors.push(e.into()),
        }
        emit(
            json!({"event_type":"stats","attempts":attempts,"sent":sent,"send_errors":errors.len()}),
        );
        if index + 1 < config.count && wait(c, Duration::from_millis(config.interval_ms)).is_err() {
            break;
        }
    }
    let data = json!({"schema_version":1,"mac":mac,"destination":dest.to_string(),"actual_local_address":actual_local_address,"attempts":attempts,"sent":sent,"send_errors":errors,"observed_at":chrono::Utc::now().to_rfc3339(),"duration_ms":began.elapsed().as_millis(),"diagnostic_outcome":"packet_send_only","limitations":["Sending does not prove delivery or device wake-up; source IP binding does not force an interface."]});
    if c.load(std::sync::atomic::Ordering::Relaxed) {
        let mut e = ToolError::new("CANCELLED", "WOL stopped");
        e.data = Some(data);
        Err(e)
    } else if sent != attempts {
        let mut e = ToolError::new("IO_ERROR", "Some WOL sends failed");
        e.data = Some(data);
        Err(e)
    } else {
        Ok(data)
    }
}
