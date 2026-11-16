use clap::Args;
use hopkit_core::{Request, mtr::Config};
#[derive(Args)]
pub struct Mtr {
    /// Optional numeric PTR resolver; independent UDP, OS-routed / 可选 PTR 解析器
    #[arg(long, default_value = "")]
    ptr_server: String,
    target: String,
    /// 0 runs until duration or Ctrl+C / 0 持续至时长或取消
    #[arg(long, default_value_t = 100)]
    cycles: u32,
    /// Seconds, 0 has no duration limit / 秒，0 不限制时长
    #[arg(long, default_value_t = 0)]
    duration: u64,
    /// Minimum cycle start interval; slow rounds never queue / 慢轮次不排队
    #[arg(long, default_value_t = 1000)]
    interval_ms: u64,
    /// Windows synchronous probe bounds Stop latency / 单探测 100..1000 ms
    #[arg(long, default_value_t = 1000)]
    timeout_ms: u64,
    #[arg(long, default_value_t = 30)]
    max_hops: u8,
    /// ICMP probes/s; shared process budget 200/s / 每秒探测上限
    #[arg(long, default_value_t = 20)]
    rate: u32,
    /// Numeric local IP; source binding does not fix an interface / 本机源 IP
    #[arg(long, default_value = "")]
    source: String,
    #[command(flatten)]
    family: super::FamilyOptions,
}
impl Mtr {
    pub fn request(self) -> Request {
        let config = Config {
            ptr_server: self.ptr_server,
            target: self.target.clone(),
            family: if self.family.ipv4 {
                "ipv4"
            } else if self.family.ipv6 {
                "ipv6"
            } else {
                "auto"
            }
            .into(),
            source: self.source,
            cycles: self.cycles,
            duration_secs: self.duration,
            interval_ms: self.interval_ms,
            timeout_ms: self.timeout_ms,
            max_hops: self.max_hops,
            rate: self.rate,
        };
        Request {
            tool: "mtr".into(),
            action: "icmp".into(),
            target: self.target,
            mtr: Some(config),
            ..Default::default()
        }
    }
}
