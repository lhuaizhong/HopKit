//! Shared, sequential dependency graph for GUI and CLI. Observations are not task status.
use crate::{
    native_http, network,
    runtime::*,
    target::{self, Target},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub id: String,
    pub stage: String,
    pub status: String,
    pub duration_ms: f64,
    pub data: Value,
    pub error: Option<ToolError>,
}
fn evidence(
    id: &str,
    stage: &str,
    start: Instant,
    result: ToolResult,
) -> Result<Evidence, ToolError> {
    if let Err(e) = &result
        && e.code == "CANCELLED"
    {
        return Err(e.clone());
    }
    Ok(match result {
        Ok(data) => Evidence {
            id: id.into(),
            stage: stage.into(),
            status: if data["success"] == false {
                "failed"
            } else {
                "observed"
            }
            .into(),
            duration_ms: start.elapsed().as_secs_f64() * 1000.,
            data,
            error: None,
        },
        Err(e) => Evidence {
            id: id.into(),
            stage: stage.into(),
            status: "failed".into(),
            duration_ms: start.elapsed().as_secs_f64() * 1000.,
            data: e.data.clone().unwrap_or(Value::Null),
            error: Some(e),
        },
    })
}
fn record(items: &mut Vec<Evidence>, emit: &Sink<'_>, item: Evidence) {
    emit(json!({"event_type":"stage","evidence":item}));
    items.push(item);
}
fn skipped(id: &str, reason: &str) -> Evidence {
    Evidence {
        id: id.into(),
        stage: id.into(),
        status: "skipped".into(),
        duration_ms: 0.,
        data: json!({"reason":reason}),
        error: None,
    }
}

fn tcp(addr: SocketAddr, source: &str, c: &Cancel, timeout: u64) -> ToolResult {
    let stream = crate::transport::connect(
        addr,
        source,
        c,
        Instant::now() + Duration::from_millis(timeout),
    )?;
    Ok(
        json!({"success":true,"remote_address":addr.to_string(),"local_address":stream.local_addr()?.to_string(),"measurement":"TCP connect only / 仅 TCP 建连"}),
    )
}

fn addresses(t: &Target, r: &Request, c: &Cancel) -> Result<Vec<SocketAddr>, ToolError> {
    if let Some(addr) = target::literal_socket(&t.host, t.port.unwrap_or(0)) {
        if r.ipv6 && addr.is_ipv4() {
            return Err(ToolError::new(
                "INVALID_ARGUMENT",
                "IPv6 selected with IPv4 target / 已选 IPv6，但目标为 IPv4",
            ));
        }
        return Ok(vec![addr]);
    }
    if t.scope.is_some() {
        return Err(ToolError::new(
            "UNSUPPORTED_CAPABILITY",
            "Use numeric IPv6 scope (see interface index) / IPv6 scope 请使用接口数字索引",
        ));
    }
    resolve_all(&t.host, t.port.unwrap_or(0), r.ipv6, c)
}

