use crate::runtime::*;
use fs2::FileExt;
use ipnet::IpNet;
use serde_json::json;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    net::{IpAddr, SocketAddr},
    path::Path,
    time::{Duration, Instant, SystemTime},
};

pub fn sources(r: &Request) -> Result<Vec<IpNet>, ToolError> {
    let ip: IpAddr = r
        .listen
        .parse()
        .map_err(|_| ToolError::new("INVALID_ARGUMENT", "Listen address must be a local IP"))?;
    let allowed: Vec<IpNet> = r
        .allow
        .split(',')
        .filter(|s| !s.trim().is_empty())
        .map(|s| {
            s.trim()
                .parse()
                .map_err(|_| ToolError::new("INVALID_ARGUMENT", "Invalid source CIDR"))
        })
        .collect::<Result<_, _>>()?;
    if !ip.is_loopback() && allowed.is_empty() {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "LAN listening requires --allow CIDR / 局域网监听必须设置来源网段",
        ));
    }
    Ok(allowed)
}
pub fn allowed(ip: IpAddr, nets: &[IpNet]) -> bool {
    if nets.is_empty() {
        ip.is_loopback()
    } else {
        nets.iter().any(|n| n.contains(&ip))
    }
}
pub fn lock_dir(path: &Path) -> Result<File, ToolError> {
    fs::create_dir_all(path)?;
    let f = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path.join(".netbox.lock"))?;
    f.try_lock_exclusive().map_err(|_| {
        ToolError::new(
            "DIRECTORY_IN_USE",
            "This directory is in use / 目录已被另一实例使用",
        )
    })?;
    Ok(f)
}
pub fn parse_priority(b: &[u8]) -> (Option<u8>, Option<u8>) {
    if b.first() != Some(&b'<') {
        return (None, None);
    }
    if let Some(end) = b.iter().take(6).position(|x| *x == b'>')
        && let Ok(n) = std::str::from_utf8(&b[1..end]).unwrap_or("").parse::<u8>()
        && n <= 191
    {
        return (Some(n / 8), Some(n % 8));
    }
    (None, None)
}
fn prune(path: &Path) -> Result<(), ToolError> {
    let mut files = Vec::new();
    for item in fs::read_dir(path)? {
        let item = item?;
        let name = item.file_name().to_string_lossy().into_owned();
        if name.starts_with("segment-") && name.ends_with(".jsonl") && item.file_type()?.is_file() {
            let m = item.metadata()?;
            files.push((
                item.path(),
                m.len(),
                m.modified().unwrap_or(SystemTime::now()),
            ));
        }
    }
    files.sort_by_key(|x| x.2);
    let mut bytes: u64 = files.iter().map(|x| x.1).sum();
    for (p, n, t) in files {
        if bytes > 2 * 1024 * 1024 * 1024
            || SystemTime::now().duration_since(t).unwrap_or_default()
                > Duration::from_secs(7 * 86400)
        {
            fs::remove_file(p)?;
            bytes -= n;
        }
    }
    Ok(())
}
pub fn run(r: &Request, c: &Cancel, emit: &Sink<'_>) -> ToolResult {
    if r.action == "send" {
        if r.message.len() > 65000 {
            return Err(ToolError::new(
                "INVALID_ARGUMENT",
                "Syslog message too large",
            ));
        }
        let dest = resolve(&r.target, if r.port == 0 { 514 } else { r.port }, r.ipv6, c)?;
        let socket = bind_udp(any_addr(dest.is_ipv6()))?;
        let text = format!(
            "<13>{} netbox netbox: {}",
            chrono::Local::now().format("%b %e %H:%M:%S"),
            r.message.replace(['\r', '\n'], " ")
        );
        let bytes = socket.send_to(text.as_bytes(), dest)?;
        return Ok(
            json!({"success":true,"bytes":bytes,"destination":dest.to_string(),"delivery":"submitted_to_local_socket"}),
        );
    }
    if r.action != "listen" {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "Syslog action must be send or listen",
        ));
    }
    if r.path.is_empty() {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "Log directory is required",
        ));
    }
    let nets = sources(r)?;
    let path = Path::new(&r.path).join("netbox-syslog");
    let _lock = lock_dir(&path)?;
    prune(&path)?;
    if fs2::available_space(&path)? < 1024 * 1024 * 1024 {
        return Err(ToolError::new(
            "STORAGE_FULL",
            "Less than 1 GiB free disk space",
        ));
    }
    let socket = bind_udp(SocketAddr::new(
        r.listen.parse().unwrap(),
        if r.port == 0 { 514 } else { r.port },
    ))?;
    let mut index = 0;
    let prefix = uuid::Uuid::new_v4().to_string();
    let mut file = create_segment(&path, &prefix, index)?;
    let mut size = 0;
    let mut received = 0_u64;
    let mut denied = 0_u64;
    let mut written = 0_u64;
    let mut last_sync = Instant::now();
    let mut last_stat = Instant::now();
    let mut preview = Instant::now();
    let mut buf = vec![0; 65535];
    emit(
        json!({"event_type":"ready","listen":socket.local_addr()?.to_string(),"directory":path.to_string_lossy(),"transport":"udp"}),
    );
    let began = Instant::now();
    loop {
        let expired =
            r.duration_secs > 0 && began.elapsed() >= Duration::from_secs(r.duration_secs);
        if cancelled(c).is_err() || expired {
            file.sync_all()?;
            emit(
                json!({"event_type":"stopped","received":received,"written":written,"denied":denied}),
            );
            if expired {
                return Ok(
                    json!({"success":true,"received":received,"written":written,"denied":denied,"directory":path.to_string_lossy()}),
                );
            }
            return Err(ToolError::new(
                "CANCELLED",
                "Syslog stopped; log file synchronized / 日志服务已停止并同步文件",
            ));
        }
        match socket.recv_from(&mut buf) {
            Ok((n, peer)) => {
                received += 1;
                if !allowed(peer.ip(), &nets) {
                    denied += 1;
                    continue;
                }
                let (facility, severity) = parse_priority(&buf[..n]);
                let row = json!({"record_id":written+1,"received_at":chrono::Utc::now().to_rfc3339(),"source":peer.to_string(),"facility":facility,"severity":severity,"text":String::from_utf8_lossy(&buf[..n]),"raw_hex":buf[..n].iter().map(|b|format!("{b:02x}")).collect::<String>()});
                let mut bytes = serde_json::to_vec(&row)
                    .map_err(|e| ToolError::new("INTERNAL_ERROR", e.to_string()))?;
                bytes.push(b'\n');
                if size + bytes.len() > 64 * 1024 * 1024 {
                    file.sync_all()?;
                    drop(file);
                    prune(&path)?;
                    index += 1;
                    file = create_segment(&path, &prefix, index)?;
                    size = 0;
                }
                file.write_all(&bytes)?;
                size += bytes.len();
                written += 1;
                // Preview is sampled independently from storage so GUI IPC stays bounded.
                if preview.elapsed() >= Duration::from_millis(50) || written <= 10 {
                    emit(json!({"event_type":"log","record":row}));
                    preview = Instant::now();
                }
            }
            Err(e) if is_timeout(&e) => {}
            Err(e) => return Err(e.into()),
        }
        if last_sync.elapsed() >= Duration::from_secs(1) {
            file.sync_data()?;
            last_sync = Instant::now();
            if fs2::available_space(&path)? < 1024 * 1024 * 1024 {
                return Err(ToolError::new(
                    "STORAGE_FULL",
                    "Disk reserve exhausted; receiver stopped",
                ));
            }
        }
        if last_stat.elapsed() >= Duration::from_secs(1) {
            emit(
                json!({"event_type":"stats","received":received,"written":written,"denied":denied,"kernel_dropped":null}),
            );
            last_stat = Instant::now();
        }
    }
}
fn create_segment(p: &Path, id: &str, index: u32) -> Result<File, ToolError> {
    Ok(OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(p.join(format!("segment-{id}-{index:05}.jsonl")))?)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn priority_boundaries() {
        assert_eq!(parse_priority(b"<191>hello"), (Some(23), Some(7)));
        assert_eq!(parse_priority(b"<192>hello"), (None, None));
        assert_eq!(parse_priority(b"garbage"), (None, None));
    }
    #[test]
    fn lan_requires_explicit_sources() {
        let mut r = Request {
            listen: "0.0.0.0".into(),
            ..Request::default()
        };
        assert!(sources(&r).is_err());
        r.allow = "192.0.2.0/24".into();
        assert!(!allowed(
            "127.0.0.1".parse().unwrap(),
            &sources(&r).unwrap()
        ));
    }
}
