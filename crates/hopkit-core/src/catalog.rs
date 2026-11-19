//! Stable protocol identities. Presentation categories are intentionally not an execution policy.
use crate::{Request, ToolError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolId {
    Snmp,
    Mtr,
    Pmtu,
    Dhcp,
    Socket,
    Connections,
    Wol,
    Scan,
    Discovery,
    Subnet,
    Check,
    Monitor,
    Network,
    Target,
    Ping,
    Trace,
    Tcping,
    Dns,
    Ntp,
    Iperf3,
    Ftp,
    Http,
    Wormhole,
    Tftp,
    Syslog,
    Doctor,
}
impl ToolId {
    pub fn parse(value: &str) -> Result<Self, ToolError> {
        serde_json::from_value(serde_json::Value::String(value.into()))
            .map_err(|_| ToolError::new("INVALID_ARGUMENT", "Unknown tool / 未知工具"))
    }
}
pub const IMPLEMENTED_TOOLS: &[&str] = &[
    "snmp",
    "mtr",
    "pmtu",
    "dhcp",
    "socket",
    "connections",
    "wol",
    "discovery",
    "scan",
    "subnet",
    "check",
    "monitor",
    "network",
    "target",
    "ping",
    "trace",
    "tcping",
    "dns",
    "ntp",
    "iperf3",
    "ftp",
    "http",
    "wormhole",
    "tftp",
    "syslog",
];
#[derive(Serialize)]
pub struct Capabilities {
    pub schema_version: u32,
    pub tools: &'static [&'static str],
    pub network_overview: bool,
    pub icmp: bool,
    pub source_binding: &'static [&'static str],
    /// Build-time adapter information; live permission failures remain structured runtime errors.
    pub adapters: Vec<Capability>,
    pub probe_budget: crate::budget::Limits,
}
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityStatus {
    Supported,
    Partial,
    Unsupported,
}
#[derive(Serialize)]
pub struct Capability {
    pub id: &'static str,
    pub status: CapabilityStatus,
    pub adapter: &'static str,
    pub limitation: &'static str,
}
pub fn capabilities() -> Capabilities {
    let icmp_adapter = if cfg!(windows) {
        "windows-iphelper"
    } else if cfg!(target_os = "macos") {
        "darwin-datagram"
    } else if cfg!(target_os = "linux") {
        "linux-datagram"
    } else {
        "unavailable"
    };
    let overview_adapter = if cfg!(windows) {
        "windows-powershell-nettcpip"
    } else if cfg!(target_os = "macos") {
        "macos-system-network-queries"
    } else {
        "unavailable"
    };
    Capabilities {
        schema_version: 1,
        tools: IMPLEMENTED_TOOLS,
        network_overview: cfg!(any(windows, target_os = "macos")),
        icmp: cfg!(any(windows, target_os = "macos", target_os = "linux")),
        source_binding: &[
            "mtr.icmp",
            "pmtu.icmp",
            "socket.tcp",
            "socket.udp",
            "wol.ipv4",
            "check.tcp",
            "check.http",
            "scan.tcp",
            "discovery.tcp",
            "discovery.icmp",
        ],
        adapters: vec![
            Capability {
                id: "dhcp_inspect",
                status: if cfg!(any(windows, target_os = "macos")) {
                    CapabilityStatus::Partial
                } else {
                    CapabilityStatus::Unsupported
                },
                adapter: if cfg!(windows) {
                    "windows-iphelper"
                } else if cfg!(target_os = "macos") {
                    "macos-systemconfiguration"
                } else {
                    "unavailable"
                },
                limitation: "Read-only current configuration; missing lease fields unknown. Windows IPv4 only; macOS native acceptance pending.",
            },
            Capability {
                id: "dhcp_discover",
                status: if cfg!(any(windows, target_os = "macos")) {
                    CapabilityStatus::Partial
                } else {
                    CapabilityStatus::Unsupported
                },
                adapter: "interface-bound-udp-68",
                limitation: "DISCOVER only, standard UDP 68 to 67, explicit interface, exclusive port (no reuse). Port conflict/permission failure preserves inspect. Receive restricted by Windows IFLIST/macOS IP_BOUND_IF. Real-device/platform wire acceptance pending.",
            },
            Capability {
                id: "pmtu_ipv4",
                status: if icmp_adapter == "unavailable" {
                    CapabilityStatus::Unsupported
                } else {
                    CapabilityStatus::Partial
                },
                adapter: icmp_adapter,
                limitation: "IPv4 DF; explicit too-big evidence only. Windows next-hop MTU unavailable. Native wire acceptance pending.",
            },
            Capability {
                id: "pmtu_ipv6",
                status: if cfg!(any(target_os = "macos", target_os = "linux")) {
                    CapabilityStatus::Partial
                } else {
                    CapabilityStatus::Unsupported
                },
                adapter: icmp_adapter,
                limitation: "Unix IPV6_DONTFRAG with matched PTB; native wire acceptance pending. Windows source fragmentation control unverified; returns unsupported, never ordinary-ping fallback.",
            },
            Capability {
                id: "snmp",
                status: CapabilityStatus::Partial,
                adapter: "rust-usm-udp",
                limitation: "Read-only v2c and v3 USM SHA256/SHA1, AES128. Numeric OIDs/built-in names only; bounded WALK and IF-MIB samples. No SET/MIB compiler/Trap/source binding. Real-device and platform acceptance pending.",
            },
            Capability {
                id: "mtr",
                status: if icmp_adapter == "unavailable" {
                    CapabilityStatus::Unsupported
                } else {
                    CapabilityStatus::Partial
                },
                adapter: icmp_adapter,
                limitation: "ICMP IPv4/IPv6, sequential TTL probes with shared rate budget. Windows synchronous probe bounded to 1s; Unix cancel polling 50ms. Optional explicit UDP PTR, separate OS routing. Gap detection heuristic; macOS native, real isolated multi-hop, physical sleep acceptance pending.",
            },
            Capability {
                id: "socket",
                status: CapabilityStatus::Supported,
                adapter: "rust-socket2",
                limitation: "TCP byte stream and UDP datagrams. Source IP binding only; max 16 live sessions, 32 queued schedules and 1 MiB preview. Echo requires explicit source CIDRs and rate limit.",
            },
            Capability {
                id: "connections",
                status: CapabilityStatus::Partial,
                adapter: if cfg!(windows) {
                    "windows-iphelper"
                } else if cfg!(target_os = "macos") {
                    "macos-lsof"
                } else {
                    "linux-proc-net"
                },
                limitation: "Read-only sequential snapshots. Names best effort on Windows, visibility permission-limited on macOS; Linux has no process attribution. OS workers bounded; query timeout 5s.",
            },
            Capability {
                id: "wol",
                status: CapabilityStatus::Supported,
                adapter: "rust-udp-broadcast",
                limitation: "IPv4 only; successful submission does not prove delivery or wake-up. Real-device wake-up and macOS native validation remain pending.",
            },
            Capability {
                id: "discovery_neighbors",
                status: if cfg!(any(windows, target_os = "macos")) {
                    CapabilityStatus::Partial
                } else {
                    CapabilityStatus::Unsupported
                },
                adapter: if cfg!(windows) {
                    "windows-iphelper"
                } else if cfg!(target_os = "macos") {
                    "macos-arp-ndp"
                } else {
                    "unavailable"
                },
                limitation: "Read-only neighbor cache; no liveness inference. macOS ARP cache age is unknown; OS queries may be restricted.",
            },
            Capability {
                id: "discovery_arp_verify",
                status: if cfg!(any(windows, target_os = "macos")) {
                    CapabilityStatus::Partial
                } else {
                    CapabilityStatus::Unsupported
                },
                adapter: if cfg!(windows) {
                    "windows-resolveipnetentry2"
                } else if cfg!(target_os = "macos") {
                    "macos-bpf-arp"
                } else {
                    "unavailable"
                },
                limitation: "IPv4 only, matching local subnet and on-link route required; routed/VPN targets skipped. macOS requires BPF access. No active ND, RFC5227 or vendor dataset. MAC changes are clues only.",
            },
            Capability {
                id: "icmp",
                status: if icmp_adapter == "unavailable" {
                    CapabilityStatus::Unsupported
                } else {
                    CapabilityStatus::Supported
                },
                adapter: icmp_adapter,
                limitation: "Runtime OS policy may deny sockets; Linux ping_group_range may restrict datagram ICMP. Permission failures are not packet loss.",
            },
            Capability {
                id: "network_overview",
                status: if overview_adapter == "unavailable" {
                    CapabilityStatus::Unsupported
                } else {
                    CapabilityStatus::Partial
                },
                adapter: overview_adapter,
                limitation: "Local queries only. Per-interface DNS attribution and command availability vary; query errors remain visible in the overview.",
            },
            Capability {
                id: "source_binding",
                status: CapabilityStatus::Partial,
                adapter: "bound-tcp-socket",
                limitation: "check TCP/HTTP, scan TCP, discovery TCP/ICMP, socket TCP/UDP and WOL IPv4 support source IP binding. Fixed-interface binding and source binding for other tools are not implemented.",
            },
        ],
        probe_budget: Default::default(),
    }
}
impl Request {
    pub fn tool_id(&self) -> Result<ToolId, ToolError> {
        ToolId::parse(&self.tool)
    }
    /// New nested tool configurations must match their stable operation identity.
    pub fn validate_tool_config(&self) -> Result<(), ToolError> {
        self.tool_id()?;
        let invalid = self.dhcp.is_some() && self.tool != "dhcp"
            || self.tool == "dhcp"
                && self
                    .dhcp
                    .as_ref()
                    .is_none_or(|s| s.operation != self.action)
            || self.pmtu.is_some() && self.tool != "pmtu"
            || self.tool == "pmtu" && (self.pmtu.is_none() || self.action != "icmp")
            || self.snmp.is_some() && self.tool != "snmp"
            || self.tool == "snmp"
                && self
                    .snmp
                    .as_ref()
                    .is_none_or(|s| s.operation != self.action)
            || self.mtr.is_some() && self.tool != "mtr"
            || self.tool == "mtr" && (self.mtr.is_none() || self.action != "icmp")
            || self.socket.is_some() && self.tool != "socket"
            || self.connections.is_some() && self.tool != "connections"
            || self.wol.is_some() && self.tool != "wol"
            || self.tool == "socket" && self.socket.as_ref().is_none_or(|s| s.mode != self.action)
            || self.tool == "connections" && (self.connections.is_none() || self.action != "query")
            || self.tool == "wol" && (self.wol.is_none() || self.action != "send")
            || self.discovery.is_some() && self.tool != "discovery"
            || self.tool == "discovery" && (self.action != "discover" || self.discovery.is_none())
            || self.scan.is_some() && self.tool != "scan"
            || self.subnet.is_some() && self.tool != "subnet"
            || self.tool == "scan" && (self.action != "tcp" || self.scan.is_none())
            || self.tool == "subnet"
                && self
                    .subnet
                    .as_ref()
                    .is_none_or(|c| c.operation != self.action);
        if invalid {
            return Err(ToolError::new(
                "INVALID_ARGUMENT",
                "Tool/action/config mismatch / 工具、操作与配置不匹配",
            ));
        }
        Ok(())
    }
    /// Explicit boundary for task/profile persistence. Runtime secrets stay on Request only.
    pub fn snapshot(&self) -> Result<serde_json::Value, ToolError> {
        self.tool_id()?;
        let mut value = serde_json::to_value(self)
            .map_err(|_| ToolError::new("INVALID_ARGUMENT", "Cannot snapshot request"))?;
        if let Some(object) = value.as_object_mut() {
            object.remove("message");
            if let Some(socket) = object.get_mut("socket").and_then(|s| s.as_object_mut()) {
                socket.remove("record_path");
            }
        }
        Ok(value)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unimplemented_tools_are_rejected() {
        assert!(ToolId::parse("unimplemented").is_err());
        assert!(ToolId::parse("http").is_ok());
        let mut r = Request {
            tool: "scan".into(),
            scan: Some(Default::default()),
            ..Default::default()
        };
        assert!(r.validate_tool_config().is_err());
        r.action = "tcp".into();
        assert!(r.validate_tool_config().is_ok());
        r.tool = "ping".into();
        assert!(r.validate_tool_config().is_err());
        r.scan = None;
        r.tool = "subnet".into();
        r.subnet = Some(Default::default());
        assert!(r.validate_tool_config().is_err());
        r.action = "inspect".into();
        assert!(r.validate_tool_config().is_ok());
    }
    #[test]
    fn snapshot_excludes_credentials_payloads_and_url_query() {
        let r = Request {
            tool: "check".into(),
            target: "https://example.com/a?token=secret#private".into(),
            ftp_password: "password-secret".into(),
            message: "payload-secret".into(),
            wormhole: Some(crate::wormhole::Config {
                code: "pairing-secret".into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let text = r.snapshot().unwrap().to_string();
        for secret in [
            "token=secret",
            "private",
            "password-secret",
            "payload-secret",
            "pairing-secret",
        ] {
            assert!(!text.contains(secret), "{secret}");
        }
        assert!(text.contains("example.com/a"));
    }
}
