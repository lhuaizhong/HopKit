//! Built-in HTTP/1.1 + rustls. No command, proxy environment, system CA lookup or
//! dynamic OpenSSL dependency. Only HEAD response headers are read.
use crate::{
    runtime::*,
    target::Target,
    transport::{self, ControlledIo},
};
use rustls::{
    ClientConfig, ClientConnection, RootCertStore, StreamOwned,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime, pem::PemObject},
};
use serde_json::{Value, json};
use std::{
    io::{self, Read, Write},
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Debug)]
struct SkipCertificateValidation;
impl ServerCertVerifier for SkipCertificateValidation {
    fn verify_server_cert(
        &self,
        _: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }
    // Even in the explicit insecure mode, verify handshake signatures against
    // the presented certificate. Only chain/name/time validation is skipped.
    fn verify_tls12_signature(
        &self,
        m: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            m,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }
    fn verify_tls13_signature(
        &self,
        m: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            m,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }
    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}
fn config(r: &Request) -> Result<ClientConfig, ToolError> {
    let mut roots = RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    if !r.tls_ca_file.is_empty() {
        let file = std::fs::File::open(&r.tls_ca_file).map_err(|e| {
            ToolError::new(
                "TLS_CA_ERROR",
                format!("Cannot open CA file / 无法打开 CA 文件: {e}"),
            )
        })?;
        if !file.metadata()?.is_file() {
            return Err(ToolError::new(
                "TLS_CA_ERROR",
                "CA must be a regular PEM file / CA 须为普通 PEM 文件",
            ));
        }
        let mut bytes = Vec::new();
        file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > 1024 * 1024 {
            return Err(ToolError::new(
                "TLS_CA_ERROR",
                "CA file exceeds 1 MiB / CA 文件超过 1 MiB",
            ));
        }
        let mut count = 0;
        for cert in CertificateDer::pem_slice_iter(&bytes) {
            let cert = cert.map_err(|_| {
                ToolError::new("TLS_CA_ERROR", "Invalid PEM certificate / PEM 证书无效")
            })?;
            roots.add(cert).map_err(|_| {
                ToolError::new("TLS_CA_ERROR", "Invalid CA certificate / CA 证书无效")
            })?;
            count += 1;
        }
        if count == 0 {
            return Err(ToolError::new(
                "TLS_CA_ERROR",
                "No certificates in CA file / CA 文件中没有证书",
            ));
        }
    }
    let mut config =
        ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .map_err(|e| ToolError::new("TLS_CONFIG_ERROR", e.to_string()))?
            .with_root_certificates(roots)
            .with_no_client_auth();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    if r.insecure_tls {
        config
            .dangerous()
            .set_certificate_verifier(Arc::new(SkipCertificateValidation));
    }
    Ok(config)
}
trait Duplex: Read + Write {}
impl<T: Read + Write> Duplex for T {}
pub struct Response {
    pub status: u16,
    pub location: Option<String>,
    pub data: Value,
}
type StageSink<'a> = dyn FnMut(&str, ToolResult, f64) + 'a;
fn failed<T>(
    stage: &str,
    start: Instant,
    error: ToolError,
    emit: &mut StageSink<'_>,
) -> Result<T, ToolError> {
    if error.code != "CANCELLED" {
        emit(
            stage,
            Err(error.clone()),
            start.elapsed().as_secs_f64() * 1000.,
        );
    }
    Err(error)
}
fn tls_error(error: io::Error, c: &Cancel) -> ToolError {
    if let Err(e) = cancelled(c) {
        return e;
    }
    if is_timeout(&error) {
        return ToolError::new("TIMEOUT", "TLS handshake timed out / TLS 握手超时");
    }
    let code = if error
        .get_ref()
        .and_then(|e| e.downcast_ref::<rustls::Error>())
        .is_some_and(|e| {
            matches!(
                e,
                rustls::Error::InvalidCertificate(_) | rustls::Error::NoCertificatesPresented
            )
        }) {
        "TLS_CERTIFICATE_ERROR"
    } else {
        "TLS_HANDSHAKE_ERROR"
    };
    ToolError::new(code, format!("{error} / TLS 握手或证书校验失败"))
}
pub fn request(
    t: &Target,
    candidates: &[SocketAddr],
    r: &Request,
    c: &Cancel,
    emit: &mut StageSink<'_>,
) -> Result<Response, ToolError> {
    let started = Instant::now();
    let deadline = started + Duration::from_millis(r.timeout_ms);
    let mut last = ToolError::new("CONNECT_ERROR", "No matching address / 无匹配地址");
    let mut socket = None;
    let mut attempts = Vec::new();
    for addr in candidates.iter().take(8) {
        match transport::connect(*addr, &r.source_address, c, deadline) {
            Ok(s) => {
                socket = Some(s);
                break;
            }
            Err(e) if e.code == "CANCELLED" => return Err(e),
            Err(e) => {
                attempts.push(json!({"address":addr.to_string(),"code":e.code}));
                last = e;
                if Instant::now() >= deadline {
                    break;
                }
            }
        }
    }
    let Some(socket) = socket else {
        last.data = Some(json!({"attempts":attempts}));
        return failed("connect", started, last, emit);
    };
    let connect_ms = started.elapsed().as_secs_f64() * 1000.;
    let local = socket.local_addr()?.ip().to_string();
    let remote = socket.peer_addr()?.ip().to_string();
    emit(
        "connect",
        Ok(
            json!({"url":t.normalized,"local_address":local,"remote_address":remote,"connect_ms":connect_ms,"failed_attempts":attempts,"measurement":"socket connect elapsed / socket 建连耗时"}),
        ),
        connect_ms,
    );
    let controlled = ControlledIo {
        socket,
        cancel: c,
        deadline,
    };
    let tls_start = Instant::now();
    let mut tls_ms = None;
    let mut channel: Box<dyn Duplex + '_> = if t.scheme.as_deref() == Some("https") {
        let handshake = (|| -> Result<_, ToolError> {
            let config = config(r)?;
            cancelled(c)?;
            let name = ServerName::try_from(t.host.clone()).map_err(|_| {
                ToolError::new(
                    "TLS_CONFIG_ERROR",
                    "Invalid TLS server name / TLS 主机名无效",
                )
            })?;
            let connection = ClientConnection::new(Arc::new(config), name)
                .map_err(|e| ToolError::new("TLS_CONFIG_ERROR", e.to_string()))?;
            let mut stream = StreamOwned::new(connection, controlled);
            while stream.conn.is_handshaking() {
                stream
                    .conn
                    .complete_io(&mut stream.sock)
                    .map_err(|e| tls_error(e, c))?;
                cancelled(c)?;
            }
            // A final flight may still be queued after is_handshaking becomes false.
            while stream.conn.wants_write() {
                stream
                    .conn
                    .complete_io(&mut stream.sock)
                    .map_err(|e| tls_error(e, c))?;
            }
            Ok(stream)
        })();
        let stream = match handshake {
            Ok(s) => s,
            Err(e) => return failed("tls", tls_start, e, emit),
        };
        let elapsed = tls_start.elapsed().as_secs_f64() * 1000.;
        tls_ms = Some(elapsed);
        emit(
            "tls",
            Ok(
                json!({"url":t.normalized,"certificate_validation":if r.insecure_tls{"SKIPPED / 已跳过"}else{"passed_by_rustls / 内置 TLS 校验通过"},"trust_store":if r.tls_ca_file.is_empty(){"bundled Mozilla roots"}else{"bundled Mozilla roots + explicit CA file"},"protocol":format!("{:?}",stream.conn.protocol_version()),"cipher_suite":format!("{:?}",stream.conn.negotiated_cipher_suite()),"peer_certificate_count":stream.conn.peer_certificates().map(|certs|certs.len()),"handshake_ms":elapsed,"measurement":"TLS handshake after established TCP / TCP 建连后 TLS 握手耗时"}),
            ),
            elapsed,
        );
        Box::new(stream)
    } else {
        Box::new(controlled)
    };
    let http_start = Instant::now();
    let url = t.url.as_ref().expect("HTTP target has URL");
    let path = format!(
        "{}{}",
        url.path(),
        url.query().map(|q| format!("?{q}")).unwrap_or_default()
    );
    let host = if t.host.contains(':') {
        format!("[{}]", t.host)
    } else {
        t.host.clone()
    };
    let host = if url.port().is_some() {
        format!("{host}:{}", t.port.unwrap())
    } else {
        host
    };
    let request = format!(
        "HEAD {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: HopKit/{}\r\nAccept: */*\r\nConnection: close\r\n\r\n",
        env!("CARGO_PKG_VERSION")
    );
    let response = (|| -> Result<_, ToolError> {
        channel
            .write_all(request.as_bytes())
            .map_err(|e| transport::io_error(e, c))?;
        channel.flush().map_err(|e| transport::io_error(e, c))?;
        read_headers(&mut channel, c)
    })();
    let (status, location, bytes) = match response {
        Ok(v) => v,
        Err(e) => return failed("http", http_start, e, emit),
    };
    Ok(Response {
        status,
        location,
        data: json!({"url":t.normalized,"method":"HEAD","status_code":status,"local_address":local,"remote_address":remote,"connect_ms":connect_ms,"tls_ms":tls_ms,"response_wait_ms":http_start.elapsed().as_secs_f64()*1000.,"total_ms":started.elapsed().as_secs_f64()*1000.,"header_bytes":bytes,"adapter":"built_in_http1_rustls","measurement":"HTTP/1.1 HEAD; response wait from request write to complete headers; body read limit 0, headers <=64 KiB / 请求写入到完整响应头，正文读取上限 0，响应头最多 64 KiB"}),
    })
}
fn read_headers(
    stream: &mut dyn Read,
    c: &Cancel,
) -> Result<(u16, Option<String>, usize), ToolError> {
    let mut total = 0;
    let mut block = Vec::new();
    let mut interim = 0;
    loop {
        cancelled(c)?;
        // Reading a byte at a time stops exactly at the header terminator; no
        // response body is read even when the peer ignores HEAD semantics.
        let mut b = [0];
        if stream.read(&mut b).map_err(|e| transport::io_error(e, c))? == 0 {
            return Err(ToolError::new(
                "PROTOCOL_ERROR",
                "Connection closed before complete headers / 响应头未完成连接即关闭",
            ));
        }
        total += 1;
        if total > 65536 {
            return Err(ToolError::new(
                "RESOURCE_EXHAUSTED",
                "HTTP headers exceed 64 KiB / HTTP 响应头超过 64 KiB",
            ));
        }
        block.push(b[0]);
        if !block.ends_with(b"\r\n\r\n") {
            continue;
        }
        let mut headers = [httparse::EMPTY_HEADER; 128];
        let mut response = httparse::Response::new(&mut headers);
        match response.parse(&block) {
            Ok(httparse::Status::Complete(_)) => {}
            _ => {
                return Err(ToolError::new(
                    "PROTOCOL_ERROR",
                    "Malformed HTTP response headers / HTTP 响应头格式无效",
                ));
            }
        }
        let status = response
            .code
            .filter(|n| (100..=599).contains(n))
            .ok_or_else(|| {
                ToolError::new("PROTOCOL_ERROR", "Invalid HTTP status / HTTP 状态码无效")
            })?;
        if (100..200).contains(&status) && status != 101 {
            interim += 1;
            if interim > 8 {
                return Err(ToolError::new(
                    "PROTOCOL_ERROR",
                    "Too many interim responses / 临时响应过多",
                ));
            }
            block.clear();
            continue;
        }
        let locations: Vec<_> = response
            .headers
            .iter()
            .filter(|h| h.name.eq_ignore_ascii_case("location"))
            .collect();
        if locations.len() > 1 {
            return Err(ToolError::new(
                "PROTOCOL_ERROR",
                "Ambiguous redirect locations / 重定向地址不明确",
            ));
        }
        let location = locations
            .first()
            .map(|h| {
                std::str::from_utf8(h.value)
                    .map(str::trim)
                    .map(String::from)
            })
            .transpose()
            .map_err(|_| {
                ToolError::new(
                    "PROTOCOL_ERROR",
                    "Invalid redirect encoding / 重定向编码无效",
                )
            })?;
        return Ok((status, location, total));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    #[test]
    fn parser_handles_interim_and_rejects_ambiguous_or_invalid_headers() {
        let c = Arc::new(AtomicBool::new(false));
        let (code,location,_)=read_headers(&mut io::Cursor::new(b"HTTP/1.1 103 Early Hints\r\nLink: x\r\n\r\nHTTP/1.1 302 Found\r\nLocation: /next\r\n\r\nBODY"),&c).unwrap();
        assert_eq!(code, 302);
        assert_eq!(location.as_deref(), Some("/next"));
        for raw in [
            b"HTTP/1.1 302 Found\r\nLocation: /a\r\nLocation: /b\r\n\r\n".as_slice(),
            b"HTTP/1.1 999 Bad\r\n\r\n",
            b"broken\r\n\r\n",
        ] {
            assert_eq!(
                read_headers(&mut io::Cursor::new(raw), &c)
                    .unwrap_err()
                    .code,
                "PROTOCOL_ERROR"
            );
        }
    }
}
