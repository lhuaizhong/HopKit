use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
fn cancel() -> Cancel {
    Arc::new(AtomicBool::new(false))
}
fn data(c: Config, mut probe: impl FnMut(u32, &str) -> ToolResult) -> ResultData {
    let header = if c.family == "ipv6" { 48 } else { 28 };
    let out = run_with(&c, &cancel(), &|_| {}, &mut |_, payload, _, _| {
        probe(payload + header, "")
    });
    serde_json::from_value(match out {
        Ok(d) => d,
        Err(e) => e.data.unwrap(),
    })
    .unwrap()
}
#[test]
fn exact_bounds_for_1500_1492_1400_v4_and_v6() {
    for v6 in [false, true] {
        for mtu in [1500, 1492, 1400] {
            let c = Config {
                target: if v6 { "::1" } else { "127.0.0.1" }.into(),
                family: if v6 { "ipv6" } else { "ipv4" }.into(),
                min_ip_bytes: if v6 { 1280 } else { 576 },
                max_ip_bytes: 1600,
                ..Default::default()
            };
            let d = data(c, |size, _| {
                Ok(if size <= mtu {
                    json!({"status":"reply"})
                } else {
                    json!({"status":"packet_too_big","reported_mtu":mtu})
                })
            });
            assert_eq!(d.confirmed_lower, Some(mtu));
            assert_eq!(d.confirmed_upper, Some(mtu + 1));
            assert!(d.success && !d.partial);
            assert_eq!(d.probes.last().unwrap().phase, "verify_upper");
            for p in d.probes {
                assert_eq!(p.ip_bytes, p.ip_header_bytes + 8 + p.payload_bytes);
            }
        }
    }
}
#[test]
fn silence_never_creates_a_failure_boundary() {
    for status in ["timeout", "unreachable", "ttl_expired"] {
        let d = data(Config::default(), |size, _| {
            Ok(json!({"status":if size==576{"reply"}else{status}}))
        });
        assert_eq!(d.confirmed_lower, Some(576));
        assert_eq!(d.confirmed_upper, None);
        assert!(!d.success && d.partial);
        assert!(d.unknown_reasons.contains(&status.to_string()));
    }
}
#[test]
fn failed_baseline_is_unknown_and_upper_only_on_explicit_error() {
    for (status, upper) in [("timeout", None), ("local_too_big", Some(576))] {
        let d = data(Config::default(), |_, _| Ok(json!({"status":status})));
        assert_eq!(d.confirmed_lower, None);
        assert_eq!(d.confirmed_upper, upper);
        assert!(!d.success);
    }
}
#[test]
fn all_success_is_only_at_least_the_requested_maximum() {
    let d = data(Config::default(), |_, _| Ok(json!({"status":"reply"})));
    assert_eq!(d.diagnostic_outcome, "at_least");
    assert_eq!(d.confirmed_lower, Some(1500));
    assert_eq!(d.confirmed_upper, None);
    assert!(d.success);
}
#[test]
fn timeouts_inside_known_interval_leave_both_bounds_intact() {
    let d = data(Config::default(), |n, _| {
        Ok(json!({"status":if n==576{"reply"}else if n==1500{"packet_too_big"}else{"timeout"}}))
    });
    assert_eq!(d.confirmed_lower, Some(576));
    assert_eq!(d.confirmed_upper, Some(1500));
    assert!(!d.success);
}
#[test]
fn changing_path_during_final_verification_invalidates_interval() {
    let mut big = 0;
    let d = data(Config::default(), |n, _| {
        if n == 1500 {
            big += 1;
        }
        Ok(json!({"status":if n==1500&&big>2{"packet_too_big"}else{"reply"}}))
    });
    assert_eq!(d.confirmed_lower, None);
    assert!(!d.success);
    assert!(d.unknown_reasons.iter().any(|s| s.contains("path_changed")));
}
#[test]
fn cancellation_error_keeps_accumulated_evidence_and_ordered_events() {
    let c = Config::default();
    let stop = cancel();
    let seq = std::sync::Mutex::new(vec![]);
    let out = run_with(
        &c,
        &stop,
        &|e| seq.lock().unwrap().push(e["event_seq"].as_u64().unwrap()),
        &mut |_, _, _, c| {
            c.store(true, Ordering::Relaxed);
            Ok(json!({"status":"reply"}))
        },
    );
    let e = out.unwrap_err();
    assert_eq!(e.code, "CANCELLED");
    assert_eq!(e.data.unwrap()["stats"]["attempts"], 1);
    assert!(seq.lock().unwrap().windows(2).all(|s| s[1] == s[0] + 1));
}
#[test]
fn deadline_and_local_failures_keep_partial_result() {
    let c = Config {
        total_timeout_ms: 1000,
        ..Default::default()
    };
    let started = Instant::now();
    let d = data(c, |_, _| {
        std::thread::sleep(Duration::from_millis(400));
        Ok(json!({"status":"timeout"}))
    });
    assert!(d.partial);
    assert!(!d.success);
    assert!(started.elapsed() < Duration::from_secs(2));
    let c = Config::default();
    let e = run_with(&c, &cancel(), &|_| {}, &mut |_, _, _, _| {
        Err(ToolError::new("PERMISSION_DENIED", "fixture"))
    })
    .unwrap_err();
    assert_eq!(e.code, "PERMISSION_DENIED");
    assert!(e.data.is_some());
}
#[test]
fn config_and_request_mismatch_are_rejected_before_io() {
    for c in [
        Config {
            target: "http://example.com".into(),
            ..Default::default()
        },
        Config {
            target: "127.0.0.1:443".into(),
            ..Default::default()
        },
        Config {
            min_ip_bytes: 67,
            ..Default::default()
        },
        Config {
            max_ip_bytes: 65536,
            ..Default::default()
        },
        Config {
            retries: 4,
            ..Default::default()
        },
        Config {
            family: "auto".into(),
            ..Default::default()
        },
        Config {
            source: "::1".into(),
            ..Default::default()
        },
    ] {
        assert!(validate(&c).is_err());
    }
    let r = Request {
        tool: "ping".into(),
        pmtu: Some(Default::default()),
        ..Default::default()
    };
    assert!(r.validate_tool_config().is_err());
}
