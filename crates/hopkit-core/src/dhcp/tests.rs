use super::*;
#[cfg(windows)]
#[test]
fn standard_ports_independent_peers_and_exclusive_bind() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let peer = UdpSocket::bind("127.0.0.1:67").unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let iface = Interface {
        id: "loopback-test".into(),
        name: "loopback".into(),
        index: 1,
        mac: Some("02:03:04:05:06:07".into()),
        dhcp_enabled: None,
        server: None,
        lease_start: None,
        lease_expires: None,
        addresses: vec![],
        gateways: vec![],
        dns: vec![],
        search_domains: vec![],
        config_source: "test-only".into(),
        field_sources: Default::default(),
        limitations: vec![],
    };
    let client = platform::socket(&iface).unwrap();
    let conflict = platform::socket(&iface).unwrap_err();
    assert_eq!(conflict.code, "ADDRESS_IN_USE");
    let thread = std::thread::spawn(move || {
        let mut b = [0; 1500];
        let (n, remote) = peer.recv_from(&mut b).unwrap();
        assert_eq!(remote.port(), 68);
        assert_eq!(n, 300);
        assert_eq!(b[240..243], [53, 1, 1]);
        assert_eq!(b[10], 128);
        // Independent BOOTP reply construction; no product encoder is used.
        let mut p = vec![0; 240];
        p[..3].copy_from_slice(&[2, 1, 6]);
        p[4..8].copy_from_slice(&b[4..8]);
        p[28..34].copy_from_slice(&b[28..34]);
        p[16..20].copy_from_slice(&[192, 0, 2, 20]);
        p[24..28].copy_from_slice(&[198, 51, 100, 1]);
        p[236..240].copy_from_slice(&[99, 130, 83, 99]);
        p.extend_from_slice(&[53, 1, 2, 54, 4, 192, 0, 2, 1, 51, 4, 0, 0, 14, 16, 255]);
        let mut bad = p.clone();
        bad[4] ^= 1;
        peer.send_to(&bad, remote).unwrap();
        peer.send_to(&[2, 1, 6], remote).unwrap();
        peer.send_to(&[0; 5000], remote).unwrap();
        peer.send_to(&p, remote).unwrap();
        peer.send_to(&p, remote).unwrap();
        p[248] = 2;
        peer.send_to(&p, remote).unwrap();
        peer.set_read_timeout(Some(Duration::from_millis(1200)))
            .unwrap();
        assert!(
            peer.recv_from(&mut b).is_err(),
            "Unexpected second packet: REQUEST/RELEASE/DECLINE must never be sent"
        );
    });
    let c = Arc::new(AtomicBool::new(false));
    let mut d = ResultData {
        schema_version: 1,
        operation: "discover".into(),
        config_source: "test".into(),
        started_at: String::new(),
        ended_at: String::new(),
        duration_ms: 0,
        interfaces: vec![],
        offers: vec![],
        server_ids: vec![],
        stats: Stats::default(),
        transaction_id: None,
        client_mac: None,
        local_endpoint: None,
        remote_endpoint: None,
        diagnostic_outcome: "unknown".into(),
        success: false,
        partial: true,
        limitations: vec![],
    };
    let cfg = Config {
        operation: "discover".into(),
        interface: "1".into(),
        duration_secs: 1,
        attempts: 1,
        expected_servers: vec!["192.0.2.1".parse().unwrap()],
    };
    collect(
        &client,
        "127.0.0.1:67".parse().unwrap(),
        &cfg,
        [1, 2, 3, 4],
        [2, 3, 4, 5, 6, 7],
        &c,
        &|_| {},
        Instant::now(),
        &mut d,
    )
    .unwrap();
    thread.join().unwrap();
    assert_eq!(d.stats.sent, 1);
    assert_eq!(d.stats.received, 6);
    assert_eq!(d.stats.ignored, 1);
    assert_eq!(d.stats.malformed, 2);
    assert_eq!(d.stats.duplicates, 1);
    assert_eq!(d.offers.len(), 2);
    assert_eq!(d.offers[0].outside_expected, Some(false));
    assert_eq!(d.offers[1].outside_expected, Some(true));
    assert_eq!(d.diagnostic_outcome, "outside_expected_observed");
    // A stopped receive window retains earlier offers and polls at 50ms.
    let flag = c.clone();
    let stopper = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        flag.store(true, Ordering::Relaxed)
    });
    let began = Instant::now();
    let error = collect(
        &client,
        "127.0.0.1:67".parse().unwrap(),
        &cfg,
        [1, 2, 3, 4],
        [2, 3, 4, 5, 6, 7],
        &c,
        &|_| {},
        Instant::now(),
        &mut d,
    )
    .unwrap_err();
    stopper.join().unwrap();
    assert_eq!(error.code, "CANCELLED");
    assert!(began.elapsed() < Duration::from_millis(500));
    assert_eq!(d.offers.len(), 2);
}
fn packet(extra: &[u8]) -> Vec<u8> {
    let mut p = wire::discover([1, 2, 3, 4], [2, 3, 4, 5, 6, 7], 0);
    p.truncate(240);
    p[0] = 2;
    p[16..20].copy_from_slice(&[192, 0, 2, 20]);
    p.extend_from_slice(&[53, 1, 2, 54, 4, 192, 0, 2, 1]);
    p.extend_from_slice(extra);
    p.push(255);
    p
}
fn parse(p: &[u8]) -> Result<Option<Offer>, ()> {
    wire::offer(
        p,
        "192.0.2.1:67".parse().unwrap(),
        [1, 2, 3, 4],
        [2, 3, 4, 5, 6, 7],
        &[],
    )
}
#[test]
fn only_discover_on_wire() {
    let p = wire::discover([1, 2, 3, 4], [2, 3, 4, 5, 6, 7], 3);
    assert_eq!(p.len(), 300);
    assert_eq!(&p[..3], &[1, 1, 6]);
    assert_eq!(&p[4..8], &[1, 2, 3, 4]);
    assert_eq!(&p[8..12], &[0, 3, 128, 0]);
    assert_eq!(&p[12..28], &[0; 16]);
    assert_eq!(&p[240..249], &[53, 1, 1, 61, 7, 1, 2, 3, 4]);
    assert!(!p[240..].windows(2).any(|s| s == [50, 4] || s == [54, 4]));
}
#[test]
fn offer_options_and_expected_list() {
    let p = packet(&[
        51, 4, 0, 0, 14, 16, 1, 4, 255, 255, 255, 0, 3, 4, 192, 0, 2, 1, 6, 4, 192, 0, 2, 53, 15,
        4, b't', b'e', b's', b't',
    ]);
    let o = parse(&p).unwrap().unwrap();
    assert_eq!(o.lease_secs, Some(3600));
    assert_eq!(o.outside_expected, None);
    assert_eq!(o.search_domains, ["test"]);
    assert_eq!(o.dns[0].to_string(), "192.0.2.53");
    let o = wire::offer(
        &p,
        "192.0.2.2:67".parse().unwrap(),
        [1, 2, 3, 4],
        [2, 3, 4, 5, 6, 7],
        &["192.0.2.2".parse().unwrap()],
    )
    .unwrap()
    .unwrap();
    assert_eq!(o.outside_expected, Some(true));
    assert_eq!(o.packet_source, "192.0.2.2:67");
}
#[test]
fn unrelated_transactions_and_clients_ignored() {
    for offset in [0, 1, 2, 4, 28] {
        let mut p = packet(&[]);
        p[offset] ^= 1;
        assert!(parse(&p).unwrap().is_none())
    }
    let p = packet(&[61, 7, 1, 2, 3, 4, 5, 6, 8]);
    assert!(parse(&p).unwrap().is_none());
}
#[test]
fn truncation_and_invalid_lengths_rejected() {
    let p = packet(&[51, 4, 0, 0, 0, 1]);
    for n in 0..p.len() {
        assert!(parse(&p[..n]).is_err(), "{n}")
    }
    for extra in [
        &[51, 3, 1, 2, 3][..],
        &[1, 4, 255, 0, 255, 0],
        &[3, 3, 1, 2, 3],
        &[53, 1, 2],
        &[54, 4, 192, 0, 2, 2],
    ] {
        assert!(parse(&packet(extra)).unwrap_or(None).is_none())
    }
}
#[test]
fn rfc3396_concatenation_and_option_overload() {
    let o = parse(&packet(&[6, 2, 192, 0, 6, 2, 2, 53]))
        .unwrap()
        .unwrap();
    assert_eq!(o.dns[0].to_string(), "192.0.2.53");
    let mut p = packet(&[52, 1, 1]);
    p[108..115].copy_from_slice(&[6, 4, 192, 0, 2, 53, 255]);
    assert_eq!(parse(&p).unwrap().unwrap().dns[0].to_string(), "192.0.2.53");
    p[108..115].copy_from_slice(&[54, 4, 192, 0, 2, 4, 255]);
    assert!(parse(&p).is_err());
}
#[test]
fn domain_search_compression_and_loops() {
    let o = parse(&packet(&[
        119, 13, 1, b'a', 4, b't', b'e', b's', b't', 0, 1, b'b', 0xc0, 2, 0,
    ]))
    .unwrap_or(None);
    assert!(o.is_none());
    let o = parse(&packet(&[
        119, 12, 1, b'a', 4, b't', b'e', b's', b't', 0, 1, b'b', 0xc0, 2,
    ]))
    .unwrap()
    .unwrap();
    assert_eq!(o.search_domains, ["a.test", "b.test"]);
    assert!(parse(&packet(&[119, 2, 0xc0, 0])).is_err());
}
#[test]
fn relay_evidence() {
    let mut p = packet(&[82, 3, 1, 1, 9]);
    p[24..28].copy_from_slice(&[198, 51, 100, 1]);
    let o = parse(&p).unwrap().unwrap();
    assert_eq!(o.relay.to_string(), "198.51.100.1");
    assert_eq!(o.relay_agent_hex.as_deref(), Some("010109"));
}
#[test]
fn invalid_config_and_mismatched_request() {
    let c = Config {
        operation: "discover".into(),
        ..Default::default()
    };
    assert!(validate(&c).is_err());
    let c = Config {
        interface: "8".into(),
        duration_secs: 31,
        ..c
    };
    assert!(validate(&c).is_err());
    let mut r = Request {
        tool: "dhcp".into(),
        action: "inspect".into(),
        dhcp: Some(Default::default()),
        ..Default::default()
    };
    assert!(r.validate_tool_config().is_ok());
    r.action = "discover".into();
    assert!(r.validate_tool_config().is_err());
    r.tool = "ping".into();
    assert!(r.validate_tool_config().is_err());
}
#[test]
fn cancel_preserves_empty_result() {
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let e = run(&Config::default(), &cancel, &|_| {}).unwrap_err();
    assert_eq!(e.code, "CANCELLED");
    assert_eq!(e.data.unwrap()["offers"], json!([]));
}
#[test]
fn exact_interface_selection_and_ambiguity() {
    assert!(selected(vec![], "wrong").is_err());
    assert!(selected(vec![], "").unwrap().is_empty());
}

