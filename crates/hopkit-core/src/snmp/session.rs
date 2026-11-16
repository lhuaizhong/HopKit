use super::*;
use ber::*;
use rusnmp::v3::{self, AuthProtocol, PrivProtocol};
use std::net::UdpSocket;
#[derive(Clone, Debug, Serialize)]
pub struct Engine {
    pub id: String,
    pub boots: u32,
    pub time: u32,
}
pub struct Session<'a> {
    config: &'a Config,
    keys: &'a Credentials,
    cancel: &'a Cancel,
    socket: UdpSocket,
    pub remote: String,
    pub local: String,
    pub engine: Option<Engine>,
    engine_id: Vec<u8>,
    engine_at: Instant,
    auth_key: Vec<u8>,
    priv_key: Vec<u8>,
    id: i32,
    salt: u64,
    admission: crate::scan::Session<'static>,
}
fn auth_error() -> ToolError {
    ToolError::new(
        "SNMP_AUTH",
        "SNMPv3 security validation failed / SNMPv3 安全校验失败",
    )
}
struct Envelope<'a> {
    id: i32,
    flags: u8,
    engine: &'a [u8],
    boots: u32,
    time: u32,
    user: &'a [u8],
    auth: &'a [u8],
    privacy: &'a [u8],
    scoped: &'a [u8],
}
fn envelope(bytes: &[u8]) -> Result<Envelope<'_>, ToolError> {
    let mut root = Reader::new(bytes);
    let mut m = root.seq()?;
    root.end()?;
    if m.int()? != 3 {
        return Err(protocol("Expected SNMPv3"));
    }
    let mut h = m.seq()?;
    let id = i32::try_from(h.int()?).map_err(|_| protocol("Message ID overflow"))?;
    if h.int()? < 484 {
        return Err(protocol("Invalid msgMaxSize"));
    }
    let flags = h.octets()?;
    if flags.len() != 1 || flags[0] & !7 != 0 || h.int()? != 3 {
        return Err(protocol("Invalid USM header"));
    }
    h.end()?;
    let mut sr = Reader::new(m.octets()?);
    let mut s = sr.seq()?;
    sr.end()?;
    let engine = s.octets()?;
    let boots = s.uint()?;
    let time = s.uint()?;
    let user = s.octets()?;
    let auth = s.octets()?;
    let privacy = s.octets()?;
    s.end()?;
    if !(5..=32).contains(&engine.len())
        || boots >= i32::MAX as u32
        || time > i32::MAX as u32
        || user.len() > 32
    {
        return Err(protocol("Invalid engine fields"));
    }
    let (tag, scoped) = m.take()?;
    m.end()?;
    if tag != if flags[0] & 2 == 0 { 0x30 } else { 4 } {
        return Err(protocol("Scoped PDU privacy mismatch"));
    }
    Ok(Envelope {
        id,
        flags: flags[0],
        engine,
        boots,
        time,
        user,
        auth,
        privacy,
        scoped,
    })
}
impl<'a> Session<'a> {
    pub fn new(c: &'a Config, k: &'a Credentials, cancel: &'a Cancel) -> Result<Self, ToolError> {
        let addr = resolve(&c.target, c.port, AddressFamily::Auto, cancel)?;
        let socket = UdpSocket::bind(any_addr(addr.is_ipv6()))?;
        socket.connect(addr)?;
        socket.set_read_timeout(Some(Duration::from_millis(50)))?;
        socket.set_write_timeout(Some(Duration::from_millis(100)))?;
        let random = uuid::Uuid::new_v4();
        let b = random.as_bytes();
        let id = i32::from_be_bytes(b[..4].try_into().unwrap()) & 0x3fffffff;
        let salt = u64::from_be_bytes(b[4..12].try_into().unwrap());
        Ok(Self {
            config: c,
            keys: k,
            cancel,
            socket,
            remote: addr.to_string(),
            local: String::new(),
            engine: None,
            engine_id: vec![],
            engine_at: Instant::now(),
            auth_key: vec![],
            priv_key: vec![],
            id,
            salt,
            admission: crate::scan::shared_scheduler().session(20),
        })
        .map(|mut s| {
            s.local = s.socket.local_addr().unwrap().to_string();
            s
        })
    }
    fn auth(&self) -> AuthProtocol {
        if self.config.auth_protocol == "sha1" {
            AuthProtocol::Sha1
        } else {
            AuthProtocol::Sha256
        }
    }
    fn level(&self) -> u8 {
        match self.config.security_level.as_str() {
            "no_auth_no_priv" => 0,
            "auth_no_priv" => 1,
            _ => 3,
        }
    }
    fn exchange(
        &mut self,
        packet: &[u8],
        deadline: Instant,
        stats: &mut Stats,
    ) -> Result<Vec<u8>, ToolError> {
        for attempt in 0..=self.config.retries {
            cancelled(self.cancel)?;
            if Instant::now() >= deadline {
                return Err(ToolError::new(
                    "SNMP_DEADLINE",
                    "SNMP total deadline reached / 达到 SNMP 总期限",
                ));
            }
            let _permit = self.admission.acquire(self.cancel)?;
            if Instant::now() >= deadline {
                return Err(ToolError::new("SNMP_LIMIT", "SNMP total deadline reached"));
            }
            self.socket.send(packet)?;
            stats.requests += 1;
            stats.retries += u64::from(attempt > 0);
            let until =
                (Instant::now() + Duration::from_millis(self.config.timeout_ms)).min(deadline);
            let mut buf = vec![0; 65535];
            loop {
                cancelled(self.cancel)?;
                if Instant::now() >= until {
                    stats.timeouts += 1;
                    break;
                }
                match self.socket.recv(&mut buf) {
                    Ok(n) => {
                        buf.truncate(n);
                        return Ok(buf);
                    }
                    Err(e) if is_timeout(&e) => {}
                    Err(e) => {
                        return Err(ToolError::new(
                            "SNMP_TRANSPORT",
                            format!("SNMP UDP receive failed: {}", e.kind()),
                        ));
                    }
                }
            }
        }
        Err(ToolError::new(
            "TIMEOUT",
            "SNMP response timeout / SNMP 响应超时",
        ))
    }
    fn discovery(&mut self, deadline: Instant, stats: &mut Stats) -> Result<(), ToolError> {
        self.id = self.id.wrapping_add(1) & 0x7fffffff;
        let scoped = v3::encode_scoped_pdu(&[], b"", 0xa0, 0, 0, 0, &[]);
        let sec = v3::encode_usm_params(&[], 0, 0, &[], &[], &[]);
        let packet = v3::encode_v3_message(self.id, 65507, 4, 3, &sec, &scoped, false);
        let data = self.exchange(&packet, deadline, stats)?;
        let m = envelope(&data)?;
        if m.id != self.id || m.flags & 3 != 0 || !m.auth.is_empty() || !m.privacy.is_empty() {
            return Err(auth_error());
        }
        let mut r = Reader::new(m.scoped);
        r.octets()?;
        r.octets()?;
        let p = pdu(&mut r)?;
        if p.tag != 0xa8
            || p.id != 0
            || p.status != 0
            || p.values.len() != 1
            || p.values[0].oid != "1.3.6.1.6.3.15.1.1.4.0"
        {
            return Err(protocol("Expected unknownEngineIDs discovery report"));
        }
        self.engine_id = m.engine.to_vec();
        self.engine = Some(Engine {
            id: m.engine.iter().map(|v| format!("{v:02x}")).collect(),
            boots: m.boots,
            time: m.time,
        });
        self.engine_at = Instant::now();
        if self.level() & 1 != 0 {
            self.auth_key = v3::password_to_key(
                self.auth(),
                self.keys.auth_password.as_bytes(),
                &self.engine_id,
            );
            if self.level() & 2 != 0 {
                self.priv_key = v3::password_to_key(
                    self.auth(),
                    self.keys.privacy_password.as_bytes(),
                    &self.engine_id,
                );
            }
        }
        Ok(())
    }
    pub fn query(
        &mut self,
        op: &str,
        oids: &[Vec<u32>],
        deadline: Instant,
        stats: &mut Stats,
    ) -> Result<Response, ToolError> {
        let tag = match op {
            "get" => 0xa0,
            "getnext" => 0xa1,
            "getbulk" => 0xa5,
            _ => return Err(invalid("Read-only SNMP operation required")),
        };
        let vb = null_varbinds(oids);
        let repetitions = if tag == 0xa5 {
            self.config.max_repetitions as i32
        } else {
            0
        };
        if self.config.version == "3" && self.engine.is_none() {
            self.discovery(deadline, stats)?;
        }
        // A single authenticated notInTimeWindow Report may resynchronize; no security downgrade.
        for resync in 0..=1 {
            self.id = self.id.wrapping_add(1) & 0x7fffffff;
            let id = self.id;
            if self.config.version == "2c" {
                let p = tlv(
                    tag,
                    &[
                        integer(id as i64),
                        integer(0),
                        integer(repetitions as i64),
                        tlv(0x30, &vb),
                    ]
                    .concat(),
                );
                let packet = tlv(
                    0x30,
                    &[integer(1), tlv(4, self.keys.community.as_bytes()), p].concat(),
                );
                let data = self.exchange(&packet, deadline, stats)?;
                let mut root = Reader::new(&data);
                let mut m = root.seq()?;
                root.end()?;
                if m.int()? != 1 || m.octets()? != self.keys.community.as_bytes() {
                    return Err(protocol("SNMP version/community mismatch"));
                }
                let p = pdu(&mut m)?;
                if p.id != id || p.tag != 0xa2 {
                    return Err(protocol("SNMP response ID/type mismatch"));
                }
                stats.responses += 1;
                return Ok(p);
            }
            let engine = self.engine.as_ref().unwrap();
            let time = engine
                .time
                .saturating_add(self.engine_at.elapsed().as_secs().min(i32::MAX as u64) as u32);
            let boots = engine.boots;
            let scoped = v3::encode_scoped_pdu(
                &self.engine_id,
                self.config.context.as_bytes(),
                tag,
                id,
                0,
                repetitions,
                &vb,
            );
            self.salt = self.salt.wrapping_add(1);
            let (payload, privacy) = if self.level() & 2 != 0 {
                v3::encrypt(
                    PrivProtocol::Aes128,
                    &self.priv_key,
                    boots,
                    time,
                    self.salt,
                    &scoped,
                )
            } else {
                (scoped, vec![])
            };
            let auth = vec![
                0;
                if self.level() & 1 == 0 {
                    0
                } else if self.config.auth_protocol == "sha1" {
                    12
                } else {
                    24
                }
            ];
            let sec = v3::encode_usm_params(
                &self.engine_id,
                boots,
                time,
                self.config.username.as_bytes(),
                &auth,
                &privacy,
            );
            let mut packet = v3::encode_v3_message(
                id,
                65507,
                self.level() | 4,
                3,
                &sec,
                &payload,
                self.level() & 2 != 0,
            );
            if self.level() & 1 != 0 {
                let m = envelope(&packet)?;
                let pos = m.auth.as_ptr() as usize - packet.as_ptr() as usize;
                let signature = v3::compute_auth(self.auth(), &self.auth_key, &packet);
                packet[pos..pos + signature.len()].copy_from_slice(&signature);
            }
            let data = self.exchange(&packet, deadline, stats)?;
            let m = envelope(&data)?;
            if m.id != id
                || m.engine != self.engine_id
                || m.user != self.config.username.as_bytes()
                || !(m.flags & 3 == self.level() || self.level() == 3 && m.flags & 3 == 1)
            {
                return Err(auth_error());
            }
            if self.level() & 1 != 0 {
                let mut verify = data.clone();
                let pos = m.auth.as_ptr() as usize - data.as_ptr() as usize;
                verify[pos..pos + m.auth.len()].fill(0);
                if !v3::verify_auth(self.auth(), &self.auth_key, &verify, m.auth) {
                    return Err(auth_error());
                }
            }
            let plain = if m.flags & 2 != 0 {
                if m.privacy.len() != 8 {
                    return Err(auth_error());
                }
                v3::decrypt(
                    PrivProtocol::Aes128,
                    &self.priv_key,
                    m.boots,
                    m.time,
                    m.privacy,
                    m.scoped,
                )
                .map_err(|_| auth_error())?
            } else {
                if !m.privacy.is_empty() || self.level() == 0 && !m.auth.is_empty() {
                    return Err(auth_error());
                }
                tlv(0x30, m.scoped)
            };
            let mut top = Reader::new(&plain);
            let mut r = top.seq()?;
            top.end()?;
            if r.octets()? != self.engine_id || r.octets()? != self.config.context.as_bytes() {
                return Err(protocol("SNMP context mismatch"));
            }
            let p = pdu(&mut r)?;
            // RFC 3412 Reports may use request-id 0 when the scoped request was not processed.
            if p.id != id && !(p.tag == 0xa8 && p.id == 0) {
                return Err(protocol("SNMP request ID mismatch"));
            }
            let time_valid =
                m.boots >= boots && (m.boots > boots || (m.time as i64 - time as i64).abs() <= 150);
            if p.tag == 0xa8 {
                if resync == 0
                    && self.level() & 1 != 0
                    && m.boots >= boots
                    && p.values.len() == 1
                    && p.values[0].oid == "1.3.6.1.6.3.15.1.1.2.0"
                {
                    self.engine = Some(Engine {
                        id: self.engine.as_ref().unwrap().id.clone(),
                        boots: m.boots,
                        time: m.time,
                    });
                    self.engine_at = Instant::now();
                    continue;
                }
                return Err(ToolError::new(
                    "SNMP_REPORT",
                    "SNMPv3 agent Report; check user/security/context / 设备返回 Report，请检查用户与安全配置",
                ));
            }
            if m.flags & 3 != self.level() || self.level() & 1 != 0 && !time_valid {
                return Err(auth_error());
            }
            self.engine = Some(Engine {
                id: self.engine.as_ref().unwrap().id.clone(),
                boots: m.boots,
                time: m.time,
            });
            self.engine_at = Instant::now();
            stats.responses += 1;
            return Ok(p);
        }
        Err(auth_error())
    }
}
