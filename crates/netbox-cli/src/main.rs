use clap::{Args, Parser, Subcommand};
use netbox_core::*;
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
    name = "netbox-cli",
    version,
    about = "netbox Network Toolkit / 网络工具箱 · Windows / macOS preview",
    long_about = "Windows / macOS preview: Ping / Trace / TCPing / DNS / NTP / TFTP / Syslog UDP.\nGUI is never launched by CLI. Ctrl+C cancels the current operation.\n命令行独立运行；Ctrl+C 取消。Run help <command> for arguments."
)]
struct Cli {
    #[arg(long,global=true,default_value="text",value_parser=["text","json","jsonl"])]
    format: String,
    #[arg(long,global=true,default_value="zh-CN",value_parser=["zh-CN","en-US"])]
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
    #[command(about = "NTP measurement; does not set time / 时间偏差测量")]
    Ntp {
        target: String,
        #[arg(short = 'c', long, default_value_t = 4)]
        count: u32,
        #[arg(long, default_value_t = 123)]
        port: u16,
        #[arg(long, default_value_t = 2000)]
        timeout_ms: u64,
        #[arg(long, default_value_t = 2000)]
        interval_ms: u64,
    },
    #[command(subcommand, about = "TFTP octet client/server / 文件传输")]
    Tftp(Tftp),
    #[command(subcommand, about = "Syslog UDP client/server / 临时日志")]
    Syslog(Syslog),
    #[command(about = "Local environment; no network probes / 本机检查")]
    Doctor,
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
        #[arg(long, default_value = "127.0.0.1")]
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
        #[arg(long, default_value = "127.0.0.1")]
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
        } => {
            r.tool = "ntp".into();
            r.target = target;
            r.count = count;
            r.port = port;
            r.timeout_ms = timeout_ms;
            r.interval_ms = interval_ms;
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
    if cli.format == "json" && (is_service && r.duration_secs == 0 || r.count == 0) {
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
        v["task_id"] = json!(id);
        v["schema_version"] = json!(1);
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
            if v["success"] == false || cli.strict && v["lost"].as_u64().unwrap_or(0) > 0 {
                1
            } else {
                0
            }
        }
        Err(e) => exit_code(&e.code),
    };
    let final_value = envelope(&id, &r.tool, result);
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
