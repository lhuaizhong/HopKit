//! Native iperf3 wire protocol. No libiperf, executable, shell or runtime download.
//! The control connection uses iperf3's 37-byte cookie, state bytes, and
//! big-endian length-prefixed JSON. Data sockets carry TCP bytes or UDP records.
use crate::runtime::*;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream, UdpSocket},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const TEST_START: u8 = 1;
const TEST_RUNNING: u8 = 2;
const TEST_END: u8 = 4;
const PARAM_EXCHANGE: u8 = 9;
const CREATE_STREAMS: u8 = 10;
const SERVER_TERMINATE: u8 = 11;
const CLIENT_TERMINATE: u8 = 12;
const EXCHANGE_RESULTS: u8 = 13;
const DISPLAY_RESULTS: u8 = 14;
const DONE: u8 = 16;
const DENIED: u8 = 255;
const SERVER_ERROR: u8 = 254;
const IO_TICK: Duration = Duration::from_millis(50);
const JSON_LIMIT: usize = 256 * 1024;

fn invalid(message: &str) -> ToolError {
    ToolError::new("INVALID_ARGUMENT", message)
}
fn protocol(message: &str) -> ToolError {
    ToolError::new("PROTOCOL_ERROR", message)
}
fn transient(e: &std::io::Error) -> bool {
    is_timeout(e) || e.kind() == std::io::ErrorKind::Interrupted
}

struct Context<'a> {
    cancel: &'a Cancel,
    expires: Option<Instant>,
    timeout: Duration,
}
impl Context<'_> {
    fn check(&self) -> Result<(), ToolError> {
        cancelled(self.cancel)?;
        if self.expires.is_some_and(|t| Instant::now() >= t) {
            return Err(ToolError::new(
                "TIMEOUT",
                "Server duration reached / 服务运行时间已到",
            ));
        }
        Ok(())
    }
    fn deadline(&self) -> Instant {
        let t = Instant::now() + self.timeout;
        self.expires.map_or(t, |end| end.min(t))
    }
}

