use crate::runtime::*;
use serde_json::json;
#[cfg(test)]
use std::fs;
use std::{
    fs::File,
    io::{Read, Write},
    net::{SocketAddr, UdpSocket},
    path::Path,
    time::{Duration, Instant},
};
const BLOCK: usize = 512;
pub const MAX_SIZE: u64 = 65535 * BLOCK as u64 - 1;

fn code(b: &[u8]) -> u16 {
    if b.len() < 2 {
        0
    } else {
        u16::from_be_bytes([b[0], b[1]])
    }
}
fn error_packet(code: u16, text: &str) -> Vec<u8> {
    let mut b = vec![0, 5];
    b.extend(code.to_be_bytes());
    b.extend(text.bytes().take(200));
    b.push(0);
    b
}
fn ack(n: u16) -> [u8; 4] {
    let x = n.to_be_bytes();
    [0, 4, x[0], x[1]]
}
pub use crate::file_io::safe_name;
pub(crate) use crate::file_io::{commit, create_temp, guard_directory, no_link, read_file};
fn exchange(
    socket: &UdpSocket,
    peer: &mut SocketAddr,
    out: &[u8],
    op: u16,
    block: u16,
    can_learn: bool,
    c: &Cancel,
) -> Result<Vec<u8>, ToolError> {
    let mut buf = [0_u8; 2048];
    for _ in 0..6 {
        cancelled(c)?;
        socket.send_to(out, *peer)?;
        let until = Instant::now() + Duration::from_secs(1);
        while Instant::now() < until {
            cancelled(c)?;
            match socket.recv_from(&mut buf) {
                Ok((n, from)) => {
                    if from.ip() != peer.ip() || (!can_learn && from.port() != peer.port()) || n < 4
                    {
                        continue;
                    }
                    if code(&buf[..n]) == 5 {
                        return Err(ToolError::new(
                            "REMOTE_ERROR",
                            String::from_utf8_lossy(&buf[4..n.saturating_sub(1)]).into_owned(),
                        ));
                    }
                    if code(&buf[..n]) == op && code(&buf[2..n]) == block {
                        if can_learn {
                            *peer = from;
                        }
                        return Ok(buf[..n].to_vec());
                    }
                    // Duplicate data is acknowledged using out on the next retry; duplicate ACK is ignored.
                }
                Err(e) if is_timeout(&e) => {}
                Err(e) => return Err(e.into()),
            }
        }
    }
    Err(ToolError::new(
        "TIMEOUT",
        "TFTP timed out; remote result may be unknown / TFTP 超时，远端文件状态可能不确定",
    ))
}
fn send_file(
    socket: &UdpSocket,
    mut peer: SocketAddr,
    mut file: File,
    c: &Cancel,
    emit: &Sink<'_>,
) -> ToolResult {
    if file.metadata()?.len() > MAX_SIZE {
        return Err(ToolError::new(
            "RESOURCE_EXHAUSTED",
            format!("Standard mode limit: {MAX_SIZE} bytes"),
        ));
    }
    let mut absolute = 1_u32;
    let mut total = 0_u64;
    let mut block = [0_u8; BLOCK];
    loop {
        cancelled(c)?;
        let mut used = 0;
        while used < BLOCK {
            let n = file.read(&mut block[used..])?;
            if n == 0 {
                break;
            }
            used += n;
        }
        if absolute > 65535 || total + used as u64 > MAX_SIZE {
            return Err(ToolError::new(
                "RESOURCE_EXHAUSTED",
                "TFTP block boundary exceeded",
            ));
        }
        let mut packet = vec![0, 3];
        packet.extend((absolute as u16).to_be_bytes());
        packet.extend(&block[..used]);
        exchange(socket, &mut peer, &packet, 4, absolute as u16, false, c)?;
        total += used as u64;
        if absolute.is_multiple_of(128) || used < BLOCK {
            emit(json!({"event_type":"progress","bytes":total,"block":absolute}));
        }
        if used < BLOCK {
            break;
        }
        absolute += 1;
    }
    Ok(
        json!({"success":true,"bytes":total,"confirmation":"final_ack_received","mode":"octet","block_size":512}),
    )
}
fn receive_file(
    socket: &UdpSocket,
    mut peer: SocketAddr,
    first: Vec<u8>,
    dest: &Path,
    overwrite: bool,
    c: &Cancel,
    emit: &Sink<'_>,
) -> ToolResult {
    let parent = dest
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let _guard = guard_directory(parent)?;
    if dest.exists() && !overwrite {
        return Err(ToolError::new(
            "ALREADY_EXISTS",
            "Destination already exists / 目标文件已存在",
        ));
    }
    if let Some(parent) = dest.parent()
        && !parent.as_os_str().is_empty()
        && fs2::available_space(parent)? < 1024 * 1024 * 1024
    {
        return Err(ToolError::new("STORAGE_FULL", "Less than 1 GiB free space"));
    }
    let (mut file, temp) = create_temp(dest)?;
    let mut packet = first;
    let mut absolute = 1_u32;
    let mut total = 0_u64;
    loop {
        cancelled(c)?;
        if packet.len() < 4
            || packet.len() > BLOCK + 4
            || code(&packet) != 3
            || code(&packet[2..]) != absolute as u16
        {
            return Err(ToolError::new("PROTOCOL_ERROR", "Invalid DATA packet"));
        }
        let data = &packet[4..];
        if total + data.len() as u64 > MAX_SIZE {
            return Err(ToolError::new(
                "RESOURCE_EXHAUSTED",
                "Standard TFTP size limit exceeded",
            ));
        }
        file.write_all(data)?;
        total += data.len() as u64;
        if absolute.is_multiple_of(128) || data.len() < BLOCK {
            emit(json!({"event_type":"progress","bytes":total,"block":absolute}));
        }
        if data.len() < BLOCK {
            file.sync_all()?;
            drop(file);
            commit(temp, dest, overwrite)?;
            socket.send_to(&ack(absolute as u16), peer)?;
            // Brief bounded dally re-ACKs a lost final ACK without re-committing the file.
            let until = Instant::now() + Duration::from_secs(2);
            let mut b = [0; 1024];
            while Instant::now() < until && !c.load(std::sync::atomic::Ordering::Relaxed) {
                match socket.recv_from(&mut b) {
                    Ok((n, p))
                        if p == peer
                            && n >= 4
                            && code(&b[..n]) == 3
                            && code(&b[2..n]) == absolute as u16 =>
                    {
                        let _ = socket.send_to(&ack(absolute as u16), peer);
                    }
                    Err(e) if is_timeout(&e) => {}
                    _ => {}
                }
            }
            return Ok(
                json!({"success":true,"bytes":total,"path":dest.to_string_lossy(),"confirmation":"local_file_committed","mode":"octet","block_size":512}),
            );
        }
        if absolute == 65535 {
            return Err(ToolError::new(
                "RESOURCE_EXHAUSTED",
                "TFTP block rollover disabled",
            ));
        }
        packet = exchange(
            socket,
            &mut peer,
            &ack(absolute as u16),
            3,
            (absolute + 1) as u16,
            false,
            c,
        )?;
        absolute += 1;
    }
}
pub fn run(r: &Request, c: &Cancel, emit: &Sink<'_>) -> ToolResult {
    if r.action == "serve" {
        return serve(r, c, emit);
    }
    if !matches!(r.action.as_str(), "get" | "put") {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "TFTP action must be get, put or serve",
        ));
    }
    let path = Path::new(&r.path);
    if r.path.is_empty() {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "Local file path required",
        ));
    }
    let remote = if r.remote_name.is_empty() {
        path.file_name().and_then(|x| x.to_str()).unwrap_or("")
    } else {
        &r.remote_name
    };
    safe_name(remote)?;
    let mut peer = resolve(&r.target, if r.port == 0 { 69 } else { r.port }, r.address_family()?, c)?;
    let socket = bind_udp(any_addr(peer.is_ipv6()))?;
    let file = if r.action == "put" {
        no_link(path)?;
        let f = read_file(path)?;
        if f.metadata()?.len() > MAX_SIZE {
            return Err(ToolError::new(
                "RESOURCE_EXHAUSTED",
                format!("Limit {MAX_SIZE} bytes; rollover unavailable in preview"),
            ));
        }
        Some(f)
    } else {
        if path.exists() && !r.overwrite {
            return Err(ToolError::new(
                "ALREADY_EXISTS",
                "Destination already exists",
            ));
        }
        None
    };
    let mut request = vec![0, if r.action == "get" { 1 } else { 2 }];
    request.extend(remote.as_bytes());
    request.extend(b"\0octet\0");
    emit(json!({"event_type":"target","address":peer.to_string(),"remote_name":remote}));
    if let Some(file) = file {
        exchange(&socket, &mut peer, &request, 4, 0, true, c)?;
        send_file(&socket, peer, file, c, emit)
    } else {
        let first = exchange(&socket, &mut peer, &request, 3, 1, true, c)?;
        receive_file(&socket, peer, first, path, r.overwrite, c, emit)
    }
}
fn serve(r: &Request, c: &Cancel, emit: &Sink<'_>) -> ToolResult {
    let nets = crate::syslog::sources(r)?;
    if r.path.is_empty() {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "Root directory required",
        ));
    }
    let root = Path::new(&r.path).canonicalize()?;
    let _guard = guard_directory(&root)?;
    no_link(&root)?;
    if !root.is_dir() {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "Root must be a directory",
        ));
    }
    let _lock = crate::syslog::lock_dir(&root)?;
    let listener = bind_udp(SocketAddr::new(
        r.listen.parse().unwrap(),
        if r.port == 0 { 69 } else { r.port },
    ))?;
    emit(
        json!({"event_type":"ready","listen":listener.local_addr()?.to_string(),"root":root.to_string_lossy(),"writable":r.writable,"concurrency":1,"block_size":512}),
    );
    let mut buf = [0; 1024];
    let began = Instant::now();
    loop {
        cancelled(c)?;
        if r.duration_secs > 0 && began.elapsed() >= Duration::from_secs(r.duration_secs) {
            return Ok(json!({"success":true,"stop_reason":"duration"}));
        }
        match listener.recv_from(&mut buf) {
            Ok((n, mut peer)) => {
                if !crate::syslog::allowed(peer.ip(), &nets) {
                    continue;
                }
                let op = code(&buf[..n]);
                if op != 1 && op != 2 {
                    continue;
                }
                let fields: Vec<_> = buf[2..n].split(|b| *b == 0).collect();
                if fields.len() < 3 {
                    continue;
                }
                let name = String::from_utf8_lossy(fields[0]);
                let mode = String::from_utf8_lossy(fields[1]);
                if safe_name(&name).is_err()
                    || !mode.eq_ignore_ascii_case("octet")
                    || op == 2 && !r.writable
                {
                    let _ = listener
                        .send_to(&error_packet(2, "Access denied or unsupported mode"), peer);
                    continue;
                }
                let path = root.join(name.as_ref());
                let socket = bind_udp(SocketAddr::new(r.listen.parse().unwrap(), 0))?;
                emit(
                    json!({"event_type":"transfer_start","peer":peer.to_string(),"name":name,"direction":if op==1{"send"}else{"receive"}}),
                );
                let result = if op == 1 {
                    no_link(&path)
                        .and_then(|_| read_file(&path))
                        .and_then(|f| send_file(&socket, peer, f, c, emit))
                } else if path.exists() {
                    Err(ToolError::new(
                        "ALREADY_EXISTS",
                        "Server never overwrites existing files",
                    ))
                } else {
                    exchange(&socket, &mut peer, &ack(0), 3, 1, false, c)
                        .and_then(|first| receive_file(&socket, peer, first, &path, false, c, emit))
                };
                match result {
                    Ok(data) => emit(json!({"event_type":"transfer_complete","data":data})),
                    Err(e) => {
                        let _ = socket.send_to(
                            &error_packet(
                                if e.code == "ALREADY_EXISTS" { 6 } else { 0 },
                                &e.message,
                            ),
                            peer,
                        );
                        emit(json!({"event_type":"transfer_error","error":e}));
                    }
                }
            }
            Err(e) if is_timeout(&e) => {}
            Err(e) => return Err(e.into()),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_escape_and_device_names() {
        for n in [
            "../a", "a/b", "C:foo", "CON", "con.txt", "NUL", "a ", ".secret", "a\\b",
        ] {
            assert!(safe_name(n).is_err(), "{n}");
        }
        assert!(safe_name("配置.cfg").is_ok());
    }
    #[test]
    fn protocol_boundary() {
        assert_eq!(MAX_SIZE, 33_553_919);
    }
    #[test]
    fn no_clobber_commit() {
        let dir = std::env::temp_dir().join(format!("hopkit-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&dir).unwrap();
        let p = dir.join("test");
        let (f, t) = create_temp(&p).unwrap();
        drop(f);
        fs::write(&p, b"original").unwrap();
        assert!(commit(t, &p, false).is_err());
        assert_eq!(fs::read(&p).unwrap(), b"original");
        fs::remove_dir_all(dir).unwrap();
    }
}
