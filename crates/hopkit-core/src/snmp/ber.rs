//! Strict, bounded BER framing around the embedded USM primitives. No indefinite lengths.
use super::*;
use rusnmp::codec::BerEncoder;

pub fn tlv(tag: u8, bytes: &[u8]) -> Vec<u8> {
    let mut e = BerEncoder::new();
    e.write_tagged(tag, bytes);
    e.into_bytes()
}
pub fn integer(n: i64) -> Vec<u8> {
    let mut e = BerEncoder::new();
    e.write_integer(n);
    e.into_bytes()
}
pub fn oid_bytes(oid: &[u32]) -> Vec<u8> {
    let mut out = vec![];
    for mut n in std::iter::once(oid[0] as u64 * 40 + oid[1] as u64)
        .chain(oid[2..].iter().map(|x| *x as u64))
    {
        let mut b = vec![(n & 127) as u8];
        n >>= 7;
        while n > 0 {
            b.push((n & 127) as u8 | 128);
            n >>= 7;
        }
        b.reverse();
        out.extend(b);
    }
    out
}
pub fn null_varbinds(oids: &[Vec<u32>]) -> Vec<u8> {
    oids.iter()
        .flat_map(|o| tlv(0x30, &[tlv(6, &oid_bytes(o)), vec![5, 0]].concat()))
        .collect()
}
#[derive(Clone, Copy)]
pub struct Reader<'a> {
    pub bytes: &'a [u8],
}
impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }
    pub fn take(&mut self) -> Result<(u8, &'a [u8]), ToolError> {
        let b = self.bytes;
        if b.len() < 2 {
            return Err(protocol("Truncated BER"));
        }
        let tag = b[0];
        let mut pos = 2;
        let mut len = b[1] as usize;
        if len & 128 != 0 {
            let count = len & 127;
            if count == 0 || count > 4 || b.len() < pos + count {
                return Err(protocol("Invalid BER length"));
            }
            len = 0;
            for v in &b[pos..pos + count] {
                len = (len << 8) | *v as usize;
            }
            pos += count;
        }
        if len > b.len() - pos {
            return Err(protocol("BER length exceeds datagram"));
        }
        self.bytes = &b[pos + len..];
        Ok((tag, &b[pos..pos + len]))
    }
    pub fn tagged(&mut self, tag: u8) -> Result<&'a [u8], ToolError> {
        let (t, b) = self.take()?;
        if t != tag {
            return Err(protocol("Unexpected BER tag"));
        }
        Ok(b)
    }
    pub fn seq(&mut self) -> Result<Reader<'a>, ToolError> {
        Ok(Self::new(self.tagged(0x30)?))
    }
    pub fn int(&mut self) -> Result<i64, ToolError> {
        let b = self.tagged(2)?;
        if b.is_empty() || b.len() > 8 {
            return Err(protocol("Invalid INTEGER"));
        }
        let mut n = if b[0] & 128 == 0 { 0i64 } else { -1 };
        for v in b {
            n = (n << 8) | *v as i64;
        }
        Ok(n)
    }
    pub fn uint(&mut self) -> Result<u32, ToolError> {
        u32::try_from(self.int()?).map_err(|_| protocol("Unsigned INTEGER out of range"))
    }
    pub fn octets(&mut self) -> Result<&'a [u8], ToolError> {
        self.tagged(4)
    }
    pub fn end(&self) -> Result<(), ToolError> {
        if self.bytes.is_empty() {
            Ok(())
        } else {
            Err(protocol("Trailing BER data"))
        }
    }
}
pub fn decode_oid(bytes: &[u8]) -> Result<Vec<u32>, ToolError> {
    let mut out = vec![];
    let mut n = 0u64;
    let mut count = 0;
    for b in bytes {
        count += 1;
        if count > 5 {
            return Err(protocol("OID arc overflow"));
        }
        n = (n << 7) | (*b & 127) as u64;
        if b & 128 == 0 {
            if out.is_empty() {
                out.push(if n < 40 {
                    0
                } else if n < 80 {
                    1
                } else {
                    2
                });
                n -= out[0] as u64 * 40;
            }
            out.push(u32::try_from(n).map_err(|_| protocol("OID overflow"))?);
            n = 0;
            count = 0;
        }
    }
    if count != 0 || out.len() < 2 || out.len() > 128 {
        return Err(protocol("Invalid OID"));
    }
    Ok(out)
}
fn unsigned(b: &[u8], bits: usize) -> Result<String, ToolError> {
    if b.is_empty()
        || b.len() > (bits / 8 + 1)
        || b[0] & 128 != 0
        || b.len() == bits / 8 + 1 && b[0] != 0
    {
        return Err(protocol("Invalid counter"));
    }
    let mut n = 0u64;
    for v in b {
        n = (n << 8) | *v as u64;
    }
    Ok(n.to_string())
}
pub fn varbinds(bytes: &[u8]) -> Result<Vec<Varbind>, ToolError> {
    let mut r = Reader::new(bytes);
    let mut out = vec![];
    while !r.bytes.is_empty() {
        if out.len() >= 2048 {
            return Err(protocol("Too many varbinds in datagram"));
        }
        let mut vb = r.seq()?;
        let oid = decode_oid(vb.tagged(6)?)?;
        let (tag, b) = vb.take()?;
        vb.end()?;
        let (kind, value, object_error) = match tag {
            2 => {
                let mut n = if b.first().is_some_and(|x| x & 128 != 0) {
                    -1i64
                } else {
                    0
                };
                if b.is_empty() || b.len() > 8 {
                    return Err(protocol("Invalid integer value"));
                }
                for v in b {
                    n = (n << 8) | *v as i64;
                }
                ("integer", n.to_string(), None)
            }
            4 => (
                "octet_string",
                match std::str::from_utf8(b) {
                    Ok(s) => s.into(),
                    Err(_) => b.iter().map(|v| format!("{v:02x}")).collect(),
                },
                None,
            ),
            6 => ("oid", oid_string(&decode_oid(b)?), None),
            0x40 if b.len() == 4 => (
                "ip_address",
                format!("{}.{}.{}.{}", b[0], b[1], b[2], b[3]),
                None,
            ),
            0x41 | 0x42 | 0x43 | 0x46 => (
                match tag {
                    0x41 => "counter32",
                    0x42 => "gauge32",
                    0x43 => "timeticks",
                    _ => "counter64",
                },
                unsigned(b, if tag == 0x46 { 64 } else { 32 })?,
                None,
            ),
            5 | 0x80 | 0x81 | 0x82 if b.is_empty() => {
                let k = match tag {
                    5 => "null",
                    0x80 => "noSuchObject",
                    0x81 => "noSuchInstance",
                    _ => "endOfMibView",
                };
                (
                    k,
                    String::new(),
                    if tag == 5 { None } else { Some(k.to_string()) },
                )
            }
            0x44 => (
                "opaque",
                b.iter().map(|v| format!("{v:02x}")).collect(),
                None,
            ),
            _ => return Err(protocol("Unsupported or invalid SNMP value")),
        };
        out.push(Varbind {
            oid: oid_string(&oid),
            kind: kind.into(),
            value,
            object_error,
        });
    }
    Ok(out)
}
pub struct Response {
    pub id: i32,
    pub tag: u8,
    pub status: u32,
    pub index: u32,
    pub values: Vec<Varbind>,
}
pub fn pdu(r: &mut Reader<'_>) -> Result<Response, ToolError> {
    let (tag, b) = r.take()?;
    if ![0xa2, 0xa8].contains(&tag) {
        return Err(protocol("Expected response/report"));
    }
    let mut p = Reader::new(b);
    let id = i32::try_from(p.int()?).map_err(|_| protocol("Request ID overflow"))?;
    let status = p.uint()?;
    let index = p.uint()?;
    let values = varbinds(p.tagged(0x30)?)?;
    p.end()?;
    r.end()?;
    Ok(Response {
        id,
        tag,
        status,
        index,
        values,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_framing_and_oid_roundtrip() {
        for bytes in [
            vec![0x30, 0x80],
            vec![0x30, 0x84, 0xff, 0xff, 0xff, 0xff],
            vec![0x30, 3, 0],
        ] {
            assert!(Reader::new(&bytes).seq().is_err());
        }
        for input in ["0.39.0", "1.3.6.1.2.1.1.3.0", "2.4294967295.4294967295"] {
            let o = oid(input).unwrap();
            assert_eq!(decode_oid(&oid_bytes(&o)).unwrap(), o);
        }
        assert!(decode_oid(&[0x2b, 0x80]).is_err());
    }
    #[test]
    fn counters_are_exact_and_malformed_varbind_never_disappears() {
        let o = tlv(6, &oid_bytes(&oid("sysUpTime").unwrap()));
        let v = tlv(
            0x30,
            &[
                o.clone(),
                tlv(0x46, &[0, 255, 255, 255, 255, 255, 255, 255, 255]),
            ]
            .concat(),
        );
        assert_eq!(varbinds(&v).unwrap()[0].value, "18446744073709551615");
        let mut malformed = v.clone();
        malformed.extend([0x30, 5, 6]);
        assert!(varbinds(&malformed).is_err());
        assert!(varbinds(&tlv(0x30, &[o, tlv(0x41, &[1, 0, 0, 0, 0])].concat())).is_err());
    }
}