fn setup(stream: &TcpStream) -> Result<(), ToolError> {
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(IO_TICK))?;
    stream.set_write_timeout(Some(IO_TICK))?;
    Ok(())
}
fn read_exact(
    stream: &mut TcpStream,
    bytes: &mut [u8],
    ctx: &Context<'_>,
) -> Result<(), ToolError> {
    let end = ctx.deadline();
    let mut offset = 0;
    while offset < bytes.len() {
        ctx.check()?;
        if Instant::now() >= end {
            return Err(ToolError::new(
                "TIMEOUT",
                "iperf3 control read timed out / 控制连接读取超时",
            ));
        }
        match stream.read(&mut bytes[offset..]) {
            Ok(0) => return Err(protocol("iperf3 peer closed the connection / 对端关闭连接")),
            Ok(n) => offset += n,
            Err(e) if transient(&e) => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
fn write_all(stream: &mut TcpStream, bytes: &[u8], ctx: &Context<'_>) -> Result<(), ToolError> {
    let end = ctx.deadline();
    let mut offset = 0;
    while offset < bytes.len() {
        ctx.check()?;
        if Instant::now() >= end {
            return Err(ToolError::new(
                "TIMEOUT",
                "iperf3 control write timed out / 控制连接写入超时",
            ));
        }
        match stream.write(&bytes[offset..]) {
            Ok(0) => return Err(protocol("iperf3 control write returned zero")),
            Ok(n) => offset += n,
            Err(e) if transient(&e) => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
fn send_state(stream: &mut TcpStream, state: u8, ctx: &Context<'_>) -> Result<(), ToolError> {
    write_all(stream, &[state], ctx)
}
fn state(stream: &mut TcpStream, ctx: &Context<'_>) -> Result<u8, ToolError> {
    let mut b = [0];
    read_exact(stream, &mut b, ctx)?;
    match b[0] {
        DENIED => Err(ToolError::new(
            "SERVER_BUSY",
            "iperf3 server busy or source denied / 服务忙或来源被拒绝",
        )),
        SERVER_TERMINATE | CLIENT_TERMINATE => {
            Err(protocol("iperf3 peer stopped the test / 对端停止测试"))
        }
        SERVER_ERROR => {
            let mut error = [0; 8];
            read_exact(stream, &mut error, ctx)?;
            Err(ToolError::new(
                "UNSUPPORTED_CAPABILITY",
                format!(
                    "iperf3 server error {} (OS {})",
                    u32::from_be_bytes(error[..4].try_into().unwrap()),
                    u32::from_be_bytes(error[4..].try_into().unwrap())
                ),
            ))
        }
        s => Ok(s),
    }
}
fn expect(stream: &mut TcpStream, wanted: u8, ctx: &Context<'_>) -> Result<(), ToolError> {
    let received = state(stream, ctx)?;
    if received != wanted {
        return Err(protocol(&format!(
            "Expected iperf3 state {wanted}, received {received}"
        )));
    }
    Ok(())
}
fn send_json(stream: &mut TcpStream, value: &Value, ctx: &Context<'_>) -> Result<(), ToolError> {
    let bytes = serde_json::to_vec(value).map_err(|e| protocol(&e.to_string()))?;
    write_all(stream, &(bytes.len() as u32).to_be_bytes(), ctx)?;
    write_all(stream, &bytes, ctx)
}
fn read_json(stream: &mut TcpStream, ctx: &Context<'_>) -> Result<Value, ToolError> {
    let mut size = [0; 4];
    read_exact(stream, &mut size, ctx)?;
    let size = u32::from_be_bytes(size) as usize;
    if size == 0 || size > JSON_LIMIT {
        return Err(protocol("iperf3 JSON frame exceeds 256 KiB or is empty"));
    }
    let mut bytes = vec![0; size];
    read_exact(stream, &mut bytes, ctx)?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|e| protocol(&e.to_string()))?;
    if !value.is_object() {
        return Err(protocol("iperf3 JSON must be an object"));
    }
    Ok(value)
}
fn connect(addr: SocketAddr, ctx: &Context<'_>) -> Result<TcpStream, ToolError> {
    // Nonblocking connect keeps cancellation responsive on unreachable hosts.
    let socket = socket2::Socket::new(
        socket2::Domain::for_address(addr),
        socket2::Type::STREAM,
        Some(socket2::Protocol::TCP),
    )?;
    socket.set_nonblocking(true)?;
    if let Err(e) = socket.connect(&addr.into()) {
        #[cfg(windows)]
        let pending = matches!(e.raw_os_error(), Some(10035 | 10036 | 10037));
        #[cfg(unix)]
        let pending = matches!(e.raw_os_error(), Some(36 | 114 | 115));
        if !pending {
            return Err(e.into());
        }
    }
    let end = ctx.deadline();
    loop {
        ctx.check()?;
        if let Some(e) = socket.take_error()? {
            return Err(e.into());
        }
        if socket.peer_addr().is_ok() {
            break;
        }
        if Instant::now() >= end {
            return Err(ToolError::new(
                "TIMEOUT",
                "iperf3 connect timed out / 连接超时",
            ));
        }
        wait(ctx.cancel, Duration::from_millis(10))?;
    }
    socket.set_nonblocking(false)?;
    let stream: TcpStream = socket.into();
    setup(&stream)?;
    Ok(stream)
}
fn listener(addr: SocketAddr) -> Result<TcpListener, ToolError> {
    let socket = socket2::Socket::new(
        socket2::Domain::for_address(addr),
        socket2::Type::STREAM,
        Some(socket2::Protocol::TCP),
    )?;
    #[cfg(unix)]
    socket.set_reuse_address(true)?;
    if addr.is_ipv6() {
        socket.set_only_v6(true)?;
    }
    socket.bind(&addr.into())?;
    socket.listen(32)?;
    socket.set_nonblocking(true)?;
    Ok(socket.into())
}

#[derive(Clone, Debug)]
struct Settings {
    udp: bool,
    parallel: usize,
    reverse: bool,
    seconds: u64,
    rate: u64,
    block: usize,
    counters64: bool,
}
impl Settings {
    fn validate(&self) -> Result<(), ToolError> {
        if !(1..=3600).contains(&self.seconds)
            || !(1..=16).contains(&self.parallel)
            || self.rate > 10_000_000_000
            || !(16..=1_048_576).contains(&self.block)
        {
            return Err(invalid(
                "iperf3: duration 1..3600 s; streams 1..16; bitrate <=10000 Mbps; block 16..1048576 B",
            ));
        }
        if self.udp && (self.parallel != 1 || self.rate == 0 || self.block > 65507) {
            return Err(ToolError::new(
                "UNSUPPORTED_CAPABILITY",
                "UDP requires one stream, a positive bitrate and a block <=65507 B / UDP 使用单流及正数目标速率",
            ));
        }
        Ok(())
    }
    fn from_request(r: &Request) -> Result<Self, ToolError> {
        if !matches!(r.iperf_protocol.as_str(), "tcp" | "udp") {
            return Err(invalid("iperf3 protocol must be tcp or udp"));
        }
        let udp = r.iperf_protocol == "udp";
        let s = Self {
            udp,
            parallel: r.iperf_parallel as usize,
            reverse: r.iperf_reverse,
            seconds: r.duration_secs,
            rate: r.iperf_bitrate_mbps as u64 * 1_000_000,
            block: if udp { 1200 } else { 128 * 1024 },
            counters64: false,
        };
        s.validate()?;
        Ok(s)
    }
    fn from_wire(v: &Value) -> Result<Self, ToolError> {
        // Reject unsupported modes explicitly instead of silently mismeasuring.
        for key in [
            "sctp",
            "bidirectional",
            "authtoken",
            "mptcp",
            "omit",
            "num",
            "blockcount",
            "burst",
            "gso",
            "gro",
            "MSS",
            "nodelay",
            "window",
            "TOS",
            "flowlabel",
            "congestion",
            "fqrate",
            "get_server_output",
        ] {
            if v.get(key)
                .is_some_and(|x| x != &Value::Null && x != false && x != 0 && x != "")
            {
                return Err(ToolError::new(
                    "UNSUPPORTED_CAPABILITY",
                    format!("Unsupported iperf3 parameter: {key}"),
                ));
            }
        }
        if (v["tcp"] == true) == (v["udp"] == true) {
            return Err(invalid("Specify exactly one of TCP or UDP"));
        }
        let udp = v["udp"] == true;
        let integer = |key: &str, default: u64| -> Result<u64, ToolError> {
            v.get(key).map_or(Ok(default), |x| {
                x.as_u64()
                    .ok_or_else(|| invalid(&format!("Invalid iperf3 {key}")))
            })
        };
        let s = Self {
            udp,
            parallel: usize::try_from(integer("parallel", 1)?)
                .map_err(|_| invalid("Invalid stream count"))?,
            reverse: v["reverse"] == true,
            seconds: integer("time", 10)?,
            rate: integer("bandwidth", if udp { 1_000_000 } else { 0 })?,
            block: usize::try_from(integer("len", if udp { 1200 } else { 131072 })?)
                .map_err(|_| invalid("Invalid block size"))?,
            counters64: v["udp_counters_64bit"] == true,
        };
        s.validate()?;
        Ok(s)
    }
    fn wire(&self) -> Value {
        let mut v = json!({"time":self.seconds,"parallel":self.parallel,"len":self.block,
            "bandwidth":self.rate,"omit":0,"num":0,"blockcount":0,"client_version":"HopKit native"});
        v[if self.udp { "udp" } else { "tcp" }] = json!(true);
        if self.reverse {
            v["reverse"] = json!(true);
        }
        v
    }
}

enum DataSocket {
    Tcp(TcpStream),
    Udp(UdpSocket),
}
#[derive(Default)]
struct Counter {
    bytes: AtomicU64,
    packets: AtomicU64,
    lost: AtomicU64,
    jitter_us: AtomicU64,
}
struct Workers {
    stop: Arc<AtomicBool>,
    counters: Vec<Arc<Counter>>,
    threads: Vec<JoinHandle<()>>,
    error: Arc<Mutex<Option<String>>>,
    began: Instant,
    sender: bool,
    seconds: Option<f64>,
}
impl Workers {
    fn start(sockets: Vec<DataSocket>, s: &Settings, sender: bool, cancel: &Cancel) -> Self {
        let mut w = Self {
            stop: Arc::new(AtomicBool::new(false)),
            counters: Vec::new(),
            threads: Vec::new(),
            error: Arc::new(Mutex::new(None)),
            began: Instant::now(),
            sender,
            seconds: None,
        };
        for socket in sockets {
            let counter = Arc::new(Counter::default());
            w.counters.push(counter.clone());
            let stop = w.stop.clone();
            let cancel = cancel.clone();
            let settings = s.clone();
            let error = w.error.clone();
            w.threads.push(thread::spawn(move || {
                if let Err(e) = transfer(socket, &settings, sender, &stop, &cancel, &counter) {
                    *error.lock().unwrap() = Some(e.to_string());
                }
            }));
        }
        w
    }
    fn finish(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
        if self.seconds.is_none() {
            self.seconds = Some(self.began.elapsed().as_secs_f64());
        }
    }
    fn bytes(&self) -> u64 {
        self.counters
            .iter()
            .map(|c| c.bytes.load(Ordering::Relaxed))
            .sum()
    }
    fn wire(&self) -> Value {
        let seconds = self
            .seconds
            .unwrap_or_else(|| self.began.elapsed().as_secs_f64());
        let streams: Vec<_> = self.counters.iter().enumerate().map(|(i,c)| json!({
            "id":if i==0 {1} else {i+2},"bytes":c.bytes.load(Ordering::Relaxed),"retransmits":-1,
            "jitter":c.jitter_us.load(Ordering::Relaxed) as f64/1_000_000.0,
            "errors":c.lost.load(Ordering::Relaxed),"packets":c.packets.load(Ordering::Relaxed),
            "omitted_errors":0,"omitted_packets":0,"start_time":0,"end_time":seconds
        })).collect();
        // CPU use and TCP retransmits are unavailable; -1 is iperf3's sentinel.
        json!({"cpu_util_total":0,"cpu_util_user":0,"cpu_util_system":0,"sender_has_retransmits":-1,"streams":streams})
    }
    fn snapshot(&self, s: &Settings, peer: &str, rate: f64) -> Value {
        let seconds = self
            .seconds
            .unwrap_or_else(|| self.began.elapsed().as_secs_f64());
        let bytes = self.bytes();
        let mut v = json!({"event_type":"stats","protocol":if s.udp {"udp"} else {"tcp"},
            "parallel":s.parallel,"reverse":s.reverse,"peer":peer,"bytes":bytes,"duration_secs":seconds,
            "mbps":if self.seconds.is_some() {bytes as f64*8.0/seconds.max(0.000001)/1_000_000.0} else {rate},"average_mbps":bytes as f64*8.0/seconds.max(0.000001)/1_000_000.0,
            "local_role":if self.sender {"sender"} else {"receiver"},"has_samples":bytes>0,
            "sent_mbps":null,"received_mbps":null,"jitter_ms":null,"loss_percent":null});
        if s.udp && !self.sender {
            let c = &self.counters[0];
            let packets = c.packets.load(Ordering::Relaxed);
            let lost = c.lost.load(Ordering::Relaxed);
            v["jitter_ms"] = json!(c.jitter_us.load(Ordering::Relaxed) as f64 / 1000.0);
            v["lost_packets"] = json!(lost);
            v["packets"] = json!(packets);
            v["loss_percent"] = if packets == 0 {
                Value::Null
            } else {
                json!(lost as f64 * 100.0 / packets as f64)
            };
        }
        v
    }
}
impl Drop for Workers {
    fn drop(&mut self) {
        self.finish();
    }
}

fn transfer(
    mut socket: DataSocket,
    s: &Settings,
    sender: bool,
    stop: &AtomicBool,
    cancel: &Cancel,
    counter: &Counter,
) -> std::io::Result<()> {
    // Each stream has a fixed buffer; no payloads or per-packet history accumulate.
    let mut buffer = vec![0x5a; if s.udp && !sender { 65535 } else { s.block }];
    // Avoid a trivially compressible payload changing throughput on links
    // with transparent compression. Seed once per stream, outside the loop.
    if sender {
        let mut seed = uuid::Uuid::new_v4().as_u128() as u64 | 1;
        for chunk in buffer.chunks_mut(8) {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            chunk.copy_from_slice(&seed.to_ne_bytes()[..chunk.len()]);
        }
    }
    let began = Instant::now();
    let mut sequence = 0_u64;
    let mut highest = 0_u64;
    // A bounded reorder window excludes duplicates from loss recovery.
    let mut seen = [0_u64; 4096];
    let mut previous_transit: Option<f64> = None;
    let mut jitter = 0.0;
    while !stop.load(Ordering::Relaxed) && !cancel.load(Ordering::Relaxed) {
        if sender && s.rate > 0 {
            let due = counter.bytes.load(Ordering::Relaxed) as f64 * 8.0 / s.rate as f64;
            let remaining = due - began.elapsed().as_secs_f64();
            if remaining > 0.0 {
                thread::sleep(Duration::from_secs_f64(remaining.min(0.002)));
                continue;
            }
        }
        if sender && s.udp {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default();
            buffer[..4].copy_from_slice(&(now.as_secs() as u32).to_be_bytes());
            buffer[4..8].copy_from_slice(&now.subsec_micros().to_be_bytes());
            let next = sequence + 1;
            if s.counters64 {
                buffer[8..16].copy_from_slice(&next.to_be_bytes());
            } else {
                buffer[8..12].copy_from_slice(&(next as u32).to_be_bytes());
            }
        }
        let result = match &mut socket {
            DataSocket::Tcp(stream) => {
                if sender {
                    stream.write(&buffer)
                } else {
                    stream.read(&mut buffer)
                }
            }
            DataSocket::Udp(stream) => {
                if sender {
                    stream.send(&buffer)
                } else {
                    stream.recv(&mut buffer)
                }
            }
        };
        match result {
            Ok(0) => break,
            Ok(n) => {
                if s.udp {
                    if sender {
                        sequence += 1;
                        counter.packets.store(sequence, Ordering::Relaxed);
                    } else {
                        if n < if s.counters64 { 16 } else { 12 } {
                            continue;
                        }
                        let id = if s.counters64 {
                            u64::from_be_bytes(buffer[8..16].try_into().unwrap())
                        } else {
                            u32::from_be_bytes(buffer[8..12].try_into().unwrap()) as u64
                        };
                        if id == 0 || seen[id as usize % seen.len()] == id {
                            continue;
                        }
                        seen[id as usize % seen.len()] = id;
                        if id > highest {
                            counter.lost.fetch_add(id - highest - 1, Ordering::Relaxed);
                            highest = id;
                        } else if highest - id < seen.len() as u64 {
                            let _ = counter.lost.fetch_update(
                                Ordering::Relaxed,
                                Ordering::Relaxed,
                                |x| Some(x.saturating_sub(1)),
                            );
                        }
                        counter.packets.store(highest, Ordering::Relaxed);
                        let sec = u32::from_be_bytes(buffer[..4].try_into().unwrap()) as f64;
                        let usec = u32::from_be_bytes(buffer[4..8].try_into().unwrap()) as f64;
                        // Offset cancels out in the difference; clocks need not be synchronized.
                        let transit = began.elapsed().as_secs_f64() - (sec + usec / 1_000_000.0);
                        if let Some(last) = previous_transit {
                            jitter += ((transit - last).abs() - jitter) / 16.0;
                        }
                        previous_transit = Some(transit);
                        counter
                            .jitter_us
                            .store((jitter * 1_000_000.0) as u64, Ordering::Relaxed);
                    }
                }
                counter.bytes.fetch_add(n as u64, Ordering::Relaxed);
            }
            Err(e) if transient(&e) => {}
            Err(e) => {
                if !stop.load(Ordering::Relaxed) && !cancel.load(Ordering::Relaxed) {
                    return Err(e);
                }
                break;
            }
        }
    }
    if let DataSocket::Tcp(stream) = socket {
        let _ = stream.shutdown(Shutdown::Both);
    }
    Ok(())
}

fn udp_client(addr: SocketAddr, ctx: &Context<'_>) -> Result<DataSocket, ToolError> {
    let socket = bind_udp(any_addr(addr.is_ipv6()))?;
    configure_udp(&socket)?;
    socket.set_read_timeout(Some(IO_TICK))?;
    socket.set_write_timeout(Some(IO_TICK))?;
    socket.connect(addr)?;
    // Modern little-endian iperf3 sends "9876". Legacy servers accept any
    // four-byte hello and reply with the host-endian integer 987654321.
    socket.send(b"9876")?;
    let end = ctx.deadline();
    let mut b = [0; 4];
    loop {
        ctx.check()?;
        if Instant::now() >= end {
            return Err(ToolError::new("TIMEOUT", "UDP stream handshake timed out"));
        }
        match socket.recv(&mut b) {
            Ok(4) if b == *b"9876" || b == *b"6789" || u32::from_ne_bytes(b) == 987654321 => {
                return Ok(DataSocket::Udp(socket));
            }
            Ok(_) => return Err(protocol("Invalid iperf3 UDP handshake")),
            Err(e) if transient(&e) => {}
            Err(e) => return Err(e.into()),
        }
    }
}
fn udp_server(
    socket: UdpSocket,
    peer: SocketAddr,
    ctx: &Context<'_>,
) -> Result<DataSocket, ToolError> {
    configure_udp(&socket)?;
    socket.set_read_timeout(Some(IO_TICK))?;
    socket.set_write_timeout(Some(IO_TICK))?;
    let end = ctx.deadline();
    let mut b = [0; 64];
    loop {
        ctx.check()?;
        if Instant::now() >= end {
            return Err(ToolError::new("TIMEOUT", "UDP stream handshake timed out"));
        }
        match socket.recv_from(&mut b) {
            Ok((4, from)) if from.ip() == peer.ip() => {
                // Works with legacy little- and big-endian clients.
                let reply = if b[..4] == 123456789_u32.to_le_bytes() {
                    987654321_u32.to_le_bytes()
                } else if b[..4] == 123456789_u32.to_be_bytes() {
                    987654321_u32.to_be_bytes()
                } else if b[..4] == *b"6789" {
                    *b"9876"
                } else if b[..4] == *b"9876" {
                    *b"6789"
                } else {
                    continue;
                };
                socket.connect(from)?;
                socket.send(&reply)?;
                return Ok(DataSocket::Udp(socket));
            }
            Ok(_) => {}
            Err(e) if transient(&e) => {}
            Err(e) => return Err(e.into()),
        }
    }
}

fn configure_udp(socket: &UdpSocket) -> Result<(), ToolError> {
    // macOS's default UDP send buffer is smaller than the block size chosen
    // by an official client on loopback. Accommodate negotiated datagrams.
    let socket = socket2::SockRef::from(socket);
    socket.set_send_buffer_size(256 * 1024)?;
    socket.set_recv_buffer_size(256 * 1024)?;
    Ok(())
}

fn measure(
    ctrl: &mut TcpStream,
    workers: &mut Workers,
    s: &Settings,
    listener: Option<&TcpListener>,
    ctx: &Context<'_>,
    emit: &Sink<'_>,
) -> Result<(), ToolError> {
    let server = listener.is_some();
    let mut last = Instant::now();
    let mut bytes = 0;
    loop {
        ctx.check()?;
        if let Some(listener) = listener {
            while let Ok((mut busy, _)) = listener.accept() {
                setup(&busy)?;
                let _ = busy.write_all(&[DENIED]);
            }
        }
        let elapsed = workers.began.elapsed();
        if !server && elapsed >= Duration::from_secs(s.seconds) {
            workers.seconds = Some(elapsed.as_secs_f64());
            // Keep the receiver alive briefly for queued bytes while the server
            // handles TEST_END. Sender threads stop before requesting results.
            if workers.sender {
                workers.finish();
            }
            send_state(ctrl, TEST_END, ctx)?;
            expect(ctrl, EXCHANGE_RESULTS, ctx)?;
            break;
        }
        if server && elapsed > Duration::from_secs(s.seconds + 5) {
            return Err(ToolError::new(
                "TIMEOUT",
                "Client did not end the iperf3 test",
            ));
        }
        if let Some(error) = workers.error.lock().unwrap().as_ref() {
            return Err(ToolError::new("IO_ERROR", error.clone()));
        }
        let mut state_byte = [0];
        // A short peek waits without consuming a fragmented control frame.
        match ctrl.peek(&mut state_byte) {
            Ok(0) => return Err(protocol("iperf3 peer disconnected during measurement")),
            Ok(_) => {
                let next = state(ctrl, ctx)?;
                if server && next == TEST_END {
                    workers.seconds = Some(workers.began.elapsed().as_secs_f64());
                    break;
                }
                return Err(protocol(&format!(
                    "Unexpected iperf3 state during test: {next}"
                )));
            }
            Err(e) if transient(&e) => {}
            Err(e) => return Err(e.into()),
        }
        if last.elapsed() >= Duration::from_millis(500) {
            let current = workers.bytes();
            emit(workers.snapshot(
                s,
                &ctrl.peer_addr()?.to_string(),
                (current - bytes) as f64 * 8.0 / last.elapsed().as_secs_f64() / 1_000_000.0,
            ));
            bytes = current;
            last = Instant::now();
        }
    }
    // Allow receivers to drain already queued data. This wait is cancellable.
    if !workers.sender {
        wait(ctx.cancel, Duration::from_millis(100))?;
    }
    workers.finish();
    Ok(())
}

fn result(workers: &Workers, s: &Settings, peer: &str, remote: Value) -> Result<Value, ToolError> {
    let remote_streams = remote["streams"]
        .as_array()
        .ok_or_else(|| protocol("Missing peer result streams"))?;
    if remote_streams.len() != s.parallel {
        return Err(protocol("Peer result stream count mismatch"));
    }
    let mut remote_bytes = 0_u64;
    let mut remote_seconds = 0.0_f64;
    let mut lost = 0_u64;
    let mut packets = 0_u64;
    let mut jitter = 0.0_f64;
    for (i, stream) in remote_streams.iter().enumerate() {
        if stream["id"].as_u64() != Some(if i == 0 { 1 } else { i as u64 + 2 }) {
            return Err(protocol("Peer result stream ID mismatch"));
        }
        remote_bytes = remote_bytes
            .checked_add(
                stream["bytes"]
                    .as_u64()
                    .ok_or_else(|| protocol("Invalid peer byte count"))?,
            )
            .ok_or_else(|| protocol("Peer byte count overflow"))?;
        if let Some(seconds) = stream["end_time"].as_f64().filter(|x| *x > 0.0) {
            remote_seconds = remote_seconds.max(seconds);
        }
        lost = lost.saturating_add(stream["errors"].as_u64().unwrap_or(0));
        packets = packets.saturating_add(stream["packets"].as_u64().unwrap_or(0));
        jitter = jitter.max(stream["jitter"].as_f64().unwrap_or(0.0));
    }
    let seconds = workers
        .seconds
        .unwrap_or_else(|| workers.began.elapsed().as_secs_f64());
    let mut v = workers.snapshot(s, peer, 0.0);
    let local_rate = workers.bytes() as f64 * 8.0 / seconds.max(0.000001) / 1_000_000.0;
    let peer_rate = remote_bytes as f64 * 8.0
        / if remote_seconds > 0.0 {
            remote_seconds
        } else {
            seconds
        }
        .max(0.000001)
        / 1_000_000.0;
    v["mbps"] = json!(local_rate);
    v["sent_mbps"] = json!(if workers.sender {
        local_rate
    } else {
        peer_rate
    });
    v["received_mbps"] = json!(if workers.sender {
        peer_rate
    } else {
        local_rate
    });
    v["sent_bytes"] = json!(if workers.sender {
        workers.bytes()
    } else {
        remote_bytes
    });
    v["received_bytes"] = json!(if workers.sender {
        remote_bytes
    } else {
        workers.bytes()
    });
    if s.udp && workers.sender {
        // The sender knows about trailing losses through its own final count.
        packets = workers.counters[0]
            .packets
            .load(Ordering::Relaxed)
            .max(packets);
        let received_packets = remote_streams[0]["packets"]
            .as_u64()
            .unwrap_or(0)
            .saturating_sub(lost);
        lost = packets.saturating_sub(received_packets);
        v["packets"] = json!(packets);
        v["lost_packets"] = json!(lost);
        v["loss_percent"] = if packets == 0 {
            Value::Null
        } else {
            json!(lost as f64 * 100.0 / packets as f64)
        };
        v["jitter_ms"] = json!(jitter * 1000.0);
    } else if s.udp {
        let received = workers.counters[0]
            .packets
            .load(Ordering::Relaxed)
            .saturating_sub(workers.counters[0].lost.load(Ordering::Relaxed));
        packets = packets.max(workers.counters[0].packets.load(Ordering::Relaxed));
        lost = packets.saturating_sub(received);
        v["packets"] = json!(packets);
        v["lost_packets"] = json!(lost);
        v["loss_percent"] = if packets == 0 {
            Value::Null
        } else {
            json!(lost as f64 * 100.0 / packets as f64)
        };
    }
    v["streams"] = workers.wire()["streams"].clone();
    v["peer_result"] = remote;
    v["unavailable_metrics"] = json!(["cpu_utilization", "tcp_retransmits"]);
    v["success"] = json!(workers.bytes() > 0 && remote_bytes > 0);
    v["event_type"] = json!("test_complete");
    Ok(v)
}

fn client(r: &Request, ctx: &Context<'_>, emit: &Sink<'_>) -> ToolResult {
    let s = Settings::from_request(r)?;
    let addrs = resolve_all(
        &r.target,
        if r.port == 0 { 5201 } else { r.port },
        r.ipv6,
        ctx.cancel,
    )?;
    let mut last_error = None;
    let mut connected = None;
    for addr in addrs {
        match connect(addr, ctx) {
            Ok(stream) => {
                connected = Some(stream);
                break;
            }
            Err(e) => {
                ctx.check()?;
                last_error = Some(e);
            }
        }
    }
    let mut ctrl =
        connected.ok_or_else(|| last_error.unwrap_or_else(|| protocol("No server address")))?;
    let peer = ctrl.peer_addr()?;
    let cookie = format!("{}\0", uuid::Uuid::new_v4());
    write_all(&mut ctrl, cookie.as_bytes(), ctx)?;
    expect(&mut ctrl, PARAM_EXCHANGE, ctx)?;
    send_json(&mut ctrl, &s.wire(), ctx)?;
    expect(&mut ctrl, CREATE_STREAMS, ctx)?;
    let mut sockets = Vec::new();
    for _ in 0..s.parallel {
        if s.udp {
            sockets.push(udp_client(peer, ctx)?);
        } else {
            let mut stream = connect(peer, ctx)?;
            write_all(&mut stream, cookie.as_bytes(), ctx)?;
            sockets.push(DataSocket::Tcp(stream));
        }
    }
    expect(&mut ctrl, TEST_START, ctx)?;
    expect(&mut ctrl, TEST_RUNNING, ctx)?;
    emit(
        json!({"event_type":"connected","peer":peer.to_string(),"protocol":if s.udp {"udp"} else {"tcp"},"reverse":s.reverse,"parallel":s.parallel}),
    );
    let mut workers = Workers::start(sockets, &s, !s.reverse, ctx.cancel);
    let measured = measure(&mut ctrl, &mut workers, &s, None, ctx, emit);
    workers.finish();
    emit(workers.snapshot(&s, &peer.to_string(), 0.0));
    measured?;
    send_json(&mut ctrl, &workers.wire(), ctx)?;
    let remote = read_json(&mut ctrl, ctx)?;
    expect(&mut ctrl, DISPLAY_RESULTS, ctx)?;
    send_state(&mut ctrl, DONE, ctx)?;
    result(&workers, &s, &peer.to_string(), remote)
}

fn server_session(
    mut ctrl: TcpStream,
    listener: &TcpListener,
    peer: SocketAddr,
    ctx: &Context<'_>,
    emit: &Sink<'_>,
) -> ToolResult {
    setup(&ctrl)?;
    let mut cookie = [0; 37];
    read_exact(&mut ctrl, &mut cookie, ctx)?;
    if cookie[36] != 0 {
        return Err(protocol("Invalid iperf3 cookie"));
    }
    send_state(&mut ctrl, PARAM_EXCHANGE, ctx)?;
    let params = read_json(&mut ctrl, ctx)?;
    let s = match Settings::from_wire(&params) {
        Ok(s) => s,
        Err(e) => {
            // iperf3 SERVER_ERROR followed by its IEUNIMP error and OS errno.
            let mut frame = vec![SERVER_ERROR];
            frame.extend(13_u32.to_be_bytes());
            frame.extend(0_u32.to_be_bytes());
            let _ = write_all(&mut ctrl, &frame, ctx);
            return Err(e);
        }
    };
    let udp = if s.udp {
        Some(bind_udp(listener.local_addr()?)?)
    } else {
        None
    };
    send_state(&mut ctrl, CREATE_STREAMS, ctx)?;
    let mut sockets = Vec::new();
    if let Some(socket) = udp {
        sockets.push(udp_server(socket, peer, ctx)?);
    } else {
        let end = ctx.deadline();
        while sockets.len() < s.parallel {
            ctx.check()?;
            if Instant::now() >= end {
                return Err(ToolError::new("TIMEOUT", "Data streams not connected"));
            }
            match listener.accept() {
                Ok((mut stream, from)) => {
                    setup(&stream)?;
                    if from.ip() != peer.ip() {
                        let _ = stream.write_all(&[DENIED]);
                        continue;
                    }
                    let mut received = [0; 37];
                    read_exact(&mut stream, &mut received, ctx)?;
                    if received != cookie {
                        let _ = stream.write_all(&[DENIED]);
                        continue;
                    }
                    sockets.push(DataSocket::Tcp(stream));
                }
                Err(e) if transient(&e) => {
                    wait(ctx.cancel, Duration::from_millis(10))?;
                }
                Err(e) => return Err(e.into()),
            }
        }
    }
    send_state(&mut ctrl, TEST_START, ctx)?;
    send_state(&mut ctrl, TEST_RUNNING, ctx)?;
    emit(
        json!({"event_type":"connected","peer":peer.to_string(),"protocol":if s.udp {"udp"} else {"tcp"},"reverse":s.reverse,"parallel":s.parallel}),
    );
    let mut workers = Workers::start(sockets, &s, s.reverse, ctx.cancel);
    let measured = measure(&mut ctrl, &mut workers, &s, Some(listener), ctx, emit);
    workers.finish();
    emit(workers.snapshot(&s, &peer.to_string(), 0.0));
    measured?;
    send_state(&mut ctrl, EXCHANGE_RESULTS, ctx)?;
    let remote = read_json(&mut ctrl, ctx)?;
    send_json(&mut ctrl, &workers.wire(), ctx)?;
    send_state(&mut ctrl, DISPLAY_RESULTS, ctx)?;
    expect(&mut ctrl, DONE, ctx)?;
    result(&workers, &s, &peer.to_string(), remote)
}

fn serve(r: &Request, ctx: &Context<'_>, emit: &Sink<'_>) -> ToolResult {
    let nets = crate::syslog::sources(r)?;
    let address = SocketAddr::new(
        r.listen.parse().map_err(|_| invalid("Invalid listen IP"))?,
        if r.port == 0 { 5201 } else { r.port },
    );
    let listener = listener(address)?;
    let mut completed = 0_u64;
    let mut errors = 0_u64;
    let mut denied = 0_u64;
    let mut last = json!({"bytes":0,"mbps":null,"has_samples":false});
    emit(
        json!({"event_type":"ready","listen":address.to_string(),"implementation":"native-rust","protocol":"tcp/udp"}),
    );
    loop {
        if ctx.check().is_err() {
            last["event_type"] = json!("stopped");
            last["completed_tests"] = json!(completed);
            last["errors"] = json!(errors);
            last["denied"] = json!(denied);
            last["listen"] = json!(address.to_string());
            emit(last.clone());
            if cancelled(ctx.cancel).is_err() {
                let mut e =
                    ToolError::new("CANCELLED", "iperf3 server stopped / iperf3 服务已停止");
                e.data = Some(last);
                return Err(e);
            }
            return Ok(last);
        }
        match listener.accept() {
            Ok((mut stream, peer)) => {
                if !crate::syslog::allowed(peer.ip(), &nets) {
                    let _ = stream.write_all(&[DENIED]);
                    denied += 1;
                    continue;
                }
                let live = Mutex::new(None);
                match server_session(stream, &listener, peer, ctx, &|mut v| {
                    v["completed_tests"] = json!(completed);
                    if v["event_type"] == "stats" {
                        *live.lock().unwrap() = Some(v.clone());
                    }
                    emit(v);
                }) {
                    Ok(mut v) => {
                        completed += 1;
                        v["completed_tests"] = json!(completed);
                        emit(v.clone());
                        last = v;
                    }
                    Err(e) => {
                        errors += 1;
                        if let Some(v) = live.into_inner().unwrap() {
                            last = v;
                        }
                        // One broken, unsupported or cancelled peer does not stop the listener.
                        if ctx.check().is_ok() {
                            emit(
                                json!({"event_type":"session_error","peer":peer.to_string(),"ok":false,"error":e,"completed_tests":completed}),
                            );
                        }
                    }
                }
            }
            Err(e) if transient(&e) => {
                wait(ctx.cancel, Duration::from_millis(10)).ok();
            }
            Err(e) => return Err(e.into()),
        }
    }
}

pub fn run(r: &Request, cancel: &Cancel, emit: &Sink<'_>) -> ToolResult {
    if !matches!(r.action.as_str(), "" | "client" | "serve") {
        return Err(invalid("iperf3 action must be client or serve"));
    }
    if r.action == "serve" && r.duration_secs > 86400 {
        return Err(invalid("Server duration must be 0..86400 seconds"));
    }
    let ctx = Context {
        cancel,
        expires: if r.action == "serve" && r.duration_secs > 0 {
            Some(Instant::now() + Duration::from_secs(r.duration_secs))
        } else {
            None
        },
        timeout: Duration::from_millis(r.timeout_ms.max(1000)),
    };
    let latest = Mutex::new(None);
    let sink = |v: Value| {
        if matches!(
            v["event_type"].as_str(),
            Some("stats" | "stopped" | "test_complete")
        ) {
            *latest.lock().unwrap() = Some(v.clone());
        }
        emit(v);
    };
    let result = if r.action == "serve" {
        serve(r, &ctx, &sink)
    } else {
        client(r, &ctx, &sink)
    };
    result
        .map(|mut v| {
            v["implementation"] = json!("native-rust");
            v
        })
        .map_err(|mut e| {
            if e.data.is_none() {
                e.data = latest.into_inner().unwrap();
            }
            e
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn udp_loss_reorder_duplicates_and_jitter_use_packet_sequence() {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        sender.connect(receiver.local_addr().unwrap()).unwrap();
        receiver.connect(sender.local_addr().unwrap()).unwrap();
        receiver.set_read_timeout(Some(IO_TICK)).unwrap();
        let counter = Arc::new(Counter::default());
        let stop = Arc::new(AtomicBool::new(false));
        let c = Arc::new(AtomicBool::new(false));
        let (counts, stopped) = (counter.clone(), stop.clone());
        let settings = Settings::from_wire(&json!({"udp":true,"len":1200})).unwrap();
        let worker = thread::spawn(move || {
            transfer(
                DataSocket::Udp(receiver),
                &settings,
                false,
                &stopped,
                &c,
                &counts,
            )
        });
        for id in [1_u32, 3, 2, 2, 5] {
            let mut packet = [0_u8; 20];
            packet[..4].copy_from_slice(&100_u32.to_be_bytes());
            packet[4..8].copy_from_slice(&(id * 1000).to_be_bytes());
            packet[8..12].copy_from_slice(&id.to_be_bytes());
            sender.send(&packet).unwrap();
        }
        let until = Instant::now() + Duration::from_secs(2);
        while counter.packets.load(Ordering::Relaxed) < 5 {
            assert!(Instant::now() < until);
            thread::sleep(Duration::from_millis(5));
        }
        stop.store(true, Ordering::Relaxed);
        worker.join().unwrap().unwrap();
        assert_eq!(counter.bytes.load(Ordering::Relaxed), 80);
        assert_eq!(counter.lost.load(Ordering::Relaxed), 1);
        assert_eq!(counter.packets.load(Ordering::Relaxed), 5);
        assert!(counter.jitter_us.load(Ordering::Relaxed) > 0);
    }
    #[test]
    fn unsupported_and_unbounded_parameters_fail_before_allocating_streams() {
        for v in [
            json!({"tcp":true,"time":0}),
            json!({"tcp":true,"parallel":10000}),
            json!({"tcp":true,"len":1073741824}),
            json!({"tcp":true,"omit":1}),
            json!({"tcp":true,"bidirectional":true}),
            json!({"udp":true,"parallel":2}),
            json!({"udp":true,"len":65508}),
            json!({"tcp":true,"udp":true}),
        ] {
            assert!(Settings::from_wire(&v).is_err(), "{v}");
        }
        assert!(Settings::from_wire(&json!({"tcp":true,"parallel":16,"time":10})).is_ok());
        assert!(
            Settings::from_wire(&json!({"udp":true,"udp_counters_64bit":true}))
                .unwrap()
                .counters64
        );
    }
}
