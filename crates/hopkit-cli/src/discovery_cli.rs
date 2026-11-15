use clap::Args;
use hopkit_core::{Request, discovery::Config};
#[derive(Args)]
pub struct Discover {
    /// Numeric IPs, IPv4 CIDR/start-end. Omit for read-only cache; never enumerates IPv6 prefixes.
    targets: Vec<String>,
    /// macOS/Linux interface name; Windows alias or numeric ifIndex. Source-IP selection, not forced routing.
    #[arg(long, default_value = "")]
    interface: String,
    #[arg(long, value_delimiter = ',', default_value = "arp,icmp,tcp")]
    methods: Vec<String>,
    /// At most 8 ports. Refusal is response evidence, not an open service.
    #[arg(long, default_value = "80,443")]
    ports: String,
    #[arg(long, default_value_t = 500)]
    timeout_ms: u64,
    #[arg(long, default_value_t = 32)]
    concurrency: usize,
    #[arg(long, default_value_t = 100)]
    rate: u32,
    /// Local numeric source IP; must belong to --interface if both are given.
    #[arg(long, default_value = "")]
    source: String,
    /// Optional UDP PTR phase (4 workers, bounded timeout); explicit resolver required.
    #[arg(long, requires = "dns_server")]
    name_lookup: bool,
    #[arg(long, default_value = "")]
    dns_server: String,
}
impl Discover {
    pub fn request(self) -> Request {
        let config = Config {
            targets: self.targets,
            interface: self.interface,
            methods: self.methods,
            ports: self.ports,
            timeout_ms: self.timeout_ms,
            concurrency: self.concurrency,
            rate: self.rate,
            source: self.source,
            name_lookup: self.name_lookup,
            dns_server: self.dns_server,
        };
        Request {
            tool: "discovery".into(),
            action: "discover".into(),
            target: config.targets.join(", "),
            discovery: Some(config),
            ..Default::default()
        }
    }
}
