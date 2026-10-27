use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    net::{IpAddr, SocketAddr, ToSocketAddrs},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

pub type Cancel = Arc<AtomicBool>;
pub type Sink<'a> = dyn Fn(Value) + Send + Sync + 'a;
pub type ToolResult = Result<Value, ToolError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolError {
    pub code: String,
    pub message: String,
}
impl ToolError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}
impl From<std::io::Error> for ToolError {
    fn from(e: std::io::Error) -> Self {
        use std::io::ErrorKind::*;
        let code = match e.kind() {
            PermissionDenied => "PERMISSION_DENIED",
            AddrInUse => "ADDRESS_IN_USE",
            TimedOut | WouldBlock => "TIMEOUT",
            NotFound => "NOT_FOUND",
            AlreadyExists => "ALREADY_EXISTS",
            _ => "IO_ERROR",
        };
        Self::new(code, e.to_string())
    }
}
impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for ToolError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Request {
    pub tool: String,
    pub target: String,
    pub port: u16,
    pub count: u32,
    pub timeout_ms: u64,
    pub interval_ms: u64,
    /// ICMP echo data bytes, excluding IP and the 8-byte ICMP header.
    pub payload_size_bytes: u32,
    pub ipv6: bool,
    pub max_hops: u8,
    pub record_type: String,
    pub dns_server: String,
    pub path: String,
    pub remote_name: String,
    pub action: String,
    pub listen: String,
    pub allow: String,
    pub writable: bool,
    pub overwrite: bool,
    pub message: String,
    pub duration_secs: u64,
}
impl Default for Request {
    fn default() -> Self {
        Self {
            tool: "ping".into(),
            target: "127.0.0.1".into(),
            port: 0,
            count: 4,
            timeout_ms: 1000,
            interval_ms: 1000,
            payload_size_bytes: 32,
            ipv6: false,
            max_hops: 30,
            record_type: "A".into(),
            dns_server: String::new(),
            path: String::new(),
            remote_name: String::new(),
            action: String::new(),
            listen: "127.0.0.1".into(),
            allow: String::new(),
            writable: false,
            overwrite: false,
            message: "netbox test".into(),
            duration_secs: 0,
        }
    }
}
pub fn cancelled(c: &Cancel) -> Result<(), ToolError> {
    if c.load(Ordering::Relaxed) {
        Err(ToolError::new(
            "CANCELLED",
            "Operation cancelled / 操作已取消",
        ))
    } else {
        Ok(())
    }
}
pub fn wait(c: &Cancel, duration: Duration) -> Result<(), ToolError> {
    let until = Instant::now() + duration;
    while Instant::now() < until {
        cancelled(c)?;
        std::thread::sleep(
            until
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(50)),
        );
    }
    cancelled(c)
}
pub fn resolve(target: &str, port: u16, ipv6: bool, c: &Cancel) -> Result<SocketAddr, ToolError> {
    resolve_all(target, port, ipv6, c)?
        .into_iter()
        .next()
        .ok_or_else(|| ToolError::new("DNS_ERROR", "No matching address / 无匹配地址"))
}
pub fn resolve_all(
    target: &str,
    port: u16,
    ipv6: bool,
    c: &Cancel,
) -> Result<Vec<SocketAddr>, ToolError> {
    if target.is_empty() || target.len() > 253 || target.chars().any(|x| x.is_control()) {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "Invalid target / 目标无效",
        ));
    }
    if let Ok(ip) = target.parse::<IpAddr>() {
        if ipv6 && ip.is_ipv4() {
            return Err(ToolError::new(
                "INVALID_ARGUMENT",
                "IPv6 requested with IPv4 target",
            ));
        }
        return Ok(vec![SocketAddr::new(ip, port)]);
    }
    static RESOLVERS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    if RESOLVERS
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
            if n < 4 { Some(n + 1) } else { None }
        })
        .is_err()
    {
        return Err(ToolError::new(
            "RESOURCE_EXHAUSTED",
            "System resolver slots are busy; retry later",
        ));
    }
    struct ResolverSlot;
    impl Drop for ResolverSlot {
        fn drop(&mut self) {
            RESOLVERS.fetch_sub(1, Ordering::Relaxed);
        }
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let host = target.to_string();
    std::thread::spawn(move || {
        let _slot = ResolverSlot;
        let r = (host.as_str(), port)
            .to_socket_addrs()
            .map(|x| x.filter(|a| !ipv6 || a.is_ipv6()).collect::<Vec<_>>());
        let _ = tx.send(r);
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        cancelled(c)?;
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(Ok(addrs)) => {
                if addrs.is_empty() {
                    return Err(ToolError::new("DNS_ERROR", "No matching address"));
                }
                return Ok(addrs);
            }
            Ok(Err(e)) => return Err(ToolError::new("DNS_ERROR", e.to_string())),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err(ToolError::new("INTERNAL_ERROR", "Resolver stopped"));
            }
            _ if Instant::now() >= deadline => {
                return Err(ToolError::new("TIMEOUT", "System DNS exceeded 5 seconds"));
            }
            _ => {}
        }
    }
}
pub fn bind_udp(addr: SocketAddr) -> Result<std::net::UdpSocket, ToolError> {
    let s = socket2::Socket::new(
        socket2::Domain::for_address(addr),
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )?;
    if addr.is_ipv6() {
        s.set_only_v6(true)?;
    }
    #[cfg(windows)]
    s.set_reuse_address(false)?;
    s.bind(&addr.into()).map_err(|e| {
        if cfg!(target_os = "macos") && addr.port() < 1024 && e.kind() == std::io::ErrorKind::PermissionDenied {
            ToolError::new("PERMISSION_DENIED", format!("macOS denied UDP port {}. Select a port >=1024 for ordinary-user testing (TFTP 1069 / Syslog 1514), or explicitly run the independent CLI with administrator privileges for standard ports. / macOS 拒绝标准端口；普通用户可手动改用高端口。OS: {e}", addr.port()))
        } else { e.into() }
    })?;
    let s: std::net::UdpSocket = s.into();
    s.set_read_timeout(Some(Duration::from_millis(100)))?;
    Ok(s)
}
pub fn any_addr(v6: bool) -> SocketAddr {
    if v6 {
        "[::]:0".parse().unwrap()
    } else {
        "0.0.0.0:0".parse().unwrap()
    }
}
pub fn is_timeout(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    )
}

