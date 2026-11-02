//! Temporary browser file sharing. Files stream directly between disk and socket.
use crate::{runtime::*, tftp};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

const TICK: Duration = Duration::from_millis(100);
const IDLE: Duration = Duration::from_secs(30);
const PAGE: &str = include_str!("http_files.html");

/// Only enumerate local addresses; avoid the much heavier route/DNS overview.
pub fn interface_addresses(c: &Cancel) -> Result<Vec<String>, ToolError> {
    #[cfg(windows)]
    {
        let script = r#"[Net.NetworkInformation.NetworkInterface]::GetAllNetworkInterfaces() | Where-Object { $_.OperationalStatus -eq 'Up' } | ForEach-Object { $_.GetIPProperties().UnicastAddresses | ForEach-Object { $_.Address.ToString() } }"#;
        let output = crate::process::text(
            "powershell.exe",
            &[
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                script,
            ],
            c,
        )?;
        Ok(output
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect())
    }
    #[cfg(target_os = "macos")]
    {
        let output = crate::process::text("/sbin/ifconfig", &["-a"], c)?;
        let mut active = false;
        let mut addresses = Vec::new();
        for line in output.lines() {
            if !line.starts_with(char::is_whitespace) {
                active = line.contains("<UP,") || line.contains(",UP,");
            } else if active {
                let mut parts = line.split_whitespace();
                if matches!(parts.next(), Some("inet" | "inet6"))
                    && let Some(ip) = parts.next()
                {
                    addresses.push(ip.to_string());
                }
            }
        }
        Ok(addresses)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        cancelled(c)?;
        Ok(Vec::new())
    }
}

/// Restrict the desktop browser launcher to a numeric HTTP address on this service's port.
pub fn validate_browser_url(value: &str, port: u16) -> Result<(), ToolError> {
    let url = url::Url::parse(value).map_err(|_| invalid("Invalid service URL"))?;
    let numeric = matches!(
        url.host(),
        Some(url::Host::Ipv4(_)) | Some(url::Host::Ipv6(_))
    );
    if url.scheme() != "http"
        || !numeric
        || url.port_or_known_default() != Some(port)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid("Expected the HTTP file service URL"));
    }
    Ok(())
}

fn invalid(message: &str) -> ToolError {
    ToolError::new("INVALID_ARGUMENT", message)
}

fn encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn decode(text: &str) -> Result<String, ToolError> {
    let mut out = Vec::new();
    let mut input = text.bytes();
    while let Some(b) = input.next() {
        out.push(if b == b'%' {
            let a = input.next().and_then(|b| (b as char).to_digit(16));
            let b = input.next().and_then(|b| (b as char).to_digit(16));
            match (a, b) {
                (Some(a), Some(b)) => (a * 16 + b) as u8,
                _ => return Err(invalid("Malformed URL escape")),
            }
        } else {
            b
        });
    }
    String::from_utf8(out).map_err(|_| invalid("Paths must be UTF-8"))
}

fn rooted(root: &Path, name: &str) -> Result<PathBuf, ToolError> {
    if name.len() > 4096 || name.starts_with('/') || name.ends_with('/') {
        return Err(invalid("Invalid file path / 文件路径无效"));
    }
    let mut path = root.to_path_buf();
    if name.is_empty() {
        return Ok(path);
    }
    let parts: Vec<_> = name.split('/').collect();
    for (i, part) in parts.iter().enumerate() {
        tftp::safe_name(part)?;
        path.push(part);
        match tftp::no_link(&path) {
            Ok(()) => {}
            Err(e) if e.code == "NOT_FOUND" && i + 1 == parts.len() => {}
            Err(e) => return Err(e),
        }
    }
    Ok(path)
}

fn read_socket(s: &mut TcpStream, buf: &mut [u8], c: &Cancel) -> Result<usize, ToolError> {
    let start = Instant::now();
    loop {
        cancelled(c)?;
        match s.read(buf) {
            Ok(n) => return Ok(n),
            Err(e) if is_timeout(&e) && start.elapsed() < IDLE => {}
            Err(e) => return Err(e.into()),
        }
    }
}

