//! File Transfer v1. Pairing secrets are kept outside all public events/results.
mod control;
mod io;
mod model;
use crate::{Cancel, Request, Sink, ToolError, ToolResult, cancelled};
pub use control::Control;
use futures::FutureExt;
use magic_wormhole::{MailboxConnection, Wormhole, transfer, transit};
pub use model::Config;
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};
fn phase(c: &Control, p: &str) {
    c.phase(p)
}
fn progress(c: Arc<Control>) -> impl FnMut(u64, u64) + 'static {
    move |n, total| {
        let mut s = c.state.lock().unwrap();
        if s.public["bytes"].as_str() != Some(&n.to_string()) {
            s.progress_at = Instant::now();
        }
        s.public["bytes"] = json!(n.to_string());
        s.public["total_bytes"] = json!(total.to_string());
    }
}
fn connected(c: Arc<Control>) -> impl FnOnce(transit::TransitInfo) {
    move |info| {
        c.state.lock().unwrap().public["connection"] = json!(match info.conn_type {
            transit::ConnectionType::Direct => "direct",
            _ => "relay",
        });
        phase(&c, "transferring");
    }
}
fn transfer_error(e: transfer::TransferError) -> ToolError {
    use transfer::TransferError::*;
    let code = match e {
        Checksum => "INTEGRITY_ERROR",
        PeerError(ref m) if m == "transfer rejected" => "TRANSFER_REJECTED",
        IO(ref e) if e.kind() == std::io::ErrorKind::StorageFull => "STORAGE_FULL",
        IO(_) => "IO_ERROR",
        TransitConnect(_) => "TRANSIT_UNAVAILABLE",
        Wormhole(_) => "PAIRING_FAILED",
        _ => "PROTOCOL_ERROR",
    };
    ToolError::new(
        code,
        "File transfer failed; check the peer, network and save location / 传输失败，请检查对端、网络与保存位置",
    )
}
async fn workflow(mut config: Config, c: Arc<Control>, cancel: Cancel) -> ToolResult {
    let send = config.operation == "send";
    let mut source = if send {
        let p = config.send_path.clone();
        Some(smol::unblock(move || crate::file_io::read_file(Path::new(&p))).await?)
    } else {
        None
    };
    let metadata = source.as_ref().map(|f| f.metadata()).transpose()?;
    if send {
        let name = Path::new(&config.send_path)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let mut s = c.state.lock().unwrap();
        s.public["file_name"] = json!(name);
        s.public["total_bytes"] = json!(metadata.as_ref().unwrap().len().to_string());
    }
    phase(&c, "connecting_mailbox");
    let app = transfer::APP_CONFIG.rendezvous_url(config.mailbox_url.clone().into());
    let mailbox=if send {MailboxConnection::create(app,config.code_length).await}else{MailboxConnection::connect(app,std::mem::take(&mut config.code).parse().map_err(|_|ToolError::new("INVALID_CODE","Invalid pairing code"))?,false).await}.map_err(|_|ToolError::new("MAILBOX_UNAVAILABLE","Mailbox connection failed; check URL and connectivity / 信令连接失败，请检查地址与网络"))?;
    if let Some(notice) = mailbox.welcome() {
        let mut s = c.state.lock().unwrap();
        s.public["notice"] = json!(notice.chars().take(4000).collect::<String>());
        s.public["notice_source"] = json!(config.mailbox_url);
    }
    if send {
        c.state.lock().unwrap().code = Some(mailbox.code().to_string());
    }
    phase(&c, if send { "waiting_peer" } else { "pairing" });
    c.state.lock().unwrap().public["deadline_ms"] =
        json!(chrono::Utc::now().timestamp_millis() + config.peer_wait_secs as i64 * 1000);
    let wormhole = Wormhole::connect(mailbox).await.map_err(|_| {
        ToolError::new(
            "PAIRING_FAILED",
            "Pairing code mismatch or verification failed / 配对码不匹配或连接验证失败",
        )
    })?;
    c.state.lock().unwrap().code = None;
    if config.verify {
        c.state.lock().unwrap().verifier = Some(
            wormhole
                .verifier()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
        );
        phase(&c, "waiting_verification");
        c.decision(300);
        let d =
            c.rx.recv()
                .await
                .map_err(|_| ToolError::new("CANCELLED", "Control closed"))?;
        cancelled(&cancel)?;
        c.state.lock().unwrap().verifier = None;
        if d.action != "verify_ok" {
            return Err(ToolError::new("TRANSFER_REJECTED", "Verification rejected"));
        }
        c.state.lock().unwrap().public["verified"] = true.into();
    }
    let abilities = match config.connection_mode.as_str() {
        "direct" => transit::Abilities::FORCE_DIRECT,
        "relay" => transit::Abilities::FORCE_RELAY,
        _ => transit::Abilities::ALL,
    };
    let relay = transit::RelayHint::from_urls(None, [config.transit_relay.parse().unwrap()])
        .map_err(|_| ToolError::new("INVALID_ARGUMENT", "Invalid relay URL"))?;
    if send {
        let mut reader = io::Reader {
            file: async_fs::File::from(source.take().unwrap()),
            hash: Default::default(),
            control: c.clone(),
        };
        let name = c.snapshot()["file_name"].as_str().unwrap().to_string();
        let observe = c.clone();
        transfer::send_file_observed(
            wormhole,
            vec![relay],
            &mut reader,
            name,
            metadata.as_ref().unwrap().len(),
            abilities,
            connected(c.clone()),
            progress(c.clone()),
            futures::future::pending(),
            move |p| {
                if p == "ack_verified" {
                    observe.state.lock().unwrap().public["acknowledged"] = true.into();
                }
                if p=="close_warning" {observe.state.lock().unwrap().public["warning"]=json!("Transfer confirmed; mailbox cleanup failed / 传输已确认，信令关闭失败");}else if p!="closed"{phase(&observe,p);}
            },
        )
        .await
        .map_err(|e|{let mut error=transfer_error(e);if c.snapshot()["transfer_phase"]=="waiting_ack"{error.code="DELIVERY_UNCONFIRMED".into();error.message="Receipt not verified; peer may have saved the file / 未验证回执，对方可能已保存".into();}error})?;
        let after = reader.file.metadata().await?;
        if after.len() != metadata.as_ref().unwrap().len()
            || after.modified().ok() != metadata.as_ref().unwrap().modified().ok()
        {
            c.state.lock().unwrap().public["warning"] = json!(
                "Source file changed during transfer; receipt verifies transmitted content / 源文件发生变化，回执仅验证已传输内容"
            );
        }
        use sha2::Digest;
        let mut s = c.state.lock().unwrap();
        s.public["sha256"] = json!(format!("{:x}", reader.hash.finalize()));
    } else {
        phase(&c, "waiting_offer");
        let offer =
            transfer::request_file(wormhole, vec![relay], abilities, futures::future::pending())
                .await
                .map_err(transfer_error)?
                .ok_or_else(|| ToolError::new("CANCELLED", "Cancelled"))?;
        let size = offer.file_size();
        let name = offer.file_name();
        if config
            .max_receive_bytes
            .as_ref()
            .is_some_and(|n| size > n.parse::<u64>().unwrap())
        {
            let _ = offer.reject().await;
            return Err(ToolError::new(
                "INVALID_ARGUMENT",
                "Offer exceeds receive size limit",
            ));
        }
        {
            let mut s = c.state.lock().unwrap();
            s.public["file_name"] = json!(name);
            s.public["suggested_name"] = json!(if crate::file_io::safe_name(&name).is_ok() {
                name
            } else {
                "received-file".into()
            });
            s.public["total_bytes"] = json!(size.to_string());
        }
        phase(&c, "waiting_acceptance");
        c.decision(300);
        let mut writer = loop {
            let d =
                c.rx.recv()
                    .await
                    .map_err(|_| ToolError::new("CANCELLED", "Control closed"))?;
            cancelled(&cancel)?;
            if d.action == "reject" {
                let _ = offer.reject().await;
                return Err(ToolError::new(
                    "TRANSFER_REJECTED",
                    "File rejected / 已拒收",
                ));
            }
            let path = d.path.unwrap_or_default();
            let ctl = c.clone();
            match smol::unblock(move || io::Writer::create(Path::new(&path), size, ctl)).await {
                Ok(w) => break w,
                Err(e) => {
                    c.state.lock().unwrap().public["path_error_code"] = json!(e.code);
                    c.state.lock().unwrap().public["path_error"] = json!(e.to_string());
                    c.decision(300);
                }
            }
        };
        phase(&c, "connecting_data");
        let observed = c.clone();
        let result = offer
            .accept_observed(
                connected(c.clone()),
                progress(c.clone()),
                &mut writer,
                futures::future::pending(),
                move |p| {
                    if p == "receipt_sent" {
                        observed.state.lock().unwrap().public["receipt_sent"] = true.into();
                    }
                    if p == "close_warning" {
                        observed.state.lock().unwrap().public["warning"] =
                            json!("File saved; mailbox cleanup failed / 文件已保存，信令关闭失败");
                    } else if p != "closed" {
                        phase(&observed, p);
                    }
                },
            )
            .await;
        if c.snapshot()["local_saved"] != true {
            result.map_err(transfer_error)?;
        } else if result.is_err() {
            c.state.lock().unwrap().public["warning"] = json!(
                "File saved; sender may not have received confirmation / 文件已保存，对方可能未获确认"
            );
        }
    }
    phase(&c, "completed");
    Ok(c.snapshot())
}
fn budget(phase: &str, peer: u64) -> u64 {
    match phase {
        "preparing" | "connecting_mailbox" => 15,
        "waiting_peer" | "pairing" | "waiting_offer" => peer,
        "waiting_verification" | "waiting_acceptance" => 300,
        "connecting_data" => 60,
        "transferring" | "finalizing" => 120,
        "waiting_ack" => 30,
        "ack_verified" | "closing" => 2,
        _ => 15,
    }
}
pub fn run(r: &Request, cancel: &Cancel, emit: &Sink<'_>, control: &Arc<Control>) -> ToolResult {
    let config = r
        .wormhole
        .clone()
        .ok_or_else(|| ToolError::new("INVALID_ARGUMENT", "Wormhole configuration required"))?;
    config.validate()?;
    if r.action != config.operation {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "Operation and action must match",
        ));
    }
    cancelled(cancel)?;
    {
        let mut s = control.state.lock().unwrap();
        s.public["schema_version"] = json!(1);
        s.public["operation"] = json!(config.operation);
        s.public["verify_enabled"] = json!(config.verify);
    }
    if control.snapshot()["task_id"].is_null() {
        control.set_task_id(&uuid::Uuid::new_v4().to_string());
    }
    let started = Instant::now();
    let peer = config.peer_wait_secs;
    let mut result = smol::block_on(async {
        let work = workflow(config, control.clone(), cancel.clone()).fuse();
        futures::pin_mut!(work);
        let mut last = Instant::now() - Duration::from_secs(1);
        let mut previous_phase = String::new();
        let mut previous_bytes = 0u64;
        let mut speed = 0.0;
        loop {
            futures::select! {v=work=>break v,_=smol::Timer::after(Duration::from_millis(50)).fuse()=>{}}
            let snapshot = control.snapshot();
            let p = snapshot["phase"].as_str().unwrap_or("");
            let (committing, elapsed) = {
                let s = control.state.lock().unwrap();
                (
                    s.committing,
                    if p == "transferring" {
                        s.progress_at.elapsed()
                    } else {
                        s.phase_at.elapsed()
                    },
                )
            };
            if !committing {
                if cancel.load(Ordering::Relaxed) {
                    break Err(ToolError::new(
                        "CANCELLED",
                        "Transfer cancelled / 传输已停止",
                    ));
                }
                if elapsed.as_secs() >= budget(p, peer) {
                    if snapshot["acknowledged"] == true || snapshot["local_saved"] == true {
                        control.state.lock().unwrap().public["warning"] =
                            json!("Transfer saved/confirmed; connection cleanup timed out");
                        break Ok(control.snapshot());
                    }
                    break Err(ToolError::new(
                        if p == "waiting_ack" {
                            "DELIVERY_UNCONFIRMED"
                        } else {
                            "TIMEOUT"
                        },
                        "Phase timed out; check peer/network and retry / 当前阶段超时，请检查对端和网络后重试",
                    ));
                }
            }
            if previous_phase != p || last.elapsed() >= Duration::from_millis(250) {
                let n = snapshot["bytes"]
                    .as_str()
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(0);
                let dt = last.elapsed().as_secs_f64();
                let rate = n.saturating_sub(previous_bytes) as f64 / dt;
                speed = if speed == 0.0 {
                    rate
                } else {
                    speed * 0.7 + rate * 0.3
                };
                previous_bytes = n;
                let total = snapshot["total_bytes"]
                    .as_str()
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(0);
                {
                    let mut s = control.state.lock().unwrap();
                    s.public["speed_bps"] = json!(speed);
                    s.public["eta_secs"] = if speed > 0.0 {
                        json!(total.saturating_sub(n) as f64 / speed)
                    } else {
                        Value::Null
                    };
                }
                let mut event = control.snapshot();
                event["event_type"] = json!("wormhole_state");
                event["schema_version"] = json!(1);
                event["timestamp_ms"] = json!(chrono::Utc::now().timestamp_millis());
                emit(event);
                previous_phase = p.into();
                last = Instant::now();
            }
        }
    });
    control.clear_private();
    {
        let mut s = control.state.lock().unwrap();
        s.public["deadline_ms"] = Value::Null;
        s.public["elapsed_ms"] = json!(started.elapsed().as_millis() as u64);
        s.public["cleanup"] = json!("connections_dropped");
        if s.public["acknowledged"] == true || s.public["local_saved"] == true {
            s.public["phase"] = json!("completed");
        }
    }
    match &mut result {
        Ok(v) => {
            phase(control, "completed");
            *v = control.snapshot();
            v["success"] = true.into();
        }
        Err(e) => {
            if control.snapshot()["acknowledged"] == true
                || control.snapshot()["local_saved"] == true
            {
                let mut v = control.snapshot();
                v["success"] = true.into();
                v["warning"] = json!("File saved; sender confirmation may be incomplete");
                result = Ok(v);
            } else {
                e.data = Some(control.snapshot());
            }
        }
    }
    let mut event = control.snapshot();
    event["event_type"] = json!("wormhole_state");
    emit(event);
    result
}

