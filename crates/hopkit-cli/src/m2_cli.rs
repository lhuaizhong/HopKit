use clap::Subcommand;
use hopkit_core::{Request, scan, subnet};
#[derive(Subcommand)]
pub enum Scan {
    /// TCP connect only; labels describe common uses, not detected services / TCP 建连扫描
    Tcp {
        /// Numeric IP, hostname, IPv4 CIDR or start-end; repeat for multiple targets
        #[arg(required = true)]
        targets: Vec<String>,
        #[arg(long, default_value = "22,80,443,3389")]
        ports: String,
        #[arg(long, default_value_t = 1000)]
        timeout_ms: u64,
        #[arg(long, default_value_t = 32)]
        concurrency: usize,
        /// At most 100 connections/s per task; 200/s shared within this process
        #[arg(long, default_value_t = 100)]
        rate: u32,
        /// Numeric local source IP (IPv6 scope must be numeric) / 本机源 IP
        #[arg(long, default_value = "")]
        source: String,
        #[command(flatten)]
        family: super::FamilyOptions,
    },
}
impl Scan {
    pub fn request(self) -> Request {
        let Self::Tcp {
            targets,
            ports,
            timeout_ms,
            concurrency,
            rate,
            source,
            family,
        } = self;
        Request {
            tool: "scan".into(),
            action: "tcp".into(),
            target: targets.join(", "),
            scan: Some(scan::Config {
                targets,
                ports,
                timeout_ms,
                concurrency,
                rate,
                source,
                family: if family.ipv4 {
                    "ipv4"
                } else if family.ipv6 {
                    "ipv6"
                } else {
                    "auto"
                }
                .into(),
            }),
            ..Default::default()
        }
    }
}
#[derive(Subcommand)]
pub enum Subnet {
    /// Normalize CIDR or IPv4/dotted-mask; no network I/O / 离线子网信息
    Inspect { input: String },
    /// Exit 0 when contained, 1 otherwise / 包含退出 0，否则 1
    Contains { input: String, address: String },
    /// Compare overlap and both containment directions / 网段重叠及包含
    Overlap { input: String, other: String },
    /// At most 1024 child networks / 最多拆分 1024 项
    Split {
        input: String,
        #[arg(long)]
        prefix: u8,
    },
    /// Exact aggregation without covering gaps / 精确聚合，不覆盖空洞
    Aggregate {
        #[arg(required = true)]
        networks: Vec<String>,
    },
}
impl Subnet {
    pub fn request(self) -> Request {
        let mut c = subnet::Config::default();
        match self {
            Self::Inspect { input } => c.input = input,
            Self::Contains { input, address } => {
                c.operation = "contains".into();
                c.input = input;
                c.other = address;
            }
            Self::Overlap { input, other } => {
                c.operation = "overlap".into();
                c.input = input;
                c.other = other;
            }
            Self::Split { input, prefix } => {
                c.operation = "split".into();
                c.input = input;
                c.prefix = prefix;
            }
            Self::Aggregate { networks } => {
                c.operation = "aggregate".into();
                c.input = networks.join(",");
            }
        }
        Request {
            tool: "subnet".into(),
            target: c.input.clone(),
            action: c.operation.clone(),
            subnet: Some(c),
            ..Default::default()
        }
    }
}
