use clap::{Args, Parser, Subcommand};
use hopkit_core::*;
use serde_json::{Value, json};
use std::{
    io::{self, IsTerminal, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Parser)]
#[command(
    name = "hopkit-cli",
    bin_name = "hopkit-cli",
    version,
    about = "HopKit Network Toolkit / 网络工具箱 · Windows / macOS preview",
    long_about = "HopKit Network Toolkit / 开源网络排障工具箱\nWindows / macOS preview: Ping / Trace / TCPing / DNS / NTP / iperf3 / FTP / TFTP / Syslog UDP.\nGUI is never launched by CLI. Ctrl+C cancels the current operation.\n命令行独立运行；Ctrl+C 取消。Run help <command> for arguments."
)]
struct Cli {
    #[arg(long,global=true,default_value="text",value_parser=["text","json","jsonl"])]
    format: String,
    #[arg(long,global=true,default_value="en-US",value_parser=["zh-CN","en-US"])]
    lang: String,
    #[arg(long, global = true)]
    no_color: bool,
    #[arg(long, global = true)]
    strict: bool,
    #[command(subcommand)]
    command: Option<Command>,
}
#[derive(Args)]
struct Probe {
    target: String,
    /// ICMP data bytes, excluding IP/ICMP headers / ICMP 数据字节数
    #[arg(long, default_value_t = 32)]
    size: u32,
    #[arg(short = 'c', long, default_value_t = 4)]
    count: u32,
    #[arg(short = 't', long, conflicts_with = "count")]
    continuous: bool,
    #[arg(long, default_value_t = 1000)]
    timeout_ms: u64,
    #[arg(long, default_value_t = 1000)]
    interval_ms: u64,
    #[arg(short = '6', long)]
    ipv6: bool,
}
#[derive(Subcommand)]
enum Command {
    #[command(about = "Continuous multi-target monitoring / 多目标持续监测")]
    Monitor {
        /// Repeat up to 3 times: ping,HOST or tcping,HOST,PORT
        #[arg(long = "target", required = true)]
        targets: Vec<String>,
        #[arg(long, default_value_t = 1000)]
        interval_ms: u64,
        #[arg(long, default_value_t = 1000)]
        timeout_ms: u64,
        /// 0 runs until Ctrl+C / 0 表示持续运行
        #[arg(long, default_value_t = 0)]
        duration: u64,
        /// Optional bounded JSONL journal directory / 可选有界日志目录
        #[arg(long, default_value = "")]
        log_directory: String,
    },
    #[command(about = "Integrated endpoint troubleshooting / 终端综合排障")]
    Check {
        target: String,
        #[arg(long, default_value_t = 2000)]
        timeout_ms: u64,
        /// Bind TCP/HTTP to a local numeric source IP / TCP、HTTP 实际绑定源 IP
        #[arg(long, default_value = "")]
        source: String,
        #[arg(long, default_value = "")]
        dns_server: String,
        #[arg(long)]
        insecure_tls: bool,
        /// Add a PEM CA file to bundled roots / 显式添加 PEM CA
        #[arg(long, default_value = "")]
        ca_file: String,
        #[arg(long)]
        trace: bool,
        #[arg(short = '6', long)]
        ipv6: bool,
    },
    #[command(about = "Local interfaces, routes and DNS; no probes / 本机网络概览，不发送探测")]
    Network,
    #[command(about = "Normalize a target without probing / 预览目标，不发送探测")]
    Target { input: String },
    #[command(about = "ICMP echo / 连通性测试")]
    Ping(Probe),
    #[command(
        alias = "tracert",
        alias = "traceroute",
        about = "ICMP route tracing / 路由追踪"
    )]
    Trace {
        target: String,
        #[arg(long, default_value_t = 30)]
        max_hops: u8,
        #[arg(long, default_value_t = 1000)]
        timeout_ms: u64,
        #[arg(short = '6', long)]
        ipv6: bool,
    },
    #[command(about = "TCP connect test / TCP 端口测试")]
    Tcping {
        target: String,
        port: u16,
        #[arg(short = 'c', long, default_value_t = 4)]
        count: u32,
        #[arg(short = 't', long)]
        continuous: bool,
        #[arg(long, default_value_t = 1000)]
        timeout_ms: u64,
        #[arg(long, default_value_t = 1000)]
        interval_ms: u64,
        #[arg(short = '6', long)]
        ipv6: bool,
    },
    #[command(about = "System or direct DNS / 域名解析")]
    Dns {
        target: String,
        record_type: Option<String>,
        #[arg(short = 's', long)]
        server: Option<String>,
        #[arg(long)]
        system: bool,
        #[arg(long, default_value_t = 2000)]
        timeout_ms: u64,
        #[arg(short = '6', long)]
        ipv6: bool,
    },
    #[command(
        about = "NTP measurement or temporary server / 时间偏差测量或临时时间服务",
        args_conflicts_with_subcommands = true,
        subcommand_precedence_over_arg = true,
        after_help = "Examples: ntp 192.168.1.10 -c 4\n          ntp serve --listen 0.0.0.0 --port 1123"
    )]
    Ntp {
        target: Option<String>,
        #[arg(short = 'c', long, default_value_t = 4)]
        count: u32,
        #[arg(long, default_value_t = 123)]
        port: u16,
        #[arg(long, default_value_t = 2000)]
        timeout_ms: u64,
        #[arg(long, default_value_t = 2000)]
        interval_ms: u64,
        #[arg(short = '6', long)]
        ipv6: bool,
        #[command(subcommand)]
        command: Option<Ntp>,
    },
    #[command(
        subcommand,
        about = "FTP passive client/server / FTP 文件传输与临时服务"
    )]
    Ftp(Ftp),
    #[command(
        subcommand,
        about = "Native iperf3 TCP/UDP client/server / 内置网络测速，无需安装 iperf3"
    )]
    Iperf3(Iperf3),
    #[command(subcommand, about = "TFTP octet client/server / 文件传输")]
    Tftp(Tftp),
    #[command(subcommand, about = "Syslog UDP client/server / 临时日志")]
    Syslog(Syslog),
    #[command(about = "Local environment; no network probes / 本机检查")]
    Doctor,
}
#[derive(Subcommand)]
enum Ntp {
    #[command(about = "Serve the local system clock over UDP / 使用本机系统时间提供 UDP 服务")]
    Serve {
        #[arg(long, default_value = "0.0.0.0")]
        listen: String,
        #[arg(long, default_value_t = 123)]
        port: u16,
        /// Comma-separated source CIDRs; blank accepts all sources / 来源网段
        #[arg(long, default_value = "")]
        allow: String,
        /// Advertised local-clock stratum; no upstream synchronization / 本地时钟层级
        #[arg(long, default_value_t = 10, value_parser = clap::value_parser!(u8).range(1..=15))]
        stratum: u8,
        /// Stop after N seconds; 0 runs until Ctrl+C / 运行秒数
        #[arg(long, default_value_t = 0)]
        duration: u64,
    },
}
#[derive(Subcommand)]
enum Iperf3 {
    #[command(about = "Measure TCP/UDP throughput / 测量网络吞吐量")]
    Client {
        target: String,
        #[arg(long, default_value_t = 5201)]
        port: u16,
        #[arg(long, default_value_t = 10)]
        duration: u64,
        #[arg(short = 'P', long, default_value_t = 1)]
        parallel: u8,
        #[arg(short = 'R', long)]
        reverse: bool,
        #[arg(short = 'u', long)]
        udp: bool,
        /// Target rate per stream in Mbps; TCP 0 = unlimited / 每流目标速率
        #[arg(long, default_value_t = 0)]
        bitrate_mbps: u32,
        #[arg(long, default_value_t = 5000)]
        timeout_ms: u64,
        #[arg(short = '6', long)]
        ipv6: bool,
    },
    #[command(about = "Serve iperf3 tests; one test at a time / 提供测速服务，每次一个测试")]
    Serve {
        #[arg(long, default_value = "0.0.0.0")]
        listen: String,
        #[arg(long, default_value_t = 5201)]
        port: u16,
        #[arg(long, default_value = "")]
        allow: String,
        /// 0 runs until Ctrl+C / 0 表示持续运行
        #[arg(long, default_value_t = 0)]
        duration: u64,
    },
}
#[derive(Args)]
struct FtpConnection {
    target: String,
    #[arg(long, default_value_t = 21)]
    port: u16,
    #[arg(long, default_value = "anonymous")]
    user: String,
    /// Password variable (legacy NETBOX_FTP_PASSWORD fallback for default only)
    #[arg(long, default_value = "HOPKIT_FTP_PASSWORD")]
    password_env: String,
    #[arg(long, default_value_t = 5000)]
    timeout_ms: u64,
    #[arg(short = '6', long)]
    ipv6: bool,
}
#[derive(Subcommand)]
enum Ftp {
    List {
        #[command(flatten)]
        connection: FtpConnection,
        #[arg(default_value = "")]
        remote_path: String,
    },
    Get {
        #[command(flatten)]
        connection: FtpConnection,
        remote_name: String,
        #[arg(short = 'o', long)]
        output: String,
        #[arg(long)]
        overwrite: bool,
    },
    Put {
        #[command(flatten)]
        connection: FtpConnection,
        path: String,
        #[arg(long)]
        remote_name: Option<String>,
    },
    Serve {
        #[arg(long)]
        root: String,
        #[arg(long, default_value = "0.0.0.0")]
        listen: String,
        #[arg(long, default_value_t = 21)]
        port: u16,
        #[arg(long, default_value = "")]
        allow: String,
        #[arg(long, default_value = "anonymous")]
        user: String,
        /// Password variable (legacy NETBOX_FTP_PASSWORD fallback for default only)
        #[arg(long, default_value = "HOPKIT_FTP_PASSWORD")]
        password_env: String,
        #[arg(long)]
        write: bool,
        #[arg(long, default_value_t = 0)]
        duration: u64,
        #[arg(long, default_value_t = 5000)]
        timeout_ms: u64,
    },
}
fn ftp_password(name: &str) -> Result<String, ToolError> {
    let value = std::env::var(name).or_else(|error| {
        if name == "HOPKIT_FTP_PASSWORD" && error == std::env::VarError::NotPresent {
            std::env::var("NETBOX_FTP_PASSWORD")
        } else {
            Err(error)
        }
    });
    match value {
        Ok(value) => Ok(value),
        Err(std::env::VarError::NotPresent) => Ok(String::new()),
        Err(_) => Err(ToolError::new(
            "INVALID_ARGUMENT",
            "FTP password environment variable must contain Unicode text",
        )),
    }
}
fn ftp_connection(r: &mut Request, connection: FtpConnection) -> Result<(), ToolError> {
    r.target = connection.target;
    r.port = connection.port;
    r.ftp_username = connection.user;
    r.ftp_password = ftp_password(&connection.password_env)?;
    r.timeout_ms = connection.timeout_ms;
    r.ipv6 = connection.ipv6;
    Ok(())
}
#[derive(Subcommand)]
enum Tftp {
    Get {
        target: String,
        remote_name: String,
        #[arg(short = 'o', long)]
        output: String,
        #[arg(long, default_value_t = 69)]
        port: u16,
        #[arg(long)]
        overwrite: bool,
    },
    Put {
        target: String,
        path: String,
        #[arg(long)]
        remote_name: Option<String>,
        #[arg(long, default_value_t = 69)]
        port: u16,
    },
    Serve {
        #[arg(long)]
        root: String,
        #[arg(long, default_value = "0.0.0.0")]
        listen: String,
        #[arg(long, default_value_t = 69)]
        port: u16,
        #[arg(long, default_value = "")]
        allow: String,
        #[arg(long)]
        write: bool,
        #[arg(long, default_value_t = 0)]
        duration: u64,
    },
}
#[derive(Subcommand)]
enum Syslog {
    Listen {
        #[arg(long, default_value = "0.0.0.0")]
        listen: String,
        #[arg(long, default_value_t = 514)]
        port: u16,
        #[arg(long, default_value = "")]
        allow: String,
        #[arg(long)]
        dir: String,
        #[arg(long, default_value_t = 0)]
        duration: u64,
    },
    Send {
        target: String,
        message: String,
        #[arg(long, default_value_t = 514)]
        port: u16,
    },
}
fn request(cmd: Command) -> Result<Request, ToolError> {
    let mut r = Request::default();
    match cmd {
        Command::Monitor {
            targets,
            interval_ms,
            timeout_ms,
            duration,
            log_directory,
        } => {
            let targets = targets
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    let parts: Vec<_> = s.split(',').collect();
                    if !matches!(parts.as_slice(), ["ping", _] | ["tcping", _, _]) {
                        return Err(ToolError::new(
                            "INVALID_ARGUMENT",
                            "Use --target ping,HOST or --target tcping,HOST,PORT",
                        ));
                    }
                    Ok(hopkit_core::monitor::Target {
                        label: format!("Target {}", i + 1),
                        protocol: parts[0].into(),
                        target: parts[1].into(),
                        port: if parts.len() == 3 {
                            parts[2].parse().map_err(|_| {
                                ToolError::new("INVALID_ARGUMENT", "Invalid TCP port")
                            })?
                        } else {
                            0
                        },
                    })
                })
                .collect::<Result<Vec<_>, ToolError>>()?;
            let config = hopkit_core::monitor::Config {
                targets,
                interval_ms,
                timeout_ms,
                duration_secs: duration,
                log_directory,
            };
            config.validate()?;
            r.tool = "monitor".into();
            r.target = "".into();
            r.monitor = Some(config);
        }
        Command::Check {
            target,
            timeout_ms,
            source,
            dns_server,
            insecure_tls,
            ca_file,
            trace,
            ipv6,
        } => {
            r.port = hopkit_core::target::parse(&target)?.port.unwrap_or(0);
            r.tool = "check".into();
            r.target = target;
            r.timeout_ms = timeout_ms;
            r.source_address = source;
            r.dns_server = dns_server;
            r.insecure_tls = insecure_tls;
            r.tls_ca_file = ca_file;
            r.include_trace = trace;
            r.ipv6 = ipv6;
        }
        Command::Network => r.tool = "network".into(),
        Command::Target { input } => {
            r.tool = "target".into();
            r.target = input;
        }
        Command::Ping(p) => {
            r.target = p.target;
            r.payload_size_bytes = p.size;
            r.count = if p.continuous { 0 } else { p.count };
            r.timeout_ms = p.timeout_ms;
            r.interval_ms = p.interval_ms;
            r.ipv6 = p.ipv6;
        }
        Command::Trace {
            target,
            max_hops,
            timeout_ms,
            ipv6,
        } => {
            r.tool = "trace".into();
            r.target = target;
            r.max_hops = max_hops;
            r.timeout_ms = timeout_ms;
            r.ipv6 = ipv6;
        }
        Command::Tcping {
            target,
            port,
            count,
            continuous,
            timeout_ms,
            interval_ms,
            ipv6,
        } => {
            r.tool = "tcping".into();
            r.target = target;
            r.port = port;
            r.count = if continuous { 0 } else { count };
            r.timeout_ms = timeout_ms;
            r.interval_ms = interval_ms;
            r.ipv6 = ipv6;
        }
        Command::Dns {
            target,
            record_type,
            server,
            system,
            timeout_ms,
            ipv6,
        } => {
            if system && (record_type.is_some() || server.is_some())
                || record_type.is_some() && server.is_none()
            {
                return Err(ToolError::new(
                    "INVALID_ARGUMENT",
                    "RR/server and --system are mutually exclusive; RR queries require -s",
                ));
            }
            r.tool = "dns".into();
            r.target = target;
            r.record_type = record_type.unwrap_or("A".into());
            r.dns_server = server.unwrap_or_default();
            r.timeout_ms = timeout_ms;
            r.ipv6 = ipv6;
        }
        Command::Ntp {
            target,
            count,
            port,
            timeout_ms,
            interval_ms,
            ipv6,
            command,
        } => {
            r.tool = "ntp".into();
            if let Some(Ntp::Serve {
                listen,
                port,
                allow,
                stratum,
                duration,
            }) = command
            {
                r.action = "serve".into();
                r.listen = listen;
                r.port = port;
                r.allow = allow;
                r.ntp_stratum = stratum;
                r.duration_secs = duration;
            } else {
                r.action = "query".into();
                if target.as_deref() == Some("serve") {
                    return Err(ToolError::new(
                        "INVALID_ARGUMENT",
                        "Place server options after ntp serve; client options cannot be combined with serve",
                    ));
                }
                r.target = target.ok_or_else(|| {
                    ToolError::new(
                        "INVALID_ARGUMENT",
                        "Specify an NTP target or use ntp serve / 请指定 NTP 目标或使用 ntp serve",
                    )
                })?;
                r.count = count;
                r.port = port;
                r.timeout_ms = timeout_ms;
                r.interval_ms = interval_ms;
                r.ipv6 = ipv6;
            }
        }
        Command::Ftp(ftp) => {
            r.tool = "ftp".into();
            match ftp {
                Ftp::List {
                    connection,
                    remote_path,
                } => {
                    r.action = "list".into();
                    r.remote_name = remote_path;
                    ftp_connection(&mut r, connection)?;
                }
                Ftp::Get {
                    connection,
                    remote_name,
                    output,
                    overwrite,
                } => {
                    r.action = "get".into();
                    r.remote_name = remote_name;
                    r.path = output;
                    r.overwrite = overwrite;
                    ftp_connection(&mut r, connection)?;
                }
                Ftp::Put {
                    connection,
                    path,
                    remote_name,
                } => {
                    r.action = "put".into();
                    r.path = path;
                    r.remote_name = remote_name.unwrap_or_default();
                    ftp_connection(&mut r, connection)?;
                }
                Ftp::Serve {
                    root,
                    listen,
                    port,
                    allow,
                    user,
                    password_env,
                    write,
                    duration,
                    timeout_ms,
                } => {
                    r.action = "serve".into();
                    r.path = root;
                    r.listen = listen;
                    r.port = port;
                    r.allow = allow;
                    r.ftp_username = user;
                    r.ftp_password = ftp_password(&password_env)?;
                    r.writable = write;
                    r.duration_secs = duration;
                    r.timeout_ms = timeout_ms;
                }
            }
        }
        Command::Tftp(t) => {
            r.tool = "tftp".into();
            match t {
                Tftp::Get {
                    target,
                    remote_name,
                    output,
                    port,
                    overwrite,
                } => {
                    r.action = "get".into();
                    r.target = target;
                    r.remote_name = remote_name;
                    r.path = output;
                    r.port = port;
                    r.overwrite = overwrite;
                }
                Tftp::Put {
                    target,
                    path,
                    remote_name,
                    port,
                } => {
                    r.action = "put".into();
                    r.target = target;
                    r.path = path;
                    r.remote_name = remote_name.unwrap_or_default();
                    r.port = port;
                }
                Tftp::Serve {
                    root,
                    listen,
                    port,
                    allow,
                    write,
                    duration,
                } => {
                    r.action = "serve".into();
                    r.path = root;
                    r.listen = listen;
                    r.port = port;
                    r.allow = allow;
                    r.writable = write;
                    r.duration_secs = duration;
                }
            }
        }
        Command::Syslog(s) => {
            r.tool = "syslog".into();
            match s {
                Syslog::Listen {
                    listen,
                    port,
                    allow,
                    dir,
                    duration,
                } => {
                    r.action = "listen".into();
                    r.listen = listen;
                    r.port = port;
                    r.allow = allow;
                    r.path = dir;
                    r.duration_secs = duration;
                }
                Syslog::Send {
                    target,
                    message,
                    port,
                } => {
                    r.action = "send".into();
                    r.target = target;
                    r.message = message;
                    r.port = port;
                }
            }
        }
        Command::Iperf3(command) => {
            r.tool = "iperf3".into();
            r.timeout_ms = 5000;
            match command {
                Iperf3::Client {
                    target,
                    port,
                    duration,
                    parallel,
                    reverse,
                    udp,
                    bitrate_mbps,
                    timeout_ms,
                    ipv6,
                } => {
                    r.action = "client".into();
                    r.target = target;
                    r.port = port;
                    r.duration_secs = duration;
                    r.iperf_parallel = parallel;
                    r.iperf_reverse = reverse;
                    r.iperf_protocol = if udp { "udp" } else { "tcp" }.into();
                    r.iperf_bitrate_mbps = if udp && bitrate_mbps == 0 {
                        10
                    } else {
                        bitrate_mbps
                    };
                    r.timeout_ms = timeout_ms;
                    r.ipv6 = ipv6;
                }
                Iperf3::Serve {
                    listen,
                    port,
                    allow,
                    duration,
                } => {
                    r.action = "serve".into();
                    r.listen = listen;
                    r.port = port;
                    r.allow = allow;
                    r.duration_secs = duration;
                }
            }
        }
        Command::Doctor => r.tool = "doctor".into(),
    }
    Ok(r)
}
fn exit_code(e: &str) -> i32 {
    match e {
        "CANCELLED" => 130,
        "INVALID_ARGUMENT" => 2,
        "PERMISSION_DENIED" | "ADDRESS_IN_USE" | "DIRECTORY_IN_USE" | "UNSUPPORTED_CAPABILITY" => 3,
        "IO_ERROR" | "STORAGE_FULL" | "INTERNAL_ERROR" | "ALREADY_EXISTS" | "NOT_FOUND" => 4,
        _ => 1,
    }
}
fn main() {
    // clap parse errors still honor --format json/jsonl, keeping stdout machine-readable.
    let cli = match Cli::try_parse() {
        Ok(v) => v,
        Err(e) => {
            let args: Vec<_> = std::env::args().collect();
            let machine = args
                .windows(2)
                .any(|x| x[0] == "--format" && x[1] != "text")
                || args
                    .iter()
                    .any(|x| x == "--format=json" || x == "--format=jsonl");
            if machine && e.use_stderr() {
                let value = envelope(
                    "",
                    "parse",
                    Err(ToolError::new("INVALID_ARGUMENT", e.to_string())),
                );
                let _ = writeln!(io::stdout(), "{value}");
                std::process::exit(2);
            }
            e.exit();
        }
    };
    let Some(command) = cli.command else {
        use clap::CommandFactory;
        let _ = Cli::command().print_help();
        let _ = writeln!(io::stdout());
        return;
    };
    let r = match request(command) {
        Ok(r) => r,
        Err(e) => {
            let code = exit_code(&e.code);
            if cli.format == "text" {
                let _ = writeln!(io::stderr(), "{e}");
            } else {
                let _ = writeln!(io::stdout(), "{}", envelope("", "parse", Err(e)));
            }
            std::process::exit(code);
        }
    };
    let is_service = matches!(r.action.as_str(), "serve" | "listen");
    if cli.format == "json"
        && (is_service && r.duration_secs == 0
            || r.count == 0
            || r.monitor.as_ref().is_some_and(|m| m.duration_secs == 0))
    {
        let _ = writeln!(
            io::stdout(),
            "{}",
            envelope(
                "",
                &r.tool,
                Err(ToolError::new(
                    "INVALID_ARGUMENT",
                    "Continuous operations require --format jsonl or text"
                ))
            )
        );
        std::process::exit(2);
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let handler = cancel.clone();
    ctrlc::set_handler(move || handler.store(true, Ordering::Relaxed))
        .expect("Ctrl+C handler unavailable");
    let failed_output = Arc::new(AtomicBool::new(false));
    let id = uuid_string();
    let format = cli.format.clone();
    let output_cancel = cancel.clone();
    let broken = failed_output.clone();
    let english = cli.lang == "en-US";
    let emit = |mut v: Value| {
        if v["event_type"] == "monitor_snapshot" {
            // Stream only counters; retained timelines are delivered in the final result.
            v = json!({"event_type":"monitor_stats","timestamp_ms":v["ended_at_ms"],"totals":v["totals"],"paused":v["paused"]});
        } else if v["event_type"] == "monitor_finished" {
            return;
        }
        v["task_id"] = json!(id);
        v["schema_version"] = json!(if r.tool == "trace" { 2 } else { 1 });
        let output = if format == "jsonl" {
            Some(v.to_string())
        } else if format == "text" {
            Some(if v["event_type"] == "sample" {
                format!(
                    "#{}  {}  {}  {} ms",
                    v["seq"],
                    v["address"].as_str().unwrap_or(""),
                    v["status"].as_str().unwrap_or(""),
                    v["rtt_ms"]
                )
            } else {
                v.to_string()
            })
        } else {
            None
        };
        if let Some(out) = output
            && writeln!(io::stdout(), "{out}").is_err()
        {
            broken.store(true, Ordering::Relaxed);
            output_cancel.store(true, Ordering::Relaxed);
        }
    };
    let result = run(&r, &cancel, &emit);
    let mut code = match &result {
        Ok(v) => {
            if v["success"] == false
                || cli.strict
                    && (v["lost"].as_u64().unwrap_or(0) > 0
                        || v["timeout_probes"].as_u64().unwrap_or(0) > 0
                        || v["totals"].as_array().is_some_and(|totals| {
                            totals.iter().any(|t| t["failed"].as_u64().unwrap_or(0) > 0)
                        }))
            {
                1
            } else {
                0
            }
        }
        Err(e) => exit_code(&e.code),
    };
    let mut final_value = envelope(&id, &r.tool, result);
    final_value["request"] = serde_json::to_value(&r).expect("Serializable request snapshot");
    let out = if cli.format == "text" {
        format!(
            "{}\n{}",
            if english { "Result" } else { "结果" },
            serde_json::to_string_pretty(&final_value).unwrap()
        )
    } else if cli.format == "jsonl" {
        let mut v = final_value.clone();
        v["event_type"] = json!("finished");
        v.to_string()
    } else {
        final_value.to_string()
    };
    if writeln!(io::stdout(), "{out}").is_err() || failed_output.load(Ordering::Relaxed) {
        code = 4;
    }
    if io::stdout().is_terminal() {
        let _ = io::stdout().flush();
    }
    std::process::exit(code);
}
fn uuid_string() -> String {
    format!(
        "cli-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Request {
        request(Cli::try_parse_from(args).unwrap().command.unwrap()).unwrap()
    }

    #[test]
    fn ntp_query_stays_compatible_and_server_has_its_own_options() {
        let query = parse(&[
            "hopkit-cli",
            "ntp",
            "127.0.0.1",
            "-c",
            "2",
            "--port",
            "1123",
        ]);
        assert_eq!(query.action, "query");
        assert_eq!(query.count, 2);
        assert_eq!(query.port, 1123);
        let server = parse(&[
            "hopkit-cli",
            "ntp",
            "serve",
            "--listen",
            "::1",
            "--port",
            "2123",
            "--allow",
            "::1/128",
            "--stratum",
            "5",
            "--duration",
            "1",
        ]);
        assert_eq!(server.action, "serve");
        assert_eq!(server.listen, "::1");
        assert_eq!(server.port, 2123);
        assert_eq!(server.allow, "::1/128");
        assert_eq!(server.ntp_stratum, 5);
        assert_eq!(server.duration_secs, 1);
        let defaults = parse(&["hopkit-cli", "ntp", "serve"]);
        assert_eq!(defaults.port, 123);
        assert_eq!(defaults.ntp_stratum, 10);
    }

    #[test]
    fn ntp_rejects_missing_target_and_conflicting_or_invalid_arguments() {
        let cli = Cli::try_parse_from(["hopkit-cli", "ntp"]).unwrap();
        assert_eq!(
            request(cli.command.unwrap()).unwrap_err().code,
            "INVALID_ARGUMENT"
        );
        for args in [
            vec!["hopkit-cli", "ntp", "serve", "--stratum", "0"],
            vec!["hopkit-cli", "ntp", "serve", "--stratum", "16"],
            vec!["hopkit-cli", "ntp", "127.0.0.1", "serve"],
            vec!["hopkit-cli", "ntp", "-c", "2", "serve"],
        ] {
            if let Ok(cli) = Cli::try_parse_from(&args) {
                assert!(
                    request(cli.command.unwrap()).is_err(),
                    "unexpectedly accepted: {args:?}"
                );
            }
        }
    }
}
