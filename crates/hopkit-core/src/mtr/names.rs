//! Independent bounded PTR queue. No lookup unless an explicit resolver is set.
use super::*;
use std::{
    collections::BTreeMap,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread::JoinHandle,
};
#[derive(Clone, Debug, Serialize)]
pub struct Name {
    pub address: String,
    pub names: Vec<String>,
    pub status: String,
}
pub(super) struct Names {
    send: SyncSender<String>,
    receive: Receiver<Name>,
    stop: Cancel,
    thread: Option<JoinHandle<()>>,
    pub rows: BTreeMap<String, Name>,
    pub dropped: u64,
}
impl Names {
    pub fn new(server: String) -> Self {
        let (send, jobs) = mpsc::sync_channel::<String>(16);
        let (results, receive) = mpsc::sync_channel(16);
        let stop = Arc::new(AtomicBool::new(false));
        let cancel = stop.clone();
        let thread = std::thread::spawn(move || {
            static BUDGET: OnceLock<crate::budget::ConnectionBudget> = OnceLock::new();
            let budget = BUDGET.get_or_init(|| crate::budget::ConnectionBudget::new(2));
            while !cancel.load(Ordering::Relaxed) {
                let address = match jobs.recv_timeout(Duration::from_millis(50)) {
                    Ok(a) => a,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(_) => break,
                };
                let permit = loop {
                    if cancel.load(Ordering::Relaxed) {
                        return;
                    }
                    if let Some(permit) = budget.try_acquire() {
                        break permit;
                    }
                    std::thread::sleep(Duration::from_millis(25));
                };
                let row = match crate::discovery::names::lookup(&address, &server, 300, &cancel) {
                    Ok(names) => Name {
                        address,
                        names: names
                            .into_iter()
                            .take(4)
                            .map(|s| s.chars().take(253).collect())
                            .collect(),
                        status: "complete".into(),
                    },
                    Err(e) => Name {
                        address,
                        names: vec![],
                        status: e.code,
                    },
                };
                drop(permit);
                if results.try_send(row).is_err() {
                    break;
                }
            }
        });
        Self {
            send,
            receive,
            stop,
            thread: Some(thread),
            rows: BTreeMap::new(),
            dropped: 0,
        }
    }
    pub fn schedule(&mut self, address: &str) {
        if self.rows.contains_key(address) {
            return;
        }
        if self.rows.len() >= 256 {
            self.dropped += 1;
            return;
        }
        let status = if self.send.try_send(address.into()).is_ok() {
            "pending"
        } else {
            "skipped_queue_full"
        };
        self.rows.insert(
            address.into(),
            Name {
                address: address.into(),
                names: vec![],
                status: status.into(),
            },
        );
    }
    pub fn drain(&mut self) {
        for row in self.receive.try_iter() {
            self.rows.insert(row.address.clone(), row);
        }
    }
    pub fn finish(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        self.drain();
        for row in self.rows.values_mut() {
            if row.status == "pending" {
                row.status = "interrupted".into();
            }
        }
    }
}
impl Drop for Names {
    fn drop(&mut self) {
        self.finish();
    }
}
