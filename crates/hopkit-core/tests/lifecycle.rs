#![allow(clippy::field_reassign_with_default)]
use hopkit_core::*;
use std::{
    net::UdpSocket,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

fn cancel() -> Cancel {
    Arc::new(AtomicBool::new(false))
}
fn dir() -> PathBuf {
    let p = std::env::temp_dir().join(format!("hopkit-lifecycle-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&p).unwrap();
    p
}
fn port() -> u16 {
    UdpSocket::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

#[test]
fn syslog_cancel_releases_socket_and_directory() {
    let path = dir();
    let port = port();
    let c = cancel();
    let task_cancel = c.clone();
    let (mut request, output) = (Request::default(), path.to_string_lossy().into_owned());
    request.tool = "syslog".into();
    request.action = "listen".into();
    request.path = output;
    request.port = port;
    let ready = Arc::new(AtomicBool::new(false));
    let signalled = ready.clone();
    let req = request.clone();
    let worker = std::thread::spawn(move || {
        run(&req, &task_cancel, &|v| {
            if v["event_type"] == "ready" {
                signalled.store(true, Ordering::Relaxed);
            }
        })
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    while !ready.load(Ordering::Relaxed) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(ready.load(Ordering::Relaxed));
    let start = Instant::now();
    c.store(true, Ordering::Relaxed);
    assert_eq!(worker.join().unwrap().unwrap_err().code, "CANCELLED");
    assert!(start.elapsed() < Duration::from_secs(1));
    let socket = UdpSocket::bind(("127.0.0.1", port)).unwrap();
    drop(socket);
    let lock = hopkit_core::syslog::lock_dir(&path.join("hopkit-syslog")).unwrap();
    drop(lock);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn tftp_cancel_releases_listener() {
    let path = dir();
    let port = port();
    let c = cancel();
    let cc = c.clone();
    let mut r = Request::default();
    r.tool = "tftp".into();
    r.action = "serve".into();
    r.path = path.to_string_lossy().into_owned();
    r.port = port;
    let (tx, rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        run(&r, &cc, &|v| {
            if v["event_type"] == "ready" {
                let _ = tx.send(());
            }
        })
    });
    rx.recv_timeout(Duration::from_secs(3)).unwrap();
    c.store(true, Ordering::Relaxed);
    assert_eq!(worker.join().unwrap().unwrap_err().code, "CANCELLED");
    drop(UdpSocket::bind(("127.0.0.1", port)).unwrap());
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn diagnostic_cancellation_does_not_wait_for_interval() {
    let c = cancel();
    let cc = c.clone();
    let mut r = Request::default();
    r.count = 0;
    r.interval_ms = 60000;
    let (tx, rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        run(&r, &cc, &|v| {
            if v["event_type"] == "sample" {
                let _ = tx.send(());
            }
        })
    });
    rx.recv_timeout(Duration::from_secs(3)).unwrap();
    let start = Instant::now();
    c.store(true, Ordering::Relaxed);
    assert_eq!(worker.join().unwrap().unwrap_err().code, "CANCELLED");
    assert!(start.elapsed() < Duration::from_secs(1));
}
#[test]
fn duplicate_directory_lock_is_rejected() {
    let p = dir();
    let first = hopkit_core::syslog::lock_dir(&p).unwrap();
    assert_eq!(
        hopkit_core::syslog::lock_dir(&p).unwrap_err().code,
        "DIRECTORY_IN_USE"
    );
    drop(first);
    std::fs::remove_dir_all(p).unwrap();
}
#[test]
fn tftp_unknown_peer_cancellation_leaves_no_final_file() {
    let path = dir();
    let dest = path.join("destination.cfg");
    let listener = UdpSocket::bind("127.0.0.1:0").unwrap();
    listener
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let c = cancel();
    let cc = c.clone();
    let mut r = Request::default();
    r.tool = "tftp".into();
    r.action = "get".into();
    r.target = "127.0.0.1".into();
    r.port = port;
    r.path = dest.to_string_lossy().into_owned();
    r.remote_name = "remote.cfg".into();
    let worker = std::thread::spawn(move || run(&r, &cc, &|_| {}));
    let mut b = [0; 1024];
    let (_, peer) = listener.recv_from(&mut b).unwrap();
    let mut data = vec![0, 3, 0, 1];
    data.extend([1_u8; 512]);
    listener.send_to(&data, peer).unwrap();
    let _ = listener.recv_from(&mut b).unwrap();
    c.store(true, Ordering::Relaxed);
    assert_eq!(worker.join().unwrap().unwrap_err().code, "CANCELLED");
    assert!(!dest.exists());
    assert!(
        !std::fs::read_dir(&path).unwrap().any(|x| x
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".part"))
    );
    std::fs::remove_dir_all(path).unwrap();
}