fn write_socket(s: &mut TcpStream, mut bytes: &[u8], c: &Cancel) -> Result<(), ToolError> {
    let mut progress = Instant::now();
    while !bytes.is_empty() {
        cancelled(c)?;
        match s.write(bytes) {
            Ok(0) => return Err(ToolError::new("IO_ERROR", "HTTP connection closed")),
            Ok(n) => {
                bytes = &bytes[n..];
                progress = Instant::now();
            }
            Err(e) if is_timeout(&e) && progress.elapsed() < IDLE => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

struct HttpRequest {
    method: String,
    target: String,
    headers: Vec<(String, String)>,
    pending: Vec<u8>,
}
impl HttpRequest {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
    fn read(s: &mut TcpStream, c: &Cancel) -> Result<Self, ToolError> {
        let start = Instant::now();
        let mut bytes = Vec::new();
        loop {
            cancelled(c)?;
            if bytes.len() > 16 * 1024 {
                return Err(invalid("HTTP headers exceed 16 KiB"));
            }
            if bytes.windows(4).any(|b| b == b"\r\n\r\n") {
                let mut headers = [httparse::EMPTY_HEADER; 48];
                let mut req = httparse::Request::new(&mut headers);
                let end = match req.parse(&bytes) {
                    Ok(httparse::Status::Complete(n)) => n,
                    _ => return Err(invalid("Malformed HTTP request")),
                };
                let method = req.method.unwrap_or("").to_string();
                let target = req.path.unwrap_or("").to_string();
                if !target.starts_with('/') || target.starts_with("//") {
                    return Err(invalid("Expected a local HTTP path"));
                }
                let mut fields = Vec::new();
                for h in req.headers.iter() {
                    let name = h.name.to_ascii_lowercase();
                    let value =
                        std::str::from_utf8(h.value).map_err(|_| invalid("Invalid header"))?;
                    if matches!(
                        name.as_str(),
                        "content-length" | "transfer-encoding" | "x-hopkit-upload"
                    ) && fields.iter().any(|(n, _)| n == &name)
                    {
                        return Err(invalid("Duplicate HTTP framing or upload header"));
                    }
                    fields.push((name, value.to_string()));
                }
                return Ok(Self {
                    method,
                    target,
                    headers: fields,
                    pending: bytes[end..].to_vec(),
                });
            }
            let mut buf = [0; 4096];
            match s.read(&mut buf) {
                Ok(0) => return Err(invalid("Incomplete HTTP request")),
                Ok(n) => bytes.extend_from_slice(&buf[..n]),
                Err(e) if is_timeout(&e) => {}
                Err(e) => return Err(e.into()),
            }
            if start.elapsed() >= Duration::from_secs(10) {
                return Err(ToolError::new("TIMEOUT", "HTTP header timeout"));
            }
        }
    }
}

fn head(
    s: &mut TcpStream,
    code: u16,
    kind: &str,
    length: u64,
    extra: &str,
    c: &Cancel,
) -> Result<(), ToolError> {
    let reason = match code {
        200 => "OK",
        201 => "Created",
        206 => "Partial Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        411 => "Length Required",
        416 => "Range Not Satisfiable",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Error",
    };
    write_socket(s, format!("HTTP/1.1 {code} {reason}\r\nContent-Type: {kind}\r\nContent-Length: {length}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'\r\n{extra}\r\n").as_bytes(), c)
}

fn reply(
    s: &mut TcpStream,
    code: u16,
    value: &Value,
    c: &Cancel,
    is_head: bool,
) -> Result<(), ToolError> {
    let bytes = serde_json::to_vec(value).unwrap();
    head(
        s,
        code,
        "application/json; charset=utf-8",
        bytes.len() as u64,
        "",
        c,
    )?;
    if !is_head {
        write_socket(s, &bytes, c)?;
    }
    Ok(())
}

#[derive(Default)]
struct Stats {
    requests: AtomicU64,
    active: AtomicU64,
    downloads: AtomicU64,
    uploads: AtomicU64,
    bytes: AtomicU64,
    denied: AtomicU64,
    errors: AtomicU64,
}
impl Stats {
    fn value(&self, event: &str) -> Value {
        json!({"event_type":event,"requests":self.requests.load(Ordering::Relaxed),
            "active":self.active.load(Ordering::Relaxed),"downloads":self.downloads.load(Ordering::Relaxed),
            "uploads":self.uploads.load(Ordering::Relaxed),"bytes":self.bytes.load(Ordering::Relaxed),
            "denied":self.denied.load(Ordering::Relaxed),"errors":self.errors.load(Ordering::Relaxed)})
    }
}

fn range(value: &str, length: u64) -> Option<(u64, u64)> {
    let value = value.strip_prefix("bytes=")?;
    let (a, b) = value.split_once('-')?;
    if length == 0 {
        return None;
    }
    let (start, end) = if a.is_empty() {
        let suffix = b.parse::<u64>().ok()?;
        if suffix == 0 {
            return None;
        }
        (length.saturating_sub(suffix), length - 1)
    } else {
        (
            a.parse().ok()?,
            if b.is_empty() {
                length - 1
            } else {
                b.parse::<u64>().ok()?.min(length - 1)
            },
        )
    };
    (start <= end && start < length).then_some((start, end))
}

struct Session<'a> {
    root: &'a Path,
    writable: bool,
    token: &'a str,
    stats: &'a Stats,
    cancel: &'a Cancel,
    emit: &'a Sink<'a>,
}
impl Session<'_> {
    fn handle(
        &self,
        s: &mut TcpStream,
        req: &HttpRequest,
        peer: SocketAddr,
        responded: &mut bool,
    ) -> Result<(), ToolError> {
        let c = self.cancel;
        let is_head = req.method == "HEAD";
        let get = req.method == "GET" || is_head;
        let (route, query) = req.target.split_once('?').unwrap_or((&req.target, ""));
        if route == "/" && get {
            *responded = true;
            head(s, 200, "text/html; charset=utf-8", PAGE.len() as u64, "", c)?;
            if !is_head {
                write_socket(s, PAGE.as_bytes(), c)?;
            }
        } else if route == "/api/config" && get {
            *responded = true;
            reply(
                s,
                200,
                &json!({"writable":self.writable,"upload_token":if self.writable { self.token } else { "" }}),
                c,
                is_head,
            )?;
        } else if route == "/api/list" && get {
            let name = url::form_urlencoded::parse(query.as_bytes())
                .find(|(k, _)| k == "path")
                .map(|(_, v)| v.into_owned())
                .unwrap_or_default();
            let path = rooted(self.root, &name)?;
            let _guard = tftp::guard_directory(&path)?;
            // Recheck components after acquiring the Windows directory handles.
            rooted(self.root, &name)?;
            let mut entries = Vec::new();
            for (index, item) in fs::read_dir(&path)?.take(10001).enumerate() {
                if index == 10000 {
                    return Err(ToolError::new(
                        "RESOURCE_EXHAUSTED",
                        "Directory exceeds 10000 entries",
                    ));
                }
                let item = item?;
                let Some(name) = item.file_name().to_str().map(str::to_string) else {
                    continue;
                };
                if tftp::safe_name(&name).is_err() || tftp::no_link(&item.path()).is_err() {
                    continue;
                }
                let meta = fs::symlink_metadata(item.path())?;
                if !meta.is_file() && !meta.is_dir() {
                    continue;
                }
                entries.push(json!({"name":name,"directory":meta.is_dir(),"size":if meta.is_file() { Some(meta.len()) } else { None },"modified":meta.modified().ok().map(chrono::DateTime::<chrono::Utc>::from).map(|d|d.to_rfc3339())}));
            }
            if entries.len() > 10000 {
                return Err(ToolError::new(
                    "RESOURCE_EXHAUSTED",
                    "Directory exceeds 10000 entries",
                ));
            }
            entries.sort_by(|a, b| {
                b["directory"]
                    .as_bool()
                    .cmp(&a["directory"].as_bool())
                    .then_with(|| a["name"].as_str().cmp(&b["name"].as_str()))
            });
            *responded = true;
            reply(s, 200, &json!({"path":name,"entries":entries}), c, is_head)?;
        } else if let Some(encoded) = route.strip_prefix("/files/") {
            let name = decode(encoded)?;
            if name.is_empty() {
                return Err(invalid("Filename required"));
            }
            let path = rooted(self.root, &name)?;
            let _guard = tftp::guard_directory(path.parent().unwrap_or(self.root))?;
            rooted(self.root, &name)?;
            if get {
                let mut file = tftp::read_file(&path)?;
                let length = file.metadata()?.len();
                let mut start = 0;
                let mut count = length;
                let mut code = 200;
                let mut extra = format!(
                    "Accept-Ranges: bytes\r\nContent-Disposition: attachment; filename=\"download\"; filename*=UTF-8''{}\r\n",
                    encode(path.file_name().unwrap().to_str().unwrap())
                );
                // If-Range without validator support requires the full representation.
                if let Some(r) = req
                    .header("range")
                    .filter(|_| req.method == "GET" && req.header("if-range").is_none())
                {
                    let Some((a, b)) = range(r, length) else {
                        *responded = true;
                        return head(
                            s,
                            416,
                            "application/octet-stream",
                            0,
                            &format!("Content-Range: bytes */{length}\r\n"),
                            c,
                        );
                    };
                    start = a;
                    count = b - a + 1;
                    code = 206;
                    extra.push_str(&format!("Content-Range: bytes {a}-{b}/{length}\r\n"));
                }
                file.seek(SeekFrom::Start(start))?;
                *responded = true;
                head(s, code, "application/octet-stream", count, &extra, c)?;
                if is_head {
                    return Ok(());
                }
                let mut left = count;
                let mut buf = [0; 64 * 1024];
                while left > 0 {
                    cancelled(c)?;
                    let limit = left.min(buf.len() as u64) as usize;
                    let n = file.read(&mut buf[..limit])?;
                    if n == 0 {
                        return Err(ToolError::new("IO_ERROR", "File changed during download"));
                    }
                    write_socket(s, &buf[..n], c)?;
                    left -= n as u64;
                }
                self.stats.downloads.fetch_add(1, Ordering::Relaxed);
                self.transferred(peer, "download", &name, count);
            } else if req.method == "PUT" {
                if !self.writable
                    || req.header("x-hopkit-upload") != Some(self.token)
                    || req.header("sec-fetch-site") == Some("cross-site")
                {
                    return Err(ToolError::new(
                        "PERMISSION_DENIED",
                        "Uploads are disabled or unauthorized / 未允许文件上传",
                    ));
                }
                if req.header("transfer-encoding").is_some() {
                    return Err(invalid("Upload requires Content-Length"));
                }
                let Some(length) = req.header("content-length") else {
                    *responded = true;
                    return reply(
                        s,
                        411,
                        &json!({"error":"Content-Length required"}),
                        c,
                        false,
                    );
                };
                if length.is_empty() || !length.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(invalid("Invalid Content-Length"));
                }
                let length = length
                    .parse::<u64>()
                    .map_err(|_| invalid("Invalid Content-Length"))?;
                if path.exists() {
                    return Err(ToolError::new(
                        "ALREADY_EXISTS",
                        "File already exists; uploads never overwrite / 同名文件已存在，不允许覆盖",
                    ));
                }
                let (mut file, temp) = tftp::create_temp(&path)?;
                if let Some(expect) = req.header("expect") {
                    if !expect.eq_ignore_ascii_case("100-continue") {
                        return Err(invalid("Unsupported Expect header"));
                    }
                    write_socket(s, b"HTTP/1.1 100 Continue\r\n\r\n", c)?;
                }
                let pending = req
                    .pending
                    .len()
                    .min(length.min(usize::MAX as u64) as usize);
                file.write_all(&req.pending[..pending])?;
                let mut left = length - pending as u64;
                let mut buf = [0; 64 * 1024];
                while left > 0 {
                    let limit = left.min(buf.len() as u64) as usize;
                    let n = read_socket(s, &mut buf[..limit], c)?;
                    if n == 0 {
                        return Err(invalid("Upload interrupted / 上传中断"));
                    }
                    file.write_all(&buf[..n])?;
                    left -= n as u64;
                }
                file.sync_all()?;
                drop(file);
                cancelled(c)?;
                tftp::commit(temp, &path, false)?;
                self.stats.uploads.fetch_add(1, Ordering::Relaxed);
                self.transferred(peer, "upload", &name, length);
                *responded = true;
                reply(s, 201, &json!({"name":name,"bytes":length}), c, false)?;
            } else {
                *responded = true;
                reply(s, 405, &json!({"error":"Use GET, HEAD or PUT"}), c, is_head)?;
            }
        } else {
            *responded = true;
            reply(
                s,
                if get { 404 } else { 405 },
                &json!({"error":"Not found / 未找到"}),
                c,
                is_head,
            )?;
        }
        Ok(())
    }
    fn transferred(&self, peer: SocketAddr, action: &str, name: &str, bytes: u64) {
        self.stats.bytes.fetch_add(bytes, Ordering::Relaxed);
        (self.emit)(
            json!({"event_type":"transfer","source":peer.to_string(),"action":action,"remote_name":name,"bytes":bytes}),
        );
    }
    fn connection(&self, mut stream: TcpStream, peer: SocketAddr) {
        let _ = stream.set_read_timeout(Some(TICK));
        let _ = stream.set_write_timeout(Some(TICK));
        let req = HttpRequest::read(&mut stream, self.cancel);
        let is_head = req.as_ref().is_ok_and(|r| r.method == "HEAD");
        let mut responded = false;
        let result = req.and_then(|req| self.handle(&mut stream, &req, peer, &mut responded));
        if let Err(e) = result
            && e.code != "CANCELLED"
        {
            self.stats.errors.fetch_add(1, Ordering::Relaxed);
            let code = match e.code.as_str() {
                "NOT_FOUND" => 404,
                "PERMISSION_DENIED" => 403,
                "ALREADY_EXISTS" => 409,
                "INVALID_ARGUMENT" => 400,
                "RESOURCE_EXHAUSTED" => 503,
                _ => 500,
            };
            // If headers were already sent, a second status must not be appended.
            // Transfer errors close the stream; clients detect incomplete Content-Length.
            if !responded {
                let _ = reply(
                    &mut stream,
                    code,
                    &json!({"error":e.message,"code":e.code}),
                    self.cancel,
                    is_head,
                );
            }
        }
        let _ = stream.shutdown(Shutdown::Write);
        // Drain a bounded amount so Windows preserves rejection responses when a
        // browser has already started sending an upload body.
        let until = Instant::now() + Duration::from_millis(200);
        let mut buf = [0; 8192];
        while Instant::now() < until && cancelled(self.cancel).is_ok() {
            if !matches!(stream.read(&mut buf), Ok(n) if n > 0) {
                break;
            }
        }
        self.stats.active.fetch_sub(1, Ordering::Relaxed);
    }
}

