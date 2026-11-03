use crate::ToolError;
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub operation: String,
    pub send_path: String,
    #[serde(skip_serializing)]
    pub code: String,
    pub mailbox_url: String,
    pub transit_relay: String,
    pub connection_mode: String,
    pub code_length: usize,
    pub verify: bool,
    pub peer_wait_secs: u64,
    pub max_receive_bytes: Option<String>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            operation: "send".into(),
            send_path: String::new(),
            code: String::new(),
            mailbox_url: "wss://relay.magic-wormhole.io/v1".into(),
            transit_relay: "tcp://transit.magic-wormhole.io:4001".into(),
            connection_mode: "auto".into(),
            code_length: 2,
            verify: false,
            peer_wait_secs: 600,
            max_receive_bytes: None,
        }
    }
}
impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        serde_json::to_value(self).unwrap().fmt(f)
    }
}
impl Config {
    pub fn validate(&self) -> Result<(), ToolError> {
        let bad = || {
            ToolError::new(
                "INVALID_ARGUMENT",
                "Check Wormhole operation, URLs, code length (2–8), connection mode and peer wait (60–3600 s)",
            )
        };
        if !matches!(self.operation.as_str(), "send" | "receive")
            || !(2..=8).contains(&self.code_length)
            || !(60..=3600).contains(&self.peer_wait_secs)
            || !matches!(self.connection_mode.as_str(), "auto" | "direct" | "relay")
        {
            return Err(bad());
        }
        for (s, mailbox) in [(&self.mailbox_url, true), (&self.transit_relay, false)] {
            let u = url::Url::parse(s).map_err(|_| bad())?;
            if s.chars().any(char::is_control)
                || !u.username().is_empty()
                || u.password().is_some()
                || u.query().is_some()
                || u.fragment().is_some()
                || u.host_str().is_none()
                || if mailbox {
                    !matches!(u.scheme(), "ws" | "wss")
                } else {
                    u.scheme() != "tcp" || u.port().is_none() || !matches!(u.path(), "" | "/")
                }
            {
                return Err(bad());
            }
        }
        if self.operation == "receive"
            && (self.code.len() > 1024
                || self.code.chars().any(char::is_control)
                || self.code.parse::<magic_wormhole::Code>().is_err())
        {
            return Err(ToolError::new(
                "INVALID_CODE",
                "Invalid pairing code / 配对码格式无效",
            ));
        }
        if let Some(n) = &self.max_receive_bytes {
            n.parse::<u64>().map_err(|_| bad())?;
        }
        if self.operation == "send" && self.send_path.is_empty() {
            return Err(bad());
        }
        Ok(())
    }
}
