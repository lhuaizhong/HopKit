use clap::Args;
use hopkit_core::{
    socket_tool::{Config as SocketConfig, Control, SendCommand},
    *,
};
use std::{
    io::{self, BufRead, Read, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
#[derive(Args)]
pub struct Connections {
    #[arg(long, default_value = "")]
    protocol: String,
    #[arg(long, default_value = "auto")]
    family: String,
    #[arg(long)]
    port: Option<u16>,
    #[arg(long)]
    pid: Option<u32>,
    #[arg(long, default_value = "")]
    state: String,
    #[arg(long, default_value = "")]
    address: String,
    #[arg(long, default_value = "")]
    process: String,
    #[arg(long)]
    listening: bool,
}
impl Connections {
    pub fn request(self) -> Request {
        Request {
            tool: "connections".into(),
            action: "query".into(),
            connections: Some(connections::Config {
                protocol: self.protocol,
                family: self.family,
                port: self.port,
                pid: self.pid,
                state: self.state,
                address: self.address,
                process: self.process,
                listening: self.listening,
            }),
            ..Default::default()
        }
    }
}
#[derive(Args)]
pub struct Wol {
    mac: String,
    #[arg(long, alias = "destination", default_value = "255.255.255.255")]
    broadcast: String,
    #[arg(long, default_value_t = 9)]
    port: u16,
    #[arg(long, default_value = "")]
    source: String,
    #[arg(long, default_value_t = 3)]
    count: u32,
    #[arg(long, default_value_t = 200)]
    interval_ms: u64,
}
impl Wol {
    pub fn request(self) -> Request {
        Request {
            tool: "wol".into(),
            action: "send".into(),
            wol: Some(wol::Config {
                mac: self.mac,
                destination: self.broadcast,
                source: self.source,
                port: self.port,
                count: self.count,
                interval_ms: self.interval_ms,
            }),
            ..Default::default()
        }
    }
}
#[derive(Args)]
pub struct Socket {
    #[arg(value_parser=["tcp","udp"])]
    protocol: String,
    #[arg(value_parser=["connect","listen","send"])]
    mode: String,
    target: Option<String>,
    target_port: Option<u16>,
    #[arg(long, default_value = "0.0.0.0")]
    listen: String,
    #[arg(long)]
    port: Option<u16>,
    #[arg(long, default_value = "")]
    source: String,
    #[arg(short = '4', conflicts_with = "ipv6")]
    ipv4: bool,
    #[arg(short = '6', conflicts_with = "ipv4")]
    ipv6: bool,
    #[arg(long, default_value = "")]
    allow: String,
    #[arg(long, default_value_t = 1000)]
    timeout_ms: u64,
    /// 0 runs until Ctrl+C; use a finite duration for JSON.
    #[arg(long, default_value_t = 0)]
    duration: u64,
    #[arg(long, default_value_t = 16)]
    max_sessions: usize,
    #[arg(long, default_value_t = 1048576)]
    preview_bytes: usize,
    #[arg(long)]
    echo: bool,
    #[arg(long, default_value_t = 10)]
    rate: u32,
    /// Explicit, bounded payload recording, create-new only.
    #[arg(long, default_value = "")]
    record_path: String,
    #[arg(long, default_value_t = 16777216)]
    record_limit_bytes: u64,
    /// One initial HEX payload. For arbitrary bodies use stdin instead.
    #[arg(long)]
    hex: Option<String>,
    /// Input and output are separate streams; JSONL stdin accepts SendCommand objects.
    #[arg(long,value_parser=["none","text","hex","raw","jsonl"],default_value="none")]
    input: String,
    #[arg(long, default_value_t = 1)]
    session: u64,
}
impl Socket {
    pub fn execute(self, format: &str, english: bool) -> i32 {
        let task_id = crate::uuid_string();
        let config = SocketConfig {
            protocol: self.protocol,
            mode: self.mode,
            target: self.target.unwrap_or_default(),
            listen: self.listen,
            port: self.port.or(self.target_port).unwrap_or(9000),
            source: self.source,
            family: if self.ipv4 {
                "ipv4"
            } else if self.ipv6 {
                "ipv6"
            } else {
                "auto"
            }
            .into(),
            allow: self.allow,
            timeout_ms: self.timeout_ms,
            duration_secs: self.duration,
            max_sessions: self.max_sessions,
            preview_bytes: self.preview_bytes,
            echo: self.echo,
            rate: self.rate,
            record_path: self.record_path,
            record_limit_bytes: self.record_limit_bytes,
        };
        let make = |data: String, encoding: &str| SendCommand {
            operation: "send".into(),
            session_id: if config.protocol == "tcp" {
                Some(self.session)
            } else {
                None
            },
            destination: None,
            encoding: encoding.into(),
            data,
            newline: String::new(),
            count: 1,
            interval_ms: 1000,
        };
        let initial = self.hex.map(|data| make(data, "hex"));
        let validation = socket_tool::validate(&config)
            .and_then(|_| {
                if format == "json" && config.duration_secs == 0 {
                    Err(ToolError::new(
                        "INVALID_ARGUMENT",
                        "JSON requires finite --duration",
                    ))
                } else {
                    Ok(())
                }
            })
            .and_then(|_| {
                initial
                    .as_ref()
                    .map(socket_tool::decode)
                    .transpose()
                    .map(|_| ())
            });
        if let Err(e) = validation {
            println!("{}", envelope(&task_id, "socket", Err(e)));
            return 2;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let handler = cancel.clone();
        let _ = ctrlc::set_handler(move || handler.store(true, Ordering::Relaxed));
        let control = Arc::new(Control::default());
        let input_control = control.clone();
        let input_cancel = cancel.clone();
        let input_error = Arc::new(std::sync::Mutex::new(None::<ToolError>));
        let errors = input_error.clone();
        let tcp = config.protocol == "tcp";
        let input = self.input;
        let session = self.session;
        std::thread::spawn(move || {
            let result = (|| -> Result<(), ToolError> {
                let start = Instant::now();
                while input_control.snapshot(false)["active"] != true {
                    if input_cancel.load(Ordering::Relaxed)
                        || start.elapsed() > Duration::from_secs(15)
                    {
                        return Ok(());
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                let send = |cmd: SendCommand| -> Result<(), ToolError> {
                    socket_tool::decode(&cmd)?;
                    let began = Instant::now();
                    loop {
                        match input_control.send(cmd.clone()) {
                            Ok(()) => return Ok(()),
                            Err(e)
                                if e.code == "INVALID_ARGUMENT"
                                    && tcp
                                    && began.elapsed() < Duration::from_secs(10) =>
                            {
                                if input_cancel.load(Ordering::Relaxed) {
                                    return Ok(());
                                }
                                std::thread::sleep(Duration::from_millis(10));
                            }
                            Err(e) => return Err(e),
                        }
                    }
                };
                if let Some(cmd) = initial {
                    send(cmd)?;
                }
                let make = |data, encoding: &str| SendCommand {
                    operation: "send".into(),
                    session_id: tcp.then_some(session),
                    destination: None,
                    encoding: encoding.into(),
                    data,
                    newline: String::new(),
                    count: 1,
                    interval_ms: 1000,
                };
                if input == "none" {
                    return Ok(());
                }
                if input == "raw" {
                    let mut bytes = Vec::new();
                    io::stdin().take(65537).read_to_end(&mut bytes)?;
                    if bytes.len() > 65536 {
                        return Err(ToolError::new(
                            "INVALID_ARGUMENT",
                            "Raw stdin exceeds 64 KiB",
                        ));
                    }
                    return send(make(
                        bytes.iter().map(|b| format!("{b:02x}")).collect(),
                        "hex",
                    ));
                }
                let mut reader = io::stdin().lock();
                loop {
                    let mut bytes = Vec::new();
                    let n = reader.by_ref().take(196610).read_until(b'\n', &mut bytes)?;
                    if n == 0 {
                        break;
                    }
                    if n > 196609 {
                        return Err(ToolError::new(
                            "INVALID_ARGUMENT",
                            "stdin control line exceeds budget",
                        ));
                    }
                    let text = String::from_utf8(bytes)
                        .map_err(|_| ToolError::new("INVALID_ARGUMENT", "stdin is not UTF-8"))?;
                    let cmd = if input == "jsonl" {
                        serde_json::from_str(&text).map_err(|_| {
                            ToolError::new("INVALID_ARGUMENT", "Invalid JSONL control")
                        })?
                    } else {
                        make(
                            text.trim_end_matches(['\r', '\n']).into(),
                            if input == "hex" { "hex" } else { "utf8" },
                        )
                    };
                    send(cmd)?;
                }
                Ok(())
            })();
            if let Err(e) = result {
                *errors.lock().unwrap() = Some(e);
                input_cancel.store(true, Ordering::Relaxed);
            }
        });
        let output_failed = AtomicBool::new(false);
        let printed_endpoint = AtomicBool::new(false);
        let emit = |mut value: serde_json::Value| {
            value["task_id"] = serde_json::json!(task_id);
            value["schema_version"] = serde_json::json!(1);
            let text = if format == "jsonl" {
                Some(value.to_string())
            } else if format == "text" && value["event_type"] == "socket_record" {
                Some(format!(
                    "{} session={} {} → {} bytes={} HEX={}",
                    value["record"]["kind"],
                    value["record"]["session_id"],
                    value["record"]["local_endpoint"],
                    value["record"]["remote_endpoint"],
                    value["record"]["bytes"],
                    value["record"]["hex"]
                ))
            } else if format == "text"
                && value["event_type"] == "socket_stats"
                && !printed_endpoint.swap(true, Ordering::Relaxed)
            {
                Some(format!(
                    "{} {} → {}",
                    if english { "Channel" } else { "通道" },
                    value["snapshot"]["local_endpoint"],
                    value["snapshot"]["remote_endpoint"]
                ))
            } else {
                None
            };
            if let Some(text) = text
                && writeln!(io::stdout(), "{text}").is_err()
            {
                output_failed.store(true, Ordering::Relaxed);
                cancel.store(true, Ordering::Relaxed);
            }
        };
        let result = socket_tool::run(&config, &cancel, &emit, &control);
        let result = if let Some(mut e) = input_error.lock().unwrap().take() {
            e.data = result.ok().or_else(|| Some(control.snapshot(false)));
            Err(e)
        } else {
            result
        };
        let code = match &result {
            Ok(_) => 0,
            Err(e) => crate::exit_code(&e.code),
        };
        let mut value = envelope(&task_id, "socket", result);
        value["request"] = Request {
            tool: "socket".into(),
            action: config.mode.clone(),
            target: config.target.clone(),
            listen: config.listen.clone(),
            port: config.port,
            socket: Some(config),
            ..Default::default()
        }
        .snapshot()
        .expect("Serializable socket request snapshot");
        value["event_type"] = serde_json::json!("finished");
        let output = if format == "text" {
            format!(
                "{} · {}: {} B · {}: {} B · {}: {}{}",
                value["status"],
                if english { "Sent" } else { "已发送" },
                value["data"]["stats"]["sent_bytes"],
                if english { "Received" } else { "已接收" },
                value["data"]["stats"]["received_bytes"],
                if english {
                    "Send errors"
                } else {
                    "发送错误"
                },
                value["data"]["stats"]["send_errors"],
                if value["error"].is_null() {
                    String::new()
                } else {
                    format!(" · {}", value["error"])
                }
            )
        } else {
            value.to_string()
        };
        if writeln!(io::stdout(), "{output}").is_err() || output_failed.load(Ordering::Relaxed) {
            4
        } else {
            code
        }
    }
}