pub fn run(req: &Request, c: &Cancel, emit: &Sink<'_>) -> ToolResult {
    cancelled(c)?;
    if req.tool == "ping" && req.payload_size_bytes > 65500 {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "Ping payload must be 0..65500 bytes / Ping 数据大小须为 0–65500 字节",
        ));
    }
    if !(100..=10000).contains(&req.timeout_ms)
        || req.interval_ms > 60000
        || req.max_hops == 0
        || req.max_hops > 64
        || req.count > 10000
    {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "Timeout 100..10000 ms; count 0..10000; hops 1..64; interval <=60000 ms",
        ));
    }
    if req.count == 0 && !matches!(req.tool.as_str(), "ping" | "tcping" | "syslog" | "tftp") {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "This operation requires a finite count",
        ));
    }
    match req.tool.as_str() {
        "ping" | "trace" | "tcping" => crate::diagnostics::probe(req, c, emit),
        "dns" => crate::dns::query(req, c, emit),
        "ntp" => crate::diagnostics::ntp(req, c, emit),
        "syslog" => crate::syslog::run(req, c, emit),
        "tftp" => crate::tftp::run(req, c, emit),
        "doctor" => Ok(
            json!({"platform":std::env::consts::OS,"architecture":std::env::consts::ARCH,"version":env!("CARGO_PKG_VERSION"),"gui_required":false,"icmp_adapter":if cfg!(windows){"windows-iphelper"}else if cfg!(target_os="macos"){"darwin-datagram"}else{"unavailable"},"tools":["ping","trace","tcping","dns","ntp","tftp","syslog"],"preview":true}),
        ),
        _ => Err(ToolError::new("INVALID_ARGUMENT", "Unknown tool")),
    }
}
pub fn envelope(id: &str, tool: &str, result: ToolResult) -> Value {
    match result {
        Ok(data) => {
            json!({"schema_version":1,"task_id":id,"command":tool,"status":"succeeded","data":data,"error":null})
        }
        Err(e) => {
            json!({"schema_version":1,"task_id":id,"command":tool,"status":if e.code=="CANCELLED" {"cancelled"} else {"failed"},"data":null,"error":e})
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_inputs_rejected_before_network() {
        let c = Arc::new(AtomicBool::new(false));
        let r = Request {
            timeout_ms: 0,
            ..Request::default()
        };
        assert_eq!(run(&r, &c, &|_| {}).unwrap_err().code, "INVALID_ARGUMENT");
        let r = Request {
            target: "must-not-resolve.invalid".into(),
            payload_size_bytes: 65501,
            ..Request::default()
        };
        assert_eq!(run(&r, &c, &|_| {}).unwrap_err().code, "INVALID_ARGUMENT");
        // Older saved requests keep the default when this new field is absent.
        let old: Request = serde_json::from_str(r#"{"tool":"ping"}"#).unwrap();
        assert_eq!(old.payload_size_bytes, 32);
    }
    #[test]
    fn cancellation_prevents_execution() {
        let c = Arc::new(AtomicBool::new(true));
        assert_eq!(
            run(&Request::default(), &c, &|_| {}).unwrap_err().code,
            "CANCELLED"
        );
    }
}
