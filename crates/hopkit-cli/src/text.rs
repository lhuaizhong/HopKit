//! Human output is deliberately separate from the machine-readable schema.
use hopkit_core::Request;
use serde_json::Value;
use std::time::{Duration, Instant};

pub struct TextOutput {
    request: Request,
    english: bool,
    hops: Vec<Value>,
    monitor_seq: u64,
    progress_at: Instant,
    phase: String,
    service_stats: Value,
    service_started: bool,
}

fn s(v: &Value, key: &str) -> String {
    v[key].as_str().map(clean).unwrap_or_default()
}
// Never allow names, log messages or peer errors to control the terminal.
fn clean(value: &str) -> String {
    value
        .chars()
        .flat_map(|c| {
            if c.is_control() {
                c.escape_debug().to_string().chars().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect::<String>()
}
fn n(v: &Value, key: &str) -> String {
    if let Some(i) = v[key].as_u64() {
        return i.to_string();
    }
    if let Some(x) = v[key].as_f64() {
        return format!("{x:.2}");
    }
    v[key].as_str().map(clean).unwrap_or_else(|| "-".into())
}
fn list(v: &Value) -> String {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(clean)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}
fn bytes(v: &Value) -> String {
    let amount = v.as_u64().or_else(|| v.as_str()?.parse().ok());
    match amount {
        Some(b) if b >= 1_073_741_824 => format!("{:.2} GiB", b as f64 / 1_073_741_824.),
        Some(b) if b >= 1_048_576 => format!("{:.2} MiB", b as f64 / 1_048_576.),
        Some(b) if b >= 1024 => format!("{:.2} KiB", b as f64 / 1024.),
        Some(b) => format!("{b} B"),
        None => "-".into(),
    }
}

impl TextOutput {
    pub fn new(request: &Request, english: bool) -> Self {
        Self {
            request: request.clone(),
            english,
            hops: vec![],
            monitor_seq: 0,
            progress_at: Instant::now(),
            phase: String::new(),
            service_stats: Value::Null,
            service_started: false,
        }
    }
    fn t<'a>(&self, en: &'a str, zh: &'a str) -> &'a str {
        if self.english { en } else { zh }
    }
    fn message(&self, value: &str) -> String {
        let text = if let Some((en, zh)) = value.split_once(" / ") {
            if self.english { en } else { zh }
        } else {
            value
        };
        clean(text)
    }
    pub fn error(&self, v: &Value) -> String {
        format!(
            "{} [{}]: {}",
            self.t("Error", "错误"),
            s(v, "code"),
            self.message(v["message"].as_str().unwrap_or(""))
        )
    }
    fn status(&self, status: &str) -> String {
        self.t(
            match status {
                "reply" | "connected" | "replied" => "OK",
                "timeout" | "TIMEOUT" => "Timed out",
                "refused" | "CONNECTION_REFUSED" => "Connection refused",
                "unreachable" => "Unreachable",
                "ttl_expired" => "TTL expired",
                "invalid_or_unsynchronized" => "Invalid or unsynchronized clock",
                "connect_error" => "Connection failed",
                "icmp_error" => "ICMP error",
                _ => status,
            },
            match status {
                "reply" | "connected" | "replied" => "成功",
                "timeout" | "TIMEOUT" => "请求超时",
                "refused" | "CONNECTION_REFUSED" => "连接被拒绝",
                "unreachable" => "目标不可达",
                "ttl_expired" => "TTL 到期",
                "invalid_or_unsynchronized" => "时钟无效或未同步",
                "connect_error" => "连接失败",
                "icmp_error" => "ICMP 错误",
                _ => status,
            },
        )
        .to_owned()
    }
    fn probe_line(&self, v: &Value) -> String {
        let address = s(v, "address");
        let address = if address.is_empty() {
            clean(&self.request.target)
        } else {
            address
        };
        let address = if self.request.tool == "tcping" {
            endpoint(&address, self.request.port)
        } else {
            address
        };
        if v["ok"] == true {
            if self.request.tool == "ntp" {
                format!(
                    "#{}  {}  {}={} ms  {}={} ms  stratum={}",
                    n(v, "seq"),
                    self.status("reply"),
                    self.t("offset", "偏差"),
                    n(v, "offset_ms"),
                    self.t("delay", "延迟"),
                    n(v, "delay_ms"),
                    n(v, "stratum")
                )
            } else {
                format!(
                    "#{}  {}  {}  {}={} ms",
                    n(v, "seq"),
                    address,
                    self.status(v["status"].as_str().unwrap_or("reply")),
                    self.t("time", "时间"),
                    n(v, "rtt_ms")
                )
            }
        } else {
            let mut out = format!(
                "#{}  {}  {}",
                n(v, "seq"),
                address,
                self.status(v["status"].as_str().unwrap_or("failed"))
            );
            if v["status"] == "icmp_error" {
                out.push_str(&format!(" ({})", n(v, "status_code")));
            }
            out
        }
    }
    fn flush_hop(&mut self) -> Option<String> {
        if self.hops.is_empty() {
            return None;
        }
        let hop = n(&self.hops[0], "hop");
        let probes = self
            .hops
            .iter()
            .map(|v| {
                if v["status"] == "timeout" {
                    return "*".into();
                }
                let mut cell = s(v, "address");
                if v["rtt_ms"].is_number() {
                    cell.push_str(&format!("  {} ms", n(v, "rtt_ms")));
                }
                if v["ok"] != true && v["status"] != "ttl_expired" {
                    cell.push_str(&format!(
                        " ({})",
                        self.status(v["status"].as_str().unwrap_or("error"))
                    ));
                }
                cell
            })
            .collect::<Vec<_>>()
            .join("   ");
        self.hops.clear();
        Some(format!("{hop:>2}  {probes}"))
    }
    fn progress(&mut self, v: &Value) -> Option<String> {
        if self.progress_at.elapsed() < Duration::from_secs(1) {
            return None;
        }
        self.progress_at = Instant::now();
        let mut out = format!(
            "{}: {}",
            self.t("Transferred", "已传输"),
            bytes(&v["bytes"])
        );
        if !v["total_bytes"].is_null() {
            out.push_str(&format!(" / {}", bytes(&v["total_bytes"])));
        }
        Some(out)
    }
    pub fn event(&mut self, v: &Value) -> Option<String> {
        let kind = v["event_type"].as_str().unwrap_or("");
        match kind {
            "target" if matches!(self.request.tool.as_str(), "ping" | "tcping" | "trace") => {
                let address = s(v, "address");
                Some(match self.request.tool.as_str() {
                    "ping" => format!(
                        "Ping {} [{}], {} {}",
                        clean(&self.request.target),
                        address,
                        self.request.payload_size_bytes,
                        self.t("bytes", "字节")
                    ),
                    "tcping" => format!("TCPing {}", endpoint(&address, self.request.port)),
                    _ => format!(
                        "{} {} [{}], {} {}",
                        self.t("Tracing", "追踪路由"),
                        clean(&self.request.target),
                        address,
                        self.request.max_hops,
                        self.t("hops max", "跳上限")
                    ),
                })
            }
            "sample" if self.request.tool == "trace" => {
                self.hops.push(v.clone());
                if self.hops.len() == 3 {
                    self.flush_hop()
                } else {
                    None
                }
            }
            "sample" => Some(self.probe_line(v)),
            "stage" => Some(self.stage(&v["evidence"])),
            "monitor_snapshot" => {
                let mut lines = vec![];
                if let Some(samples) = v["samples"].as_array() {
                    for sample in samples {
                        let seq = sample["seq"].as_u64().unwrap_or(0);
                        if seq <= self.monitor_seq {
                            continue;
                        }
                        self.monitor_seq = seq;
                        let i = sample["target_index"].as_u64().unwrap_or(0) as usize;
                        let name = self.monitor_target(i);
                        let status = self.status(sample["status"].as_str().unwrap_or("unknown"));
                        lines.push(if sample["ok"] == true {
                            format!("#{seq}  {name}  {status}  {} ms", n(sample, "rtt_ms"))
                        } else {
                            format!("#{seq}  {name}  {status}")
                        });
                    }
                }
                if lines.is_empty() {
                    None
                } else {
                    Some(lines.join("\n"))
                }
            }
            "ready" => {
                self.service_started = true;
                let mut out = format!("{}: {}", self.t("Listening", "正在监听"), s(v, "listen"));
                if v["url"].is_string() {
                    out.push_str(&format!("\n{}", s(v, "url")));
                }
                for (key, en, zh) in [("root", "Directory", "目录"), ("directory", "Logs", "日志")]
                {
                    if v[key].is_string() {
                        out.push_str(&format!("\n{}: {}", self.t(en, zh), s(v, key)));
                    }
                }
                if v["writable"].is_boolean() {
                    out.push_str(&format!(
                        "\n{}",
                        if v["writable"] == true {
                            self.t("Uploads enabled", "已允许上传")
                        } else {
                            self.t("Read only", "只读")
                        }
                    ));
                }
                Some(out)
            }
            "connected" => Some(format!(
                "{} {}  {}",
                self.t("Connected to", "已连接"),
                s(v, "peer"),
                s(v, "protocol").to_uppercase()
            )),
            "stats" if self.request.tool == "iperf3" => Some(self.rate(v)),
            "stats" | "stopped" => {
                self.service_stats = v.clone();
                None
            }
            "test_complete" if self.request.action == "serve" => Some(format!(
                "{}: {}",
                self.t("Test complete", "测试完成"),
                self.rate(v)
            )),
            "progress" => self.progress(v),
            "transfer" => Some(format!(
                "{}  {}  {}  {}",
                s(v, "source"),
                s(v, "action"),
                s(v, "remote_name"),
                bytes(&v["bytes"])
            )),
            "transfer_start" => Some(format!(
                "{}  {}  {}",
                s(v, "peer"),
                s(v, "direction"),
                s(v, "name")
            )),
            "transfer_complete" => Some(format!(
                "{}: {}",
                self.t("Transfer complete", "传输完成"),
                bytes(&v["data"]["bytes"])
            )),
            "transfer_error" | "session_error" => Some(self.error(&v["error"])),
            "request" => Some(format!(
                "{}  {}",
                s(v, "source"),
                self.t("Replied", "已应答")
            )),
            "log" => Some(format!(
                "{}  {}",
                s(&v["record"], "source"),
                s(&v["record"], "text")
            )),
            "wormhole_state" => {
                let phase = s(v, "phase");
                if phase == self.phase {
                    return if phase == "transferring" {
                        self.progress(v)
                    } else {
                        None
                    };
                }
                self.phase = phase.clone();
                let label = match phase.as_str() {
                    "preparing" => self.t("Preparing file", "正在准备文件"),
                    "connecting_mailbox" => {
                        self.t("Connecting to signaling service", "正在连接信令服务")
                    }
                    "waiting_peer" => self.t("Waiting for peer", "等待对方连接"),
                    "pairing" => self.t("Pairing", "正在配对"),
                    "waiting_verification" => self.t("Waiting for verification", "等待校验确认"),
                    "waiting_offer" => self.t("Waiting for file offer", "等待文件信息"),
                    "waiting_acceptance" => self.t("Waiting for acceptance", "等待确认接收"),
                    "connecting_data" => self.t("Connecting for transfer", "正在建立传输连接"),
                    "transferring" => self.t("Transferring", "正在传输"),
                    "waiting_ack" => self.t("Waiting for receipt", "等待接收回执"),
                    "completed" | "failed" | "cancelled" => return None,
                    _ => return None,
                };
                Some(label.into())
            }
            // Snapshots, duplicate final results, and internal bookkeeping stay off the terminal.
            _ => None,
        }
    }
    fn monitor_target(&self, index: usize) -> String {
        self.request
            .monitor
            .as_ref()
            .and_then(|m| m.targets.get(index))
            .map(|t| {
                if t.protocol == "tcping" {
                    endpoint(&clean(&t.target), t.port)
                } else {
                    clean(&t.target)
                }
            })
            .unwrap_or_else(|| format!("#{}", index + 1))
    }
    fn rate(&self, v: &Value) -> String {
        let mut out = format!(
            "{} s  {}  {} Mbps",
            n(v, "duration_secs"),
            bytes(&v["bytes"]),
            n(v, "mbps")
        );
        if !v["loss_percent"].is_null() {
            out.push_str(&format!(
                "  {}={}%",
                self.t("loss", "丢包"),
                n(v, "loss_percent")
            ));
        }
        if !v["jitter_ms"].is_null() {
            out.push_str(&format!(
                "  {}={} ms",
                self.t("jitter", "抖动"),
                n(v, "jitter_ms")
            ));
        }
        out
    }
    fn stage(&self, e: &Value) -> String {
        let stage = s(e, "stage").to_uppercase();
        let data = &e["data"];
        let (status, detail) = if e["status"] == "skipped" {
            (
                self.t("SKIP", "跳过"),
                self.message(data["reason"].as_str().unwrap_or("")),
            )
        } else if e["error"].is_object() {
            (
                self.t("FAIL", "失败"),
                format!(
                    "{}: {}",
                    s(&e["error"], "code"),
                    self.message(e["error"]["message"].as_str().unwrap_or(""))
                ),
            )
        } else if e["stage"] == "http" {
            let code = data["status_code"].as_u64().unwrap_or(0);
            (
                if code >= 400 {
                    self.t("FAIL", "失败")
                } else {
                    self.t("OK", "成功")
                },
                format!("HTTP {code}"),
            )
        } else if data["certificate_validation"]
            .as_str()
            .is_some_and(|s| s.starts_with("SKIPPED"))
        {
            (
                self.t("WARN", "注意"),
                self.t("Certificate verification skipped", "已跳过证书校验")
                    .into(),
            )
        } else if e["status"] == "failed" {
            (
                self.t("FAIL", "失败"),
                self.t("No successful response", "未收到成功响应").into(),
            )
        } else {
            let detail = if data["addresses"].is_array() {
                list(&data["addresses"])
            } else if data["remote_address"].is_string() {
                s(data, "remote_address")
            } else {
                String::new()
            };
            (self.t("OK", "成功"), detail)
        };
        format!(
            "{stage:<10} {status}{}",
            if detail.is_empty() {
                String::new()
            } else {
                format!("  {detail}")
            }
        )
    }
    pub fn finish(&mut self, envelope: &Value) -> String {
        let mut lines = vec![];
        if let Some(hop) = self.flush_hop() {
            lines.push(hop);
        }
        let data = &envelope["data"];
        if !data.is_null() {
            match self.request.tool.as_str() {
                "ping" | "tcping" => {
                    lines.push(format!(
                        "\n{}: {} {}, {} {}, {} {} ({}%)",
                        self.t("Statistics", "统计"),
                        self.t("Sent", "发送"),
                        n(data, "sent"),
                        self.t("Received", "成功"),
                        n(data, "received"),
                        self.t("Lost", "丢失"),
                        n(data, "lost"),
                        n(data, "loss_percent")
                    ));
                    if !data["rtt_avg_ms"].is_null() {
                        lines.push(format!(
                            "{} = {}/{}/{} ms",
                            self.t("RTT min/avg/max", "延迟 最小/平均/最大"),
                            n(data, "rtt_min_ms"),
                            n(data, "rtt_avg_ms"),
                            n(data, "rtt_max_ms")
                        ));
                    }
                    if data["interrupted_probes"].as_u64().unwrap_or(0) > 0 {
                        lines.push(format!(
                            "{}: {}",
                            self.t("Interrupted (excluded from loss)", "中断（不计丢包）"),
                            n(data, "interrupted_probes")
                        ));
                    }
                }
                "trace" => lines.push(if data["reached"] == true {
                    self.t("Destination reached.", "已到达目标。").into()
                } else {
                    self.t("Destination not reached.", "未到达目标。").into()
                }),
                "dns" => {
                    lines.push(format!(
                        "DNS {} ({})",
                        clean(&self.request.target),
                        if self.request.dns_server.is_empty() {
                            self.t("system", "系统").into()
                        } else {
                            clean(&self.request.dns_server)
                        }
                    ));
                    if let Some(addrs) = data["addresses"].as_array() {
                        lines.extend(addrs.iter().filter_map(Value::as_str).map(clean));
                    }
                    if let Some(records) = data["records"].as_array() {
                        for record in records.iter().filter(|r| r["section"] == "answer") {
                            lines.push(format!(
                                "{}  {}  {}",
                                s(record, "name"),
                                rrtype(record["type"].as_u64().unwrap_or(0)),
                                s(record, "value")
                            ));
                        }
                        if records.iter().all(|r| r["section"] != "answer") {
                            lines.push(self.t("No answer records.", "无应答记录。").into());
                        }
                    }
                    if data["rcode"].as_u64().unwrap_or(0) != 0 {
                        let code = data["rcode"].as_u64().unwrap_or(0);
                        lines.push(format!(
                            "DNS: {}",
                            match code {
                                1 => "FORMERR".into(),
                                2 => "SERVFAIL".into(),
                                3 => "NXDOMAIN".into(),
                                5 => "REFUSED".into(),
                                _ => format!("RCODE {code}"),
                            }
                        ));
                    }
                }
                "ntp" if self.request.action != "serve" => {
                    lines.push(format!(
                        "{}: {}/{}",
                        self.t("Valid replies", "有效应答"),
                        n(data, "valid_samples"),
                        data["samples"].as_array().map_or(0, Vec::len)
                    ));
                    if data["best_sample"].is_object() {
                        let b = &data["best_sample"];
                        lines.push(format!(
                            "{}={} ms  {}={} ms  stratum={}",
                            self.t("Best offset", "最佳偏差"),
                            n(b, "offset_ms"),
                            self.t("delay", "延迟"),
                            n(b, "delay_ms"),
                            n(b, "stratum")
                        ));
                    }
                }
                "target" => lines.push(format!(
                    "{}: {}",
                    self.t("Target", "目标"),
                    s(data, "normalized")
                )),
                "doctor" => {
                    lines.push(format!(
                        "HopKit {}  {} / {}",
                        s(data, "version"),
                        s(data, "platform"),
                        s(data, "architecture")
                    ));
                    lines.push(format!("ICMP: {}", s(data, "icmp_adapter")));
                    lines.push(format!(
                        "{}: {}",
                        self.t("Tools", "工具"),
                        list(&data["tools"])
                    ));
                }
                "network" => lines.extend(self.network(data)),
                "check" => {
                    let issues = data["evidence"].as_array().map_or(0, |es| {
                        es.iter()
                            .filter(|e| {
                                e["status"] == "failed"
                                    || e["data"]["status_code"].as_u64().is_some_and(|n| n >= 400)
                            })
                            .count()
                    });
                    lines.push(format!(
                        "\n{}: {} {}",
                        if data["execution_complete"] == true {
                            self.t("Check complete", "检查完成")
                        } else {
                            self.t("Partial check", "检查未完成")
                        },
                        issues,
                        self.t("issue(s)", "项异常")
                    ));
                    if let Some(findings) = data["findings"].as_array() {
                        for f in findings.iter().take(3) {
                            let i = if self.english { 1 } else { 0 };
                            if let Some(fact) = f["fact"][i].as_str() {
                                lines.push(clean(fact));
                            }
                            if let Some(next) = f["next_step"][i].as_str() {
                                lines.push(format!("  {}", clean(next)));
                            }
                        }
                    }
                }
                "monitor" => {
                    if let Some(totals) = data["totals"].as_array() {
                        lines.push(self.t("\nMonitor summary:", "\n监测汇总：").into());
                        for (i, t) in totals.iter().enumerate() {
                            lines.push(format!(
                                "{}  {}={}  {}={}  {}={}",
                                self.monitor_target(i),
                                self.t("probes", "探测"),
                                n(t, "completed"),
                                self.t("OK", "成功"),
                                n(t, "succeeded"),
                                self.t("failed", "失败"),
                                n(t, "failed")
                            ));
                        }
                    }
                }
                "iperf3" if self.request.action != "serve" => {
                    lines.push(format!(
                        "\n{}: {}",
                        self.t("Summary", "汇总"),
                        self.rate(data)
                    ));
                    if data["success"] == false {
                        lines.push(
                            self.t(
                                "Test failed: no confirmed traffic at both ends.",
                                "测试失败：未确认双端均收到数据。",
                            )
                            .into(),
                        );
                    }
                }
                "ftp" if self.request.action == "list" => {
                    if let Some(entries) = data["entries"].as_array() {
                        lines.extend(entries.iter().filter_map(Value::as_str).map(clean));
                        lines.push(format!("{} {}", entries.len(), self.t("entries", "项")));
                    }
                    if data["entries_truncated"] == true {
                        lines.push(self.t("Listing truncated.", "目录列表已截断。").into());
                    }
                }
                "wormhole" => {
                    lines.push(format!(
                        "{}: {}",
                        if data["local_saved"] == true {
                            self.t("Saved", "已保存")
                        } else if data["success"] == true {
                            self.t("Transfer complete", "传输完成")
                        } else {
                            self.t("Transferred", "已传输")
                        },
                        bytes(&data["bytes"])
                    ));
                    if data["save_path"].is_string() {
                        lines.push(s(data, "save_path"));
                    }
                    if data["warning"].is_string() {
                        lines.push(format!(
                            "{}: {}",
                            self.t("Warning", "注意"),
                            self.message(data["warning"].as_str().unwrap())
                        ));
                    }
                }
                "syslog" if self.request.action == "send" => lines.push(format!(
                    "{}: {} -> {}",
                    self.t("UDP submitted", "已提交 UDP 数据"),
                    bytes(&data["bytes"]),
                    s(data, "destination")
                )),
                _ if !matches!(self.request.action.as_str(), "serve" | "listen") => {
                    lines.push(format!(
                        "{}: {}",
                        if envelope["error"].is_object() {
                            self.t("Transferred", "已传输")
                        } else {
                            self.t("Transfer complete", "传输完成")
                        },
                        bytes(&data["bytes"])
                    ));
                    if data["path"].is_string() {
                        lines.push(s(data, "path"));
                    }
                }
                _ => {}
            }
        }
        if self.service_started && matches!(self.request.action.as_str(), "serve" | "listen") {
            let stats = if data.is_object() {
                data
            } else {
                &self.service_stats
            };
            let mut parts = vec![];
            for (key, en, zh) in [
                ("received", "received", "接收"),
                ("written", "saved", "写入"),
                ("replied", "replied", "应答"),
                ("transfers", "transfers", "传输"),
                ("downloads", "downloads", "下载"),
                ("uploads", "uploads", "上传"),
                ("completed_tests", "tests", "测试"),
                ("denied", "denied", "拒绝"),
                ("errors", "errors", "错误"),
                ("invalid", "invalid", "无效"),
                ("send_errors", "send errors", "发送错误"),
            ] {
                if !stats[key].is_null() {
                    parts.push(format!("{}={}", self.t(en, zh), n(stats, key)));
                }
            }
            lines.push(format!(
                "{}{}",
                self.t("Stopped", "服务已停止"),
                if parts.is_empty() {
                    String::new()
                } else {
                    format!(": {}", parts.join("  "))
                }
            ));
        }
        if envelope["error"].is_object() {
            if envelope["error"]["code"] == "CANCELLED" {
                lines.push(self.t("Cancelled.", "已取消。").into());
            } else {
                lines.push(self.error(&envelope["error"]));
            }
        }
        lines.join("\n")
    }
    fn network(&self, data: &Value) -> Vec<String> {
        let mut lines = vec![];
        if let Some(interfaces) = data["interfaces"].as_array() {
            for i in interfaces {
                let addresses = i["addresses"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|a| format!("{}/{}", s(a, "address"), n(a, "prefix")))
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                if addresses.is_empty() {
                    continue;
                }
                lines.push(format!(
                    "{}  {}  {}",
                    s(i, "name"),
                    s(i, "link_status"),
                    addresses
                ));
                if i["gateways"].as_array().is_some_and(|a| !a.is_empty()) {
                    lines.push(format!(
                        "  {}: {}",
                        self.t("Gateway", "网关"),
                        list(&i["gateways"])
                    ));
                }
            }
        }
        if data["dns"].as_array().is_some_and(|a| !a.is_empty()) {
            lines.push(format!("DNS: {}", list(&data["dns"])));
        }
        if let Some(errors) = data["errors"].as_array() {
            for e in errors {
                lines.push(format!(
                    "{}: {} {}",
                    self.t("Query failed", "查询失败"),
                    s(e, "adapter"),
                    s(e, "code")
                ));
            }
        }
        if lines.is_empty() {
            lines.push(
                self.t("No interface addresses found.", "未找到接口地址。")
                    .into(),
            );
        }
        lines
    }
}

fn endpoint(address: &str, port: u16) -> String {
    if address.contains(':') {
        format!("[{address}]:{port}")
    } else {
        format!("{address}:{port}")
    }
}
fn rrtype(kind: u64) -> String {
    match kind {
        1 => "A",
        2 => "NS",
        5 => "CNAME",
        6 => "SOA",
        12 => "PTR",
        15 => "MX",
        16 => "TXT",
        28 => "AAAA",
        _ => return format!("TYPE{kind}"),
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hopkit_core::{ToolError, envelope};
    use serde_json::json;

    fn output(tool: &str, action: &str, english: bool) -> TextOutput {
        TextOutput::new(
            &Request {
                tool: tool.into(),
                action: action.into(),
                target: "127.0.0.1".into(),
                port: 443,
                ..Request::default()
            },
            english,
        )
    }
    fn concise(text: &str) {
        for internal in [
            "task_id",
            "schema_version",
            "event_type",
            "samples\"",
            "\"request\"",
            "null",
            "\u{1b}",
        ] {
            assert!(!text.contains(internal), "unexpected {internal}: {text}");
        }
    }
    #[test]
    fn probes_distinguish_reply_timeout_refusal_and_cancelled_statistics() {
        for english in [true, false] {
            let mut out = output("tcping", "", english);
            let reply = out.event(&json!({"event_type":"sample","seq":1,"address":"::1","status":"connected","ok":true,"rtt_ms":1.234})).unwrap();
            assert!(reply.contains("[::1]:443") && reply.contains("1.23 ms"));
            let timeout = out
                .event(&json!({"event_type":"sample","seq":2,"status":"timeout","rtt_ms":null}))
                .unwrap();
            let refused = out
                .event(&json!({"event_type":"sample","seq":3,"status":"refused"}))
                .unwrap();
            assert_ne!(timeout, refused);
            assert!(!timeout.contains(" ms"));
            let mut error = ToolError::new("CANCELLED", "cancelled");
            error.data = Some(
                json!({"sent":4,"completed":3,"received":1,"lost":2,"loss_percent":66.67,"interrupted_probes":1,"rtt_min_ms":1.23,"rtt_avg_ms":1.23,"rtt_max_ms":1.23,"samples":[{}]}),
            );
            let summary = out.finish(&envelope("secret", "tcping", Err(error)));
            assert!(summary.contains("66.67%") && summary.contains("1.23/1.23/1.23 ms"));
            assert!(summary.contains(if english { "Interrupted" } else { "中断" }));
            concise(&format!("{reply}\n{timeout}\n{refused}\n{summary}"));
            assert!(
                out.event(&json!({"event_type":"stats","samples":[{}]}))
                    .is_none()
            );
        }
    }
    #[test]
    fn trace_groups_three_probes_and_flushes_partial_hop_on_cancel() {
        let mut out = output("trace", "", true);
        for seq in 1..=3 {
            let line =
                out.event(&json!({"event_type":"sample","seq":seq,"hop":1,"status":"timeout"}));
            if seq < 3 {
                assert!(line.is_none());
            } else {
                assert_eq!(line.unwrap(), " 1  *   *   *");
            }
        }
        assert!(out.event(&json!({"event_type":"sample","seq":4,"hop":2,"address":"192.0.2.1","status":"ttl_expired","rtt_ms":2})).is_none());
        let mut e = ToolError::new("CANCELLED", "cancelled");
        e.data = Some(json!({"reached":false}));
        let final_text = out.finish(&envelope("id", "trace", Err(e)));
        assert!(final_text.contains(" 2  192.0.2.1  2 ms"));
        assert!(final_text.contains("Destination not reached"));
        concise(&final_text);
    }
    #[test]
    fn diagnostic_stages_surface_http_errors_and_skipped_tls_validation() {
        let mut out = output("check", "", true);
        let http = json!({"event_type":"stage","evidence":{"stage":"http","status":"observed","data":{"status_code":403}}});
        assert!(out.event(&http).unwrap().contains("FAIL  HTTP 403"));
        let tls = json!({"event_type":"stage","evidence":{"stage":"tls","status":"observed","data":{"certificate_validation":"SKIPPED / 已跳过"}}});
        assert!(out.event(&tls).unwrap().contains("WARN"));
        let dns = json!({"event_type":"stage","evidence":{"stage":"dns","status":"failed","error":{"code":"DNS_ERROR","message":"Cannot resolve / 无法解析"}}});
        assert!(
            out.event(&dns)
                .unwrap()
                .contains("DNS_ERROR: Cannot resolve")
        );
        let summary = out.finish(&envelope(
            "id",
            "check",
            Ok(json!({"execution_complete":true,"evidence":[http["evidence"],dns["evidence"]]})),
        ));
        assert!(summary.contains("2 issue(s)"));
    }
    #[test]
    fn finite_commands_show_only_useful_fields() {
        let cases = [
            (
                "dns",
                "",
                json!({"source":"system","addresses":["127.0.0.1"],"success":true}),
                "127.0.0.1",
            ),
            (
                "dns",
                "",
                json!({"records":[{"section":"answer","name":"example.test","type":28,"value":"::1","ttl":60}],"rcode":0}),
                "example.test  AAAA  ::1",
            ),
            (
                "dns",
                "",
                json!({"records":[],"rcode":3,"success":false}),
                "NXDOMAIN",
            ),
            (
                "ntp",
                "query",
                json!({"valid_samples":1,"samples":[{}],"best_sample":{"offset_ms":-2.5,"delay_ms":1.1,"stratum":2}}),
                "-2.50 ms",
            ),
            (
                "network",
                "",
                json!({"interfaces":[{"name":"Ethernet","link_status":"Up","addresses":[{"address":"192.0.2.1","prefix":24}],"gateways":["192.0.2.254"]}],"dns":["192.0.2.53"]}),
                "192.0.2.1/24",
            ),
            (
                "target",
                "",
                json!({"normalized":"https://example.test/","query_redacted":true}),
                "https://example.test/",
            ),
            (
                "doctor",
                "",
                json!({"version":"test","platform":"windows","architecture":"x86_64","icmp_adapter":"windows-iphelper","tools":["ping"]}),
                "windows / x86_64",
            ),
            (
                "ftp",
                "list",
                json!({"entries":["report.txt"],"success":true}),
                "report.txt",
            ),
            (
                "ftp",
                "get",
                json!({"path":"report.txt","bytes":1024,"success":true}),
                "1.00 KiB",
            ),
            (
                "tftp",
                "put",
                json!({"bytes":1024,"success":true}),
                "1.00 KiB",
            ),
            (
                "syslog",
                "send",
                json!({"bytes":12,"destination":"127.0.0.1:514","success":true}),
                "UDP submitted",
            ),
            (
                "iperf3",
                "client",
                json!({"bytes":1048576,"duration_secs":1,"mbps":8.39,"loss_percent":2.5,"jitter_ms":0.5,"success":true}),
                "loss=2.50%",
            ),
            (
                "wormhole",
                "receive",
                json!({"local_saved":true,"bytes":"4294967296","save_path":"报告.zip","warning":"saved / 已保存"}),
                "4.00 GiB",
            ),
        ];
        for (tool, action, data, expected) in cases {
            let value = envelope("internal", tool, Ok(data));
            let line = output(tool, action, true).finish(&value);
            assert!(line.contains(expected), "{tool}: {line}");
            concise(&line);
            concise(&output(tool, action, false).finish(&value));
        }
    }
    #[test]
    fn services_keep_ready_errors_and_final_counters_without_stats_spam() {
        for tool in ["ftp", "http", "tftp", "ntp", "syslog", "iperf3"] {
            let mut out = output(tool, "serve", true);
            let ready = out.event(&json!({"event_type":"ready","listen":"127.0.0.1:8080","root":"shared","writable":false})).unwrap();
            assert!(ready.contains("127.0.0.1:8080"));
            if tool != "iperf3" {
                assert!(
                    out.event(&json!({"event_type":"stats","received":4,"errors":1}))
                        .is_none()
                );
            }
            let summary = out.finish(&envelope(
                "id",
                tool,
                Ok(json!({"received":4,"errors":1,"success":true})),
            ));
            assert!(summary.contains("received=4") && summary.contains("errors=1"));
            concise(&summary);
            let failed = output(tool, "serve", true).finish(&envelope(
                "id",
                tool,
                Err(ToolError::new("ADDRESS_IN_USE", "Port in use")),
            ));
            assert!(failed.contains("ADDRESS_IN_USE") && !failed.contains("Stopped"));
        }
    }
    #[test]
    fn monitor_deduplicates_snapshots_and_reports_cumulative_failures() {
        let mut out = output("monitor", "", true);
        out.request.monitor = Some(hopkit_core::monitor::Config {
            targets: vec![hopkit_core::monitor::Target {
                label: "Server".into(),
                protocol: "tcping".into(),
                target: "::1".into(),
                port: 443,
            }],
            ..Default::default()
        });
        let event = json!({"event_type":"monitor_snapshot","samples":[{"seq":1,"target_index":0,"ok":false,"status":"refused"}]});
        assert!(
            out.event(&event)
                .unwrap()
                .contains("[::1]:443  Connection refused")
        );
        assert!(out.event(&event).is_none());
        let end = out.finish(&envelope(
            "id",
            "monitor",
            Ok(json!({"totals":[{"completed":1,"succeeded":0,"failed":1}]})),
        ));
        assert!(end.contains("failed=1"));
    }
    #[test]
    fn wormhole_progress_is_throttled_codes_never_print_and_warnings_survive() {
        let mut out = output("wormhole", "send", true);
        assert!(
            out.event(
                &json!({"event_type":"wormhole_state","phase":"waiting_peer","code":"secret-code"})
            )
            .unwrap()
            .contains("Waiting for peer")
        );
        assert!(
            out.event(
                &json!({"event_type":"wormhole_state","phase":"waiting_peer","code":"secret-code"})
            )
            .is_none()
        );
        out.event(&json!({"event_type":"wormhole_state","phase":"transferring"}));
        assert!(
            out.event(
                &json!({"event_type":"wormhole_state","phase":"transferring","bytes":"1024"})
            )
            .is_none()
        );
        out.progress_at = Instant::now() - Duration::from_secs(2);
        assert!(
            out.event(
                &json!({"event_type":"wormhole_state","phase":"transferring","bytes":"1024"})
            )
            .unwrap()
            .contains("1.00 KiB")
        );
        let text = out.finish(&envelope("id","wormhole",Ok(json!({"success":true,"bytes":"1024","warning":"Receipt may be incomplete","code":"secret-code"}))));
        assert!(text.contains("Receipt may be incomplete") && !text.contains("secret-code"));
    }
    #[test]
    fn remote_names_cannot_inject_lines_or_terminal_escape_codes() {
        let mut out = output("syslog", "listen", true);
        let log = out.event(&json!({"event_type":"log","record":{"source":"127.0.0.1","text":"hello\n\u{1b}[2J\\share\\file"}})).unwrap();
        assert!(!log.contains('\n') && !log.contains('\u{1b}'));
        assert!(log.contains("\\n") && log.contains("\\share\\file"));
    }
}
