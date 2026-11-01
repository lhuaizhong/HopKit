//! Bounded, sequential multi-target monitoring. One monitor uses one task slot
//! and at most one active probe, just like an integrated check.
use crate::runtime::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    io::{BufWriter, Write},
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const RAW_LIMIT: usize = 900;
pub const EVENT_LIMIT: usize = 300;
pub const BUCKET_LIMIT: usize = 1024;
const FILE_LIMIT: u64 = 8 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub label: String,
    pub protocol: String,
    pub target: String,
    #[serde(default)]
    pub port: u16,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub targets: Vec<Target>,
    pub interval_ms: u64,
    pub timeout_ms: u64,
    pub duration_secs: u64,
    pub log_directory: String,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            targets: vec![],
            interval_ms: 1000,
            timeout_ms: 1000,
            duration_secs: 0,
            log_directory: String::new(),
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<(), ToolError> {
        if self.targets.is_empty()
            || self.targets.len() > 3
            || !(1000..=60000).contains(&self.interval_ms)
            || !(100..=3000).contains(&self.timeout_ms)
            || self.duration_secs > 604800
        {
            return Err(ToolError::new(
                "INVALID_ARGUMENT",
                "Monitor: 1..3 targets; interval 1000..60000 ms; timeout 100..3000 ms; duration 0..604800 s",
            ));
        }
        for t in &self.targets {
            let parsed = crate::target::parse(&t.target)?;
            if !matches!(t.protocol.as_str(), "ping" | "tcping")
                || (t.protocol == "tcping" && t.port == 0)
                || parsed.scheme.is_some()
                || parsed.port.is_some()
                || parsed.scope.is_some()
                || t.label.trim().is_empty()
                || t.label.len() > 80
                || t.label.chars().any(char::is_control)
            {
                return Err(ToolError::new(
                    "INVALID_ARGUMENT",
                    "Monitor requires a host/IP without URL, port or scope; TCP needs a separate port; label <=80 bytes",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct Control(Mutex<VecDeque<(String, String)>>);
impl Control {
    pub fn send(&self, action: String, text: String) -> Result<(), ToolError> {
        if !matches!(
            action.as_str(),
            "pause" | "resume" | "note" | "network_change"
        ) || text.len() > 500
            || text.chars().any(|c| c.is_control() && c != '\n')
        {
            return Err(ToolError::new(
                "INVALID_ARGUMENT",
                "Invalid monitor action or note (max 500 bytes)",
            ));
        }
        let mut q = self
            .0
            .lock()
            .map_err(|_| ToolError::new("INTERNAL_ERROR", "Monitor control unavailable"))?;
        if q.len() >= 32 {
            return Err(ToolError::new(
                "RESOURCE_EXHAUSTED",
                "Monitor control queue full",
            ));
        }
        q.push_back((action, text));
        Ok(())
    }
}

#[derive(Clone, Default, Serialize)]
struct Counters {
    completed: u64,
    succeeded: u64,
    failed: u64,
    failures: BTreeMap<String, u64>,
    latency_sum_ms: f64,
    min_ms: Option<f64>,
    max_ms: Option<f64>,
}
impl Counters {
    fn add(&mut self, s: &Value) {
        self.completed += 1;
        if s["ok"] == true {
            self.succeeded += 1;
            if let Some(ms) = s["rtt_ms"].as_f64() {
                self.latency_sum_ms += ms;
                self.min_ms = Some(self.min_ms.map_or(ms, |v| v.min(ms)));
                self.max_ms = Some(self.max_ms.map_or(ms, |v| v.max(ms)));
            }
        } else {
            self.failed += 1;
            *self
                .failures
                .entry(s["status"].as_str().unwrap_or("unknown").into())
                .or_default() += 1;
        }
    }
    fn merge(&mut self, other: &Self) {
        self.completed += other.completed;
        self.succeeded += other.succeeded;
        self.failed += other.failed;
        self.latency_sum_ms += other.latency_sum_ms;
        if let Some(v) = other.min_ms {
            self.min_ms = Some(self.min_ms.map_or(v, |n| n.min(v)));
        }
        if let Some(v) = other.max_ms {
            self.max_ms = Some(self.max_ms.map_or(v, |n| n.max(v)));
        }
        for (k, v) in &other.failures {
            *self.failures.entry(k.clone()).or_default() += v;
        }
    }
}

struct State {
    config: Config,
    started: u64,
    now: u64,
    raw: VecDeque<Value>,
    records: VecDeque<Value>,
    totals: Vec<Counters>,
    bins: BTreeMap<u64, Vec<Counters>>,
    width: u64,
    outages: Vec<Option<u64>>,
    paused: bool,
    gap_start: Option<u64>,
    record_count: u64,
    sample_count: u64,
}
impl State {
    fn new(config: Config, started: u64) -> Self {
        let n = config.targets.len();
        Self {
            config,
            started,
            now: started,
            raw: VecDeque::new(),
            records: VecDeque::new(),
            totals: vec![Counters::default(); n],
            bins: BTreeMap::new(),
            width: 60000,
            outages: vec![None; n],
            paused: false,
            gap_start: None,
            record_count: 0,
            sample_count: 0,
        }
    }
    fn event(&mut self, mut v: Value) -> Value {
        self.record_count += 1;
        v["id"] = json!(self.record_count);
        self.records.push_back(v.clone());
        if self.records.len() > EVENT_LIMIT {
            self.records.pop_front();
        }
        v
    }
    fn gap(&mut self, from: u64, to: u64, reason: &str) -> Value {
        // End failed observations as unknown, never as a successful recovery.
        for i in 0..self.outages.len() {
            if let Some(start) = self.outages[i].take() {
                self.event(json!({"kind":"failure_interval","target_index":i,"from_ms":start,"to_ms":from,"end_reason":"measurement_gap"}));
            }
        }
        self.event(json!({"kind":"measurement_gap","from_ms":from,"to_ms":to,"reason":reason}))
    }
    fn record(&mut self, i: usize, mut s: Value, time: u64) -> Value {
        self.now = self.now.max(time);
        self.sample_count += 1;
        s["target_index"] = json!(i);
        s["timestamp_ms"] = json!(time);
        s["seq"] = json!(self.sample_count);
        self.totals[i].add(&s);
        while time.saturating_sub(self.started) / self.width >= BUCKET_LIMIT as u64 {
            self.width *= 2;
            let mut bins: BTreeMap<u64, Vec<Counters>> = BTreeMap::new();
            for (key, values) in std::mem::take(&mut self.bins) {
                let dst = bins
                    .entry(key / 2)
                    .or_insert_with(|| vec![Counters::default(); self.totals.len()]);
                for (a, b) in dst.iter_mut().zip(values) {
                    a.merge(&b);
                }
            }
            self.bins = bins;
        }
        self.bins
            .entry(time.saturating_sub(self.started) / self.width)
            .or_insert_with(|| vec![Counters::default(); self.totals.len()])[i]
            .add(&s);
        if s["ok"] == true {
            if let Some(from) = self.outages[i].take() {
                self.event(json!({"kind":"failure_interval","target_index":i,"from_ms":from,"to_ms":time,"end_reason":"observed_recovery"}));
            }
        } else if self.outages[i].is_none() {
            self.outages[i] = Some(time);
        }
        self.raw.push_back(s.clone());
        if self.raw.len() > RAW_LIMIT {
            self.raw.pop_front();
        }
        s
    }
    fn snapshot(&self) -> Value {
        let buckets: Vec<_> = self.bins.iter().map(|(k,v)|json!({"from_ms":self.started+k*self.width,"to_ms":(self.started+(k+1)*self.width).min(self.now+1),"targets":v})).collect();
        json!({"event_type":"monitor_snapshot","started_at_ms":self.started,"ended_at_ms":self.now,"config":self.config,"paused":self.paused,"gap_start_ms":self.gap_start,"totals":self.totals,"samples":self.raw,"sample_count":self.sample_count,"records":self.records,"record_count":self.record_count,"open_failures":self.outages,"buckets":buckets,"bucket_width_ms":self.width,
            "budgets":{"active_probes":1,"raw_samples":RAW_LIMIT,"records":EVENT_LIMIT,"buckets":BUCKET_LIMIT,"snapshot_interval_ms":1000,"log_files":2,"log_file_bytes":FILE_LIMIT,"log_flush_ms":5000},
            "statistics_scope":"independent_cumulative_and_bounded_recent_samples","interruption_is_failure":false})
    }
}

struct Journal {
    writer: BufWriter<std::fs::File>,
    path: std::path::PathBuf,
    bytes: u64,
    flushed: Instant,
    last_event: u64,
}
impl Journal {
    fn new(directory: &str) -> Result<Self, ToolError> {
        std::fs::create_dir_all(directory)?;
        let path = std::path::Path::new(directory)
            .join(format!("hopkit-monitor-{}.jsonl", uuid::Uuid::new_v4()));
        let writer = BufWriter::with_capacity(
            65536,
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?,
        );
        Ok(Self {
            writer,
            path,
            bytes: 0,
            flushed: Instant::now(),
            last_event: 0,
        })
    }
    fn events(&mut self, state: &State) -> Result<(), ToolError> {
        for event in &state.records {
            let id = event["id"].as_u64().unwrap_or(0);
            if id > self.last_event {
                self.write(event)?;
                self.last_event = id;
            }
        }
        Ok(())
    }
    fn write(&mut self, value: &Value) -> Result<(), ToolError> {
        let line = serde_json::to_vec(value)
            .map_err(|e| ToolError::new("INTERNAL_ERROR", e.to_string()))?;
        if self.bytes + line.len() as u64 + 1 > FILE_LIMIT {
            self.writer.flush()?;
            let old = self.path.with_extension("previous.jsonl");
            // Copy rather than rename an open file, which Windows disallows.
            std::fs::copy(&self.path, old)?;
            self.writer.get_ref().set_len(0)?;
            use std::io::{Seek, SeekFrom};
            self.writer.seek(SeekFrom::Start(0))?;
            self.bytes = 0;
        }
        self.writer.write_all(&line)?;
        self.writer.write_all(b"\n")?;
        self.bytes += line.len() as u64 + 1;
        if self.flushed.elapsed() >= Duration::from_secs(5) {
            self.writer.flush()?;
            self.flushed = Instant::now();
        }
        Ok(())
    }
}
fn wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn discontinuity(before: u64, after: u64, elapsed: Duration, expected_ms: u64) -> bool {
    after < before
        || after
            .saturating_sub(before)
            .abs_diff(elapsed.as_millis() as u64)
            > 2000
        || after.saturating_sub(before) > expected_ms + 2000
}
pub fn run(req: &Request, c: &Cancel, emit: &Sink<'_>, control: &Control) -> ToolResult {
    let config = req
        .monitor
        .clone()
        .ok_or_else(|| ToolError::new("INVALID_ARGUMENT", "Monitor configuration required"))?;
    config.validate()?;
    let mut journal = if config.log_directory.is_empty() {
        None
    } else {
        Some(Journal::new(&config.log_directory)?)
    };
    let mut state = State::new(config.clone(), wall_ms());
    if let Some(j) = &mut journal {
        j.write(&json!({"kind":"started","schema_version":1,"started_at_ms":state.started,"config":config}))?;
    }
    emit(state.snapshot());
    let mut network = None;
    let mut network_at = Instant::now()
        .checked_sub(Duration::from_secs(30))
        .unwrap_or_else(Instant::now);
    let mut last_emit = Instant::now()
        .checked_sub(Duration::from_secs(1))
        .unwrap_or_else(Instant::now);
    let mut last_wall = wall_ms();
    let mut last_instant = Instant::now();
    let execution = (|| -> Result<(), ToolError> {
        loop {
            cancelled(c)?;
            let now = wall_ms();
            state.now = state.now.max(now);
            if discontinuity(last_wall, now, last_instant.elapsed(), config.interval_ms) {
                state.gap(last_wall, now, "sleep_or_scheduler_or_clock_gap");
                if let Some(j) = &mut journal {
                    j.events(&state)?;
                }
            }
            if config.duration_secs > 0
                && now.saturating_sub(state.started) >= config.duration_secs * 1000
            {
                break;
            }
            let actions: Vec<_> = control
                .0
                .lock()
                .map_err(|_| ToolError::new("INTERNAL_ERROR", "Control unavailable"))?
                .drain(..)
                .collect();
            for (action, text) in actions {
                match action.as_str() {
                    "pause" if !state.paused => {
                        state.gap(now, now, "manual_pause");
                        state.paused = true;
                        state.gap_start = Some(now);
                    }
                    "resume" if state.paused => {
                        state.paused = false;
                        let from = state.gap_start.take().unwrap_or(now);
                        state.gap(from, now, "manual_pause");
                    }
                    _ => {}
                }
                state.event(json!({"kind":action,"timestamp_ms":now,"text":text}));
                if let Some(j) = &mut journal {
                    j.events(&state)?;
                }
            }
            if !state.paused {
                if network_at.elapsed() >= Duration::from_secs(30) {
                    let query_wall = wall_ms();
                    let query_instant = Instant::now();
                    match crate::network::overview(c) {
                        Ok(mut value) => {
                            value.as_object_mut().unwrap().remove("captured_at");
                            // Keep evidence bounded even on hosts with hundreds of adapters.
                            if value.to_string().len() > 8192 {
                                use std::hash::{Hash, Hasher};
                                let mut hash = std::collections::hash_map::DefaultHasher::new();
                                value.to_string().hash(&mut hash);
                                value = json!({"fingerprint":hash.finish(),"details_omitted":true,"interface_count":value["interfaces"].as_array().map(Vec::len)});
                            }
                            if network.as_ref() != Some(&value) {
                                state.event(json!({"kind":if network.is_none(){"network_baseline"}else{"network_change"},"timestamp_ms":wall_ms(),"network":value}));
                                if let Some(j) = &mut journal {
                                    j.events(&state)?;
                                }
                                network = Some(value);
                            }
                        }
                        Err(e) if e.code == "CANCELLED" => return Err(e),
                        Err(e) => {
                            state.event(json!({"kind":"network_query_error","timestamp_ms":wall_ms(),"code":e.code}));
                            if let Some(j) = &mut journal {
                                j.events(&state)?;
                            }
                        }
                    }
                    let query_end = wall_ms();
                    if discontinuity(query_wall, query_end, query_instant.elapsed(), 6000) {
                        state.gap(query_wall, query_end, "network_query_interrupted");
                        if let Some(j) = &mut journal {
                            j.events(&state)?;
                        }
                    }
                    network_at = Instant::now();
                }
                for (i, t) in config.targets.iter().enumerate() {
                    cancelled(c)?;
                    if !control
                        .0
                        .lock()
                        .map_err(|_| ToolError::new("INTERNAL_ERROR", "Control unavailable"))?
                        .is_empty()
                    {
                        break;
                    }
                    if config.duration_secs > 0
                        && wall_ms().saturating_sub(state.started) >= config.duration_secs * 1000
                    {
                        break;
                    }
                    let r = Request {
                        tool: t.protocol.clone(),
                        target: crate::target::parse(&t.target)?.host,
                        port: t.port,
                        timeout_ms: config.timeout_ms,
                        count: 1,
                        ..Request::default()
                    };
                    let before = wall_ms();
                    let instant = Instant::now();
                    let resolved = resolve(&r.target, r.port, false, c);
                    let probe_at = wall_ms();
                    let probe_instant = Instant::now();
                    let measured = resolved
                        .and_then(|addr| crate::diagnostics::network_probe(&r, addr, 128, c));
                    cancelled(c)?;
                    let after = wall_ms();
                    if discontinuity(
                        before,
                        probe_at,
                        instant.elapsed().saturating_sub(probe_instant.elapsed()),
                        5000,
                    ) || discontinuity(
                        probe_at,
                        after,
                        probe_instant.elapsed(),
                        config.timeout_ms,
                    ) {
                        state.gap(before, after, "probe_interrupted");
                        if let Some(j) = &mut journal {
                            j.events(&state)?;
                        }
                        continue;
                    }
                    let sample = measured
                        .unwrap_or_else(|e| json!({"ok":false,"status":e.code,"rtt_ms":null}));
                    let sample = state.record(i, sample, before);
                    if let Some(j) = &mut journal {
                        j.events(&state)?;
                        j.write(&json!({"kind":"sample","sample":sample}))?;
                    }
                }
            }
            state.now = state.now.max(wall_ms());
            if let Some(j) = &mut journal
                && j.flushed.elapsed() >= Duration::from_secs(5)
            {
                j.writer.flush()?;
                j.flushed = Instant::now();
            }
            if last_emit.elapsed() >= Duration::from_secs(1) {
                emit(state.snapshot());
                last_emit = Instant::now();
            }
            last_wall = wall_ms();
            last_instant = Instant::now();
            // A single worker serializes probes; no backlog or catch-up burst.
            let wait_ms = if state.paused {
                100
            } else {
                config.interval_ms
            };
            let until = Instant::now() + Duration::from_millis(wait_ms);
            while Instant::now() < until {
                cancelled(c)?;
                if !control
                    .0
                    .lock()
                    .map_err(|_| ToolError::new("INTERNAL_ERROR", "Control unavailable"))?
                    .is_empty()
                    || config.duration_secs > 0
                        && wall_ms().saturating_sub(state.started) >= config.duration_secs * 1000
                {
                    break;
                }
                std::thread::sleep(
                    Duration::from_millis(50).min(until.saturating_duration_since(Instant::now())),
                );
            }
        }
        Ok(())
    })();
    state.now = state.now.max(wall_ms());
    if let Some(from) = state.gap_start.take() {
        state.gap(from, state.now, "manual_pause");
    }
    let mut result = state.snapshot();
    result["event_type"] = json!("monitor_finished");
    if let Some(j) = &mut journal {
        result["journal_path"] = json!(j.path);
        if let Err(e) = j
            .events(&state)
            .and_then(|_|j.write(&json!({"kind":"summary","totals":state.totals,"sample_count":state.sample_count})))
            .and_then(|_| j.writer.flush().map_err(ToolError::from))
        {
            let mut e = e;
            e.data = Some(result);
            return Err(e);
        }
    }
    emit(result.clone());
    match execution {
        Ok(()) => Ok(result),
        Err(mut e) => {
            e.data = Some(result);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> Config {
        Config {
            targets: vec![Target {
                label: "business".into(),
                protocol: "tcping".into(),
                target: "127.0.0.1".into(),
                port: 443,
            }],
            ..Config::default()
        }
    }
    #[test]
    fn long_history_is_bounded_and_counters_survive_compaction() {
        let mut s = State::new(config(), 1000);
        for n in 0..100_000 {
            s.record(0,json!({"ok":n%5!=0,"status":if n%5==0{"refused"}else{"connected"},"rtt_ms":if n%5==0{None}else{Some(30.0)}}),1000+n*1000);
        }
        assert_eq!(s.totals[0].completed, 100_000);
        assert_eq!(s.totals[0].failed, 20_000);
        assert_eq!(s.raw.len(), RAW_LIMIT);
        assert!(s.records.len() <= EVENT_LIMIT);
        assert!(s.bins.len() <= BUCKET_LIMIT);
        assert_eq!(
            s.bins.values().map(|b| b[0].completed).sum::<u64>(),
            100_000
        );
        assert_eq!(s.bins.values().map(|b| b[0].failed).sum::<u64>(), 20_000);
        assert!(s.width > 60000);
        assert!(s.snapshot().to_string().len() < 1024 * 1024);
    }
    #[test]
    fn gaps_never_count_as_failures_or_recoveries() {
        let mut s = State::new(config(), 0);
        s.record(0, json!({"ok":false,"status":"refused"}), 1000);
        s.gap(2000, 3_602_000, "sleep");
        s.record(
            0,
            json!({"ok":true,"status":"connected","rtt_ms":1}),
            3_603_000,
        );
        assert_eq!(s.totals[0].completed, 2);
        assert_eq!(s.totals[0].failed, 1);
        assert_eq!(s.records[0]["end_reason"], "measurement_gap");
        assert_eq!(s.records[1]["kind"], "measurement_gap");
        assert_eq!(s.records.len(), 2);
        assert!(discontinuity(1000, 3_601_000, Duration::from_secs(1), 1000));
        assert!(discontinuity(
            1000,
            3_601_000,
            Duration::from_secs(3600),
            1000
        ));
        assert!(discontinuity(1000, 500, Duration::from_secs(1), 1000));
    }
    #[test]
    fn recovery_is_first_observed_success() {
        let mut s = State::new(config(), 0);
        for (time, ok) in [(1000, false), (2000, false), (3000, true)] {
            s.record(0, json!({"ok":ok,"status":"test","rtt_ms":1}), time);
        }
        assert_eq!(s.records[0]["from_ms"], 1000);
        assert_eq!(s.records[0]["to_ms"], 3000);
        assert_eq!(s.records[0]["end_reason"], "observed_recovery");
    }
    #[test]
    fn inputs_and_control_are_bounded() {
        let mut c = config();
        c.validate().unwrap();
        for host in ["Example.COM", "例子.测试", "0:0:0:0:0:0:0:1", "[::1]"] {
            c.targets[0].target = host.into();
            c.validate().unwrap();
        }
        c.targets[0].target = "https://user:secret@example.com".into();
        assert!(c.validate().is_err());
        let mut c = config();
        c.targets = vec![c.targets[0].clone(); 4];
        assert!(c.validate().is_err());
        let control = Control::default();
        for _ in 0..32 {
            control.send("note".into(), "ok".into()).unwrap();
        }
        assert!(control.send("note".into(), "overflow".into()).is_err());
    }
    #[test]
    fn journal_rotates_with_a_fixed_disk_budget_and_reports_invalid_paths() {
        let dir =
            std::env::temp_dir().join(format!("hopkit-monitor-test-{}", uuid::Uuid::new_v4()));
        let mut j = Journal::new(dir.to_str().unwrap()).unwrap();
        for n in 0..400 {
            j.write(&json!({"seq":n,"text":"x".repeat(50000)})).unwrap();
        }
        j.writer.flush().unwrap();
        assert!(j.path.metadata().unwrap().len() <= FILE_LIMIT);
        let old = j.path.with_extension("previous.jsonl");
        assert!(old.metadata().unwrap().len() <= FILE_LIMIT);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
        let lines = std::fs::read_to_string(&j.path).unwrap();
        assert!(
            lines
                .lines()
                .all(|l| serde_json::from_str::<Value>(l).is_ok())
        );
        assert!(Journal::new(j.path.to_str().unwrap()).is_err());
        let path = j.path.clone();
        drop(j);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(old).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
}
