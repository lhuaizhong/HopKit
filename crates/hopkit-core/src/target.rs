//! A target is parsed once. Execution URLs never enter serialized evidence.
use crate::runtime::ToolError;
use serde::{Serialize, Serializer};
use std::net::{IpAddr, SocketAddr};
use url::{Host, Url};

#[derive(Clone, Serialize)]
pub struct Target {
    pub kind: String,
    pub host: String,
    pub port: Option<u16>,
    pub scheme: Option<String>,
    pub path: Option<String>,
    pub normalized: String,
    pub query_redacted: bool,
    pub scope: Option<String>,
    #[serde(skip)]
    pub url: Option<Url>,
}
fn invalid() -> ToolError {
    ToolError::new(
        "INVALID_ARGUMENT",
        "Enter an IP, hostname, host:port or HTTP(S) URL without credentials / 请输入 IP、域名、主机:端口或不含凭据的 HTTP(S) URL",
    )
}
fn port(s: &str) -> Result<u16, ToolError> {
    if s.is_empty() || !s.bytes().all(|c| c.is_ascii_digit()) {
        return Err(invalid());
    }
    s.parse::<u16>().ok().filter(|p| *p > 0).ok_or_else(invalid)
}
fn address(host: &str) -> Result<(String, Option<String>), ToolError> {
    if let Some((ip, scope)) = host.split_once('%') {
        if ip.parse::<std::net::Ipv6Addr>().is_err()
            || scope.is_empty()
            || !scope
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(invalid());
        }
        return Ok((
            format!("{}%{scope}", ip.parse::<std::net::Ipv6Addr>().unwrap()),
            Some(scope.into()),
        ));
    }
    match Host::parse(host).map_err(|_| invalid())? {
        Host::Domain(d) => {
            if d.len() > 253
                || d.trim_end_matches('.').split('.').any(|l| {
                    l.is_empty()
                        || l.len() > 63
                        || l.starts_with('-')
                        || l.ends_with('-')
                        || !l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                })
            {
                return Err(invalid());
            }
            Ok((d, None))
        }
        h => Ok((h.to_string(), None)),
    }
}
pub fn parse(input: &str) -> Result<Target, ToolError> {
    let s = input.trim();
    if s.is_empty()
        || s.len() > 4096
        || s.chars().any(|c| c.is_control() || c.is_whitespace())
        || s.contains('\\')
    {
        return Err(invalid());
    }
    if s.contains("://") {
        let u = Url::parse(s).map_err(|_| invalid())?;
        if !matches!(u.scheme(), "http" | "https")
            || !u.username().is_empty()
            || u.password().is_some()
            || s.split("://")
                .nth(1)
                .unwrap_or("")
                .split(['/', '?', '#'])
                .next()
                .unwrap_or("")
                .contains('@')
        {
            return Err(invalid());
        }
        let host = match u.host().ok_or_else(invalid)? {
            Host::Domain(d) => d.to_string(),
            Host::Ipv4(ip) => ip.to_string(),
            Host::Ipv6(ip) => ip.to_string(),
        };
        if !host.contains(':') {
            address(&host)?;
        }
        let p = u
            .port_or_known_default()
            .filter(|p| *p > 0)
            .ok_or_else(invalid)?;
        let mut safe = u.clone();
        safe.set_query(None);
        safe.set_fragment(None);
        let redacted = u.query().is_some() || u.fragment().is_some();
        let normalized = format!(
            "{}{}",
            safe,
            if u.query().is_some() {
                "?[redacted]"
            } else {
                ""
            }
        );
        return Ok(Target {
            kind: "url".into(),
            host,
            port: Some(p),
            scheme: Some(u.scheme().into()),
            path: Some(u.path().into()),
            normalized,
            query_redacted: redacted,
            scope: None,
            url: Some(u),
        });
    }
    if s.contains(['/', '?', '#', '@']) {
        return Err(invalid());
    }
    let (host, p) = if let Some(rest) = s.strip_prefix('[') {
        let (h, tail) = rest.split_once(']').ok_or_else(invalid)?;
        if h.split('%')
            .next()
            .unwrap_or("")
            .parse::<std::net::Ipv6Addr>()
            .is_err()
        {
            return Err(invalid());
        }
        (
            h,
            if tail.is_empty() {
                None
            } else {
                Some(port(tail.strip_prefix(':').ok_or_else(invalid)?)?)
            },
        )
    } else if s.matches(':').count() == 1 {
        let (h, p) = s.split_once(':').unwrap();
        (h, Some(port(p)?))
    } else {
        (s, None)
    };
    let (host, scope) = if host.contains(':') {
        let ip = host.split('%').next().unwrap_or("");
        ip.parse::<std::net::Ipv6Addr>().map_err(|_| invalid())?;
        if host.contains('%') {
            address(host)?
        } else {
            (ip.parse::<IpAddr>().unwrap().to_string(), None)
        }
    } else {
        address(host)?
    };
    let normalized = if let Some(p) = p {
        if host.contains(':') {
            format!("[{host}]:{p}")
        } else {
            format!("{host}:{p}")
        }
    } else {
        host.clone()
    };
    Ok(Target {
        kind: if host
            .split('%')
            .next()
            .unwrap_or("")
            .parse::<IpAddr>()
            .is_ok()
        {
            "ip"
        } else {
            "hostname"
        }
        .into(),
        host,
        port: p,
        scheme: None,
        path: None,
        normalized,
        query_redacted: false,
        scope,
        url: None,
    })
}
pub fn redact(input: &str) -> String {
    if input.contains("://") {
        parse(input)
            .map(|t| t.normalized)
            .unwrap_or_else(|_| "[invalid URL omitted]".into())
    } else {
        input.into()
    }
}
pub fn serialize_target<S: Serializer>(s: &str, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&redact(s))
}

pub fn literal_socket(host: &str, port: u16) -> Option<SocketAddr> {
    if host.contains(':') {
        format!("[{host}]:{port}").parse().ok()
    } else {
        format!("{host}:{port}").parse().ok()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn targets_and_secrets() {
        for s in [
            "127.0.0.1",
            "::1",
            "[::1]:443",
            "[fe80::1%4]:80",
            "例子.中国",
            "example.com:8443",
            " https://例子.中国:444/a?token=secret#private ",
        ] {
            assert!(parse(s).is_ok(), "{s}");
        }
        for s in [
            "",
            "a b",
            "example.com:0",
            "example.com:65536",
            "[::1]:x",
            "ftp://example.com",
            "https://a:b@host/",
            "https://@host/",
            "http://host:0/",
            "a/path",
            "a..b",
            "-bad.example",
        ] {
            assert!(parse(s).is_err(), "{s}");
        }
        let t = parse("https://例子.中国:444/a?token=secret#private").unwrap();
        assert_eq!(t.host, "xn--fsqu00a.xn--fiqs8s");
        assert_eq!(t.port, Some(444));
        let safe = serde_json::to_string(&t).unwrap();
        assert!(!safe.contains("secret") && !safe.contains("private"));
        assert_eq!(t.url.unwrap().query(), Some("token=secret"));
        assert_eq!(parse("example.com").unwrap().port, None);
        assert_eq!(parse("http://[::1]:8080/").unwrap().host, "::1");
        assert!(parse("a..b.").is_err());
    }
}