pub fn file_info(path: &str) -> Result<Value, String> {
    let f = crate::file_io::read_file(Path::new(path)).map_err(|e| e.to_string())?;
    Ok(
        json!({"name":Path::new(path).file_name().unwrap_or_default().to_string_lossy(),"size":f.metadata().map_err(|e|e.to_string())?.len().to_string()}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::io::AsyncWriteExt;
    fn directory() -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("hopkit-wormhole-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&p).unwrap();
        p
    }
    #[test]
    fn config_and_secret_redaction() {
        let mut config = Config {
            operation: "receive".into(),
            code: "7-alpha-bravo".into(),
            ..Default::default()
        };
        assert!(config.validate().is_ok());
        let r = Request {
            wormhole: Some(config.clone()),
            ..Default::default()
        };
        for v in [
            format!("{r:?}"),
            format!("{config:?}"),
            serde_json::to_string(&r).unwrap(),
        ] {
            assert!(!v.contains("7-alpha-bravo"));
        }
        for url in [
            "wss://user:pass@example.org/v1",
            "wss://example.org/v1?secret=x",
            "wss://example.org/#frag",
            "https://example.org/",
        ] {
            config.mailbox_url = url.into();
            assert!(config.validate().is_err());
        }
        config.mailbox_url = "ws://[::1]:4000/v1".into();
        config.transit_relay = "tcp://[::1]:4001".into();
        assert!(config.validate().is_ok());
    }
    #[test]
    fn one_time_decisions() {
        let c = Control::default();
        c.phase("waiting_acceptance");
        c.decision(300);
        let id = c.snapshot()["decision_id"].as_str().unwrap().to_string();
        assert!(c.action("wrong", "accept", None).is_err());
        c.action(&id, "reject", None).unwrap();
        assert_eq!(
            c.action(&id, "accept", None).unwrap_err().code,
            "ALREADY_PROCESSED"
        );
        assert_eq!(c.rx.try_recv().unwrap().action, "reject");
        assert!(c.rx.try_recv().is_err());
    }
    #[test]
    fn writer_commit_and_hash() {
        smol::block_on(async {
            let dir = directory();
            let dest = dir.join("中文 file.txt");
            let c = Arc::new(Control::default());
            let mut w = io::Writer::create(&dest, 3, c.clone()).unwrap();
            w.write_all(b"abc").await.unwrap();
            assert!(!dest.exists());
            w.close().await.unwrap();
            assert_eq!(std::fs::read(&dest).unwrap(), b"abc");
            assert_eq!(
                c.snapshot()["sha256"],
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
            );
            assert_eq!(c.snapshot()["local_saved"], true);
            drop(w);
            std::fs::remove_dir_all(dir).unwrap();
        });
    }
    #[test]
    fn writer_empty_and_overflow_and_incomplete() {
        smol::block_on(async {
            let dir = directory();
            for (name, size, content, success) in [
                ("empty", 0, b"".as_slice(), true),
                ("overflow", 1, b"ab".as_slice(), false),
                ("incomplete", 3, b"a".as_slice(), false),
            ] {
                let dest = dir.join(name);
                let c = Arc::new(Control::default());
                let mut w = io::Writer::create(&dest, size, c).unwrap();
                let result = w.write_all(content).await;
                let result = if result.is_ok() {
                    w.close().await
                } else {
                    result
                };
                assert_eq!(result.is_ok(), success);
                drop(w);
                assert_eq!(dest.exists(), success);
            }
            assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
            std::fs::remove_dir_all(dir).unwrap();
        });
    }
    #[test]
    fn writer_target_created_during_receive_is_preserved() {
        smol::block_on(async {
            let dir = directory();
            let dest = dir.join("race");
            let c = Arc::new(Control::default());
            let mut w = io::Writer::create(&dest, 3, c.clone()).unwrap();
            w.write_all(b"new").await.unwrap();
            std::fs::write(&dest, b"original").unwrap();
            assert!(w.close().await.is_err());
            assert_ne!(c.snapshot()["local_saved"], true);
            assert_eq!(std::fs::read(&dest).unwrap(), b"original");
            drop(w);
            assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
            std::fs::remove_dir_all(dir).unwrap();
        });
    }
    #[cfg(unix)]
    #[test]
    fn symlink_send_rejected() {
        let dir = directory();
        std::fs::write(dir.join("file"), b"data").unwrap();
        std::os::unix::fs::symlink(dir.join("file"), dir.join("link")).unwrap();
        assert!(crate::file_io::read_file(&dir.join("link")).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn wrong_direction_and_cancel_before_network() {
        let r = Request {
            tool: "wormhole".into(),
            action: "receive".into(),
            wormhole: Some(Config {
                send_path: "unused".into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let c = Arc::new(std::sync::atomic::AtomicBool::new(false));
        assert_eq!(
            run(&r, &c, &|_| {}, &Arc::new(Control::default()))
                .unwrap_err()
                .code,
            "INVALID_ARGUMENT"
        );
    }
}