pub fn run(r: &Request, c: &Cancel, emit: &Sink<'_>) -> ToolResult {
    let target = target::parse(&r.target)?;
    if !r.source_address.is_empty() && target::literal_socket(&r.source_address, 0).is_none() {
        return Err(ToolError::new(
            "INVALID_ARGUMENT",
            "Numeric source address required; use numeric IPv6 scope / 请填写源 IP，IPv6 scope 使用数字",
        ));
    }
    let started = chrono::Utc::now();
    let start = Instant::now();
    let mut items = Vec::new();
    let execution = (|| -> Result<(), ToolError> {
        cancelled(c)?;
        record(
            &mut items,
            emit,
            evidence("network", "network", Instant::now(), network::overview(c))?,
        );
        let dns_start = Instant::now();
        let resolved = addresses(&target, r, c);
        let dns_result=resolved.as_ref().map(|a|json!({"addresses":a.iter().map(|a|a.ip().to_string()).collect::<Vec<_>>(),"scope":"system resolver; not source-bound / 系统解析器，不绑定源地址","literal":target.host.split('%').next().unwrap_or("").parse::<std::net::IpAddr>().is_ok()})).map_err(Clone::clone);
        record(
            &mut items,
            emit,
            evidence("dns", "dns", dns_start, dns_result)?,
        );
        if !r.dns_server.is_empty() {
            let mut q = r.clone();
            q.tool = "dns".into();
            q.target = target.host.clone();
            q.port = 0;
            q.record_type = if r.ipv6 { "AAAA" } else { "A" }.into();
            record(
                &mut items,
                emit,
                evidence(
                    "dns_compare",
                    "dns_compare",
                    Instant::now(),
                    crate::dns::query(&q, c, &|_| {}),
                )?,
            );
        }
        let addrs = match resolved {
            Ok(v) => v,
            Err(_) => {
                for id in ["route", "icmp", "tcp", "http"] {
                    record(
                        &mut items,
                        emit,
                        skipped(
                            id,
                            "System DNS failed; dependent checks skipped / 系统解析失败，跳过依赖检查",
                        ),
                    );
                }
                return Ok(());
            }
        };
        let candidates: Vec<_> = addrs
            .iter()
            .copied()
            .filter(|a| {
                r.source_address.is_empty()
                    || target::literal_socket(&r.source_address, 0)
                        .is_some_and(|s| s.is_ipv4() == a.is_ipv4())
            })
            .collect();
        let addr = *candidates.first().ok_or_else(|| {
            ToolError::new(
                "UNSUPPORTED_CAPABILITY",
                "No resolved address matches the source family / 无匹配源地址族的目标地址",
            )
        })?;
        record(
            &mut items,
            emit,
            evidence(
                "route",
                "route",
                Instant::now(),
                network::route(
                    &match addr {
                        SocketAddr::V6(a) if a.scope_id() != 0 => {
                            format!("{}%{}", a.ip(), a.scope_id())
                        }
                        _ => addr.ip().to_string(),
                    },
                    c,
                ),
            )?,
        );
        if r.source_address.is_empty() {
            let mut ping = r.clone();
            ping.tool = "ping".into();
            ping.target = addr.ip().to_string();
            ping.count = 1;
            ping.interval_ms = 0;
            ping.ipv6 = addr.is_ipv6();
            if addr.is_ipv6() && matches!(addr,SocketAddr::V6(a) if a.scope_id()!=0) {
                record(
                    &mut items,
                    emit,
                    skipped(
                        "icmp",
                        "Scoped ICMP unavailable in this workflow / 本流程暂不支持带 scope 的 ICMP",
                    ),
                );
            } else {
                record(
                    &mut items,
                    emit,
                    evidence(
                        "icmp",
                        "icmp",
                        Instant::now(),
                        crate::diagnostics::probe(&ping, c, &|_| {}),
                    )?,
                );
            }
        } else {
            record(
                &mut items,
                emit,
                skipped(
                    "icmp",
                    "ICMP source binding unavailable; TCP/HTTP are bound / 暂不支持 ICMP 源绑定，TCP/HTTP 实际绑定",
                ),
            );
        }
        if target.port.is_some() {
            let mut attempts = Vec::new();
            let connect_start = Instant::now();
            let mut successful = None;
            for candidate in candidates.iter().take(8) {
                cancelled(c)?;
                match tcp(*candidate, &r.source_address, c, r.timeout_ms) {
                    Ok(v) => {
                        successful = Some(v);
                        break;
                    }
                    Err(e) if e.code == "CANCELLED" => return Err(e),
                    Err(e) => attempts.push(json!({"address":candidate.to_string(),"code":e.code})),
                }
            }
            let result = successful
                .map(|mut v| {
                    v["failed_attempts"] = json!(attempts);
                    v
                })
                .ok_or_else(|| ToolError {
                    code: attempts
                        .last()
                        .and_then(|v| v["code"].as_str())
                        .unwrap_or("CONNECT_ERROR")
                        .into(),
                    message: "No TCP connection succeeded / TCP 未连接成功".into(),
                    data: Some(json!({"attempts":attempts})),
                });
            record(
                &mut items,
                emit,
                evidence("tcp", "tcp", connect_start, result)?,
            );
        } else {
            record(
                &mut items,
                emit,
                skipped("tcp", "No service port specified / 未指定服务端口"),
            );
        }
        if let Some(url) = target.url.clone() {
            http(url, r, c, emit, &mut items)?;
        } else {
            record(
                &mut items,
                emit,
                skipped(
                    "http",
                    "A hostname does not identify an HTTP service; enter an HTTP(S) URL / 仅主机不能确定 HTTP 服务，请输入 URL",
                ),
            );
        }
        if r.include_trace {
            if !r.source_address.is_empty() || target.scope.is_some() {
                record(
                    &mut items,
                    emit,
                    skipped(
                        "trace",
                        "Source-bound/scoped traceroute unavailable / 暂不支持源绑定或 scope 路由追踪",
                    ),
                );
            } else {
                let mut q = r.clone();
                q.tool = "trace".into();
                q.target = addr.ip().to_string();
                q.ipv6 = addr.is_ipv6();
                record(
                    &mut items,
                    emit,
                    evidence(
                        "trace",
                        "trace",
                        Instant::now(),
                        crate::diagnostics::probe(&q, c, &|_| {}),
                    )?,
                );
            }
        }
        cancelled(c)
    })();
    let result = json!({"target":target,"started_at":started.to_rfc3339(),"finished_at":chrono::Utc::now().to_rfc3339(),"timezone":"UTC","duration_ms":start.elapsed().as_secs_f64()*1000.,"version":env!("CARGO_PKG_VERSION"),"platform":std::env::consts::OS,"source_address":r.source_address,"proxy_mode":"direct; system/PAC/environment proxies disabled / 直连，不使用系统/PAC/环境代理","http_adapter":"built_in_rust_http1_rustls","tls_trust":if r.tls_ca_file.is_empty(){"bundled Mozilla roots"}else{"bundled Mozilla roots + explicit CA file"},"tls_verification":if r.insecure_tls{"SKIPPED / 已跳过"}else{"enabled / 已启用"},"evidence":items,"findings":explain(&items),"scope":"One diagnostic session; does not establish long-term health / 单次诊断，不能证明长期网络正常","execution_complete":execution.is_ok()});
    match execution {
        Ok(()) => Ok(result),
        Err(mut e) => {
            e.data = Some(result);
            Err(e)
        }
    }
}

