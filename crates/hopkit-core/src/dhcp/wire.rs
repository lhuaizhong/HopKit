use super::*;
use std::collections::BTreeMap;
pub(super) fn mac(s: &str) -> Result<[u8; 6], ToolError> {
    let parts: Vec<_> = s
        .split([':', '-'])
        .map(|s| u8::from_str_radix(s, 16))
        .collect();
    if parts.len() != 6 || parts.iter().any(Result::is_err) {
        return Err(ToolError::new(
            "UNSUPPORTED_CAPABILITY",
            "DHCP discovery needs a six-byte Ethernet MAC",
        ));
    }
    let bytes: Vec<u8> = parts.into_iter().map(Result::unwrap).collect();
    let m: [u8; 6] = bytes.try_into().unwrap();
    if m[0] & 1 != 0 || m == [0; 6] {
        return Err(ToolError::new(
            "UNSUPPORTED_CAPABILITY",
            "Invalid interface MAC",
        ));
    }
    Ok(m)
}
pub(super) fn discover(xid: [u8; 4], mac: [u8; 6], secs: u16) -> Vec<u8> {
    let mut p = vec![0; 240];
    p[..3].copy_from_slice(&[1, 1, 6]);
    p[4..8].copy_from_slice(&xid);
    p[8..10].copy_from_slice(&secs.to_be_bytes());
    p[10] = 0x80;
    p[28..34].copy_from_slice(&mac);
    p[236..240].copy_from_slice(&[99, 130, 83, 99]);
    p.extend_from_slice(&[53, 1, 1, 61, 7, 1]);
    p.extend_from_slice(&mac);
    p.extend_from_slice(&[55, 7, 1, 3, 6, 15, 51, 54, 119, 255]);
    p.resize(300, 0);
    p
}
fn options(bytes: &[u8], map: &mut BTreeMap<u8, Vec<u8>>) -> Result<(), ()> {
    let mut i = 0;
    while i < bytes.len() {
        let code = bytes[i];
        i += 1;
        if code == 0 {
            continue;
        }
        if code == 255 {
            return Ok(());
        }
        let n = *bytes.get(i).ok_or(())? as usize;
        i += 1;
        let value = bytes.get(i..i + n).ok_or(())?;
        i += n;
        // RFC 3396 concatenation, followed by exact semantic length checks.
        let entry = map.entry(code).or_default();
        if entry.len() + n > 2048 {
            return Err(());
        }
        entry.extend_from_slice(value);
    }
    Err(())
}
fn ip(b: &[u8]) -> Result<Ipv4Addr, ()> {
    Ok(Ipv4Addr::from(<[u8; 4]>::try_from(b).map_err(|_| ())?))
}
fn ips(v: Option<&Vec<u8>>) -> Result<Vec<Ipv4Addr>, ()> {
    let Some(v) = v else { return Ok(vec![]) };
    if v.is_empty() || v.len() % 4 != 0 {
        return Err(());
    }
    v.chunks(4).map(ip).collect()
}
fn domain_list(b: &[u8]) -> Result<Vec<String>, ()> {
    let mut names = Vec::new();
    let mut next = 0;
    while next < b.len() {
        let mut pos = next;
        let mut end = None;
        let mut labels = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        loop {
            if !seen.insert(pos) || seen.len() > 128 {
                return Err(());
            }
            let len = *b.get(pos).ok_or(())?;
            pos += 1;
            if len == 0 {
                next = end.unwrap_or(pos);
                break;
            }
            if len & 0xc0 == 0xc0 {
                let low = *b.get(pos).ok_or(())? as usize;
                let ptr = (((len & 63) as usize) << 8) | low;
                if ptr >= pos - 1 {
                    return Err(());
                }
                end.get_or_insert(pos + 1);
                pos = ptr;
                continue;
            }
            if len > 63 {
                return Err(());
            }
            let label = b.get(pos..pos + len as usize).ok_or(())?;
            pos += len as usize;
            if label.iter().any(|c| !c.is_ascii_graphic() || *c == b'.') {
                return Err(());
            }
            labels.push(String::from_utf8_lossy(label).into_owned());
        }
        if labels.is_empty() {
            return Err(());
        }
        let name = labels.join(".");
        if name.len() > 253 || names.len() >= 32 {
            return Err(());
        }
        names.push(name);
    }
    Ok(names)
}
pub(super) fn offer(
    p: &[u8],
    source: SocketAddr,
    xid: [u8; 4],
    mac: [u8; 6],
    expected: &[Ipv4Addr],
) -> Result<Option<Offer>, ()> {
    if p.len() < 240 {
        return Err(());
    }
    if p[0] != 2 || p[1] != 1 || p[2] != 6 || p[4..8] != xid || p[28..34] != mac {
        return Ok(None);
    }
    if p[236..240] != [99, 130, 83, 99] {
        return Err(());
    }
    let mut opts = BTreeMap::new();
    options(&p[240..], &mut opts)?;
    if let Some(v) = opts.get(&52) {
        let flags = *v.first().ok_or(())?;
        if v.len() != 1 || !(1..=3).contains(&flags) {
            return Err(());
        }
        if flags & 1 != 0 {
            options(&p[108..236], &mut opts)?
        }
        if flags & 2 != 0 {
            options(&p[44..108], &mut opts)?
        }
    }
    let kind = opts.get(&53).ok_or(())?;
    if kind.len() != 1 {
        return Err(());
    }
    if kind[0] != 2 {
        return Ok(None);
    }
    if let Some(id) = opts.get(&61) {
        let mut ours = vec![1];
        ours.extend_from_slice(&mac);
        if *id != ours {
            return Ok(None);
        }
    }
    let server = ip(opts.get(&54).ok_or(())?)?;
    let yiaddr = ip(&p[16..20])?;
    if server.is_unspecified()
        || server.is_broadcast()
        || server.is_multicast()
        || yiaddr.is_unspecified()
        || yiaddr.is_broadcast()
        || yiaddr.is_multicast()
    {
        return Err(());
    }
    let lease = opts
        .get(&51)
        .map(|v| {
            <[u8; 4]>::try_from(v.as_slice())
                .map(u32::from_be_bytes)
                .map_err(|_| ())
        })
        .transpose()?;
    let mask = opts.get(&1).map(|v| ip(v)).transpose()?;
    if let Some(mask) = mask {
        let inv = !u32::from(mask);
        if inv & (inv.wrapping_add(1)) != 0 {
            return Err(());
        }
    }
    let mut domains = opts
        .get(&119)
        .map(|v| domain_list(v))
        .transpose()?
        .unwrap_or_default();
    if let Some(v) = opts.get(&15) {
        if v.len() > 253 || v.iter().any(|b| !b.is_ascii_graphic()) {
            return Err(());
        }
        let s = String::from_utf8_lossy(v).into_owned();
        if !domains.contains(&s) {
            domains.push(s)
        }
    }
    Ok(Some(Offer {
        observed_at: chrono::Utc::now().to_rfc3339(),
        server_id: server,
        packet_source: source.to_string(),
        yiaddr,
        siaddr: ip(&p[20..24])?,
        relay: ip(&p[24..28])?,
        relay_agent_hex: opts
            .get(&82)
            .map(|v| v.iter().map(|b| format!("{b:02x}")).collect()),
        lease_secs: lease,
        mask,
        routers: ips(opts.get(&3))?,
        dns: ips(opts.get(&6))?,
        search_domains: domains,
        outside_expected: (!expected.is_empty()).then(|| !expected.contains(&server)),
    }))
}
