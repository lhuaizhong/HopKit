use hopkit_core::*;
use serde_json::Value;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream, UdpSocket},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

struct Server {
    request: Request,
    cancel: Cancel,
    worker: Option<JoinHandle<ToolResult>>,
    events: Arc<Mutex<Vec<Value>>>,
}
impl Server {
    fn start(host: &str, allow: &str) -> Self {
        let port = TcpListener::bind((host, 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let request = Request {
            tool: "iperf3".into(),
            action: "serve".into(),
            listen: host.into(),
            port,
            allow: allow.into(),
            timeout_ms: 1000,
            ..Request::default()
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let events = Arc::new(Mutex::new(Vec::new()));
        let (req, c, e) = (request.clone(), cancel.clone(), events.clone());
        let (tx, rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            run(&req, &c, &|v| {
                if v["event_type"] == "ready" {
                    tx.send(()).unwrap();
                }
                e.lock().unwrap().push(v);
            })
        });
        rx.recv_timeout(Duration::from_secs(3))
            .expect("native server ready");
        Self {
            request,
            cancel,
            worker: Some(worker),
            events,
        }
    }
    fn stop(&mut self) -> ToolError {
        let began = Instant::now();
        self.cancel.store(true, Ordering::Relaxed);
        let error = self.worker.take().unwrap().join().unwrap().unwrap_err();
        assert_eq!(error.code, "CANCELLED");
        assert!(began.elapsed() < Duration::from_secs(1));
        drop(TcpListener::bind((self.request.listen.as_str(), self.request.port)).unwrap());
        drop(UdpSocket::bind((self.request.listen.as_str(), self.request.port)).unwrap());
        error
    }
    fn client(&self, udp: bool, reverse: bool, parallel: u8) -> Request {
        Request {
            tool: "iperf3".into(),
            action: "client".into(),
            target: self.request.listen.clone(),
            port: self.request.port,
            duration_secs: 1,
            iperf_protocol: if udp { "udp" } else { "tcp" }.into(),
            iperf_reverse: reverse,
            iperf_parallel: parallel,
            iperf_bitrate_mbps: 5,
            timeout_ms: 3000,
            ..Request::default()
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
fn cancel() -> Cancel {
    Arc::new(AtomicBool::new(false))
}

#[test]
fn native_tcp_udp_upload_download_and_parallel_ipv4_ipv6() {
    for host in ["127.0.0.1", "::1"] {
        let mut server = Server::start(host, "");
        for (udp, reverse, parallel) in [
            (false, false, 3),
            (false, true, 2),
            (true, false, 1),
            (true, true, 1),
        ] {
            let result = run(&server.client(udp, reverse, parallel), &cancel(), &|_| {}).unwrap();
            assert_eq!(result["implementation"], "native-rust");
            assert_eq!(result["success"], true);
            assert!(result["sent_bytes"].as_u64().unwrap() > 0);
            assert!(result["received_bytes"].as_u64().unwrap() > 0);
            assert!(result["sent_mbps"].as_f64().unwrap() > 0.1);
            assert!(result["received_mbps"].as_f64().unwrap() > 0.1);
            assert_eq!(
                result["streams"].as_array().unwrap().len(),
                parallel as usize
            );
            if udp {
                assert_eq!(result["loss_percent"], 0.0);
                assert!(result["jitter_ms"].is_number());
            } else {
                assert!(result["loss_percent"].is_null());
            }
        }
        let final_stats = server.stop().data.unwrap();
        assert_eq!(final_stats["completed_tests"], 4);
        assert_eq!(final_stats["errors"], 0);
    }
}

#[test]
fn cancellation_retains_partial_bytes_and_releases_active_sockets() {
    let mut server = Server::start("127.0.0.1", "");
    let c = cancel();
    let client_cancel = c.clone();
    let mut req = server.client(true, false, 1);
    req.duration_secs = 60;
    let (tx, rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        run(&req, &client_cancel, &|v| {
            if v["event_type"] == "stats" {
                let _ = tx.send(());
            }
        })
    });
    rx.recv_timeout(Duration::from_secs(3)).unwrap();
    let began = Instant::now();
    c.store(true, Ordering::Relaxed);
    let error = worker.join().unwrap().unwrap_err();
    assert_eq!(error.code, "CANCELLED");
    assert!(error.data.unwrap()["bytes"].as_u64().unwrap() > 0);
    assert!(began.elapsed() < Duration::from_secs(1));
    server.stop();
}

#[test]
fn stopping_server_interrupts_active_test_and_retains_statistics() {
    let mut server = Server::start("127.0.0.1", "");
    let mut request = server.client(false, true, 1);
    request.duration_secs = 60;
    let worker = std::thread::spawn(move || run(&request, &cancel(), &|_| {}));
    let until = Instant::now() + Duration::from_secs(4);
    while !server
        .events
        .lock()
        .unwrap()
        .iter()
        .any(|v| v["event_type"] == "stats")
    {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(server.stop().data.unwrap()["bytes"].as_u64().unwrap() > 0);
    assert!(worker.join().unwrap().is_err());
}

#[test]
fn malformed_or_unsupported_sessions_do_not_kill_server() {
    let mut server = Server::start("127.0.0.1", "");
    for payload in [
        None,
        Some(serde_json::json!({"tcp":true,"bidirectional":true})),
    ] {
        let mut peer = TcpStream::connect(("127.0.0.1", server.request.port)).unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        peer.write_all(&[0; 37]).unwrap();
        let mut state = [0];
        peer.read_exact(&mut state).unwrap();
        assert_eq!(state, [9]);
        if let Some(ref payload) = payload {
            let bytes = serde_json::to_vec(&payload).unwrap();
            peer.write_all(&(bytes.len() as u32).to_be_bytes()).unwrap();
            peer.write_all(&bytes).unwrap();
            let mut response = [0; 9];
            peer.read_exact(&mut response).unwrap();
            assert_eq!(response[0], 254);
        } else {
            peer.write_all(&u32::MAX.to_be_bytes()).unwrap();
            let mut end = [0];
            assert_eq!(peer.read(&mut end).unwrap(), 0);
        }
        drop(peer);
        // Observe recovery before starting the next session.
        let wanted = if payload.is_none() { 1 } else { 2 };
        let until = Instant::now() + Duration::from_secs(2);
        while server
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|v| v["event_type"] == "session_error")
            .count()
            < wanted
        {
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    assert_eq!(
        run(&server.client(false, false, 1), &cancel(), &|_| {}).unwrap()["success"],
        true
    );
    server.stop();
}

#[test]
fn idle_partial_handshake_cancel_source_restriction_and_port_conflict() {
    let mut server = Server::start("127.0.0.1", "");
    let _peer = TcpStream::connect(("127.0.0.1", server.request.port)).unwrap();
    let error = run(&server.request, &cancel(), &|_| {}).unwrap_err();
    assert_eq!(error.code, "ADDRESS_IN_USE");
    server.stop();
    let mut server = Server::start("127.0.0.1", "192.0.2.0/24");
    assert_eq!(
        run(&server.client(false, false, 1), &cancel(), &|_| {})
            .unwrap_err()
            .code,
        "SERVER_BUSY"
    );
    server.stop();
    let mut server = Server::start("127.0.0.1", "");
    server.stop();
}

#[test]
fn finite_server_duration_expires_during_partial_handshake() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let req = Request {
        tool: "iperf3".into(),
        action: "serve".into(),
        listen: "127.0.0.1".into(),
        port,
        duration_secs: 1,
        timeout_ms: 5000,
        ..Request::default()
    };
    let (tx, rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        run(&req, &cancel(), &|v| {
            if v["event_type"] == "ready" {
                tx.send(()).unwrap();
            }
        })
    });
    rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let _peer = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let began = Instant::now();
    assert_eq!(worker.join().unwrap().unwrap()["completed_tests"], 0);
    assert!(began.elapsed() < Duration::from_secs(2));
    drop(TcpListener::bind(("127.0.0.1", port)).unwrap());
}