#[test]
fn bounded_offer_retention_keeps_counts_and_first_evidence() {
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let mut d = ResultData {
        schema_version: 1,
        operation: "discover".into(),
        config_source: "test".into(),
        started_at: String::new(),
        ended_at: String::new(),
        duration_ms: 0,
        interfaces: vec![],
        offers: vec![],
        server_ids: vec![],
        stats: Stats::default(),
        transaction_id: None,
        client_mac: None,
        local_endpoint: None,
        remote_endpoint: None,
        diagnostic_outcome: "unknown".into(),
        success: false,
        partial: true,
        limitations: vec![],
    };
    for n in 0..300u32 {
        let mut offer = parse(&packet(&[])).unwrap().unwrap();
        offer.yiaddr = Ipv4Addr::from(0xc0000200 + n);
        retain(&mut d, offer);
    }
    assert_eq!(d.offers.len(), 256);
    assert_eq!(d.stats.offers_dropped, 44);
    assert_eq!(d.server_ids.len(), 1);
    assert!(!retain(&mut d, parse(&packet(&[])).unwrap().unwrap()));
    assert_eq!(d.stats.duplicates, 1);
    assert_eq!(d.stats.offers_dropped, 44);
    assert_eq!(
        collect(
            &UdpSocket::bind("127.0.0.1:0").unwrap(),
            "127.0.0.1:67".parse().unwrap(),
            &Config::default(),
            [1, 2, 3, 4],
            [2, 3, 4, 5, 6, 7],
            &cancel,
            &|_| {},
            Instant::now(),
            &mut d
        )
        .unwrap_err()
        .code,
        "CANCELLED"
    );
    assert_eq!(d.offers.len(), 256);
}
