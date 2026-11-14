//! Stable protocol identities. Presentation categories are intentionally not an execution policy.
use crate::{Request, ToolError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolId {
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
    "check", "monitor", "network", "target", "ping", "trace", "tcping", "dns", "ntp", "iperf3",
    "ftp", "http", "wormhole", "tftp", "syslog",
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
        source_binding: &["check.tcp", "check.http"],
        adapters: vec![
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
                limitation: "check TCP/HTTP connections support source IP binding. Fixed-interface binding and source binding for other tools are not implemented.",
            },
        ],
        probe_budget: Default::default(),
    }
}
impl Request {
    pub fn tool_id(&self) -> Result<ToolId, ToolError> {
        ToolId::parse(&self.tool)
    }
    /// Explicit boundary for task/profile persistence. Runtime secrets stay on Request only.
    pub fn snapshot(&self) -> Result<serde_json::Value, ToolError> {
        self.tool_id()?;
        let mut value = serde_json::to_value(self)
            .map_err(|_| ToolError::new("INVALID_ARGUMENT", "Cannot snapshot request"))?;
        if let Some(object) = value.as_object_mut() {
            object.remove("message");
        }
        Ok(value)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unimplemented_tools_are_rejected() {
        assert!(ToolId::parse("scan").is_err());
        assert!(ToolId::parse("http").is_ok());
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
