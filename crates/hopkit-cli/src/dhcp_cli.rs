use clap::{Args, Subcommand};
use hopkit_core::{Request, dhcp::Config};
#[derive(Args)]
pub struct Dhcp {
    #[command(subcommand)]
    operation: Operation,
}
#[derive(Subcommand)]
enum Operation {
    /// Read current IPv4 configuration; no packets sent / 只读检查，不发报文
    Inspect {
        #[arg(long, default_value = "")]
        interface: String,
    },
    /// DISCOVER only; may temporarily reserve an offer. Never requests a lease.
    Discover {
        /// Use adapter ID/index from inspect / 使用 inspect 返回的接口 ID 或索引
        #[arg(long)]
        interface: String,
        #[arg(long, default_value_t = 5)]
        duration: u64,
        #[arg(long, default_value_t = 3)]
        attempts: u8,
        /// User expectation only; multiple offers can be legitimate / 仅对比预期清单
        #[arg(long = "expected-server")]
        expected_servers: Vec<std::net::Ipv4Addr>,
    },
}
impl Dhcp {
    pub fn request(self) -> Request {
        let c = match self.operation {
            Operation::Inspect { interface } => Config {
                interface,
                ..Default::default()
            },
            Operation::Discover {
                interface,
                duration,
                attempts,
                expected_servers,
            } => Config {
                operation: "discover".into(),
                interface,
                duration_secs: duration,
                attempts,
                expected_servers,
            },
        };
        Request {
            tool: "dhcp".into(),
            action: c.operation.clone(),
            target: c.interface.clone(),
            dhcp: Some(c),
            ..Default::default()
        }
    }
}
