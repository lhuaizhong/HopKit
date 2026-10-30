use crate::runtime::*;
use serde_json::json;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    net::{SocketAddr, UdpSocket},
    path::{Path, PathBuf},
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
pub fn safe_name(n: &str) -> Result<(), ToolError> {
    let base = n.split('.').next().unwrap_or("").to_uppercase();
    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if n.is_empty()
        || n.len() > 240
        || n.starts_with('.')
        || n.ends_with(['.', ' '])
        || n.chars()
            .any(|c| c.is_control() || "/\\:*?\"<>|".contains(c))
        || reserved.contains(&base.as_str())
    {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "Only a safe filename in the root directory is supported / 仅支持根目录内的安全文件名",
        ));
    }
    Ok(())
}
pub(crate) fn no_link(p: &Path) -> Result<(), ToolError> {
    let m = fs::symlink_metadata(p)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if m.file_attributes() & 0x400 != 0 {
            return Err(ToolError::new(
                "PERMISSION_DENIED",
                "Reparse points are not allowed",
            ));
        }
    }
    if m.file_type().is_symlink() {
        return Err(ToolError::new(
            "PERMISSION_DENIED",
            "Symlinks are not allowed",
        ));
    }
    Ok(())
}

// Hold every canonical ancestor open without FILE_SHARE_DELETE. Remote requests
// cannot race a parent-directory rename/junction replacement on Windows.
pub(crate) fn guard_directory(path: &Path) -> Result<Vec<File>, ToolError> {
    let canonical = path.canonicalize()?;
    let mut held = Vec::new();
    #[cfg(windows)]
    {
        use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
        if canonical.to_string_lossy().starts_with("\\\\?\\UNC\\") {
            return Err(ToolError::new(
                "UNSUPPORTED_CAPABILITY",
                "TFTP preview requires a local disk root",
            ));
        }
        for ancestor in canonical.ancestors() {
            let file = OpenOptions::new()
                .read(true)
                .share_mode(3)
                .custom_flags(0x02000000 | 0x00200000)
                .open(ancestor)?;
            if file.metadata()?.file_attributes() & 0x400 != 0 {
                return Err(ToolError::new(
                    "PERMISSION_DENIED",
                    "Directory reparse point rejected",
                ));
            }
            held.push(file);
        }
    }
    #[cfg(not(windows))]
    {
        held.push(File::open(canonical)?);
    }
    Ok(held)
}
pub(crate) fn read_file(path: &Path) -> Result<File, ToolError> {
    no_link(path)?;
    if !fs::symlink_metadata(path)?.is_file() {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "Only regular files are supported",
        ));
    }
    let mut opts = OpenOptions::new();
    opts.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        opts.custom_flags(0x00200000).share_mode(1);
    }
    let file = opts.open(path)?;
    let meta = file.metadata()?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err(ToolError::new(
                "PERMISSION_DENIED",
                "Reparse point rejected",
            ));
        }
    }
    if !meta.is_file() {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "Only regular files are supported",
        ));
    }
    Ok(file)
}
pub(crate) struct Temporary {
    path: PathBuf,
    committed: bool,
}
impl Drop for Temporary {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_file(&self.path);
        }
    }
}
pub(crate) fn create_temp(dest: &Path) -> Result<(File, Temporary), ToolError> {
    let p = dest.parent().unwrap_or(Path::new("."));
    let path = p.join(format!(".netbox-{}.part", uuid::Uuid::new_v4()));
    let f = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)?;
    Ok((
        f,
        Temporary {
            path,
            committed: false,
        },
    ))
}
pub(crate) fn commit(mut temp: Temporary, dest: &Path, overwrite: bool) -> Result<(), ToolError> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::*;
        let src: Vec<u16> = temp.path.as_os_str().encode_wide().chain(Some(0)).collect();
        let dst: Vec<u16> = dest.as_os_str().encode_wide().chain(Some(0)).collect();
        let flags = MOVEFILE_WRITE_THROUGH
            | if overwrite {
                MOVEFILE_REPLACE_EXISTING
            } else {
                0
            };
        if unsafe { MoveFileExW(src.as_ptr(), dst.as_ptr(), flags) } == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    #[cfg(not(windows))]
    {
        if overwrite {
            fs::rename(&temp.path, dest)?;
        } else {
            fs::hard_link(&temp.path, dest)?;
            fs::remove_file(&temp.path)?;
        }
    }
    temp.committed = true;
    Ok(())
}
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
    let mut peer = resolve(&r.target, if r.port == 0 { 69 } else { r.port }, r.ipv6, c)?;
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
        let dir = std::env::temp_dir().join(format!("nb-{}", uuid::Uuid::new_v4()));
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
