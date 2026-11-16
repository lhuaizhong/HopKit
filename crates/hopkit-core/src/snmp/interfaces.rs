use super::*;
use std::collections::BTreeMap;
#[derive(Clone, Debug, Serialize)]
pub struct Interface {
    pub index: u32,
    pub name: String,
    pub alias: String,
    pub admin_status: Option<String>,
    pub oper_status: Option<String>,
    pub speed_bps: Option<String>,
    pub in_octets: Option<String>,
    pub out_octets: Option<String>,
    pub counter_bits: u32,
    pub in_bps: Option<f64>,
    pub out_bps: Option<f64>,
    pub discontinuity: Option<String>,
    pub counter_discontinuity_time: Option<String>,
    pub in_errors: Option<String>,
    pub out_errors: Option<String>,
    pub in_discards: Option<String>,
    pub out_discards: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Sample {
    pub observed_at: String,
    pub elapsed_ms: u64,
    pub interval_ms: Option<u64>,
    pub uptime: Option<String>,
    pub gap: bool,
    pub error: Option<String>,
    pub interfaces: Vec<Interface>,
}
const COLS: &[(&str, &str)] = &[
    ("descr", "1.3.6.1.2.1.2.2.1.2"),
    ("speed", "1.3.6.1.2.1.2.2.1.5"),
    ("admin", "1.3.6.1.2.1.2.2.1.7"),
    ("oper", "1.3.6.1.2.1.2.2.1.8"),
    ("in32", "1.3.6.1.2.1.2.2.1.10"),
    ("out32", "1.3.6.1.2.1.2.2.1.16"),
    ("in_discards", "1.3.6.1.2.1.2.2.1.13"),
    ("in_errors", "1.3.6.1.2.1.2.2.1.14"),
    ("out_discards", "1.3.6.1.2.1.2.2.1.19"),
    ("out_errors", "1.3.6.1.2.1.2.2.1.20"),
    ("name", "1.3.6.1.2.1.31.1.1.1.1"),
    ("in64", "1.3.6.1.2.1.31.1.1.1.6"),
    ("out64", "1.3.6.1.2.1.31.1.1.1.10"),
    ("high_speed", "1.3.6.1.2.1.31.1.1.1.15"),
    ("alias", "1.3.6.1.2.1.31.1.1.1.18"),
    ("discontinuity", "1.3.6.1.2.1.31.1.1.1.19"),
];
fn delta(old: Option<&str>, new: Option<&str>, millis: u64) -> Option<f64> {
    let a = old?.parse::<u64>().ok()?;
    let b = new?.parse::<u64>().ok()?;
    if millis == 0 || b < a {
        return None;
    }
    Some((b - a) as f64 * 8000.0 / millis as f64)
}
fn rates(current: &mut Interface, old: Option<&Interface>, millis: Option<u64>, reset: bool) {
    current.in_bps = None;
    current.out_bps = None;
    current.discontinuity = None;
    let Some(old) = old else {
        current.discontinuity = Some("baseline".into());
        return;
    };
    let reason = if reset {
        Some("device_restart_or_sampling_gap")
    } else if current.counter_bits != old.counter_bits {
        Some("counter_width_changed")
    } else if current.counter_discontinuity_time != old.counter_discontinuity_time {
        Some("counter_discontinuity_changed")
    } else if current.name != old.name || current.alias != old.alias {
        Some("interface_identity_changed")
    } else if current.counter_discontinuity_time.is_none() {
        Some("discontinuity_time_unavailable")
    } else if current.counter_bits == 32
        && millis.is_some_and(|ms| {
            current
                .speed_bps
                .as_ref()
                .and_then(|s| s.parse::<u64>().ok())
                .is_none_or(|speed| speed as u128 * ms as u128 >= (1u128 << 32) * 8000)
        })
    {
        Some("counter32_wrap_possible")
    } else {
        None
    };
    if let Some(r) = reason {
        current.discontinuity = Some(r.into());
        return;
    }
    current.in_bps =
        millis.and_then(|ms| delta(old.in_octets.as_deref(), current.in_octets.as_deref(), ms));
    current.out_bps =
        millis.and_then(|ms| delta(old.out_octets.as_deref(), current.out_octets.as_deref(), ms));
    if current.in_bps.is_none() || current.out_bps.is_none() {
        current.discontinuity = Some("counter_decreased_or_unavailable".into());
        current.in_bps = None;
        current.out_bps = None;
    }
}
impl Runner<'_> {
    pub(super) fn observe(&mut self) -> Result<(), ToolError> {
        let start = Instant::now();
        let mut last: Option<Sample> = None;
        let mut retained_bytes = 0;
        loop {
            cancelled(self.cancel)?;
            self.deadline = Instant::now() + Duration::from_secs(self.config.deadline_secs);
            let cycle = Instant::now();
            let sample = match self.collect_sample(start, last.as_ref()) {
                Ok(sample) => {
                    self.result.stats.samples_valid += 1;
                    sample
                }
                Err(e)
                    if matches!(
                        e.code.as_str(),
                        "TIMEOUT" | "SNMP_TRANSPORT" | "SNMP_DEADLINE"
                    ) && !self.result.partial =>
                {
                    self.result.stats.samples_failed += 1;
                    self.result.stats.gaps += 1;
                    Sample {
                        observed_at: chrono::Utc::now().to_rfc3339(),
                        elapsed_ms: start.elapsed().as_millis() as u64,
                        interval_ms: last.as_ref().map(|s| {
                            (start.elapsed().as_millis() as u64).saturating_sub(s.elapsed_ms)
                        }),
                        uptime: None,
                        gap: true,
                        error: Some(e.code),
                        interfaces: vec![],
                    }
                }
                Err(e) => return Err(e),
            };
            let size = serde_json::to_vec(&sample).unwrap().len();
            if size > self.config.max_bytes {
                return Err(ToolError::new(
                    "SNMP_LIMIT",
                    "Interface sample exceeds byte budget",
                ));
            }
            while !self.result.interface_samples.is_empty()
                && (self.result.interface_samples.len() >= 128
                    || retained_bytes + size > self.config.max_bytes)
            {
                let old = self.result.interface_samples.remove(0);
                retained_bytes -= serde_json::to_vec(&old).unwrap().len();
                self.result.stats.samples_dropped += 1;
            }
            retained_bytes += size;
            self.result.interface_samples.push(sample.clone());
            last = Some(sample);
            self.result.stats.samples += 1;
            self.snapshot();
            if self.config.samples > 0 && self.result.stats.samples >= self.config.samples as u64 {
                break;
            }
            let wait_for =
                Duration::from_millis(self.config.interval_ms).saturating_sub(cycle.elapsed());
            wait(self.cancel, wait_for)?;
        }
        Ok(())
    }
    fn collect_sample(
        &mut self,
        start: Instant,
        last: Option<&Sample>,
    ) -> Result<Sample, ToolError> {
        let uptime = self
            .query("get", &[oid("sysUpTime")?])?
            .first()
            .filter(|v| v.kind == "timeticks")
            .map(|v| v.value.clone());
        let mut rows: BTreeMap<u32, BTreeMap<&str, String>> = BTreeMap::new();
        let mut raw_bytes = 0;
        let mut raw_items = 0;
        for (key, root) in COLS {
            let prefix = oid(root)?;
            for vb in self.walk(prefix.clone())? {
                raw_items += 1;
                raw_bytes += serde_json::to_vec(&vb).unwrap().len();
                if raw_items > self.config.max_results || raw_bytes > self.config.max_bytes {
                    return Err(ToolError::new(
                        "SNMP_LIMIT",
                        "Interface snapshot budget exceeded",
                    ));
                }
                let parts = oid(&vb.oid)?;
                if parts.len() != prefix.len() + 1 {
                    continue;
                }
                let index = parts[prefix.len()];
                if index == 0 {
                    continue;
                }
                if !rows.contains_key(&index) && rows.len() >= 512 {
                    return Err(ToolError::new(
                        "SNMP_LIMIT",
                        "Interface limit (512) exceeded",
                    ));
                }
                let numeric = !["name", "alias", "descr"].contains(key);
                if numeric && vb.value.parse::<u64>().is_err() {
                    return Err(protocol("Non-numeric IF-MIB counter/status"));
                }
                if key.ends_with("64") && vb.kind != "counter64" {
                    return Err(protocol("Expected IF-MIB Counter64"));
                }
                rows.entry(index).or_default().insert(key, vb.value);
            }
        }
        if self.result.partial {
            return Err(ToolError::new(
                "SNMP_LIMIT",
                "Incomplete interface snapshot",
            ));
        }
        if rows.is_empty() {
            return Err(ToolError::new(
                "SNMP_OBJECT",
                "No readable IF-MIB interfaces / 没有可读取的 IF-MIB 接口",
            ));
        }
        let elapsed = start.elapsed().as_millis() as u64;
        let interval = last.map(|s| elapsed.saturating_sub(s.elapsed_ms));
        let wall_gap = last
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s.observed_at).ok())
            .is_some_and(|at| {
                (chrono::Utc::now() - at.with_timezone(&chrono::Utc)).num_milliseconds()
                    > self.config.interval_ms as i64 + 2000
            });
        let gap = wall_gap || interval.is_some_and(|ms| ms > self.config.interval_ms + 2000);
        self.result.stats.gaps += u64::from(gap);
        let reboot = match (last.and_then(|s| s.uptime.as_ref()), uptime.as_ref()) {
            (Some(a), Some(b)) => b.parse::<u64>().ok() < a.parse::<u64>().ok(),
            _ => true,
        };
        let mut current = vec![];
        for (index, r) in rows {
            let get = |key: &str| r.get(key).cloned();
            let hc = r.contains_key("in64") && r.contains_key("out64");
            let speed = get("high_speed")
                .and_then(|s| s.parse::<u64>().ok())
                .filter(|v| *v > 0)
                .and_then(|v| v.checked_mul(1000000))
                .map(|v| v.to_string())
                .or_else(|| get("speed"));
            let mut iface = Interface {
                index,
                name: get("name").or_else(|| get("descr")).unwrap_or_default(),
                alias: get("alias").unwrap_or_default(),
                admin_status: get("admin"),
                oper_status: get("oper"),
                speed_bps: speed,
                in_octets: get(if hc { "in64" } else { "in32" }),
                out_octets: get(if hc { "out64" } else { "out32" }),
                counter_bits: if hc { 64 } else { 32 },
                in_bps: None,
                out_bps: None,
                discontinuity: None,
                counter_discontinuity_time: get("discontinuity"),
                in_errors: get("in_errors"),
                out_errors: get("out_errors"),
                in_discards: get("in_discards"),
                out_discards: get("out_discards"),
            };
            rates(
                &mut iface,
                last.and_then(|s| s.interfaces.iter().find(|i| i.index == index)),
                interval,
                reboot || gap || last.is_some_and(|s| s.gap),
            );
            current.push(iface);
        }
        Ok(Sample {
            observed_at: chrono::Utc::now().to_rfc3339(),
            elapsed_ms: elapsed,
            interval_ms: interval,
            uptime,
            gap,
            error: None,
            interfaces: current,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn iface() -> Interface {
        Interface {
            index: 1,
            name: "eth0".into(),
            alias: "".into(),
            admin_status: None,
            oper_status: None,
            speed_bps: None,
            in_octets: Some("9007199254740993".into()),
            out_octets: Some("18446744073709550000".into()),
            counter_bits: 64,
            in_bps: None,
            out_bps: None,
            discontinuity: None,
            counter_discontinuity_time: Some("0".into()),
            in_errors: None,
            out_errors: None,
            in_discards: None,
            out_discards: None,
        }
    }
    #[test]
    fn exact_counter64_delta_and_breaks() {
        let old = iface();
        let mut n = old.clone();
        n.in_octets = Some("9007199254741993".into());
        n.out_octets = Some("18446744073709551000".into());
        rates(&mut n, Some(&old), Some(2000), false);
        assert_eq!(n.in_bps, Some(4000.));
        assert_eq!(n.out_bps, Some(4000.));
        n.counter_discontinuity_time = Some("10".into());
        rates(&mut n, Some(&old), Some(2000), false);
        assert!(n.discontinuity.is_some());
        let mut n = old.clone();
        n.in_octets = Some("0".into());
        rates(&mut n, Some(&old), Some(2000), false);
        assert!(n.in_bps.is_none());
        assert!(n.out_bps.is_none());
    }
    #[test]
    fn baseline_reboot_gap_and_counter32_ambiguity_clear_rates() {
        let old = iface();
        for reason in [
            "baseline",
            "restart",
            "missing_discontinuity",
            "identity",
            "counter32",
        ] {
            let mut current = old.clone();
            let mut previous = old.clone();
            if reason == "missing_discontinuity" {
                current.counter_discontinuity_time = None;
                previous.counter_discontinuity_time = None;
            }
            if reason == "identity" {
                current.name = "replacement".into();
            }
            if reason == "counter32" {
                current.counter_bits = 32;
                previous.counter_bits = 32;
                current.speed_bps = Some("100000000000".into());
            }
            rates(
                &mut current,
                if reason == "baseline" {
                    None
                } else {
                    Some(&previous)
                },
                Some(2000),
                reason == "restart",
            );
            assert!(current.in_bps.is_none());
            assert!(current.out_bps.is_none());
            assert!(current.discontinuity.is_some());
        }
    }
}
