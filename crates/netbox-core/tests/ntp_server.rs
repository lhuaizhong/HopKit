use netbox_core::*;
use serde_json::Value;
use std::{
    net::{SocketAddr, UdpSocket},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

struct Server {
    cancel: Cancel,
    worker: Option<JoinHandle<ToolResult>>,
    addr: SocketAddr,
    events: mpsc::Receiver<Value>,
}
impl Server {
    fn start(mut request: Request) -> Self {
        let ip = request.listen.parse::<std::net::IpAddr>().unwrap();
        let port = UdpSocket::bind((ip, 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        request.port = port;
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let (tx, events) = mpsc::channel();
        let worker = thread::spawn(move || {
            run(&request, &worker_cancel, &|v| {
                let _ = tx.send(v);
            })
        });
        let ready = events.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_eq!(ready["event_type"], "ready");
        assert_eq!(ready["clock_source"], "system");
        Self {
            cancel,
            worker: Some(worker),
            addr: ready["listen"].as_str().unwrap().parse().unwrap(),
            events,
        }
    }
    fn stop(&mut self) -> ToolResult {
        self.cancel.store(true, Ordering::Relaxed);
        self.worker.take().unwrap().join().unwrap()
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
fn request() -> Request {
    Request {
        tool: "ntp".into(),
        action: "serve".into(),
        listen: "127.0.0.1".into(),
        ..Request::default()
    }
}
fn client(addr: SocketAddr) -> UdpSocket {
    let socket = UdpSocket::bind(if addr.is_ipv6() {
        "[::1]:0"
    } else {
        "127.0.0.1:0"
    })
    .unwrap();
    socket.connect(addr).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_millis(300)))
        .unwrap();
    socket
}
fn stamp() -> u64 {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    ((now.as_secs() + 2_208_988_800) << 32)
        | ((u64::from(now.subsec_nanos()) << 32) / 1_000_000_000)
}
fn packet(version: u8) -> [u8; 48] {
    let mut packet = [0; 48];
    packet[0] = version << 3 | 3;
    packet[2] = 6;
    packet[40..48].copy_from_slice(&stamp().to_be_bytes());
    packet
}

#[test]
fn v3_v4_clients_receive_system_time_and_cancellation_releases_port() {
    let mut server = Server::start(request());
    let socket = client(server.addr);
    for version in [3, 4] {
        let query = packet(version);
        let before = stamp();
        socket.send(&query).unwrap();
        let mut response = [0; 512];
        assert_eq!(socket.recv(&mut response).unwrap(), 48);
        let after = stamp();
        assert_eq!(response[0], version << 3 | 4);
        assert_eq!(response[1], 10);
        assert_eq!(&response[24..32], &query[40..48]);
        let receive = u64::from_be_bytes(response[32..40].try_into().unwrap());
        let transmit = u64::from_be_bytes(response[40..48].try_into().unwrap());
        assert!(before <= receive && receive <= transmit && transmit <= after);
    }
    let query = Request {
        tool: "ntp".into(),
        target: "127.0.0.1".into(),
        port: server.addr.port(),
        count: 1,
        ..Request::default()
    };
    let result = run(&query, &Arc::new(AtomicBool::new(false)), &|_| {}).unwrap();
    assert_eq!(result["valid_samples"], 1);
    assert_eq!(result["best_sample"]["stratum"], 10);
    let start = Instant::now();
    assert_eq!(server.stop().unwrap_err().code, "CANCELLED");
    assert!(start.elapsed() < Duration::from_secs(1));
    drop(UdpSocket::bind(server.addr).unwrap());
    let stopped = server.events.try_iter().last().unwrap();
    assert_eq!(stopped["event_type"], "stopped");
    assert_eq!(stopped["replied"], 3);
}

#[test]
fn invalid_packets_are_ignored_and_finite_server_reports_totals() {
    let mut server = Server::start(Request {
        duration_secs: 1,
        ..request()
    });
    let socket = client(server.addr);
    for bytes in [
        vec![],
        vec![0x23; 47],
        vec![0x23; 68],
        vec![0x26; 48],
        vec![0x13; 48],
    ] {
        socket.send(&bytes).unwrap();
    }
    let query = packet(4);
    socket.send(&query).unwrap();
    let mut response = [0; 512];
    assert_eq!(socket.recv(&mut response).unwrap(), 48);
    assert_eq!(&response[24..32], &query[40..48]);
    assert!(socket.recv(&mut response).is_err());
    let result = server.worker.take().unwrap().join().unwrap().unwrap();
    assert_eq!(result["received"], 6);
    assert_eq!(result["replied"], 1);
    assert_eq!(result["invalid"], 5);
    assert_eq!(result["send_errors"], 0);
    assert_eq!(result["clock_changed"], false);
    drop(UdpSocket::bind(server.addr).unwrap());
}

#[test]
fn source_cidr_denies_requests_without_reply() {
    let mut server = Server::start(Request {
        allow: "192.0.2.0/24".into(),
        ..request()
    });
    let socket = client(server.addr);
    socket.send(&packet(4)).unwrap();
    assert!(socket.recv(&mut [0; 512]).is_err());
    assert_eq!(server.stop().unwrap_err().code, "CANCELLED");
    let stopped = server.events.try_iter().last().unwrap();
    assert_eq!(stopped["received"], 1);
    assert_eq!(stopped["denied"], 1);
    assert_eq!(stopped["replied"], 0);
}

#[test]
fn ipv6_and_primary_local_clock_reference() {
    let mut server = Server::start(Request {
        listen: "::1".into(),
        allow: "::1/128".into(),
        ntp_stratum: 1,
        count: 0,
        ..request()
    });
    let socket = client(server.addr);
    socket.send(&packet(4)).unwrap();
    let mut response = [0; 512];
    assert_eq!(socket.recv(&mut response).unwrap(), 48);
    assert_eq!(response[1], 1);
    assert_eq!(&response[12..16], b"LOCL");
    assert_eq!(server.stop().unwrap_err().code, "CANCELLED");
}

#[test]
fn bad_configuration_and_port_conflict_return_structured_errors() {
    let cancel = Arc::new(AtomicBool::new(false));
    for request in [
        Request {
            ntp_stratum: 0,
            ..request()
        },
        Request {
            ntp_stratum: 16,
            ..request()
        },
        Request {
            listen: "localhost".into(),
            ..request()
        },
        Request {
            allow: "invalid".into(),
            ..request()
        },
        Request {
            action: "invalid".into(),
            ..request()
        },
    ] {
        assert_eq!(
            run(&request, &cancel, &|_| {}).unwrap_err().code,
            "INVALID_ARGUMENT"
        );
    }
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let request = Request {
        port: socket.local_addr().unwrap().port(),
        ..request()
    };
    assert_eq!(
        run(&request, &cancel, &|_| {}).unwrap_err().code,
        "ADDRESS_IN_USE"
    );
    let legacy: Request = serde_json::from_str(r#"{"tool":"ntp"}"#).unwrap();
    assert_eq!(legacy.ntp_stratum, 10);
}
