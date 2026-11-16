//! Read-only SNMP. Serializable configuration and ephemeral credentials are separate.
mod ber;
mod interfaces;
mod session;
use crate::runtime::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub target: String,
    pub port: u16,
    pub version: String,
    pub operation: String,
    pub oids: Vec<String>,
    pub username: String,
    pub context: String,
    pub security_level: String,
    pub auth_protocol: String,
    pub privacy_protocol: String,
    pub timeout_ms: u64,
    pub retries: u32,
    pub max_repetitions: u32,
    pub max_results: usize,
    pub max_bytes: usize,
    pub deadline_secs: u64,
    pub interval_ms: u64,
    pub samples: u32,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            target: "127.0.0.1".into(),
            port: 161,
            version: "2c".into(),
            operation: "get".into(),
            oids: vec!["sysUpTime".into()],
            username: String::new(),
            context: String::new(),
            security_level: "auth_priv".into(),
            auth_protocol: "sha256".into(),
            privacy_protocol: "aes128".into(),
            timeout_ms: 1000,
            retries: 1,
            max_repetitions: 10,
            max_results: 10000,
            max_bytes: 4 * 1024 * 1024,
            deadline_secs: 60,
            interval_ms: 2000,
            samples: 2,
        }
    }
}
#[derive(Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Credentials {
    pub community: String,
    pub auth_password: String,
    pub privacy_password: String,
}
impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Credentials([redacted])")
    }
}
pub(super) fn invalid(message: &str) -> ToolError {
    ToolError::new("INVALID_ARGUMENT", message)
}
pub(super) fn protocol(message: &str) -> ToolError {
    ToolError::new("SNMP_PROTOCOL", message)
}
pub fn oid_string(o: &[u32]) -> String {
    o.iter().map(u32::to_string).collect::<Vec<_>>().join(".")
}
pub fn oid(input: &str) -> Result<Vec<u32>, ToolError> {
    let value = match input {
        "sysDescr" => "1.3.6.1.2.1.1.1.0",
        "sysObjectID" => "1.3.6.1.2.1.1.2.0",
        "sysUpTime" => "1.3.6.1.2.1.1.3.0",
        "sysName" => "1.3.6.1.2.1.1.5.0",
        "system" => "1.3.6.1.2.1.1",
        "ifTable" => "1.3.6.1.2.1.2.2",
        "ifXTable" => "1.3.6.1.2.1.31.1.1",
        s => s.trim_start_matches('.'),
    };
    if value.len() > 1408 {
        return Err(invalid("OID too long"));
    }
    let out = value
        .split('.')
        .map(|s| {
            if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
                Err(())
            } else {
                s.parse::<u32>().map_err(|_| ())
            }
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| invalid("Use numeric OIDs or built-in names / 请使用数值 OID 或内置名称"))?;
    if out.len() < 2 || out.len() > 128 || out[0] > 2 || out[0] < 2 && out[1] > 39 {
        return Err(invalid("Invalid OID arcs"));
    }
    Ok(out)
}
pub fn validate(c: &Config) -> Result<(), ToolError> {
    if c.target.is_empty()
        || c.target.len() > 253
        || c.target
            .chars()
            .any(|v| v.is_whitespace() || v.is_control() || "/@?#".contains(v))
        || c.port == 0
        || !["2c", "3"].contains(&c.version.as_str())
        || ![
            "get",
            "getnext",
            "getbulk",
            "walk",
            "overview",
            "interfaces",
        ]
        .contains(&c.operation.as_str())
    {
        return Err(invalid("Invalid SNMP target/version/operation"));
    }
    if !(100..=10000).contains(&c.timeout_ms)
        || c.retries > 3
        || !(1..=50).contains(&c.max_repetitions)
        || !(1..=10000).contains(&c.max_results)
        || !(1024..=4194304).contains(&c.max_bytes)
        || !(1..=60).contains(&c.deadline_secs)
        || !(1000..=60000).contains(&c.interval_ms)
        || c.samples > 100000
        || c.oids.len() > 32
        || c.context.len() > 255
        || c.username.len() > 32
        || c.context.chars().any(char::is_control)
        || c.username.chars().any(char::is_control)
    {
        return Err(invalid(
            "SNMP resource/field limit exceeded / 超过 SNMP 参数边界",
        ));
    }
    if ["get", "getnext", "getbulk", "walk"].contains(&c.operation.as_str())
        && (c.oids.is_empty() || c.operation == "walk" && c.oids.len() != 1)
    {
        return Err(invalid("GET requires OIDs; WALK requires one subtree"));
    }
    for o in &c.oids {
        oid(o)?;
    }
    if c.version == "3"
        && (c.username.is_empty()
            || !["no_auth_no_priv", "auth_no_priv", "auth_priv"]
                .contains(&c.security_level.as_str())
            || !["sha256", "sha1"].contains(&c.auth_protocol.as_str())
            || c.privacy_protocol != "aes128")
    {
        return Err(invalid(
            "SNMPv3 requires username and supported explicit USM algorithms",
        ));
    }
    Ok(())
}
pub fn validate_credentials(c: &Config, k: &Credentials) -> Result<(), ToolError> {
    for s in [&k.community, &k.auth_password, &k.privacy_password] {
        if s.len() > 1024 {
            return Err(invalid("SNMP credential too long"));
        }
    }
    if c.version == "2c" && (k.community.is_empty() || k.community.len() > 255)
        || c.version == "3" && c.security_level != "no_auth_no_priv" && k.auth_password.len() < 8
        || c.version == "3" && c.security_level == "auth_priv" && k.privacy_password.len() < 8
    {
        return Err(invalid(
            "SNMP credentials missing (USM passwords minimum 8 bytes) / 缺少运行凭据",
        ));
    }
    Ok(())
}
#[derive(Debug, Clone, Serialize)]
pub struct Varbind {
    pub oid: String,
    pub kind: String,
    pub value: String,
    pub object_error: Option<String>,
}
#[derive(Default, Debug, Clone, Serialize)]
pub struct Stats {
    pub requests: u64,
    pub retries: u64,
    pub responses: u64,
    pub timeouts: u64,
    pub object_errors: u64,
    pub results: usize,
    pub decoded_bytes: usize,
    pub samples: u64,
    pub samples_valid: u64,
    pub samples_failed: u64,
    pub gaps: u64,
    pub samples_dropped: u64,
}
#[derive(Debug, Clone, Serialize)]
pub struct ResultData {
    pub schema_version: u32,
    pub target: String,
    pub remote_endpoint: String,
    pub local_endpoint: String,
    pub version: String,
    pub security_level: String,
    pub engine: Option<session::Engine>,
    pub varbinds: Vec<Varbind>,
    pub interface_samples: Vec<interfaces::Sample>,
    pub stats: Stats,
    pub completion_reason: String,
    pub failure: Option<Value>,
    pub success: bool,
    pub partial: bool,
    pub started_at: String,
    pub ended_at: String,
    pub limitations: Vec<String>,
}
struct Runner<'a> {
    config: &'a Config,
    session: session::Session<'a>,
    cancel: &'a Cancel,
    emit: &'a Sink<'a>,
    deadline: Instant,
    result: ResultData,
    seq: u64,
    preview_at: Instant,
}
impl Runner<'_> {
    fn snapshot(&mut self) {
        self.seq += 1;
        self.result.engine = self.session.engine.clone();
        self.result.ended_at = chrono::Utc::now().to_rfc3339();
        (self.emit)(
            json!({"event_type":"snmp_snapshot","event_seq":self.seq,"snapshot":self.result}),
        );
    }
    fn query(&mut self, op: &str, oids: &[Vec<u32>]) -> Result<Vec<Varbind>, ToolError> {
        let reply = self
            .session
            .query(op, oids, self.deadline, &mut self.result.stats)?;
        if reply.status != 0 {
            let mut e = ToolError::new(
                "SNMP_AGENT",
                if reply.status == 1 {
                    "Agent returned tooBig / 设备返回 tooBig"
                } else {
                    "Agent returned a PDU error / 设备返回 PDU 错误"
                },
            );
            e.data = Some(json!({"error_status":reply.status,"error_index":reply.index}));
            return Err(e);
        }
        if op != "getbulk" && reply.values.len() != oids.len() {
            return Err(protocol("Unexpected varbind count"));
        }
        if op == "getbulk" && reply.values.len() > oids.len() * self.config.max_repetitions as usize
        {
            return Err(protocol("GETBULK response exceeds repetition budget"));
        }
        let mut previous = oids.to_vec();
        for (i, v) in reply.values.iter().enumerate() {
            let got = oid(&v.oid).map_err(|_| protocol("Invalid returned OID"))?;
            if op == "getbulk" && v.object_error.is_none() {
                let column = i % oids.len();
                if got <= previous[column] {
                    return Err(protocol("GETBULK OID did not advance"));
                }
                previous[column] = got.clone();
            }
            if op == "get" && got != oids[i]
                || op == "getnext" && v.object_error.is_none() && got <= oids[i]
            {
                return Err(protocol("Response OID mismatch or not advancing"));
            }
        }
        Ok(reply.values)
    }
    fn retain(&mut self, v: Varbind) -> Result<bool, ToolError> {
        let size = serde_json::to_vec(&v).unwrap().len();
        if self.result.stats.results >= self.config.max_results
            || size
                > self
                    .config
                    .max_bytes
                    .saturating_sub(self.result.stats.decoded_bytes)
        {
            self.result.completion_reason = "result_limit".into();
            self.result.partial = true;
            return Ok(false);
        }
        self.result.stats.decoded_bytes += size;
        self.result.stats.results += 1;
        self.result.stats.object_errors += u64::from(v.object_error.is_some());
        self.result.varbinds.push(v);
        Ok(true)
    }
    fn walk(&mut self, root: Vec<u32>) -> Result<Vec<Varbind>, ToolError> {
        let mut cursor = root.clone();
        let mut out = vec![];
        let mut bytes = 0;
        loop {
            let values = self.query("getbulk", &[cursor.clone()])?;
            if values.is_empty() {
                return Err(protocol("Empty GETBULK before subtree end"));
            }
            for v in values {
                if let Some(error) = &v.object_error {
                    if error != "endOfMibView" && self.config.operation == "walk" {
                        self.retain(v)?;
                    }
                    return Ok(out);
                }
                let next = oid(&v.oid)?;
                if next <= cursor {
                    return Err(protocol("WALK OID did not advance"));
                }
                if !next.starts_with(&root) {
                    return Ok(out);
                }
                cursor = next;
                if self.config.operation != "walk" {
                    bytes += serde_json::to_vec(&v).unwrap().len();
                }
                if out.len() + self.result.stats.results >= self.config.max_results
                    || bytes + self.result.stats.decoded_bytes > self.config.max_bytes
                {
                    self.result.partial = true;
                    self.result.completion_reason = "result_limit".into();
                    return Ok(out);
                }
                if self.config.operation == "walk" {
                    if !self.retain(v)? {
                        return Ok(out);
                    }
                } else {
                    out.push(v);
                }
            }
            if self.config.operation == "walk"
                && self.preview_at.elapsed() >= Duration::from_millis(500)
            {
                self.snapshot();
                self.preview_at = Instant::now();
            }
        }
    }
    fn execute(&mut self) -> Result<(), ToolError> {
        match self.config.operation.as_str() {
            "interfaces" => self.observe(),
            "walk" => {
                let values = self.walk(oid(&self.config.oids[0])?)?;
                for v in values {
                    if !self.retain(v)? {
                        break;
                    }
                }
                Ok(())
            }
            op => {
                let oids = if op == "overview" {
                    ["sysName", "sysDescr", "sysObjectID", "sysUpTime"]
                        .iter()
                        .map(|s| oid(s).unwrap())
                        .collect()
                } else {
                    self.config
                        .oids
                        .iter()
                        .map(|s| oid(s))
                        .collect::<Result<Vec<_>, _>>()?
                };
                let values = self.query(if op == "overview" { "get" } else { op }, &oids)?;
                for v in values {
                    if !self.retain(v)? {
                        break;
                    }
                }
                Ok(())
            }
        }
    }
}
pub fn run(c: &Config, k: &Credentials, cancel: &Cancel, emit: &Sink<'_>) -> ToolResult {
    validate(c)?;
    validate_credentials(c, k)?;
    cancelled(cancel)?;
    let started = chrono::Utc::now().to_rfc3339();
    let session = session::Session::new(c, k, cancel)?;
    let result=ResultData{schema_version:1,target:c.target.clone(),remote_endpoint:session.remote.clone(),local_endpoint:session.local.clone(),version:c.version.clone(),security_level:if c.version=="2c"{"community".into()}else{c.security_level.clone()},engine:None,varbinds:vec![],interface_samples:vec![],stats:Stats::default(),completion_reason:"complete".into(),failure:None,success:false,partial:false,started_at:started.clone(),ended_at:started,limitations:vec!["Numeric OIDs and built-in symbols only; no MIB compiler, SET, Trap or Inform. Source interface binding is unavailable.".into(),"IF-MIB columns are polled sequentially, not as an atomic device snapshot. Missing discontinuity metadata clears rates; at most 128 interface snapshots are retained under the byte budget.".into()]};
    let mut runner = Runner {
        config: c,
        session,
        cancel,
        emit,
        deadline: Instant::now() + Duration::from_secs(c.deadline_secs),
        result,
        seq: 0,
        preview_at: Instant::now(),
    };
    let outcome = runner.execute();
    runner.result.success = outcome.is_ok()
        && !runner.result.partial
        && runner.result.stats.object_errors == 0
        && runner.result.stats.samples_failed == 0;
    runner.result.partial |= outcome.is_err();
    if let Err(e) = &outcome {
        runner.result.completion_reason = e.code.to_lowercase();
        runner.result.failure = Some(json!({"code":e.code,"details":e.data}));
    }
    runner.snapshot();
    let value = serde_json::to_value(&runner.result).unwrap();
    match outcome {
        Ok(()) => Ok(value),
        Err(mut e) => {
            e.data = Some(value);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn secrets_are_ephemeral_and_invalid_operations_rejected() {
        let mut r = crate::Request {
            tool: "snmp".into(),
            action: "get".into(),
            snmp: Some(Config::default()),
            snmp_credentials: Credentials {
                community: "gold-community".into(),
                auth_password: "gold-authentication".into(),
                privacy_password: "gold-privacy".into(),
            },
            ..Default::default()
        };
        for serialized in [
            r.snapshot().unwrap().to_string(),
            format!("{r:?}"),
            format!("{:?}", r.snmp_credentials),
        ] {
            for secret in ["gold-community", "gold-authentication", "gold-privacy"] {
                assert!(!serialized.contains(secret));
            }
        }
        r.action = "set".into();
        assert!(r.validate_tool_config().is_err());
        let mut c = Config {
            operation: "set".into(),
            ..Default::default()
        };
        assert!(validate(&c).is_err());
        c.operation = "walk".into();
        c.oids = vec!["system".into(), "ifTable".into()];
        assert!(validate(&c).is_err());
    }
    #[test]
    fn usm_algorithm_and_budgets_are_explicit() {
        let mut c = Config {
            version: "3".into(),
            username: "user".into(),
            ..Default::default()
        };
        assert!(validate(&c).is_ok());
        assert!(validate_credentials(&c, &Credentials::default()).is_err());
        c.auth_protocol = "md5".into();
        assert!(validate(&c).is_err());
        c.auth_protocol = "sha256".into();
        c.max_results = 10001;
        assert!(validate(&c).is_err());
    }
}
