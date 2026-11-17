use clap::Args;
use hopkit_core::{Request, pmtu::Config};
#[derive(Args)]
pub struct Mtu {
    target: String,
    #[command(flatten)]
    family: super::FamilyOptions,
    /// IP total bytes, including IP + ICMP headers / IP 包总长度
    #[arg(long)]
    min: Option<u32>,
    #[arg(long, default_value_t = 1500)]
    max: u32,
    /// Extra attempts after two verification attempts / 额外重试
    #[arg(long, default_value_t = 1)]
    retries: u8,
    #[arg(long, default_value_t = 1000)]
    timeout_ms: u64,
    #[arg(long, default_value_t = 30000)]
    total_timeout_ms: u64,
    #[arg(long, default_value = "")]
    source: String,
}
impl Mtu {
    pub fn request(self) -> Request {
        let c = Config {
            target: self.target.clone(),
            family: if self.family.ipv6 { "ipv6" } else { "ipv4" }.into(),
            source: self.source,
            min_ip_bytes: self
                .min
                .unwrap_or(if self.family.ipv6 { 1280 } else { 576 }),
            max_ip_bytes: self.max,
            retries: self.retries,
            timeout_ms: self.timeout_ms,
            total_timeout_ms: self.total_timeout_ms,
        };
        Request {
            tool: "pmtu".into(),
            action: "icmp".into(),
            target: self.target,
            pmtu: Some(c),
            ..Default::default()
        }
    }
}
