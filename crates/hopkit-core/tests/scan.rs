use hopkit_core::{
    scan::{self, Config},
    *,
};
use std::{
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
fn cancel() -> Cancel {
    Arc::new(AtomicBool::new(false))
}
#[test]
fn independent_listener_refusal_source_and_dual_stack() {
    for host in ["127.0.0.1", "::1"] {
        let open = TcpListener::bind((host, 0)).unwrap();
        let port = open.local_addr().unwrap().port();
        let closed = TcpListener::bind((host, 0)).unwrap();
        let refused = closed.local_addr().unwrap().port();
        drop(closed);
        let peer = std::thread::spawn(move || open.accept().unwrap().0.peer_addr().unwrap());
        let value = scan::run(
            &Config {
                targets: vec![host.into(), host.into()],
                ports: format!("{port},{refused},{port}"),
                source: host.into(),
                ..Default::default()
            },
            &cancel(),
            &|_| {},
        )
        .unwrap();
        assert_eq!(value["stats"]["total"], 2);
        assert_eq!(value["stats"]["counts"]["open"], 1);
        assert_eq!(value["stats"]["counts"]["refused"], 1);
        assert!(!value["partial"].as_bool().unwrap());
        let actual = peer.join().unwrap();
        assert_eq!(actual.ip().to_string(), host);
        let row = value["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["status"] == "open")
            .unwrap();
        assert!(row["local_endpoint"].as_str().unwrap().contains(host));
    }
    let value = scan::run(
        &Config {
            targets: vec!["localhost".into()],
            ports: "65535".into(),
            ..Default::default()
        },
        &cancel(),
        &|_| {},
    )
    .unwrap();
    assert!(!value["candidates"].as_array().unwrap().is_empty());
}
#[test]
fn stop_is_prompt_and_retains_partial_results() {
    let c = cancel();
    let worker_c = c.clone();
    let begin = Instant::now();
    let worker = std::thread::spawn(move || {
        scan::run(
            &Config {
                ports: "1-100".into(),
                rate: 1,
                ..Default::default()
            },
            &worker_c,
            &|_| {},
        )
    });
    std::thread::sleep(Duration::from_millis(150));
    c.store(true, Ordering::Relaxed);
    let e = worker.join().unwrap().unwrap_err();
    assert_eq!(e.code, "CANCELLED");
    assert!(begin.elapsed() < Duration::from_secs(1));
    let d = e.data.unwrap();
    assert!(d["partial"].as_bool().unwrap());
    assert!(d["stats"]["remaining"].as_u64().unwrap() > 0);
    let e = scan::run(
        &Config {
            targets: vec!["127.0.0.1".into(), "::1".into()],
            source: "127.0.0.1".into(),
            ..Default::default()
        },
        &cancel(),
        &|_| {},
    )
    .unwrap_err();
    assert_eq!(e.code, "INVALID_ARGUMENT");
}