pub fn run(r: &Request, c: &Cancel, emit: &Sink<'_>) -> ToolResult {
    if r.action != "serve" {
        return Err(invalid("HTTP action must be serve"));
    }
    let nets = crate::syslog::sources(r)?;
    tftp::no_link(Path::new(&r.path))?;
    let root = Path::new(&r.path).canonicalize()?;
    if !root.is_dir() {
        return Err(invalid(
            "Choose an existing service directory / 请选择已有的服务目录",
        ));
    }
    let _guard = tftp::guard_directory(&root)?;
    let addr = SocketAddr::new(
        r.listen
            .parse::<IpAddr>()
            .map_err(|_| invalid("Invalid listen IP"))?,
        if r.port == 0 { 80 } else { r.port },
    );
    let socket = socket2::Socket::new(
        socket2::Domain::for_address(addr),
        socket2::Type::STREAM,
        Some(socket2::Protocol::TCP),
    )?;
    if addr.is_ipv6() {
        socket.set_only_v6(true)?;
    }
    #[cfg(not(windows))]
    socket.set_reuse_address(true)?;
    socket.bind(&addr.into())?;
    socket.listen(32)?;
    let listener: TcpListener = socket.into();
    listener.set_nonblocking(true)?;
    let listen = listener.local_addr()?;
    let browser_addr = if listen.ip().is_unspecified() {
        SocketAddr::new(
            if listen.is_ipv4() {
                "127.0.0.1".parse().unwrap()
            } else {
                "::1".parse().unwrap()
            },
            listen.port(),
        )
    } else {
        listen
    };
    let url = format!("http://{browser_addr}/");
    emit(
        json!({"event_type":"ready","listen":listen.to_string(),"url":url,"root":root.to_string_lossy(),"transport":"http","writable":r.writable}),
    );
    let stats = Stats::default();
    let worker_cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let token = uuid::Uuid::new_v4().to_string();
    let session = Session {
        root: &root,
        writable: r.writable,
        token: &token,
        stats: &stats,
        cancel: &worker_cancel,
        emit,
    };
    let snapshot = |event: &str| {
        let mut value = stats.value(event);
        value["listen"] = json!(listen.to_string());
        value["url"] = json!(url);
        value["root"] = json!(root.to_string_lossy());
        value["writable"] = json!(r.writable);
        value
    };
    let began = Instant::now();
    let mut last = Instant::now();
    let result: Result<(), ToolError> = std::thread::scope(|scope| {
        let mut workers = Vec::new();
        let outcome = loop {
            if c.load(Ordering::Relaxed)
                || r.duration_secs > 0 && began.elapsed() >= Duration::from_secs(r.duration_secs)
            {
                break Ok(());
            }
            let mut index = 0;
            while index < workers.len() {
                let worker: &std::thread::ScopedJoinHandle<'_, ()> = &workers[index];
                if worker.is_finished() {
                    let _ = workers.swap_remove(index).join();
                } else {
                    index += 1;
                }
            }
            match listener.accept() {
                Ok((mut stream, peer)) => {
                    stats.requests.fetch_add(1, Ordering::Relaxed);
                    if !crate::syslog::allowed(peer.ip(), &nets) || workers.len() >= 16 {
                        stats.denied.fetch_add(1, Ordering::Relaxed);
                        let _ = stream.set_write_timeout(Some(TICK));
                        let _ = reply(
                            &mut stream,
                            503,
                            &json!({"error":"Source denied or server busy"}),
                            &worker_cancel,
                            false,
                        );
                        let _ = stream.shutdown(Shutdown::Write);
                    } else {
                        stats.active.fetch_add(1, Ordering::Relaxed);
                        let session = &session;
                        workers.push(scope.spawn(move || session.connection(stream, peer)));
                    }
                }
                Err(e) if is_timeout(&e) => {}
                Err(e) => break Err(e.into()),
            }
            if last.elapsed() >= Duration::from_millis(500) {
                emit(snapshot("stats"));
                last = Instant::now();
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        worker_cancel.store(true, Ordering::Relaxed);
        for worker in workers {
            let _ = worker.join();
        }
        outcome
    });
    result?;
    let mut data = snapshot("stopped");
    emit(data.clone());
    data.as_object_mut().unwrap().remove("event_type");
    if let Err(mut e) = cancelled(c) {
        e.data = Some(data);
        return Err(e);
    }
    Ok(data)
}
