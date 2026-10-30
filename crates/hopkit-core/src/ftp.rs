//! Basic FTP client and temporary passive-mode server (RFC 959 / RFC 2428).
use crate::{runtime::*, tftp};
use serde_json::json;
use std::{
    fs,
    io::{Read, Write},
    net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

const MAX_LISTING: usize = 4 * 1024 * 1024;
const MAX_LINE: usize = 4096;
const TICK: Duration = Duration::from_millis(100);

fn argument(s: &str) -> Result<(), ToolError> {
    if s.len() > 2048 || s.chars().any(|c| c.is_control()) {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "FTP argument is too long or contains control characters",
        ));
    }
    Ok(())
}
fn configure(s: &TcpStream) -> Result<(), ToolError> {
    s.set_read_timeout(Some(TICK))?;
    s.set_write_timeout(Some(TICK))?;
    Ok(())
}
fn write_bytes(
    s: &mut TcpStream,
    mut bytes: &[u8],
    c: &Cancel,
    timeout: Duration,
) -> Result<(), ToolError> {
    let mut progress = Instant::now();
    while !bytes.is_empty() {
        cancelled(c)?;
        match s.write(bytes) {
            Ok(0) => {
                return Err(ToolError::new(
                    "IO_ERROR",
                    "FTP connection closed during write",
                ));
            }
            Ok(n) => {
                bytes = &bytes[n..];
                progress = Instant::now();
            }
            Err(e) if is_timeout(&e) => {
                if progress.elapsed() >= timeout {
                    return Err(ToolError::new("TIMEOUT", "FTP write timed out"));
                }
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
struct Control {
    stream: TcpStream,
    pending: Vec<u8>,
    timeout: Duration,
}
impl Control {
    fn new(stream: TcpStream, timeout: Duration) -> Result<Self, ToolError> {
        configure(&stream)?;
        Ok(Self {
            stream,
            pending: Vec::new(),
            timeout,
        })
    }
    fn line(&mut self, c: &Cancel) -> Result<String, ToolError> {
        let began = Instant::now();
        loop {
            cancelled(c)?;
            if let Some(end) = self.pending.iter().position(|b| *b == b'\n') {
                if end > MAX_LINE {
                    return Err(ToolError::new(
                        "FTP_PROTOCOL",
                        "FTP line exceeds 4096 bytes",
                    ));
                }
                let bytes: Vec<_> = self.pending.drain(..=end).collect();
                return String::from_utf8(bytes)
                    .map(|s| s.trim_end_matches(['\r', '\n']).to_string())
                    .map_err(|_| ToolError::new("FTP_PROTOCOL", "FTP control text must be UTF-8"));
            }
            if self.pending.len() > MAX_LINE {
                return Err(ToolError::new(
                    "FTP_PROTOCOL",
                    "FTP line exceeds 4096 bytes",
                ));
            }
            let mut buf = [0; 2048];
            match self.stream.read(&mut buf) {
                Ok(0) => return Err(ToolError::new("IO_ERROR", "FTP control connection closed")),
                Ok(n) => self.pending.extend_from_slice(&buf[..n]),
                Err(e) if is_timeout(&e) => {}
                Err(e) => return Err(e.into()),
            }
            if began.elapsed() >= self.timeout {
                return Err(ToolError::new("TIMEOUT", "FTP control timed out"));
            }
        }
    }
    fn send(&mut self, text: &str, c: &Cancel) -> Result<(), ToolError> {
        write_bytes(
            &mut self.stream,
            format!("{text}\r\n").as_bytes(),
            c,
            self.timeout,
        )
    }
    fn response(&mut self, c: &Cancel) -> Result<(u16, String), ToolError> {
        let first = self.line(c)?;
        let bytes = first.as_bytes();
        if bytes.len() < 4
            || !bytes[..3].iter().all(u8::is_ascii_digit)
            || !matches!(bytes[3], b' ' | b'-')
        {
            return Err(ToolError::new("FTP_PROTOCOL", "Invalid FTP response"));
        }
        let code = first[..3].parse().unwrap();
        let mut text = first.clone();
        if bytes[3] == b'-' {
            let end = format!("{code} ");
            for _ in 0..128 {
                let line = self.line(c)?;
                text.push('\n');
                text.push_str(&line);
                if line.starts_with(&end) {
                    return Ok((code, text));
                }
            }
            return Err(ToolError::new(
                "FTP_PROTOCOL",
                "FTP multiline response exceeds limit",
            ));
        }
        Ok((code, text))
    }
    fn command(&mut self, command: &str, c: &Cancel) -> Result<(u16, String), ToolError> {
        self.send(command, c)?;
        self.response(c)
    }
}
fn expect(reply: (u16, String), codes: &[u16]) -> Result<String, ToolError> {
    if codes.contains(&reply.0) {
        return Ok(reply.1);
    }
    // Do not echo remote text: some servers include the USER/PASS arguments.
    Err(ToolError::new(
        if reply.0 == 530 {
            "FTP_AUTH"
        } else {
            "FTP_REMOTE"
        },
        format!("FTP server returned status {}", reply.0),
    ))
}
fn passive_port(reply: &str, epsv: bool) -> Result<u16, ToolError> {
    let inside = reply
        .split_once('(')
        .and_then(|(_, tail)| tail.split_once(')'))
        .map(|(s, _)| s)
        .ok_or_else(|| ToolError::new("FTP_PROTOCOL", "Malformed passive reply"))?;
    let port = if epsv {
        let delimiter = inside
            .chars()
            .next()
            .ok_or_else(|| ToolError::new("FTP_PROTOCOL", "Empty EPSV reply"))?;
        let fields: Vec<_> = inside.split(delimiter).collect();
        if fields.len() != 5 || fields[..3] != ["", "", ""] || !fields[4].is_empty() {
            return Err(ToolError::new("FTP_PROTOCOL", "Malformed EPSV reply"));
        }
        fields[3].parse::<u16>().ok()
    } else {
        let fields: Vec<u8> = inside
            .split(',')
            .map(str::parse)
            .collect::<Result<_, _>>()
            .map_err(|_| ToolError::new("FTP_PROTOCOL", "Malformed PASV reply"))?;
        if fields.len() != 6 {
            return Err(ToolError::new("FTP_PROTOCOL", "Malformed PASV reply"));
        }
        Some(u16::from(fields[4]) * 256 + u16::from(fields[5]))
    };
    port.filter(|p| *p != 0)
        .ok_or_else(|| ToolError::new("FTP_PROTOCOL", "Invalid passive port"))
}
fn passive(control: &mut Control, c: &Cancel) -> Result<TcpStream, ToolError> {
    let mut peer = control.stream.peer_addr()?;
    let reply = control.command("EPSV", c)?;
    let port = if reply.0 == 229 {
        passive_port(&reply.1, true)?
    } else if peer.is_ipv4() && matches!(reply.0, 500 | 501 | 502 | 504 | 522) {
        passive_port(&expect(control.command("PASV", c)?, &[227])?, false)?
    } else {
        expect(reply, &[229])?;
        unreachable!()
    };
    // Use the control peer, never a server-supplied third-party PASV address.
    peer.set_port(port);
    cancelled(c)?;
    let data = TcpStream::connect_timeout(&peer, control.timeout)?;
    configure(&data)?;
    Ok(data)
}
fn transfer<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    c: &Cancel,
    timeout: Duration,
    emit: &Sink<'_>,
) -> Result<u64, ToolError> {
    let mut buf = [0; 64 * 1024];
    let mut bytes = 0_u64;
    let mut progress = Instant::now();
    let mut preview = Instant::now();
    loop {
        cancelled(c)?;
        match reader.read(&mut buf) {
            Ok(0) => {
                emit(json!({"event_type":"progress","bytes":bytes}));
                return Ok(bytes);
            }
            Ok(n) => {
                let mut rest = &buf[..n];
                while !rest.is_empty() {
                    cancelled(c)?;
                    match writer.write(rest) {
                        Ok(0) => {
                            return Err(ToolError::new(
                                "IO_ERROR",
                                "FTP transfer write returned zero",
                            ));
                        }
                        Ok(n) => {
                            rest = &rest[n..];
                            bytes += n as u64;
                            progress = Instant::now();
                        }
                        Err(e) if is_timeout(&e) => {
                            if progress.elapsed() >= timeout {
                                return Err(ToolError::new("TIMEOUT", "FTP data write timed out"));
                            }
                        }
                        Err(e) => return Err(e.into()),
                    }
                }
                if preview.elapsed() >= Duration::from_millis(200) {
                    emit(json!({"event_type":"progress","bytes":bytes}));
                    preview = Instant::now();
                }
            }
            Err(e) if is_timeout(&e) => {
                if progress.elapsed() >= timeout {
                    return Err(ToolError::new("TIMEOUT", "FTP data read timed out"));
                }
            }
            Err(e) => return Err(e.into()),
        }
    }
}
struct Listing(Vec<u8>);
impl Write for Listing {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.0.len() + bytes.len() > MAX_LISTING {
            return Err(std::io::Error::other("FTP listing exceeds 4 MiB"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn client(r: &Request, c: &Cancel, emit: &Sink<'_>) -> ToolResult {
    argument(&r.remote_name)?;
    if r.action == "get" && (r.path.is_empty() || r.remote_name.is_empty()) {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "Download requires a remote filename and local output path",
        ));
    }
    if r.action == "get" && !r.overwrite && Path::new(&r.path).exists() {
        return Err(ToolError::new(
            "ALREADY_EXISTS",
            "Local output already exists",
        ));
    }
    let mut input = if r.action == "put" {
        Some(tftp::read_file(Path::new(&r.path))?)
    } else {
        None
    };
    let remote = if r.action == "put" && r.remote_name.is_empty() {
        Path::new(&r.path)
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| ToolError::new("INVALID_ARGUMENT", "Invalid local filename"))?
            .to_string()
    } else {
        r.remote_name.clone()
    };
    argument(&remote)?;
    let timeout = Duration::from_millis(r.timeout_ms);
    let peer = resolve(&r.target, if r.port == 0 { 21 } else { r.port }, r.ipv6, c)?;
    let mut control = Control::new(TcpStream::connect_timeout(&peer, timeout)?, timeout)?;
    expect(control.response(c)?, &[220])?;
    let user = control.command(&format!("USER {}", r.ftp_username), c)?;
    if user.0 == 331 {
        let password = if r.ftp_username == "anonymous" && r.ftp_password.is_empty() {
            "hopkit@localhost"
        } else {
            &r.ftp_password
        };
        expect(control.command(&format!("PASS {password}"), c)?, &[230])?;
    } else {
        expect(user, &[230])?;
    }
    expect(control.command("TYPE I", c)?, &[200])?;
    let mut data = passive(&mut control, c)?;
    let command = match r.action.as_str() {
        "list" => "LIST",
        "get" => "RETR",
        _ => "STOR",
    };
    let command = if remote.is_empty() {
        command.into()
    } else {
        format!("{command} {remote}")
    };
    expect(control.command(&command, c)?, &[125, 150])?;
    let began = Instant::now();
    let mut result = match r.action.as_str() {
        "list" => {
            let mut listing = Listing(Vec::new());
            transfer(&mut data, &mut listing, c, timeout, emit)?;
            let text = String::from_utf8_lossy(&listing.0);
            json!({"entries":text.lines().take(10000).collect::<Vec<_>>(),"entries_truncated":text.lines().count()>10000,"bytes":listing.0.len()})
        }
        "get" => {
            let dest = Path::new(&r.path);
            let (mut file, temp) = tftp::create_temp(dest)?;
            let bytes = transfer(&mut data, &mut file, c, timeout, emit)?;
            file.sync_all()?;
            drop(file);
            drop(data);
            expect(control.response(c)?, &[226, 250])?;
            cancelled(c)?;
            tftp::commit(temp, dest, r.overwrite)?;
            let _ = control.command("QUIT", c);
            return Ok(
                json!({"success":true,"bytes":bytes,"path":r.path,"remote_name":remote,"server":peer.to_string(),"mode":"passive","encrypted":false,"duration_ms":began.elapsed().as_millis()}),
            );
        }
        _ => {
            let bytes = transfer(input.as_mut().unwrap(), &mut data, c, timeout, emit)?;
            data.shutdown(Shutdown::Write)?;
            json!({"bytes":bytes,"path":r.path})
        }
    };
    drop(data);
    expect(control.response(c)?, &[226, 250])?;
    let _ = control.command("QUIT", c);
    result["success"] = json!(true);
    result["server"] = json!(peer.to_string());
    result["remote_name"] = json!(remote);
    result["mode"] = json!("passive");
    result["encrypted"] = json!(false);
    result["duration_ms"] = json!(began.elapsed().as_millis());
    Ok(result)
}

fn listener(addr: SocketAddr) -> Result<TcpListener, ToolError> {
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
    socket.listen(16)?;
    let listener: TcpListener = socket.into();
    listener.set_nonblocking(true)?;
    Ok(listener)
}
fn rooted(root: &Path, cwd: &[String], name: &str) -> Result<(PathBuf, Vec<String>), ToolError> {
    argument(name)?;
    if name.contains('\\') {
        return Err(ToolError::new("PERMISSION_DENIED", "Invalid FTP path"));
    }
    let mut parts = if name.starts_with('/') {
        Vec::new()
    } else {
        cwd.to_vec()
    };
    for part in name.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(ToolError::new(
                        "PERMISSION_DENIED",
                        "Path is outside FTP root",
                    ));
                }
            }
            _ => {
                tftp::safe_name(part)?;
                parts.push(part.to_string());
            }
        }
    }
    let mut path = root.to_path_buf();
    for (index, part) in parts.iter().enumerate() {
        path.push(part);
        match tftp::no_link(&path) {
            Ok(()) => {}
            Err(e) if e.code == "NOT_FOUND" && index + 1 == parts.len() => {}
            Err(e) => return Err(e),
        }
    }
    Ok((path, parts))
}
fn listing(path: &Path, kind: &str) -> Result<Vec<u8>, ToolError> {
    let mut output = Vec::new();
    let paths = if path.is_dir() {
        let mut paths = Vec::new();
        for item in fs::read_dir(path)?.take(10001) {
            paths.push(item?.path());
            if paths.len() > 10000 {
                return Err(ToolError::new(
                    "RESOURCE_EXHAUSTED",
                    "FTP directory exceeds 10000 entries",
                ));
            }
        }
        paths.sort();
        paths
    } else {
        vec![path.to_path_buf()]
    };
    for path in paths {
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if tftp::safe_name(name).is_err() || tftp::no_link(&path).is_err() {
            continue;
        }
        let meta = fs::metadata(&path)?;
        if !meta.is_file() && !meta.is_dir() {
            continue;
        }
        let modified = chrono::DateTime::<chrono::Utc>::from(meta.modified()?);
        let line = match kind {
            "NLST" => name.to_string(),
            "MLSD" => format!(
                "type={};size={};modify={}; {name}",
                if meta.is_dir() { "dir" } else { "file" },
                meta.len(),
                modified.format("%Y%m%d%H%M%S")
            ),
            _ => format!(
                "{} 1 ftp ftp {:>12} {} {name}",
                if meta.is_dir() {
                    "drwxr-xr-x"
                } else {
                    "-rw-r--r--"
                },
                meta.len(),
                modified.format("%b %d %Y")
            ),
        };
        output.extend_from_slice(line.as_bytes());
        output.extend_from_slice(b"\r\n");
        if output.len() > MAX_LISTING {
            return Err(ToolError::new(
                "RESOURCE_EXHAUSTED",
                "FTP directory listing exceeds 4 MiB",
            ));
        }
    }
    Ok(output)
}
#[derive(Default)]
struct Stats {
    connections: AtomicU64,
    active: AtomicU64,
    denied: AtomicU64,
    transfers: AtomicU64,
    bytes: AtomicU64,
    errors: AtomicU64,
}
impl Stats {
    fn value(&self, event: &str) -> serde_json::Value {
        json!({"event_type":event,"connections":self.connections.load(Ordering::Relaxed),"active":self.active.load(Ordering::Relaxed),
            "denied":self.denied.load(Ordering::Relaxed),"transfers":self.transfers.load(Ordering::Relaxed),
            "bytes":self.bytes.load(Ordering::Relaxed),"errors":self.errors.load(Ordering::Relaxed)})
    }
}
fn accept_data(
    listener: TcpListener,
    peer: IpAddr,
    c: &Cancel,
    timeout: Duration,
) -> Result<TcpStream, ToolError> {
    let began = Instant::now();
    loop {
        cancelled(c)?;
        match listener.accept() {
            Ok((s, addr)) if addr.ip() == peer => {
                configure(&s)?;
                return Ok(s);
            }
            Ok(_) => {}
            Err(e) if is_timeout(&e) => {}
            Err(e) => return Err(e.into()),
        }
        if began.elapsed() >= timeout {
            return Err(ToolError::new(
                "TIMEOUT",
                "FTP passive data connection timed out",
            ));
        }
        wait(c, Duration::from_millis(20))?;
    }
}
struct Session<'a> {
    request: &'a Request,
    root: &'a Path,
    cancel: &'a Cancel,
    emit: &'a Sink<'a>,
    stats: &'a Stats,
}
impl Session<'_> {
    fn run(&self, stream: TcpStream, peer: SocketAddr) -> Result<(), ToolError> {
        let (r, c) = (self.request, self.cancel);
        let local = stream.local_addr()?;
        let mut control = Control::new(stream, Duration::from_secs(60))?;
        control.send("220 HopKit FTP ready", c)?;
        let mut user = String::new();
        let mut logged_in = false;
        let mut binary = false;
        let mut failures = 0;
        let mut cwd = Vec::new();
        let mut data_listener = None;
        loop {
            let line = control.line(c)?;
            let (verb, arg) = line.split_once(' ').unwrap_or((&line, ""));
            let verb = verb.to_ascii_uppercase();
            let reply = match verb.as_str() {
                "QUIT" => {
                    control.send("221 Goodbye", c)?;
                    return Ok(());
                }
                "USER" => {
                    logged_in = false;
                    data_listener = None;
                    user = arg.to_string();
                    "331 Password required".to_string()
                }
                "PASS" => {
                    logged_in = !user.is_empty()
                        && user == r.ftp_username
                        && (r.ftp_username == "anonymous" || arg == r.ftp_password);
                    if logged_in {
                        "230 Login successful".into()
                    } else {
                        failures += 1;
                        if failures >= 3 {
                            control.send("421 Too many login failures", c)?;
                            return Ok(());
                        }
                        "530 Login incorrect".into()
                    }
                }
                "FEAT" => {
                    "211-Features\r\n UTF8\r\n EPSV\r\n SIZE\r\n MDTM\r\n MLSD\r\n211 End".into()
                }
                "OPTS" if arg.eq_ignore_ascii_case("UTF8 ON") => "200 UTF8 enabled".into(),
                "SYST" => "215 UNIX Type: L8".into(),
                "NOOP" => "200 OK".into(),
                _ if !logged_in => "530 Login required".into(),
                "TYPE" if arg.eq_ignore_ascii_case("I") || arg.eq_ignore_ascii_case("A") => {
                    binary = arg.eq_ignore_ascii_case("I");
                    "200 Type accepted".into()
                }
                "MODE" if arg.eq_ignore_ascii_case("S") => "200 Stream mode".into(),
                "STRU" if arg.eq_ignore_ascii_case("F") => "200 File structure".into(),
                "PWD" | "XPWD" => format!("257 \"/{}\"", cwd.join("/")),
                "CWD" | "CDUP" => {
                    let name = if verb == "CDUP" { ".." } else { arg };
                    if name == ".." && cwd.is_empty() {
                        "250 Directory changed".into()
                    } else {
                        match rooted(self.root, &cwd, name) {
                            Ok((path, parts)) if path.is_dir() => {
                                cwd = parts;
                                "250 Directory changed".into()
                            }
                            _ => "550 Directory unavailable".into(),
                        }
                    }
                }
                "EPSV" | "PASV" => {
                    data_listener = None;
                    if verb == "PASV" && local.is_ipv6() {
                        "522 Use EPSV for IPv6".into()
                    } else if !arg.is_empty() {
                        "501 Use EPSV without arguments".into()
                    } else {
                        let socket = listener(SocketAddr::new(local.ip(), 0))?;
                        let port = socket.local_addr()?.port();
                        data_listener = Some(socket);
                        if verb == "EPSV" {
                            format!("229 Entering Extended Passive Mode (|||{port}|)")
                        } else {
                            let IpAddr::V4(ip) = local.ip() else {
                                unreachable!()
                            };
                            let [a, b, d, e] = ip.octets();
                            format!(
                                "227 Entering Passive Mode ({a},{b},{d},{e},{},{})",
                                port / 256,
                                port % 256
                            )
                        }
                    }
                }
                "SIZE" | "MDTM" => match rooted(self.root, &cwd, arg) {
                    Ok((path, _)) if path.is_file() => {
                        let meta = fs::metadata(path)?;
                        if verb == "SIZE" {
                            format!("213 {}", meta.len())
                        } else {
                            format!(
                                "213 {}",
                                chrono::DateTime::<chrono::Utc>::from(meta.modified()?)
                                    .format("%Y%m%d%H%M%S")
                            )
                        }
                    }
                    _ => "550 File unavailable".into(),
                },
                "RETR" | "STOR" if !binary => "504 File transfers require TYPE I".into(),
                "LIST" | "NLST" | "MLSD" | "RETR" | "STOR" => {
                    let Some(listener) = data_listener.take() else {
                        control.send("425 Use EPSV or PASV first", c)?;
                        continue;
                    };
                    let result = self.data(&mut control, listener, peer, &cwd, &verb, arg);
                    match result {
                        Ok(bytes) => {
                            self.stats.transfers.fetch_add(1, Ordering::Relaxed);
                            self.stats.bytes.fetch_add(bytes, Ordering::Relaxed);
                            (self.emit)(
                                json!({"event_type":"transfer","ok":true,"source":peer.to_string(),"action":verb,"remote_name":arg,"bytes":bytes}),
                            );
                            "226 Transfer complete".into()
                        }
                        Err(e) if e.code == "CANCELLED" => return Err(e),
                        Err(e) => {
                            self.stats.errors.fetch_add(1, Ordering::Relaxed);
                            if e.code == "TIMEOUT" {
                                "425 Data connection timed out".into()
                            } else {
                                "550 Transfer failed or access denied".into()
                            }
                        }
                    }
                }
                "PORT" | "EPRT" => "502 Active mode unsupported; use EPSV or PASV".into(),
                _ => "502 Command unsupported".into(),
            };
            control.send(&reply, c)?;
        }
    }
    fn data(
        &self,
        control: &mut Control,
        socket: TcpListener,
        peer: SocketAddr,
        cwd: &[String],
        verb: &str,
        name: &str,
    ) -> Result<u64, ToolError> {
        let (r, c) = (self.request, self.cancel);
        let (path, _) = rooted(self.root, cwd, name)?;
        let _guard = tftp::guard_directory(path.parent().unwrap_or(self.root))?;
        let timeout = Duration::from_millis(r.timeout_ms.max(1000));
        // Validate access and prepare files before sending the preliminary reply.
        let mut input = if verb == "RETR" {
            Some(tftp::read_file(&path)?)
        } else {
            None
        };
        let output = if verb == "STOR" {
            if !r.writable || name.is_empty() {
                return Err(ToolError::new(
                    "PERMISSION_DENIED",
                    "FTP server is read-only",
                ));
            }
            if path.exists() {
                return Err(ToolError::new(
                    "ALREADY_EXISTS",
                    "FTP uploads never overwrite existing files",
                ));
            }
            Some(tftp::create_temp(&path)?)
        } else {
            None
        };
        let listing = if matches!(verb, "LIST" | "NLST" | "MLSD") {
            Some(listing(&path, verb)?)
        } else {
            None
        };
        control.send("150 Opening binary data connection", c)?;
        let mut data = accept_data(socket, peer.ip(), c, timeout)?;
        let quiet = |_: serde_json::Value| {};
        if let Some((mut file, temp)) = output {
            let bytes = transfer(&mut data, &mut file, c, timeout, &quiet)?;
            file.sync_all()?;
            drop(file);
            cancelled(c)?;
            tftp::commit(temp, &path, false)?;
            Ok(bytes)
        } else {
            let bytes = if let Some(mut input) = input.take() {
                transfer(&mut input, &mut data, c, timeout, &quiet)?
            } else {
                let bytes = listing.unwrap();
                write_bytes(&mut data, &bytes, c, timeout)?;
                bytes.len() as u64
            };
            data.shutdown(Shutdown::Write)?;
            Ok(bytes)
        }
    }
}
fn serve(r: &Request, c: &Cancel, emit: &Sink<'_>) -> ToolResult {
    let nets = crate::syslog::sources(r)?;
    let root = Path::new(&r.path).canonicalize()?;
    tftp::no_link(Path::new(&r.path))?;
    if !root.is_dir() {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "FTP root must be a directory",
        ));
    }
    let _guard = tftp::guard_directory(&root)?;
    let _lock = crate::syslog::lock_dir(&root)?;
    let socket = listener(SocketAddr::new(
        r.listen.parse().unwrap(),
        if r.port == 0 { 21 } else { r.port },
    ))?;
    let listen = socket.local_addr()?.to_string();
    let cancel = Arc::new(AtomicBool::new(false));
    let stats = Stats::default();
    emit(
        json!({"event_type":"ready","listen":listen,"root":root.to_string_lossy(),"transport":"tcp","mode":"passive","encrypted":false,"writable":r.writable}),
    );
    let began = Instant::now();
    let mut last = Instant::now();
    let session = Session {
        request: r,
        root: &root,
        cancel: &cancel,
        emit,
        stats: &stats,
    };
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
            match socket.accept() {
                Ok((mut stream, peer)) => {
                    stats.connections.fetch_add(1, Ordering::Relaxed);
                    if !crate::syslog::allowed(peer.ip(), &nets) || workers.len() >= 4 {
                        stats.denied.fetch_add(1, Ordering::Relaxed);
                        let _ = stream.set_write_timeout(Some(TICK));
                        let _ = stream.write_all(b"421 Connection unavailable\r\n");
                    } else {
                        stats.active.fetch_add(1, Ordering::Relaxed);
                        let session = &session;
                        workers.push(scope.spawn(move || {
                            let result = session.run(stream, peer);
                            session.stats.active.fetch_sub(1, Ordering::Relaxed);
                            if result.is_err_and(|e| {
                                !matches!(e.code.as_str(), "CANCELLED" | "IO_ERROR")
                            }) {
                                session.stats.errors.fetch_add(1, Ordering::Relaxed);
                            }
                        }));
                    }
                }
                Err(e) if is_timeout(&e) => {}
                Err(e) => break Err(e.into()),
            }
            if last.elapsed() >= Duration::from_secs(1) {
                emit(stats.value("stats"));
                last = Instant::now();
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        cancel.store(true, Ordering::Relaxed);
        for worker in workers {
            let _ = worker.join();
        }
        outcome
    });
    result?;
    emit(stats.value("stopped"));
    cancelled(c)?;
    let mut result = stats.value("stopped");
    result.as_object_mut().unwrap().remove("event_type");
    result["success"] = json!(true);
    result["listen"] = json!(listen);
    result["encrypted"] = json!(false);
    Ok(result)
}
pub fn run(r: &Request, c: &Cancel, emit: &Sink<'_>) -> ToolResult {
    argument(&r.ftp_username)?;
    argument(&r.ftp_password)?;
    if r.ftp_username.is_empty()
        || r.action == "serve" && r.ftp_username != "anonymous" && r.ftp_password.is_empty()
    {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "FTP username is required; a named server account also requires a password",
        ));
    }
    match r.action.as_str() {
        "list" | "get" | "put" => client(r, c, emit),
        "serve" => serve(r, c, emit),
        _ => Err(ToolError::new(
            "INVALID_ARGUMENT",
            "FTP action must be list, get, put or serve",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passive_replies_are_validated() {
        assert_eq!(passive_port("229 EPSV (|||12345|)", true).unwrap(), 12345);
        assert_eq!(passive_port("229 EPSV (!!!21!)", true).unwrap(), 21);
        assert_eq!(
            passive_port("227 PASV (192,0,2,1,8,73)", false).unwrap(),
            2121
        );
        for reply in [
            "229 missing",
            "229 (|||0|)",
            "229 (||21|)",
            "229 (|||65536|)",
            "229 (|||21|extra)",
        ] {
            assert!(passive_port(reply, true).is_err());
        }
        assert!(passive_port("227 (1,2,3,4,999,1)", false).is_err());
    }

    #[test]
    fn protocol_arguments_and_password_redaction() {
        assert!(argument("file\r\nDELE other").is_err());
        assert!(argument(&"a".repeat(2049)).is_err());
        let request: Request =
            serde_json::from_value(json!({"tool":"ftp","ftp_password":"secret-example"})).unwrap();
        assert_eq!(request.ftp_username, "anonymous");
        assert_eq!(request.ftp_password, "secret-example");
        assert!(
            !serde_json::to_string(&request)
                .unwrap()
                .contains("secret-example")
        );
        assert!(!format!("{request:?}").contains("secret-example"));
    }
}
