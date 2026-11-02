use hopkit_core::*;
use serde_json::Value;
use std::{
    fs,
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

struct Server {
    root: PathBuf,
    addr: String,
    cancel: Cancel,
    worker: Option<thread::JoinHandle<ToolResult>>,
}
impl Server {
    fn start(writable: bool, ipv6: bool) -> Self {
        Self::configured(writable, ipv6, "", 0)
    }
    fn configured(writable: bool, ipv6: bool, allow: &str, duration_secs: u64) -> Self {
        let ip = if ipv6 { "::1" } else { "127.0.0.1" };
        let port = TcpListener::bind((ip, 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let root = std::env::temp_dir().join(format!("hopkit-http-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("子目录")).unwrap();
        fs::write(root.join("中文 & file.txt"), b"0123456789").unwrap();
        fs::write(root.join("empty.bin"), b"").unwrap();
        fs::write(root.join(".hidden"), b"private").unwrap();
        fs::write(root.join("子目录/nested.txt"), b"nested").unwrap();
        let request = Request {
            tool: "http".into(),
            action: "serve".into(),
            listen: ip.into(),
            port,
            path: root.to_string_lossy().into(),
            writable,
            allow: allow.into(),
            duration_secs,
            count: 0,
            ..Request::default()
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let (tx, rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            run(&request, &worker_cancel, &|v| {
                let _ = tx.send(v);
            })
        });
        let ready = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(ready["event_type"], "ready");
        Self {
            root,
            addr: ready["listen"].as_str().unwrap().into(),
            cancel,
            worker: Some(worker),
        }
    }
    fn connect(&self) -> TcpStream {
        let stream = TcpStream::connect(&self.addr).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream
    }
    fn request(
        &self,
        method: &str,
        path: &str,
        headers: &str,
        body: &[u8],
    ) -> (u16, String, Vec<u8>) {
        let mut stream = self.connect();
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: localhost\r\n{headers}\r\n"
        )
        .unwrap();
        let _ = stream.write_all(body);
        let _ = stream.shutdown(Shutdown::Write);
        let mut response = Vec::new();
        stream.read_to_end(&mut response).unwrap();
        let end = response.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        let head = String::from_utf8(response[..end].to_vec()).unwrap();
        let code = head.split_whitespace().nth(1).unwrap().parse().unwrap();
        (code, head, response[end..].to_vec())
    }
    fn token(&self) -> String {
        let (code, _, body) = self.request("GET", "/api/config", "", b"");
        assert_eq!(code, 200);
        serde_json::from_slice::<Value>(&body).unwrap()["upload_token"]
            .as_str()
            .unwrap()
            .into()
    }
    fn stop(&mut self) -> ToolResult {
        self.cancel.store(true, Ordering::Relaxed);
        self.worker.take().unwrap().join().unwrap()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        if self.worker.is_some() {
            let _ = self.stop();
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn browser_listing_unicode_nested_downloads_ranges_and_head() {
    let s = Server::start(false, false);
    let (code, headers, page) = s.request("GET", "/", "", b"");
    assert_eq!(code, 200);
    assert!(headers.contains("text/html; charset=utf-8"));
    assert!(String::from_utf8(page).unwrap().contains("file-input"));
    let (code, _, body) = s.request("GET", "/api/list", "", b"");
    assert_eq!(code, 200);
    let list: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(list["entries"].as_array().unwrap().len(), 3);
    assert_eq!(list["entries"][0]["name"], "子目录");
    assert!(!String::from_utf8(body).unwrap().contains(".hidden"));
    let file = "/files/%E4%B8%AD%E6%96%87%20%26%20file.txt";
    let (code, headers, body) = s.request("GET", file, "", b"");
    assert_eq!(code, 200);
    assert_eq!(body, b"0123456789");
    assert!(headers.contains("Content-Disposition: attachment;"));
    assert!(headers.contains("filename*=UTF-8''%E4%B8%AD%E6%96%87%20%26%20file.txt"));
    let (code, headers, body) = s.request("HEAD", file, "", b"");
    assert_eq!(code, 200);
    assert!(body.is_empty());
    assert!(headers.contains("Content-Length: 10"));
    for (value, expected) in [
        ("bytes=2-5", b"2345".as_slice()),
        ("bytes=-3", b"789"),
        ("bytes=8-", b"89"),
    ] {
        let (code, headers, body) = s.request("GET", file, &format!("Range: {value}\r\n"), b"");
        assert_eq!(code, 206);
        assert_eq!(body, expected);
        assert!(headers.contains("Content-Range: bytes"));
    }
    assert_eq!(s.request("GET", file, "Range: bytes=100-\r\n", b"").0, 416);
    assert_eq!(
        s.request(
            "GET",
            file,
            "Range: bytes=2-5\r\nIf-Range: \"stale\"\r\n",
            b""
        )
        .0,
        200
    );
    assert_eq!(s.request("GET", "/files/empty.bin", "", b"").2, b"");
    assert_eq!(
        s.request(
            "GET",
            "/files/%E5%AD%90%E7%9B%AE%E5%BD%95%2Fnested.txt",
            "",
            b""
        )
        .2,
        b"nested"
    );
    assert_eq!(s.request("GET", "/files/missing.txt", "", b"").0, 404);
}

#[test]
fn uploads_require_enabled_switch_and_token_and_never_overwrite() {
    let mut s = Server::start(true, false);
    let token = s.token();
    assert!(!token.is_empty());
    assert_eq!(
        s.request("PUT", "/files/new.bin", "Content-Length: 3\r\n", b"abc")
            .0,
        403
    );
    let headers = format!("Content-Length: 3\r\nX-HopKit-Upload: {token}\r\n");
    assert_eq!(s.request("PUT", "/files/new.bin", &headers, b"abc").0, 201);
    assert_eq!(fs::read(s.root.join("new.bin")).unwrap(), b"abc");
    assert_eq!(s.request("PUT", "/files/new.bin", &headers, b"xyz").0, 409);
    assert_eq!(fs::read(s.root.join("new.bin")).unwrap(), b"abc");
    assert_eq!(
        s.request(
            "PUT",
            "/files/zero.bin",
            &format!("Content-Length: 0\r\nX-HopKit-Upload: {token}\r\n"),
            b""
        )
        .0,
        201
    );
    assert_eq!(
        s.request(
            "PUT",
            "/files/cross.bin",
            &format!("{headers}Sec-Fetch-Site: cross-site\r\n"),
            b"abc"
        )
        .0,
        403
    );
    assert_eq!(
        s.request(
            "PUT",
            "/files/bad.bin",
            &format!("{headers}Content-Length: 3\r\n"),
            b"abc"
        )
        .0,
        400
    );
    assert_eq!(
        s.request(
            "PUT",
            "/files/bad.bin",
            &format!("{headers}Transfer-Encoding: chunked\r\n"),
            b"abc"
        )
        .0,
        400
    );
    let result = s.stop().unwrap_err();
    assert_eq!(result.code, "CANCELLED");
    assert_eq!(result.data.as_ref().unwrap()["uploads"], 2);
    assert_eq!(result.data.as_ref().unwrap()["bytes"], 3);
    let s = Server::start(false, false);
    assert_eq!(s.token(), "");
    assert_eq!(s.request("PUT", "/files/no.bin", &headers, b"abc").0, 403);
    assert!(!s.root.join("no.bin").exists());
}

#[test]
fn rejects_traversal_hidden_files_and_unsafe_paths() {
    let s = Server::start(true, false);
    for path in [
        "../escape.txt",
        "%2e%2e%2fescape.txt",
        "%2Fabsolute.txt",
        "sub%5Cescape.txt",
        "C%3Asecret.txt",
        ".hidden",
        "NUL.txt",
        "file.txt%3Astream",
        "broken%GG",
    ] {
        let (code, _, _) = s.request("GET", &format!("/files/{path}"), "", b"");
        assert_eq!(code, 400, "{path}");
    }
    assert_eq!(s.request("GET", "/api/list?path=..", "", b"").0, 400);
    assert_eq!(s.request("DELETE", "/files/empty.bin", "", b"").0, 405);
    assert!(s.root.join("empty.bin").exists());
}

#[test]
fn interrupted_and_cancelled_uploads_clean_up_and_release_port() {
    let mut s = Server::start(true, false);
    let token = s.token();
    assert_eq!(
        s.request(
            "PUT",
            "/files/broken.bin",
            &format!("Content-Length: 999\r\nX-HopKit-Upload: {token}\r\n"),
            b"partial"
        )
        .0,
        400
    );
    assert!(!s.root.join("broken.bin").exists());
    let mut stream = s.connect();
    write!(stream,"PUT /files/pending.bin HTTP/1.1\r\nHost: localhost\r\nContent-Length: 9999\r\nX-HopKit-Upload: {token}\r\n\r\npartial").unwrap();
    let start = Instant::now();
    while !fs::read_dir(&s.root)
        .unwrap()
        .any(|p| p.unwrap().file_name().to_string_lossy().ends_with(".part"))
    {
        assert!(start.elapsed() < Duration::from_secs(2));
        thread::sleep(Duration::from_millis(10));
    }
    let idle = s.connect();
    let start = Instant::now();
    assert_eq!(s.stop().unwrap_err().code, "CANCELLED");
    assert!(start.elapsed() < Duration::from_secs(1));
    drop(stream);
    drop(idle);
    assert!(!s.root.join("pending.bin").exists());
    assert!(
        !fs::read_dir(&s.root).unwrap().any(|p| p
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".part"))
    );
    assert!(TcpListener::bind(&s.addr).is_ok());
}

#[test]
fn ipv6_browser_download_and_port_conflict() {
    let s = Server::start(false, true);
    assert_eq!(s.request("GET", "/files/empty.bin", "", b"").0, 200);
    let addr: std::net::SocketAddr = s.addr.parse().unwrap();
    let request = Request {
        tool: "http".into(),
        action: "serve".into(),
        listen: addr.ip().to_string(),
        port: addr.port(),
        path: s.root.to_string_lossy().into(),
        ..Request::default()
    };
    assert_eq!(
        run(&request, &Arc::new(AtomicBool::new(false)), &|_| {})
            .unwrap_err()
            .code,
        "ADDRESS_IN_USE"
    );
}

#[test]
fn browser_launcher_only_accepts_numeric_http_urls_on_the_service_port() {
    for url in ["http://127.0.0.1/", "http://192.168.1.1/", "http://[::1]/"] {
        assert!(http_files::validate_browser_url(url, 80).is_ok());
    }
    for url in [
        "file:///etc/passwd",
        "https://127.0.0.1/",
        "http://example.com/",
        "http://127.0.0.1:81/",
        "http://user@127.0.0.1/",
        "http://127.0.0.1/?arg=x",
    ] {
        assert!(http_files::validate_browser_url(url, 80).is_err());
    }
}

#[test]
fn explicit_cidr_denies_clients_and_duration_stops_idle_connections() {
    let s = Server::configured(false, false, "192.0.2.0/24", 0);
    assert_eq!(s.request("GET", "/", "", b"").0, 503);
    drop(s);
    let mut s = Server::configured(false, false, "", 1);
    let _idle = s.connect();
    let result = s.worker.take().unwrap().join().unwrap().unwrap();
    assert_eq!(result["active"], 0);
    assert!(TcpListener::bind(&s.addr).is_ok());
}

#[test]
fn simultaneous_uploads_commit_only_one_file() {
    let s = Server::start(true, false);
    let token = s.token();
    let headers = format!("Content-Length: 3\r\nX-HopKit-Upload: {token}\r\n");
    let (a, b) = thread::scope(|scope| {
        let a = scope.spawn(|| s.request("PUT", "/files/race.bin", &headers, b"aaa").0);
        let b = scope.spawn(|| s.request("PUT", "/files/race.bin", &headers, b"bbb").0);
        (a.join().unwrap(), b.join().unwrap())
    });
    assert!(matches!((a, b), (201, 409) | (409, 201)));
    assert!(matches!(
        fs::read(s.root.join("race.bin")).unwrap().as_slice(),
        b"aaa" | b"bbb"
    ));
}

#[test]
#[cfg(any(windows, target_os = "macos"))]
fn lightweight_interface_query_returns_numeric_local_addresses() {
    let addresses = http_files::interface_addresses(&Arc::new(AtomicBool::new(false))).unwrap();
    assert!(!addresses.is_empty());
    for address in addresses {
        let ip = address.split('%').next().unwrap();
        assert!(ip.parse::<std::net::IpAddr>().is_ok(), "{address}");
    }
}
