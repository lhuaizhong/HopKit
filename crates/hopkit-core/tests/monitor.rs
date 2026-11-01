use hopkit_core::{
    monitor::{Config, Control, Target},
    *,
};
use serde_json::{Value, json};
use std::{
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[test]
fn real_failures_recover_pause_resume_notes_and_cancel_keep_evidence() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    let config = Config {
        targets: vec![Target {
            label: "business".into(),
            protocol: "tcping".into(),
            target: "127.0.0.1".into(),
            port,
        }],
        timeout_ms: 100,
        interval_ms: 1000,
        ..Config::default()
    };
    let control = Arc::new(Control::default());
    let cancel = Arc::new(AtomicBool::new(false));
    let snapshots = Arc::new(Mutex::new(Vec::<Value>::new()));
    let worker = {
        let control = control.clone();
        let cancel = cancel.clone();
        let snapshots = snapshots.clone();
        std::thread::spawn(move || {
            monitor::run(
                &Request {
                    tool: "monitor".into(),
                    monitor: Some(config),
                    ..Request::default()
                },
                &cancel,
                &|v| snapshots.lock().unwrap().push(v),
                &control,
            )
        })
    };
    let wait_for = |predicate: &dyn Fn(&Value) -> bool| {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if snapshots.lock().unwrap().last().is_some_and(predicate) {
                return;
            }
            assert!(Instant::now() < deadline, "Timed out waiting for monitor");
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    wait_for(&|v| v["totals"][0]["failed"].as_u64().unwrap_or(0) > 0);
    let listener = TcpListener::bind(addr).unwrap();
    listener.set_nonblocking(true).unwrap();
    let server_cancel = cancel.clone();
    let server = std::thread::spawn(move || {
        while !server_cancel.load(Ordering::Relaxed) {
            if let Ok((s, _)) = listener.accept() {
                drop(s);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    });
    wait_for(&|v| v["totals"][0]["succeeded"].as_u64().unwrap_or(0) > 0);
    control
        .send("note".into(), "现场检查 <script>literal</script>".into())
        .unwrap();
    control
        .send(
            "network_change".into(),
            "Switch to backup interface (manual)".into(),
        )
        .unwrap();
    control.send("pause".into(), "".into()).unwrap();
    wait_for(&|v| v["paused"] == true);
    let count = snapshots.lock().unwrap().last().unwrap()["sample_count"].clone();
    std::thread::sleep(Duration::from_millis(1200));
    assert_eq!(
        snapshots.lock().unwrap().last().unwrap()["sample_count"],
        count
    );
    control.send("resume".into(), "".into()).unwrap();
    wait_for(&|v| v["sample_count"].as_u64() > count.as_u64());
    cancel.store(true, Ordering::Relaxed);
    let result = worker.join().unwrap().unwrap_err();
    server.join().unwrap();
    assert_eq!(result.code, "CANCELLED");
    let data = result.data.unwrap();
    let records = data["records"].as_array().unwrap();
    assert!(
        records
            .iter()
            .any(|e| e["end_reason"] == "observed_recovery")
    );
    assert!(
        records
            .iter()
            .any(|e| e["kind"] == "measurement_gap" && e["reason"] == "manual_pause")
    );
    assert!(records.iter().any(|e| e["kind"] == "note"));
    assert!(records.iter().any(|e| e["kind"] == "network_change"));
    assert_eq!(data["sample_count"], data["totals"][0]["completed"]);
}

#[test]
fn cancelled_before_first_probe_preserves_empty_result() {
    let config = Config {
        targets: vec![Target {
            label: "local".into(),
            protocol: "ping".into(),
            target: "127.0.0.1".into(),
            port: 0,
        }],
        ..Config::default()
    };
    let result = run(
        &Request {
            tool: "monitor".into(),
            monitor: Some(config),
            ..Request::default()
        },
        &Arc::new(AtomicBool::new(true)),
        &|_| {},
    )
    .unwrap_err();
    assert_eq!(result.code, "CANCELLED");
    assert_eq!(result.data.unwrap()["sample_count"], json!(0));
}
