use hopkit_core::{
    socket_tool::{Config, Control, SendCommand},
    *,
};
use std::{
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream, UdpSocket},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
fn cancel() -> Cancel {
    Arc::new(AtomicBool::new(false))
}
fn send(id: Option<u64>, data: &str) -> SendCommand {
    SendCommand {
        operation: "send".into(),
        session_id: id,
        destination: None,
        encoding: "hex".into(),
        data: data.into(),
        newline: "none".into(),
        count: 1,
        interval_ms: 1000,
    }
}
fn until(mut f: impl FnMut() -> bool) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(4), "condition timed out");
        thread::sleep(Duration::from_millis(10));
    }
}
fn port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}
#[test]
fn wol_exact_bytes_repetition_source_interval_and_cancellation() {
    let peer = UdpSocket::bind("127.0.0.1:0").unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let config = wol::Config {
        mac: "aabb.ccdd.ee02".into(),
        destination: "127.0.0.1".into(),
        source: "127.0.0.1".into(),
        port: peer.local_addr().unwrap().port(),
        ..Default::default()
    };
    let c = cancel();
    let worker = thread::spawn(move || wol::run(&config, &c, &|_| {}).unwrap());
    let mut bytes = [0u8; 200];
    let mut times = Vec::new();
    for _ in 0..3 {
        let (n, from) = peer.recv_from(&mut bytes).unwrap();
        times.push(Instant::now());
        assert_eq!(n, 102);
        assert_eq!(&bytes[..6], &[255; 6]);
        assert_eq!(
            &bytes[6..102],
            &[0xaa, 0xbb, 0xcc, 0xdd, 0xee, 2].repeat(16)
        );
        assert!(from.ip().is_loopback());
    }
    assert!(times[2].duration_since(times[0]) >= Duration::from_millis(350));
    let r = worker.join().unwrap();
    assert_eq!(r["sent"], 3);
    assert!(r.get("wake_success").is_none());
    let c = cancel();
    let wc = c.clone();
    let p = peer.local_addr().unwrap().port();
    let worker = thread::spawn(move || {
        wol::run(
            &wol::Config {
                mac: "AA:BB:CC:DD:EE:02".into(),
                destination: "127.0.0.1".into(),
                port: p,
                count: 100,
                ..Default::default()
            },
            &wc,
            &|_| {},
        )
    });
    peer.recv_from(&mut bytes).unwrap();
    let at = Instant::now();
    c.store(true, Ordering::Relaxed);
    assert_eq!(worker.join().unwrap().unwrap_err().code, "CANCELLED");
    assert!(at.elapsed() < Duration::from_secs(1));
}
#[test]
fn invalid_inputs_and_config_identity_before_io() {
    for mac in [
        "FF:FF:FF:FF:FF:FF",
        "01:00:00:00:00:01",
        "00:00:00:00:00:00",
        "zabbccddeeff",
        "éééééé",
    ] {
        assert!(wol::packet(mac).is_err());
    }
    for data in ["0", "0x12", "gg", "中"] {
        assert!(socket_tool::decode(&send(None, data)).is_err());
    }
    assert_eq!(
        socket_tool::decode(&send(None, "00 ff 01")).unwrap(),
        [0, 255, 1]
    );
    let request = Request {
        tool: "ping".into(),
        socket: Some(Default::default()),
        ..Default::default()
    };
    assert_eq!(
        run(&request, &cancel(), &|_| {}).unwrap_err().code,
        "INVALID_ARGUMENT"
    );
    assert!(
        socket_tool::validate(&Config {
            echo: true,
            mode: "listen".into(),
            ..Default::default()
        })
        .is_err()
    );
    let ctrl = Control::default();
    assert_eq!(
        ctrl.send(send(Some(1), "00")).unwrap_err().code,
        "TASK_STOPPED"
    );
}
#[test]
fn tcp_binary_stream_half_close_source_and_no_payload_in_result() {
    for host in ["127.0.0.1", "::1"] {
        let listener = TcpListener::bind((host, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let peer = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            s.write_all(&[0, 0xf0, 0x9f]).unwrap();
            s.write_all(&[0x98, 0x80, 255]).unwrap();
            s.shutdown(Shutdown::Write).unwrap();
            let mut b = [0; 3];
            s.read_exact(&mut b).unwrap();
            assert_eq!(b, [0, 255, 1]);
        });
        let ctrl = Arc::new(Control::default());
        let worker_ctrl = ctrl.clone();
        let c = cancel();
        let wc = c.clone();
        let config = Config {
            target: host.into(),
            port,
            source: host.into(),
            duration_secs: 5,
            ..Default::default()
        };
        let worker = thread::spawn(move || socket_tool::run(&config, &wc, &|_| {}, &worker_ctrl));
        until(|| ctrl.snapshot(false)["sessions"][0]["read_closed"] == true);
        ctrl.send(send(Some(1), "00ff01")).unwrap();
        peer.join().unwrap();
        until(|| ctrl.snapshot(false)["stats"]["sent_bytes"] == 3);
        let snapshot = ctrl.snapshot(true);
        let received = snapshot["records"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["kind"] == "received_stream_bytes")
            .map(|r| r["hex"].as_str().unwrap())
            .collect::<String>();
        assert_eq!(received, "00f09f9880ff");
        let at = Instant::now();
        c.store(true, Ordering::Relaxed);
        let error = worker.join().unwrap().unwrap_err();
        assert_eq!(error.code, "CANCELLED");
        assert!(at.elapsed() < Duration::from_secs(1));
        assert!(error.data.unwrap().get("records").is_none());
        assert_eq!(
            ctrl.send(send(Some(1), "00")).unwrap_err().code,
            "TASK_STOPPED"
        );
    }
}
#[test]
fn udp_empty_datagrams_boundaries_echo_source_and_preview_budget() {
    for host in ["127.0.0.1", "::1"] {
        let peer = UdpSocket::bind((host, 0)).unwrap();
        let local = UdpSocket::bind((host, 0)).unwrap().local_addr().unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let ctrl = Arc::new(Control::default());
        let wc = ctrl.clone();
        let c = cancel();
        let cc = c.clone();
        let config = Config {
            protocol: "udp".into(),
            mode: "listen".into(),
            listen: host.into(),
            port: local.port(),
            preview_bytes: 1024,
            echo: true,
            allow: if host == "::1" {
                "::1/128"
            } else {
                "127.0.0.1/32"
            }
            .into(),
            rate: 100,
            ..Default::default()
        };
        let worker = thread::spawn(move || socket_tool::run(&config, &cc, &|_| {}, &wc));
        until(|| ctrl.snapshot(false)["active"] == true);
        let mut bytes = [0u8; 10];
        peer.send_to(&[], local).unwrap();
        assert_eq!(peer.recv_from(&mut bytes).unwrap().0, 0);
        peer.send_to(&[0, 255], local).unwrap();
        assert_eq!(peer.recv_from(&mut bytes).unwrap().0, 2);
        for _ in 0..20 {
            peer.send_to(&[42; 500], local).unwrap();
        }
        until(|| {
            ctrl.snapshot(false)["stats"]["datagrams_received"]
                .as_u64()
                .unwrap_or(0)
                >= 22
        });
        assert!(
            ctrl.snapshot(false)["stats"]["preview_dropped"]
                .as_u64()
                .unwrap()
                > 0
        );
        assert!(ctrl.snapshot(true)["records"].to_string().len() < 1100);
        let mut oversize = send(None, &"00".repeat(65508));
        oversize.destination = Some(peer.local_addr().unwrap().to_string());
        assert_eq!(ctrl.send(oversize).unwrap_err().code, "INVALID_ARGUMENT");
        c.store(true, Ordering::Relaxed);
        worker.join().unwrap().unwrap_err();
    }
}
#[test]
fn tcp_listener_session_limit_source_deny_port_conflict_and_rebind() {
    let p = port();
    let config = Config {
        mode: "listen".into(),
        listen: "127.0.0.1".into(),
        port: p,
        max_sessions: 1,
        ..Default::default()
    };
    let ctrl = Arc::new(Control::default());
    let wc = ctrl.clone();
    let c = cancel();
    let cc = c.clone();
    let worker = thread::spawn(move || socket_tool::run(&config, &cc, &|_| {}, &wc));
    until(|| ctrl.snapshot(false)["active"] == true);
    assert_eq!(
        socket_tool::run(
            &Config {
                mode: "listen".into(),
                listen: "127.0.0.1".into(),
                port: p,
                ..Default::default()
            },
            &cancel(),
            &|_| {},
            &Default::default()
        )
        .unwrap_err()
        .code,
        "ADDRESS_IN_USE"
    );
    let _a = TcpStream::connect(("127.0.0.1", p)).unwrap();
    until(|| {
        !ctrl.snapshot(false)["sessions"]
            .as_array()
            .unwrap()
            .is_empty()
    });
    let _b = TcpStream::connect(("127.0.0.1", p)).unwrap();
    until(|| {
        ctrl.snapshot(false)["stats"]["rejected_sessions"]
            .as_u64()
            .unwrap_or(0)
            > 0
    });
    _a.shutdown(Shutdown::Write).unwrap();
    until(|| ctrl.snapshot(false)["sessions"][0]["read_closed"] == true);
    let mut half_close = send(Some(1), "");
    half_close.operation = "shutdown_write".into();
    ctrl.send(half_close).unwrap();
    until(|| ctrl.snapshot(false)["sessions"][0]["closed"] == true);
    let _next = TcpStream::connect(("127.0.0.1", p)).unwrap();
    until(|| ctrl.snapshot(false)["sessions"][1]["id"] == 2);
    assert_eq!(ctrl.snapshot(false)["sessions"][1]["closed"], false);
    c.store(true, Ordering::Relaxed);
    worker.join().unwrap().unwrap_err();
    // Different family/source CIDR rejects the connection without creating a session.
    let p = port();
    let ctrl = Arc::new(Control::default());
    let wc = ctrl.clone();
    let c = cancel();
    let cc = c.clone();
    let worker = thread::spawn(move || {
        socket_tool::run(
            &Config {
                mode: "listen".into(),
                listen: "127.0.0.1".into(),
                port: p,
                allow: "192.0.2.0/24".into(),
                ..Default::default()
            },
            &cc,
            &|_| {},
            &wc,
        )
    });
    until(|| ctrl.snapshot(false)["active"] == true);
    let _a = TcpStream::connect(("127.0.0.1", p)).unwrap();
    until(|| {
        ctrl.snapshot(false)["stats"]["rejected_sources"]
            .as_u64()
            .unwrap_or(0)
            > 0
    });
    assert!(
        ctrl.snapshot(false)["sessions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    c.store(true, Ordering::Relaxed);
    worker.join().unwrap().unwrap_err();
    assert!(TcpListener::bind(("127.0.0.1", p)).is_ok());
}
#[test]
fn connections_observes_independent_v4_v6_endpoints_and_pid() {
    for host in ["127.0.0.1", "::1"] {
        let tcp = TcpListener::bind((host, 0)).unwrap();
        let udp = UdpSocket::bind((host, 0)).unwrap();
        let client = TcpStream::connect(tcp.local_addr().unwrap()).unwrap();
        let (_accepted, _) = tcp.accept().unwrap();
        for (protocol, port) in [
            ("tcp", tcp.local_addr().unwrap().port()),
            ("udp", udp.local_addr().unwrap().port()),
        ] {
            let result = connections::run(
                &connections::Config {
                    port: Some(port),
                    protocol: protocol.into(),
                    ..Default::default()
                },
                &cancel(),
                &|_| {},
            )
            .unwrap();
            assert!(!result["rows"].as_array().unwrap().is_empty());
            for row in result["rows"].as_array().unwrap() {
                if protocol == "udp" {
                    assert!(row["state"].is_null());
                }
                #[cfg(windows)]
                assert_eq!(row["pid"], std::process::id());
            }
        }
        drop(client);
    }
    assert_eq!(
        connections::run(
            &Default::default(),
            &Arc::new(AtomicBool::new(true)),
            &|_| {}
        )
        .unwrap_err()
        .code,
        "CANCELLED"
    );
}
#[test]
fn recording_create_new_cap_and_payload_free_snapshot() {
    let path = std::env::temp_dir().join(format!("hopkit-m4-{}.jsonl", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let p = port();
    let ctrl = Arc::new(Control::default());
    let wc = ctrl.clone();
    let c = cancel();
    let cc = c.clone();
    let file = path.to_string_lossy().to_string();
    let worker = thread::spawn(move || {
        socket_tool::run(
            &Config {
                mode: "listen".into(),
                listen: "127.0.0.1".into(),
                port: p,
                record_path: file,
                record_limit_bytes: 1024,
                ..Default::default()
            },
            &cc,
            &|_| {},
            &wc,
        )
    });
    until(|| ctrl.snapshot(false)["active"] == true);
    let mut peer = TcpStream::connect(("127.0.0.1", p)).unwrap();
    peer.write_all(&[42; 2000]).unwrap();
    until(|| ctrl.snapshot(false)["stats"]["recording_stopped"] == true);
    c.store(true, Ordering::Relaxed);
    worker.join().unwrap().unwrap_err();
    assert!(std::fs::metadata(&path).unwrap().len() <= 1024);
    std::fs::remove_file(path).unwrap();
    let r = Request {
        tool: "socket".into(),
        socket: Some(Config {
            record_path: "private-path".into(),
            ..Default::default()
        }),
        ..Default::default()
    };
    assert!(!r.snapshot().unwrap().to_string().contains("private-path"));
}
