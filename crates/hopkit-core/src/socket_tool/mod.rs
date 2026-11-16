//! A bounded, single-owner socket loop. Control objects never own live OS sockets.
use crate::runtime::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    io::{Read, Write},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream, UdpSocket},
    sync::Mutex,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub protocol: String,
    pub mode: String,
    pub target: String,
    pub listen: String,
    pub port: u16,
    pub source: String,
    pub family: String,
    pub allow: String,
    pub timeout_ms: u64,
    pub duration_secs: u64,
    pub max_sessions: usize,
    pub preview_bytes: usize,
    pub echo: bool,
    pub rate: u32,
    pub record_path: String,
    pub record_limit_bytes: u64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            protocol: "tcp".into(),
            mode: "connect".into(),
            target: "127.0.0.1".into(),
            listen: "0.0.0.0".into(),
            port: 9000,
            source: String::new(),
            family: "auto".into(),
            allow: String::new(),
            timeout_ms: 1000,
            duration_secs: 0,
            max_sessions: 16,
            preview_bytes: 1024 * 1024,
            echo: false,
            rate: 10,
            record_path: String::new(),
            record_limit_bytes: 16 * 1024 * 1024,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SendCommand {
    #[serde(default = "send_operation")]
    pub operation: String,
    pub session_id: Option<u64>,
    pub destination: Option<String>,
    pub encoding: String,
    pub data: String,
    #[serde(default)]
    pub newline: String,
    #[serde(default = "one")]
    pub count: u32,
    #[serde(default = "interval")]
    pub interval_ms: u64,
}
fn one() -> u32 {
    1
}
fn interval() -> u64 {
    1000
}
fn send_operation() -> String {
    "send".into()
}
pub fn decode(command: &SendCommand) -> Result<Vec<u8>, ToolError> {
    let bad = || {
        ToolError::new(
            "INVALID_ARGUMENT",
            "Send: UTF-8 text or even HEX; <=64 KiB; count 1..1000, interval 10..60000 ms / 发送格式或预算无效",
        )
    };
    if command.data.len() > 196608
        || !(1..=1000).contains(&command.count)
        || !(10..=60000).contains(&command.interval_ms)
    {
        return Err(bad());
    }
    let mut bytes = match command.encoding.as_str() {
        "utf8" => command.data.as_bytes().to_vec(),
        "hex" => {
            let hex: String = command
                .data
                .chars()
                .filter(|c| !c.is_ascii_whitespace())
                .collect();
            if !hex.is_ascii() || !hex.len().is_multiple_of(2) {
                return Err(bad());
            }
            (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| bad()))
                .collect::<Result<Vec<_>, _>>()?
        }
        _ => return Err(bad()),
    };
    bytes.extend_from_slice(match command.newline.as_str() {
        "" | "none" => b"",
        "lf" => b"\n",
        "crlf" => b"\r\n",
        _ => return Err(bad()),
    });
    if bytes.len() > 65536 {
        return Err(bad());
    }
    Ok(bytes)
}
pub fn validate(config: &Config) -> Result<(), ToolError> {
    if !matches!(
        (config.protocol.as_str(), config.mode.as_str()),
        ("tcp", "connect" | "listen") | ("udp", "send" | "listen")
    ) || config.port == 0
        || !(100..=10000).contains(&config.timeout_ms)
        || !(1..=16).contains(&config.max_sessions)
        || !(1024..=1024 * 1024).contains(&config.preview_bytes)
        || !(1..=100).contains(&config.rate)
        || !(1024..=1024 * 1024 * 1024).contains(&config.record_limit_bytes)
        || config.duration_secs > 86400
        || config.target.len() > 253
        || config.listen.len() > 128
        || config.source.len() > 128
        || config.record_path.len() > 4096
        || config.mode == "listen" && !config.source.is_empty()
        || !matches!(config.family.as_str(), "auto" | "ipv4" | "ipv6")
        || config.echo && (config.mode != "listen" || config.allow.is_empty())
    {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "Invalid socket mode/budget; echo requires explicit source CIDRs / 收发模式或预算无效；回显须限制来源",
        ));
    }
    parse_allow(&config.allow)?;
    if config.mode == "listen" {
        endpoint(&config.listen, config.port)?;
    }
    if !config.source.is_empty() {
        endpoint(&config.source, 0)?;
    }
    Ok(())
}
fn endpoint(host: &str, port: u16) -> Result<SocketAddr, ToolError> {
    format!("[{host}]:{port}")
        .parse()
        .or_else(|_| format!("{host}:{port}").parse())
        .map_err(|_| {
            ToolError::new(
                "INVALID_ARGUMENT",
                "Numeric IP (optional IPv6 %scope index) required / 须为 IP 地址",
            )
        })
}
fn parse_allow(text: &str) -> Result<Vec<ipnet::IpNet>, ToolError> {
    if text.len() > 4096 {
        return Err(ToolError::new("INVALID_ARGUMENT", "Source list too large"));
    }
    text.split([',', ' ', '\n'])
        .filter(|v| !v.is_empty())
        .map(|v| {
            v.parse()
                .map_err(|_| ToolError::new("INVALID_ARGUMENT", "Invalid source CIDR"))
        })
        .collect()
}
#[derive(Default)]
struct State {
    pending_count: usize,
    active: bool,
    udp: bool,
    mode: String,
    sessions: BTreeMap<u64, Value>,
    queue: VecDeque<SendCommand>,
    preview: VecDeque<Value>,
    preview_size: usize,
    snapshot: Value,
}
#[derive(Default)]
pub struct Control(Mutex<State>);
impl Control {
    pub fn send(&self, command: SendCommand) -> Result<(), ToolError> {
        if !matches!(
            command.operation.as_str(),
            "send" | "shutdown_write" | "close"
        ) {
            return Err(ToolError::new(
                "INVALID_ARGUMENT",
                "Unknown socket control operation",
            ));
        }
        let bytes = decode(&command)?;
        let mut state = self
            .0
            .lock()
            .map_err(|_| ToolError::new("INTERNAL_ERROR", "Socket control unavailable"))?;
        if !state.active {
            return Err(ToolError::new(
                "TASK_STOPPED",
                "Socket task is not active / 收发任务已停止",
            ));
        }
        if state.udp {
            if command.operation != "send" {
                return Err(ToolError::new("INVALID_ARGUMENT", "UDP supports send only"));
            }
            if bytes.len() > 65507 {
                return Err(ToolError::new(
                    "INVALID_ARGUMENT",
                    "UDP payload maximum is 65507 bytes",
                ));
            }
            if state.mode == "listen"
                && command
                    .destination
                    .as_ref()
                    .is_none_or(|s| s.parse::<SocketAddr>().is_err())
            {
                return Err(ToolError::new(
                    "INVALID_ARGUMENT",
                    "UDP listener sends require an explicit numeric destination endpoint",
                ));
            }
            if command.session_id.is_some() {
                return Err(ToolError::new("INVALID_ARGUMENT", "UDP has no TCP session"));
            }
            if state.mode == "send" && command.destination.is_some() {
                return Err(ToolError::new(
                    "INVALID_ARGUMENT",
                    "UDP client destination is fixed at start",
                ));
            }
        } else if command.destination.is_some()
            || command
                .session_id
                .and_then(|id| state.sessions.get(&id))
                .is_none_or(|s| {
                    command.operation != "close" && s["write_closed"] == true || s["closed"] == true
                })
        {
            return Err(ToolError::new(
                "INVALID_ARGUMENT",
                "Select an active TCP session / 请选择活动 TCP 会话",
            ));
        }
        if state.queue.len() + state.pending_count >= 32 {
            return Err(ToolError::new(
                "RESOURCE_EXHAUSTED",
                "Send queue full (32 schedules) / 发送队列已满",
            ));
        }
        state.queue.push_back(command);
        Ok(())
    }
    pub fn snapshot(&self, include_payload: bool) -> Value {
        let state = self.0.lock().unwrap();
        let mut value = state.snapshot.clone();
        if !value.is_object() {
            value = json!({});
        }
        value["active"] = json!(state.active);
        value["sessions"] = json!(state.sessions.values().collect::<Vec<_>>());
        if include_payload {
            value["records"] = json!(state.preview);
        }
        value
    }
}
struct ActiveGuard<'a>(&'a Control);
impl Drop for ActiveGuard<'_> {
    fn drop(&mut self) {
        let mut s = self.0.0.lock().unwrap();
        s.active = false;
        s.queue.clear();
        for row in s.sessions.values_mut() {
            row["closed"] = json!(true);
        }
    }
}
struct Session {
    stream: TcpStream,
    remote: SocketAddr,
    local: SocketAddr,
    read_closed: bool,
    write_closed: bool,
}
struct Pending {
    command: SendCommand,
    bytes: Vec<u8>,
    offset: usize,
    next: Instant,
    deadline: Instant,
}
#[derive(Default, Serialize)]
struct Stats {
    sent_bytes: u64,
    received_bytes: u64,
    datagrams_sent: u64,
    datagrams_received: u64,
    sessions_total: u64,
    send_errors: u64,
    preview_dropped: u64,
    rejected_sources: u64,
    rejected_sessions: u64,
    records: u64,
    recorded_bytes: u64,
    recording_stopped: bool,
    last_error: Option<ToolError>,
}
struct Recorder<'a> {
    control: &'a Control,
    config: &'a Config,
    emit: &'a Sink<'a>,
    stats: Stats,
    file: Option<std::fs::File>,
    began: Instant,
}
impl Recorder<'_> {
    fn record(
        &mut self,
        kind: &str,
        id: Option<u64>,
        local: SocketAddr,
        remote: SocketAddr,
        bytes: &[u8],
    ) {
        self.stats.records += 1;
        let mut row = json!({"seq":self.stats.records,"elapsed_ms":self.began.elapsed().as_millis(),"observed_at":chrono::Utc::now().to_rfc3339(),"kind":kind,"session_id":id,"local_endpoint":local.to_string(),"remote_endpoint":remote.to_string(),"bytes":bytes.len()});
        row["hex"] = json!(bytes.iter().map(|b| format!("{b:02x}")).collect::<String>());
        row["text"] = json!(String::from_utf8_lossy(bytes));
        row["utf8_valid"] = json!(std::str::from_utf8(bytes).is_ok());
        (self.emit)(json!({"event_type":"socket_record","record":row}));
        if let Some(file) = self.file.as_mut() {
            let mut line = row.to_string();
            line.push('\n');
            if self.stats.recorded_bytes + line.len() as u64 > self.config.record_limit_bytes
                || file.write_all(line.as_bytes()).is_err()
            {
                self.stats.recording_stopped = true;
                self.file = None;
            } else {
                self.stats.recorded_bytes += line.len() as u64;
            }
        }
        let size = row.to_string().len();
        let mut state = self.control.0.lock().unwrap();
        if size > self.config.preview_bytes {
            self.stats.preview_dropped += 1;
            return;
        }
        while state.preview_size + size > self.config.preview_bytes || state.preview.len() >= 1000 {
            if let Some(old) = state.preview.pop_front() {
                state.preview_size -= old.to_string().len();
                self.stats.preview_dropped += 1;
            } else {
                break;
            }
        }
        state.preview_size += size;
        state.preview.push_back(row);
    }
}
pub fn run(config: &Config, c: &Cancel, emit: &Sink<'_>, control: &Control) -> ToolResult {
    validate(config)?;
    cancelled(c)?;
    let family = match config.family.as_str() {
        "ipv4" => AddressFamily::Ipv4,
        "ipv6" => AddressFamily::Ipv6,
        _ => AddressFamily::Auto,
    };
    let allowed = parse_allow(&config.allow)?;
    let permits = |address: SocketAddr| {
        allowed.is_empty() || allowed.iter().any(|n| n.contains(&address.ip()))
    };
    let remote = if config.mode == "listen" {
        None
    } else {
        Some(if let Ok(addr) = endpoint(&config.target, config.port) {
            family.validate(addr.ip())?;
            addr
        } else {
            resolve(&config.target, config.port, family, c)?
        })
    };
    let mut listener = None;
    let mut udp: Option<UdpSocket> = None;
    let mut sessions = BTreeMap::new();
    let local;
    if config.protocol == "tcp" {
        if let Some(remote) = remote {
            let stream = crate::transport::connect(
                remote,
                &config.source,
                c,
                Instant::now() + Duration::from_millis(config.timeout_ms),
            )?;
            stream.set_nonblocking(true)?;
            local = stream.local_addr()?;
            sessions.insert(
                1,
                Session {
                    stream,
                    remote,
                    local,
                    read_closed: false,
                    write_closed: false,
                },
            );
        } else {
            let addr = endpoint(&config.listen, config.port)?;
            family.validate(addr.ip())?;
            let socket = socket2::Socket::new(
                socket2::Domain::for_address(addr),
                socket2::Type::STREAM,
                Some(socket2::Protocol::TCP),
            )?;
            if addr.is_ipv6() {
                socket.set_only_v6(true)?;
            }
            socket.bind(&addr.into())?;
            socket.listen(16)?;
            socket.set_nonblocking(true)?;
            let socket: TcpListener = socket.into();
            local = socket.local_addr()?;
            listener = Some(socket);
        }
    } else {
        let addr = if config.mode == "listen" {
            endpoint(&config.listen, config.port)?
        } else if config.source.is_empty() {
            any_addr(remote.unwrap().is_ipv6())
        } else {
            endpoint(&config.source, 0)?
        };
        family.validate(addr.ip())?;
        let socket = bind_udp(addr)?;
        if let Some(remote) = remote {
            socket.connect(remote)?;
        }
        socket.set_nonblocking(true)?;
        local = socket.local_addr()?;
        udp = Some(socket);
    }
    let file = if config.record_path.is_empty() {
        None
    } else {
        Some(
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&config.record_path)?,
        )
    };
    {
        let mut s = control.0.lock().unwrap();
        if s.active {
            return Err(ToolError::new(
                "INVALID_ARGUMENT",
                "Control already running",
            ));
        }
        s.active = true;
        s.udp = udp.is_some();
        s.mode = config.mode.clone();
    }
    let _guard = ActiveGuard(control);
    let began = Instant::now();
    let mut next_id = if sessions.is_empty() { 1 } else { 2 };
    let mut recorder = Recorder {
        control,
        config,
        emit,
        stats: Stats {
            sessions_total: sessions.len() as u64,
            ..Default::default()
        },
        file,
        began,
    };
    let mut pending = VecDeque::<Pending>::new();
    let mut buffer = vec![0u8; 65536];
    let mut last_emit = Instant::now() - Duration::from_secs(1);
    let mut last_send = Instant::now() - Duration::from_secs(1);
    let mut failure = None;
    loop {
        if c.load(std::sync::atomic::Ordering::Relaxed)
            || config.duration_secs > 0 && began.elapsed().as_secs() >= config.duration_secs
        {
            break;
        }
        if let Some(listener) = &listener {
            for _ in 0..16 {
                match listener.accept() {
                    Ok((stream, remote)) => {
                        if !permits(remote) {
                            recorder.stats.rejected_sources += 1;
                            continue;
                        }
                        if sessions.len() >= config.max_sessions {
                            recorder.stats.rejected_sessions += 1;
                            continue;
                        }
                        stream.set_nonblocking(true)?;
                        let local = stream.local_addr()?;
                        sessions.insert(
                            next_id,
                            Session {
                                stream,
                                remote,
                                local,
                                read_closed: false,
                                write_closed: false,
                            },
                        );
                        next_id += 1;
                        recorder.stats.sessions_total += 1;
                    }
                    Err(e) if is_timeout(&e) => break,
                    Err(e) => {
                        failure = Some(ToolError::from(e));
                        break;
                    }
                }
            }
        }
        {
            let mut state = control.0.lock().unwrap();
            state.pending_count = pending.len();
            for (id, s) in &sessions {
                state.sessions.insert(*id,json!({"id":id,"local_endpoint":s.local.to_string(),"remote_endpoint":s.remote.to_string(),"read_closed":s.read_closed,"write_closed":s.write_closed,"closed":false}));
            }
            while state.sessions.len() > 1000 {
                let id = *state
                    .sessions
                    .keys()
                    .find(|id| !sessions.contains_key(id))
                    .unwrap();
                state.sessions.remove(&id);
            }
            while pending.len() < 32 {
                let Some(command) = state.queue.pop_front() else {
                    break;
                };
                pending.push_back(Pending {
                    bytes: decode(&command)?,
                    command,
                    offset: 0,
                    next: Instant::now(),
                    deadline: Instant::now() + Duration::from_millis(config.timeout_ms),
                });
                state.pending_count = pending.len();
            }
        }
        // At most one read per session and 16 datagrams per turn bounds work and cancellation latency.
        let mut closing = Vec::new();
        let mut echoes = Vec::new();
        for (id, s) in &mut sessions {
            if s.read_closed {
                continue;
            }
            match s.stream.read(&mut buffer[..16384]) {
                Ok(0) => {
                    s.read_closed = true;
                    recorder.record("read_half_closed", Some(*id), s.local, s.remote, &[]);
                }
                Ok(n) => {
                    recorder.stats.received_bytes += n as u64;
                    recorder.record(
                        "received_stream_bytes",
                        Some(*id),
                        s.local,
                        s.remote,
                        &buffer[..n],
                    );
                    if config.echo {
                        echoes.push((Some(*id), None, buffer[..n].to_vec()));
                    }
                }
                Err(e) if is_timeout(&e) => {}
                Err(_) => {
                    recorder.record("disconnected", Some(*id), s.local, s.remote, &[]);
                    closing.push(*id);
                }
            }
        }
        if let Some(socket) = &udp {
            for _ in 0..16 {
                match socket.recv_from(&mut buffer) {
                    Ok((n, peer)) => {
                        if !permits(peer) {
                            recorder.stats.rejected_sources += 1;
                            continue;
                        }
                        recorder.stats.received_bytes += n as u64;
                        recorder.stats.datagrams_received += 1;
                        recorder.record("received_datagram", None, local, peer, &buffer[..n]);
                        if config.echo {
                            echoes.push((None, Some(peer.to_string()), buffer[..n].to_vec()));
                        }
                    }
                    Err(e) if is_timeout(&e) => break,
                    Err(e) => {
                        failure = Some(e.into());
                        break;
                    }
                }
            }
        }
        for (id, dest, bytes) in echoes {
            let mut state = control.0.lock().unwrap();
            if pending.len() + state.queue.len() < 32 {
                pending.push_back(Pending {
                    command: SendCommand {
                        operation: "send".into(),
                        session_id: id,
                        destination: dest,
                        encoding: "hex".into(),
                        data: String::new(),
                        newline: String::new(),
                        count: 1,
                        interval_ms: 1000,
                    },
                    bytes,
                    offset: 0,
                    next: Instant::now(),
                    deadline: Instant::now() + Duration::from_millis(config.timeout_ms),
                });
                state.pending_count = pending.len();
            } else {
                recorder.stats.send_errors += 1;
            }
        }
        if let Some(mut p) = pending.pop_front() {
            if p.command.session_id.is_some()
                && pending.iter().any(|earlier| {
                    earlier.command.session_id == p.command.session_id && earlier.offset > 0
                })
            {
                // Complete an in-flight partial TCP write before another schedule
                // writes to that stream. Other sessions can still make progress.
                pending.push_back(p);
            } else if p.command.operation != "send" {
                if let Some(s) = p.command.session_id.and_then(|id| sessions.get_mut(&id)) {
                    let close = p.command.operation == "close";
                    let _ = s.stream.shutdown(if close {
                        Shutdown::Both
                    } else {
                        Shutdown::Write
                    });
                    s.write_closed = true;
                    recorder.record(
                        if close { "closed" } else { "write_half_closed" },
                        p.command.session_id,
                        s.local,
                        s.remote,
                        &[],
                    );
                    if close {
                        closing.push(p.command.session_id.unwrap());
                    }
                }
            } else if Instant::now() < p.next
                || last_send.elapsed() < Duration::from_secs_f64(1.0 / config.rate as f64)
            {
                pending.push_back(p);
            } else {
                let dest = if config.mode == "listen" && udp.is_some() {
                    p.command
                        .destination
                        .as_ref()
                        .and_then(|v| v.parse::<SocketAddr>().ok())
                } else {
                    remote
                };
                let result = if let Some(socket) = &udp {
                    match dest {
                        Some(d) if permits(d) => socket.send_to(&p.bytes, d),
                        _ => Err(std::io::Error::new(
                            std::io::ErrorKind::PermissionDenied,
                            "Destination outside allowed CIDRs",
                        )),
                    }
                } else if let Some(s) = p.command.session_id.and_then(|id| sessions.get_mut(&id)) {
                    s.stream.write(&p.bytes[p.offset..])
                } else {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::NotConnected,
                        "Session closed",
                    ))
                };
                match result {
                    Ok(n) if n > 0 || p.bytes.is_empty() => {
                        recorder.stats.sent_bytes += n as u64;
                        let (l, r) = if let Some(s) =
                            p.command.session_id.and_then(|id| sessions.get(&id))
                        {
                            (s.local, s.remote)
                        } else {
                            (local, dest.unwrap())
                        };
                        recorder.record(
                            if udp.is_some() {
                                "sent_datagram"
                            } else {
                                "sent_stream_bytes"
                            },
                            p.command.session_id,
                            l,
                            r,
                            &p.bytes[p.offset..p.offset + n],
                        );
                        p.offset += n;
                        if p.offset == p.bytes.len() {
                            if udp.is_some() {
                                recorder.stats.datagrams_sent += 1;
                            }
                            last_send = Instant::now();
                            p.command.count -= 1;
                            p.offset = 0;
                            p.next = Instant::now() + Duration::from_millis(p.command.interval_ms);
                            p.deadline = p.next + Duration::from_millis(config.timeout_ms);
                            if p.command.count > 0 {
                                pending.push_back(p);
                            }
                        } else {
                            pending.push_back(p);
                        }
                    }
                    Err(e) if is_timeout(&e) && Instant::now() < p.deadline => pending.push_back(p),
                    result => {
                        recorder.stats.send_errors += 1;
                        recorder.stats.last_error = Some(match result {
                            Err(e) => e.into(),
                            _ => ToolError::new("IO_ERROR", "Socket write returned zero"),
                        });
                        if let Some(s) = p.command.session_id.and_then(|id| sessions.get_mut(&id)) {
                            s.write_closed = true;
                            recorder.record(
                                "send_failed",
                                p.command.session_id,
                                s.local,
                                s.remote,
                                &[],
                            );
                        }
                    }
                }
            }
        }
        // Once both directions are closed, release the live-session budget.
        // Keep the stable ID in the bounded session history for inspection.
        closing.extend(
            sessions
                .iter()
                .filter_map(|(id, s)| (s.read_closed && s.write_closed).then_some(*id)),
        );
        for id in closing {
            sessions.remove(&id);
            if let Some(s) = control.0.lock().unwrap().sessions.get_mut(&id) {
                s["closed"] = json!(true);
            }
        }
        if last_emit.elapsed() >= Duration::from_millis(100) {
            let snapshot = json!({"schema_version":1,"stats":recorder.stats,"local_endpoint":local.to_string(),"remote_endpoint":remote.map(|s|s.to_string()),"duration_ms":began.elapsed().as_millis(),"queued_schedules":pending.len()+control.0.lock().unwrap().queue.len(),"recording_policy":if config.record_path.is_empty(){"memory_preview_only"}else{"explicit_bounded_jsonl"},"limitations":["TCP reads are byte-stream chunks, not application messages; HEX preserves exact bytes. UDP sends prove local submission only."]});
            control.0.lock().unwrap().snapshot = snapshot.clone();
            emit(json!({"event_type":"socket_stats","snapshot":snapshot}));
            last_emit = Instant::now();
        }
        if failure.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    for s in sessions.values() {
        let _ = s.stream.shutdown(Shutdown::Both);
    }
    recorder.stats.send_errors +=
        pending.len() as u64 + control.0.lock().unwrap().queue.len() as u64;
    let mut data = control.snapshot(false);
    data["stats"] = json!(recorder.stats);
    data["duration_ms"] = json!(began.elapsed().as_millis());
    data["active"] = json!(false);
    if let Some(rows) = data["sessions"].as_array_mut() {
        for row in rows {
            row["closed"] = json!(true);
        }
    }
    control.0.lock().unwrap().snapshot = data.clone();
    if let Some(mut error) = failure {
        error.data = Some(data);
        return Err(error);
    }
    if c.load(std::sync::atomic::Ordering::Relaxed) {
        let mut error = ToolError::new("CANCELLED", "Socket task stopped");
        error.data = Some(data);
        Err(error)
    } else {
        Ok(data)
    }
}

#[cfg(test)]
mod control_tests {
    use super::*;
    #[test]
    fn budgets_cover_pending_schedules_and_reject_stopped_or_oversized_sends() {
        let control = Control::default();
        {
            let mut state = control.0.lock().unwrap();
            state.active = true;
            state.udp = true;
            state.mode = "send".into();
            state.pending_count = 31;
        }
        let command = SendCommand {
            operation: "send".into(),
            session_id: None,
            destination: None,
            encoding: "hex".into(),
            data: "00".into(),
            newline: String::new(),
            count: 1000,
            interval_ms: 1000,
        };
        control.send(command.clone()).unwrap();
        assert_eq!(
            control.send(command.clone()).unwrap_err().code,
            "RESOURCE_EXHAUSTED"
        );
        let mut oversized = command.clone();
        oversized.data = "00".repeat(65508);
        assert_eq!(
            control.send(oversized).unwrap_err().code,
            "INVALID_ARGUMENT"
        );
        control.0.lock().unwrap().active = false;
        assert_eq!(control.send(command).unwrap_err().code, "TASK_STOPPED");
    }
}