// Each redirect is a separate request. No cookies, Authorization, proxy or referrer
// are sent. HEAD reads no response body, with a hard 64 KiB total header budget.
fn http(
    mut url: url::Url,
    r: &Request,
    c: &Cancel,
    emit: &Sink<'_>,
    items: &mut Vec<Evidence>,
) -> Result<(), ToolError> {
    for hop in 0..=5 {
        cancelled(c)?;
        let t = target::parse(url.as_str())?;
        let dns_start = Instant::now();
        let addrs = addresses(&t, r, c);
        let id = format!("http_{hop}");
        let addrs = match addrs {
            Ok(v) => v,
            Err(e) => {
                record(items, emit, evidence(&id, "dns", dns_start, Err(e))?);
                return Ok(());
            }
        };
        record(
            items,
            emit,
            evidence(
                &format!("http_dns_{hop}"),
                "dns",
                dns_start,
                Ok(
                    json!({"addresses":addrs.iter().map(|a|a.to_string()).collect::<Vec<_>>(),"scope":"system resolver, not source-bound / 系统解析器，不绑定源地址"}),
                ),
            )?,
        );
        let candidates: Vec<_> = addrs
            .into_iter()
            .filter(|a| {
                r.source_address.is_empty()
                    || target::literal_socket(&r.source_address, 0)
                        .is_some_and(|s| s.is_ipv4() == a.is_ipv4())
            })
            .take(8)
            .collect();
        if candidates.is_empty() {
            record(
                items,
                emit,
                evidence(
                    &id,
                    "connect",
                    Instant::now(),
                    Err(ToolError::new(
                        "UNSUPPORTED_CAPABILITY",
                        "Source family mismatch",
                    )),
                )?,
            );
            return Ok(());
        }
        let outcome =
            native_http::request(&t, &candidates, r, c, &mut |stage, result, duration| {
                let id = format!("{stage}_{hop}");
                let mut item = evidence(&id, stage, Instant::now(), result)
                    .expect("cancellation is propagated outside stage events");
                item.duration_ms = duration;
                record(items, emit, item);
            });
        let response = match outcome {
            Ok(v) => v,
            Err(e) if e.code == "CANCELLED" => return Err(e),
            Err(_) => return Ok(()), // the failing phase is already recorded
        };
        let status = response.status;
        let location = response.location.as_deref();
        let id = format!("http_{hop}");
        let redirect = if (300..400).contains(&status) {
            location.map(|s| url.join(s))
        } else {
            None
        };
        let redirect_safe = redirect
            .as_ref()
            .and_then(|v| v.as_ref().ok())
            .map(|u| target::redact(u.as_str()));
        let mut data = response.data;
        data["redirect_to"] = json!(redirect_safe);
        let duration = data["response_wait_ms"].as_f64().unwrap_or(0.);
        let mut item = evidence(&id, "http", Instant::now(), Ok(data))?;
        item.duration_ms = duration;
        record(items, emit, item);
        let Some(next) = redirect else { return Ok(()) };
        let next = next.map_err(|_| {
            ToolError::new("INVALID_REDIRECT", "Invalid redirect URL / 重定向 URL 无效")
        });
        let next = match next {
            Ok(u)
                if target::parse(u.as_str()).is_ok()
                    && !(url.scheme() == "https" && u.scheme() == "http") =>
            {
                u
            }
            _ => {
                record(
                    items,
                    emit,
                    evidence(
                        "redirect",
                        "http",
                        Instant::now(),
                        Err(ToolError::new(
                            "INVALID_REDIRECT",
                            "Redirect rejected: credentials, scheme or HTTPS downgrade / 重定向因凭据、协议或 HTTPS 降级被拒绝",
                        )),
                    )?,
                );
                return Ok(());
            }
        };
        if hop == 5 {
            record(
                items,
                emit,
                evidence(
                    "redirect",
                    "http",
                    Instant::now(),
                    Err(ToolError::new(
                        "REDIRECT_LIMIT",
                        "Redirect limit 5 reached / 已达 5 次重定向上限",
                    )),
                )?,
            );
            return Ok(());
        }
        url = next;
    }
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct Finding {
    pub rule: String,
    pub evidence_ids: Vec<String>,
    pub fact: [String; 2],
    pub possible_cause: [String; 2],
    pub next_step: [String; 2],
}
/// Deterministic rules use only measured evidence; no ownership claims.
pub fn explain(items: &[Evidence]) -> Vec<Finding> {
    let mut out = Vec::new();
    let find = |id: &str| items.iter().find(|e| e.id == id);
    let mut add =
        |rule: &str, ids: Vec<String>, fact: [&str; 2], cause: [&str; 2], next: [&str; 2]| {
            out.push(Finding {
                rule: rule.into(),
                evidence_ids: ids,
                fact: fact.map(String::from),
                possible_cause: cause.map(String::from),
                next_step: next.map(String::from),
            })
        };
    if let (Some(icmp), Some(tcp)) = (find("icmp"), find("tcp"))
        && icmp.data["success"] == false
        && icmp.data["has_samples"] == true
        && tcp.status == "observed"
    {
        add(
            "icmp_filtered",
            vec![icmp.id.clone(), tcp.id.clone()],
            [
                "ICMP 未响应，TCP 端口可连接",
                "ICMP did not respond; TCP connected",
            ],
            [
                "可能存在 ICMP 过滤或限速",
                "ICMP may be filtered or rate limited",
            ],
            [
                "检查 ICMP 策略；继续查看应用结果",
                "Check ICMP policy and review application evidence",
            ],
        );
    }
    for tls in items
        .iter()
        .filter(|e| e.stage == "tls" && e.status == "failed")
    {
        let connect_id = tls.id.replace("tls_", "connect_");
        if let Some(tcp) = find(&connect_id).filter(|e| e.status == "observed") {
            add(
                "tls_failed",
                vec![tcp.id.clone(), tls.id.clone()],
                [
                    "TCP 可连接，TLS 握手或证书校验失败",
                    "TCP connected; TLS handshake or validation failed",
                ],
                [
                    "可能涉及时间、证书链、主机名或中间设备",
                    "Clock, certificate chain, hostname or middlebox may be involved",
                ],
                [
                    "查看 TLS 错误码，核对系统时间和证书",
                    "Review the TLS error code, system clock and certificate",
                ],
            );
        }
    }
    for e in items {
        if e.stage == "http"
            && let Some(code) = e.data["status_code"].as_u64()
        {
            if code >= 400 {
                add(
                    if code == 403 {
                        "http_forbidden"
                    } else {
                        "http_error"
                    },
                    vec![e.id.clone()],
                    if code == 403 {
                        [
                            "服务器返回 HTTP 403，拒绝请求",
                            "Server returned HTTP 403 and refused the request",
                        ]
                    } else {
                        ["收到 HTTP 错误状态码", "Received an HTTP error status"]
                    },
                    [
                        "服务、权限或请求策略可能影响响应",
                        "Service, permissions or request policy may affect the response",
                    ],
                    [
                        "核对状态码与服务日志；HEAD 可能与 GET 不同",
                        "Review status and service logs; HEAD may differ from GET",
                    ],
                );
            } else {
                add(
                    "http_response",
                    vec![e.id.clone()],
                    ["收到 HTTP 响应", "Received an HTTP response"],
                    [
                        "单次响应不能证明业务功能或长期网络正常",
                        "One response does not establish business or long-term network health",
                    ],
                    [
                        "按业务场景验证请求内容和持续表现",
                        "Verify business requests and behavior over time",
                    ],
                );
            }
        }
    }
    if let (Some(system), Some(direct)) = (find("dns"), find("dns_compare"))
        && system.status == "observed"
        && direct.status == "observed"
        && direct.data["rcode"] == 0
    {
        let mut a: Vec<_> = system.data["addresses"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(String::from)
            .collect();
        // Compare the same RR family. Aliases and other sections do not count.
        let family = direct.data["records"]
            .as_array()
            .into_iter()
            .flatten()
            .find_map(|v| match v["type"].as_u64() {
                Some(1) => Some(false),
                Some(28) => Some(true),
                _ => None,
            });
        a.retain(|s| family.is_some_and(|v6| s.contains(':') == v6));
        a.sort();
        a.dedup();
        let mut b: Vec<_> = direct.data["records"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|v| v["section"] == "answer" && matches!(v["type"].as_u64(), Some(1 | 28)))
            .filter_map(|v| v["value"].as_str())
            .map(String::from)
            .collect();
        b.sort();
        b.dedup();
        if !a.is_empty() && !b.is_empty() && a != b {
            add(
                "dns_difference",
                vec![system.id.clone(), direct.id.clone()],
                [
                    "系统与指定 DNS 返回不同地址",
                    "System and direct DNS returned different addresses",
                ],
                [
                    "可能涉及分区解析或 CDN，不能直接判定哪一个错误",
                    "Split DNS or CDN may explain this; neither is proven wrong",
                ],
                [
                    "核对 DNS 作用域、记录类型与业务预期",
                    "Check resolver scope, record family and expected service answers",
                ],
            );
        }
    }
    if out.is_empty() {
        out.push(Finding {
            rule: "insufficient_evidence".into(),
            evidence_ids: items.iter().map(|e| e.id.clone()).collect(),
            fact: [
                "现有证据不足以确定故障归属".into(),
                "Evidence is insufficient to assign fault".into(),
            ],
            possible_cause: [
                "可能存在多个原因".into(),
                "Several causes remain possible".into(),
            ],
            next_step: [
                "查看失败阶段，补充端口或 HTTP(S) URL 后验证".into(),
                "Review failed stages; specify a port or HTTP(S) URL and verify".into(),
            ],
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, atomic::AtomicBool};
    fn e(id: &str, stage: &str, status: &str, data: Value) -> Evidence {
        Evidence {
            id: id.into(),
            stage: stage.into(),
            status: status.into(),
            duration_ms: 0.,
            data,
            error: None,
        }
    }
    fn rules(v: &[Evidence]) -> Vec<String> {
        let f = explain(v);
        for item in &f {
            assert!(item.fact.iter().all(|s| !s.is_empty()));
            assert!(item.possible_cause.iter().all(|s| !s.is_empty()));
            assert!(item.next_step.iter().all(|s| !s.is_empty()));
            assert!(
                item.evidence_ids
                    .iter()
                    .all(|id| v.iter().any(|e| &e.id == id))
            );
        }
        f.into_iter().map(|f| f.rule).collect()
    }
    #[test]
    fn deterministic_rules_positive_negative_and_insufficient() {
        let icmp = e(
            "icmp",
            "icmp",
            "failed",
            json!({"success":false,"has_samples":true}),
        );
        let tcp = e("tcp", "tcp", "observed", json!({"success":true}));
        assert!(rules(&[icmp.clone(), tcp.clone()]).contains(&"icmp_filtered".into()));
        assert!(
            !rules(&[
                e(
                    "icmp",
                    "icmp",
                    "observed",
                    json!({"success":true,"has_samples":true})
                ),
                tcp.clone()
            ])
            .contains(&"icmp_filtered".into())
        );
        assert_eq!(rules(&[icmp]), vec!["insufficient_evidence"]);
        // Unsupported ICMP is not evidence of missing replies.
        assert_eq!(
            rules(&[e("icmp", "icmp", "failed", Value::Null), tcp]),
            vec!["insufficient_evidence"]
        );
        let connect = e("connect_0", "connect", "observed", json!({}));
        let tls = e("tls_0", "tls", "failed", Value::Null);
        assert!(rules(&[connect.clone(), tls.clone()]).contains(&"tls_failed".into()));
        assert_eq!(
            rules(&[connect, e("tls_0", "tls", "observed", json!({}))]),
            vec!["insufficient_evidence"]
        );
        assert_eq!(rules(&[tls]), vec!["insufficient_evidence"]);
        for (status, rule) in [
            (403, "http_forbidden"),
            (500, "http_error"),
            (200, "http_response"),
        ] {
            assert!(
                rules(&[e(
                    "http_0",
                    "http",
                    "observed",
                    json!({"status_code":status})
                )])
                .contains(&rule.into())
            );
            assert!(
                !rules(&[e(
                    "http_0",
                    "http",
                    "observed",
                    json!({"status_code":if status==200{403}else{200}})
                )])
                .contains(&rule.into())
            );
            assert_eq!(
                rules(&[e("http_0", "http", "failed", Value::Null)]),
                vec!["insufficient_evidence"]
            );
        }
        let system = e(
            "dns",
            "dns",
            "observed",
            json!({"addresses":["192.0.2.1","::1"]}),
        );
        let direct = |ip: &str| {
            e(
                "dns_compare",
                "dns_compare",
                "observed",
                json!({"rcode":0,"records":[{"type":1,"section":"answer","value":ip}]}),
            )
        };
        assert!(rules(&[system.clone(), direct("192.0.2.2")]).contains(&"dns_difference".into()));
        assert_eq!(
            rules(&[system.clone(), direct("192.0.2.1")]),
            vec!["insufficient_evidence"]
        );
        assert_eq!(
            rules(&[
                system,
                e("dns_compare", "dns_compare", "failed", Value::Null)
            ]),
            vec!["insufficient_evidence"]
        );
    }
    #[test]
    fn cancellation_before_first_stage_preserves_partial_result() {
        let c = Arc::new(AtomicBool::new(true));
        let error = run(
            &Request {
                tool: "check".into(),
                target: "http://127.0.0.1/?secret=test-secret".into(),
                ..Request::default()
            },
            &c,
            &|_| panic!("Must not probe"),
        )
        .unwrap_err();
        assert_eq!(error.code, "CANCELLED");
        assert_eq!(error.data.as_ref().unwrap()["evidence"], json!([]));
        assert!(
            !serde_json::to_string(&error)
                .unwrap()
                .contains("test-secret")
        );
    }
    #[test]
    fn immutable_request_redacts_even_on_invalid_urls() {
        for target in [
            "http://host/?token=must-never-save",
            "http://user:must-never-save@host/",
        ] {
            let r = Request {
                target: target.into(),
                ..Request::default()
            };
            assert!(
                !serde_json::to_string(&r)
                    .unwrap()
                    .contains("must-never-save")
            );
            assert!(!format!("{r:?}").contains("must-never-save"));
        }
    }
}
