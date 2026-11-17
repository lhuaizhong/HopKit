//! DHCPv4 inspection and bounded DISCOVER-only diagnostics. Never acquires a lease.
use crate::runtime::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    net::{Ipv4Addr, SocketAddr, UdpSocket},
    sync::{OnceLock, mpsc},
    time::{Duration, Instant},
};
mod platform;
#[cfg(test)]
mod tests;
mod wire;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub operation: String,
    /// Windows stable adapter GUID, name, or current index; macOS BSD name.
    pub interface: String,
    pub duration_secs: u64,
    pub attempts: u8,
    pub expected_servers: Vec<Ipv4Addr>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            operation: "inspect".into(),
            interface: String::new(),
            duration_secs: 5,
            attempts: 3,
            expected_servers: vec![],
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Address {
    pub address: String,
    pub mask: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Interface {
    pub id: String,
    pub name: String,
    pub index: u32,
    pub mac: Option<String>,
    pub dhcp_enabled: Option<bool>,
    pub server: Option<String>,
    pub lease_start: Option<String>,
    pub lease_expires: Option<String>,
    pub addresses: Vec<Address>,
    pub gateways: Vec<String>,
    pub dns: Vec<String>,
    pub search_domains: Vec<String>,
    pub config_source: String,
    pub field_sources: std::collections::BTreeMap<String, String>,
    pub limitations: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Offer {
    pub observed_at: String,
    pub server_id: Ipv4Addr,
    pub packet_source: String,
    pub yiaddr: Ipv4Addr,
    pub siaddr: Ipv4Addr,
    pub relay: Ipv4Addr,
    pub relay_agent_hex: Option<String>,
    pub lease_secs: Option<u32>,
    pub mask: Option<Ipv4Addr>,
    pub routers: Vec<Ipv4Addr>,
    pub dns: Vec<Ipv4Addr>,
    pub search_domains: Vec<String>,
    pub outside_expected: Option<bool>,
}
#[derive(Debug, Clone, Default, Serialize)]
pub struct Stats {
    pub sent: u32,
    pub received: u32,
    pub ignored: u32,
    pub malformed: u32,
    pub duplicates: u32,
    pub offers_dropped: u32,
}
#[derive(Debug, Clone, Serialize)]
pub struct ResultData {
    pub schema_version: u32,
    pub operation: String,
    pub config_source: String,
    pub started_at: String,
    pub ended_at: String,
    pub duration_ms: u64,
    pub interfaces: Vec<Interface>,
    pub offers: Vec<Offer>,
    pub server_ids: Vec<Ipv4Addr>,
    pub stats: Stats,
    pub transaction_id: Option<String>,
    pub client_mac: Option<String>,
    pub local_endpoint: Option<String>,
    pub remote_endpoint: Option<String>,
    pub diagnostic_outcome: String,
    pub success: bool,
    pub partial: bool,
    pub limitations: Vec<String>,
}
pub fn validate(c: &Config) -> Result<(), ToolError> {
    if !matches!(c.operation.as_str(), "inspect" | "discover")
        || c.interface.len() > 256
        || c.interface.chars().any(char::is_control)
        || (c.operation == "discover" && c.interface.is_empty())
        || !(1..=30).contains(&c.duration_secs)
        || !(1..=3).contains(&c.attempts)
        || c.expected_servers.len() > 32
        || c.expected_servers
            .iter()
            .any(|ip| ip.is_unspecified() || ip.is_broadcast() || ip.is_multicast())
    {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "DHCP: select an interface; duration 1–30s, attempts 1–3, up to 32 IPv4 servers / DHCP 参数无效",
        ));
    }
    Ok(())
}
// Native queries cannot always be interrupted. Two workers retain their permit until they return.
fn interfaces(c: &Cancel) -> Result<Vec<Interface>, ToolError> {
    static BUDGET: OnceLock<crate::budget::ConnectionBudget> = OnceLock::new();
    let permit = BUDGET
        .get_or_init(|| crate::budget::ConnectionBudget::new(2))
        .try_acquire()
        .ok_or_else(|| ToolError::new("RESOURCE_EXHAUSTED", "DHCP inspection workers busy"))?;
    let (tx, rx) = mpsc::sync_channel(1);
    let cancel = c.clone();
    std::thread::Builder::new()
        .name("hopkit-dhcp-inspect".into())
        .spawn(move || {
            let _permit = permit;
            let _ = tx.send(platform::inspect(&cancel));
        })
        .map_err(|e| ToolError::new("RESOURCE_EXHAUSTED", e.to_string()))?;
    let start = Instant::now();
    loop {
        cancelled(c)?;
        if start.elapsed() > Duration::from_secs(5) {
            return Err(ToolError::new(
                "TIMEOUT",
                "DHCP inspection exceeded 5s; bounded worker may finish later",
            ));
        }
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(r) => return r,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => return Err(ToolError::new("SYSTEM_QUERY_FAILED", "DHCP worker stopped")),
        }
    }
}
fn selected(rows: Vec<Interface>, name: &str) -> Result<Vec<Interface>, ToolError> {
    if name.is_empty() {
        return Ok(rows);
    }
    let rows: Vec<_> = rows
        .into_iter()
        .filter(|r| r.id == name || r.name == name || r.index.to_string() == name)
        .collect();
    if rows.len() != 1 {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "Interface not found or ambiguous; run dhcp inspect / 接口不存在或不唯一",
        ));
    }
    Ok(rows)
}
pub fn run(config: &Config, c: &Cancel, emit: &Sink<'_>) -> ToolResult {
    let sequence = std::sync::atomic::AtomicU64::new(0);
    let sequenced = |mut event: serde_json::Value| {
        event["event_seq"] = json!(sequence.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1);
        event["schema_version"] = json!(1);
        emit(event)
    };
    let emit: &Sink<'_> = &sequenced;
    validate(config)?;
    let start = Instant::now();
    let mut d=ResultData{schema_version:1,operation:config.operation.clone(),config_source:platform::ADAPTER.into(),started_at:chrono::Utc::now().to_rfc3339(),ended_at:String::new(),duration_ms:0,interfaces:vec![],offers:vec![],server_ids:vec![],stats:Stats::default(),transaction_id:None,client_mac:None,local_endpoint:None,remote_endpoint:None,diagnostic_outcome:"unknown".into(),success:false,partial:true,limitations:vec!["DHCPv4 only; unknown fields are not inferred. No DHCPREQUEST/RELEASE/DECLINE, address or lease changes. / 仅 DHCPv4；未知字段不推断，不修改地址或租约。".into()]};
    let result = (|| {
        cancelled(c)?;
        d.interfaces = selected(interfaces(c)?, &config.interface)?;
        if config.operation == "inspect" {
            d.diagnostic_outcome = "configuration_observed".into();
            d.success = true;
            d.partial = false;
            return Ok(());
        }
        d.limitations.push("DISCOVER may reserve an offered address temporarily. Multiple offers can be legitimate; outside_expected is only a user-list comparison. No offer does not prove no DHCP server. / DISCOVER 可能临时预留地址；多个 Offer 可合法；无 Offer 不证明无服务器。".into());
        d.limitations.push("UDP adapter uses the OS-selected source IPv4 on the pinned interface; 0.0.0.0:68 is the wildcard receive bind, not proof of an on-wire 0.0.0.0 source. Unconfigured-interface and physical DHCP device acceptance pending. / UDP 出站源地址由指定接口的系统栈选择；通配绑定不代表线上源 IP 为零。未配置接口和真实设备兼容待验收。".into());
        let iface = d.interfaces[0].clone();
        let mac = wire::mac(iface.mac.as_deref().unwrap_or(""))?;
        let id = uuid::Uuid::new_v4();
        let xid: [u8; 4] = id.as_bytes()[..4].try_into().unwrap();
        d.transaction_id = Some(xid.iter().map(|b| format!("{b:02x}")).collect());
        d.client_mac = iface.mac.clone();
        let socket = platform::socket(&iface)?;
        d.local_endpoint = Some(socket.local_addr()?.to_string());
        d.remote_endpoint = Some("255.255.255.255:67".into());
        collect(
            &socket,
            "255.255.255.255:67".parse().unwrap(),
            config,
            xid,
            mac,
            c,
            emit,
            start,
            &mut d,
        )
    })();
    d.duration_ms = start.elapsed().as_millis() as u64;
    d.ended_at = chrono::Utc::now().to_rfc3339();
    emit(json!({"event_type":"dhcp_snapshot","snapshot":d}));
    match result {
        Ok(()) => Ok(json!(d)),
        Err(mut e) => {
            e.data = Some(json!(d));
            Err(e)
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn collect(
    socket: &UdpSocket,
    destination: SocketAddr,
    config: &Config,
    xid: [u8; 4],
    mac: [u8; 6],
    c: &Cancel,
    emit: &Sink<'_>,
    start: Instant,
    d: &mut ResultData,
) -> Result<(), ToolError> {
    let began = Instant::now();
    let duration = Duration::from_secs(config.duration_secs);
    let mut next = Duration::ZERO;
    let mut buffer = [0u8; 4097];
    let mut seq = 0u64;
    while began.elapsed() < duration {
        cancelled(c)?;
        if d.stats.sent < u32::from(config.attempts) && began.elapsed() >= next {
            let packet = wire::discover(xid, mac, began.elapsed().as_secs().min(65535) as u16);
            socket.send_to(&packet, destination)?;
            d.stats.sent += 1;
            next = duration.mul_f64(d.stats.sent as f64 / config.attempts as f64);
            seq += 1;
            emit(
                json!({"event_type":"dhcp_sent","event_seq":seq,"observed_at":chrono::Utc::now().to_rfc3339(),"attempt":d.stats.sent,"message_type":"DHCPDISCOVER"}),
            );
        }
        match socket.recv_from(&mut buffer) {
            Ok((n, source)) => {
                d.stats.received += 1;
                if n > 4096 || source.port() != 67 {
                    d.stats.ignored += 1;
                    continue;
                }
                match wire::offer(&buffer[..n], source, xid, mac, &config.expected_servers) {
                    Ok(Some(offer)) => {
                        if !retain(d, offer) {
                            continue;
                        }
                        d.duration_ms = start.elapsed().as_millis() as u64;
                        emit(json!({"event_type":"dhcp_snapshot","snapshot":d}));
                    }
                    Ok(None) => d.stats.ignored += 1,
                    Err(()) => d.stats.malformed += 1,
                }
            }
            Err(e) if is_timeout(&e) => {}
            // Winsock consumes oversized UDP datagrams and reports WSAEMSGSIZE.
            // Remote malformed input must not terminate the collection window.
            Err(e) if e.raw_os_error() == Some(10040) => {
                d.stats.received += 1;
                d.stats.malformed += 1;
            }
            Err(e) => return Err(e.into()),
        }
    }
    d.partial = d.stats.offers_dropped > 0;
    d.success = !d.offers.is_empty() && !d.partial;
    d.diagnostic_outcome = if d.offers.is_empty() {
        "no_offer_observed"
    } else if d.offers.iter().any(|o| o.outside_expected == Some(true)) {
        "outside_expected_observed"
    } else {
        "offers_observed"
    }
    .into();
    Ok(())
}

fn retain(d: &mut ResultData, offer: Offer) -> bool {
    if d.offers.iter().any(|o| {
        o.server_id == offer.server_id
            && o.yiaddr == offer.yiaddr
            && o.packet_source == offer.packet_source
            && o.relay == offer.relay
    }) {
        d.stats.duplicates += 1;
        return false;
    }
    if d.offers.len() >= 256 {
        d.stats.offers_dropped += 1;
        return false;
    }
    if !d.server_ids.contains(&offer.server_id) {
        d.server_ids.push(offer.server_id)
    }
    d.offers.push(offer);
    true
}
