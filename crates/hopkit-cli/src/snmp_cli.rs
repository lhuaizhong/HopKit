use clap::Args;
use hopkit_core::{
    Request, ToolError,
    snmp::{Config, Credentials},
};
#[derive(Args)]
pub struct Snmp {
    #[arg(value_parser=["get","getnext","getbulk","walk","overview","interfaces"])]
    operation: String,
    target: String,
    /// Numeric OIDs or sysName/sysDescr/sysObjectID/sysUpTime/system/ifTable/ifXTable
    oids: Vec<String>,
    #[arg(long,default_value="2c",value_parser=["2c","3"])]
    snmp_version: String,
    #[arg(long, default_value_t = 161)]
    port: u16,
    /// Environment variable name; never pass the community on the command line / community 环境变量名
    #[arg(long, default_value = "HOPKIT_SNMP_COMMUNITY")]
    community_env: String,
    #[arg(long, default_value = "")]
    username: String,
    #[arg(long, default_value = "")]
    context: String,
    #[arg(long,default_value="auth_priv",value_parser=["no_auth_no_priv","auth_no_priv","auth_priv"])]
    security_level: String,
    #[arg(long,default_value="sha256",value_parser=["sha256","sha1"])]
    auth_protocol: String,
    #[arg(long,default_value="aes128",value_parser=["aes128"])]
    privacy_protocol: String,
    #[arg(long, default_value = "HOPKIT_SNMP_AUTH_PASSWORD")]
    auth_env: String,
    #[arg(long, default_value = "HOPKIT_SNMP_PRIVACY_PASSWORD")]
    privacy_env: String,
    #[arg(long, default_value_t = 1000)]
    timeout_ms: u64,
    #[arg(long, default_value_t = 1)]
    retries: u32,
    #[arg(long, default_value_t = 10)]
    max_repetitions: u32,
    #[arg(long, default_value_t = 10000)]
    max_results: usize,
    #[arg(long, default_value_t = 4194304)]
    max_bytes: usize,
    /// Total WALK/query or per-interface-snapshot deadline, maximum 60s / 总期限
    #[arg(long, default_value_t = 60)]
    deadline_secs: u64,
    #[arg(long, default_value_t = 2000)]
    interval_ms: u64,
    /// Interface samples; 0 continues until Ctrl+C / 接口采样数，0 持续
    #[arg(long, default_value_t = 2)]
    samples: u32,
}
impl Snmp {
    pub fn request(self) -> Result<Request, ToolError> {
        let env = |key: &str| -> Result<String, ToolError> {
            if key.is_empty()
                || key.len() > 128
                || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            {
                return Err(ToolError::new(
                    "INVALID_ARGUMENT",
                    "Invalid SNMP environment variable name",
                ));
            }
            Ok(std::env::var(key).unwrap_or_default())
        };
        let credentials = Credentials {
            community: if self.snmp_version == "2c" {
                env(&self.community_env)?
            } else {
                String::new()
            },
            auth_password: if self.snmp_version == "3" && self.security_level != "no_auth_no_priv" {
                env(&self.auth_env)?
            } else {
                String::new()
            },
            privacy_password: if self.snmp_version == "3" && self.security_level == "auth_priv" {
                env(&self.privacy_env)?
            } else {
                String::new()
            },
        };
        let c = Config {
            target: self.target.clone(),
            port: self.port,
            version: self.snmp_version,
            operation: self.operation.clone(),
            oids: if self.oids.is_empty() && self.operation == "get" {
                vec!["sysUpTime".into()]
            } else {
                self.oids
            },
            username: self.username,
            context: self.context,
            security_level: self.security_level,
            auth_protocol: self.auth_protocol,
            privacy_protocol: self.privacy_protocol,
            timeout_ms: self.timeout_ms,
            retries: self.retries,
            max_repetitions: self.max_repetitions,
            max_results: self.max_results,
            max_bytes: self.max_bytes,
            deadline_secs: self.deadline_secs,
            interval_ms: self.interval_ms,
            samples: self.samples,
        };
        hopkit_core::snmp::validate(&c)?;
        hopkit_core::snmp::validate_credentials(&c, &credentials)?;
        Ok(Request {
            tool: "snmp".into(),
            action: self.operation,
            target: self.target,
            snmp: Some(c),
            snmp_credentials: credentials,
            ..Default::default()
        })
    }
}
