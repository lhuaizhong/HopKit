//! Darwin routing socket + Ethernet BPF, using SDK net/route.h and net/bpf.h ABI.
use super::*;
use std::{
    ffi::CStr,
    io,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
};
fn os_error(context: &str) -> ToolError {
    let e = io::Error::last_os_error();
    ToolError::new(
        if matches!(e.raw_os_error(), Some(libc::EPERM | libc::EACCES)) {
            "PERMISSION_DENIED"
        } else {
            "LOCAL_PROBE_FAILED"
        },
        format!("{context}: {e}"),
    )
}
fn wait(fd: i32, deadline: Instant, c: &Cancel) -> Result<(), ToolError> {
    loop {
        cancelled(c)?;
        let left = deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(25));
        if left.is_zero() {
            return Err(ToolError::new("TIMEOUT", "ARP/route query timeout"));
        }
        let mut p = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let rc = unsafe { libc::poll(&mut p, 1, left.as_millis().max(1) as i32) };
        if rc > 0 {
            if p.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                return Err(ToolError::new(
                    "LOCAL_PROBE_FAILED",
                    "ARP/route descriptor closed",
                ));
            }
            if p.revents & libc::POLLIN != 0 {
                return Ok(());
            }
        } else if rc < 0 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
            return Err(os_error("poll"));
        }
    }
}
fn write(fd: i32, data: &[u8]) -> Result<(), ToolError> {
    if unsafe { libc::write(fd, data.as_ptr().cast(), data.len()) } != data.len() as isize {
        return Err(os_error("ARP/route write"));
    }
    Ok(())
}
/// Read-only RTM_GET. Never forces a requested interface over an existing VPN/gateway route.
pub(super) fn on_link(
    ip: Ipv4Addr,
    deadline: Instant,
    c: &Cancel,
) -> Result<Option<u32>, ToolError> {
    let fd = unsafe { libc::socket(libc::PF_ROUTE, libc::SOCK_RAW, libc::AF_INET) };
    if fd < 0 {
        return Err(os_error("ARP route socket"));
    }
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    let mut msg: libc::rt_msghdr = unsafe { std::mem::zeroed() };
    let hdr = std::mem::size_of_val(&msg);
    msg.rtm_msglen = (hdr + 16) as u16;
    msg.rtm_version = libc::RTM_VERSION as u8;
    msg.rtm_type = libc::RTM_GET as u8;
    msg.rtm_addrs = libc::RTA_DST;
    msg.rtm_pid = unsafe { libc::getpid() };
    msg.rtm_seq = 1;
    let mut bytes = vec![0u8; hdr + 16];
    unsafe {
        std::ptr::copy_nonoverlapping(
            (&msg as *const libc::rt_msghdr).cast::<u8>(),
            bytes.as_mut_ptr(),
            hdr,
        )
    };
    bytes[hdr] = 16;
    bytes[hdr + 1] = libc::AF_INET as u8;
    bytes[hdr + 4..hdr + 8].copy_from_slice(&ip.octets());
    write(fd.as_raw_fd(), &bytes)?;
    let mut buf = [0u8; 8192];
    loop {
        wait(fd.as_raw_fd(), deadline, c)?;
        let n = unsafe { libc::read(fd.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
        if n < 0 {
            if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(os_error("ARP route read"));
        }
        if n < (hdr as isize) {
            continue;
        }
        let r = unsafe { std::ptr::read_unaligned(buf.as_ptr().cast::<libc::rt_msghdr>()) };
        if r.rtm_type != msg.rtm_type || r.rtm_seq != 1 || r.rtm_pid != msg.rtm_pid {
            continue;
        }
        if r.rtm_errno != 0 {
            return Err(ToolError::new(
                "SYSTEM_QUERY_FAILED",
                format!("ARP route: {}", io::Error::from_raw_os_error(r.rtm_errno)),
            ));
        }
        return Ok(route_index(r.rtm_index as u32, r.rtm_flags));
    }
}
fn route_index(index: u32, flags: i32) -> Option<u32> {
    (index != 0
        && flags & libc::RTF_UP != 0
        && flags
            & (libc::RTF_GATEWAY
                | libc::RTF_REJECT
                | libc::RTF_BLACKHOLE
                | libc::RTF_LOCAL
                | libc::RTF_BROADCAST
                | libc::RTF_MULTICAST)
            == 0)
        .then_some(index)
}
fn local_mac(name: &str) -> Result<[u8; 6], ToolError> {
    let mut list = std::ptr::null_mut();
    if unsafe { libc::getifaddrs(&mut list) } != 0 {
        return Err(os_error("ARP interface MAC"));
    }
    struct Guard(*mut libc::ifaddrs);
    impl Drop for Guard {
        fn drop(&mut self) {
            unsafe { libc::freeifaddrs(self.0) }
        }
    }
    let _guard = Guard(list);
    let mut entry = list;
    while !entry.is_null() {
        let a = unsafe { &*entry };
        entry = a.ifa_next;
        if a.ifa_addr.is_null()
            || unsafe { (*a.ifa_addr).sa_family } as i32 != libc::AF_LINK
            || unsafe { CStr::from_ptr(a.ifa_name) }.to_bytes() != name.as_bytes()
        {
            continue;
        }
        let d = unsafe { &*a.ifa_addr.cast::<libc::sockaddr_dl>() };
        if d.sdl_alen == 6 && d.sdl_len as usize >= 8 + d.sdl_nlen as usize + 6 {
            let raw = unsafe {
                std::slice::from_raw_parts(a.ifa_addr.cast::<u8>().add(8 + d.sdl_nlen as usize), 6)
            };
            let mac: [u8; 6] = raw.try_into().unwrap();
            if neighbors::mac(
                &mac.iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<Vec<_>>()
                    .join(":"),
            )
            .is_some()
            {
                return Ok(mac);
            }
        }
    }
    Err(ToolError::new(
        "UNSUPPORTED_CAPABILITY",
        "ARP requires a visible Ethernet MAC on the chosen interface / 此接口无可用以太网 MAC",
    ))
}
#[repr(C)]
struct Insn {
    code: u16,
    jt: u8,
    jf: u8,
    k: u32,
}
#[repr(C)]
struct Program {
    len: u32,
    insns: *mut Insn,
}
fn ioctl<T>(fd: i32, code: libc::c_ulong, value: &mut T) -> Result<(), ToolError> {
    if unsafe { libc::ioctl(fd, code, value as *mut T) } < 0 {
        return Err(os_error("ARP BPF configuration"));
    }
    Ok(())
}
fn open(iface: &str, local: Ipv4Addr, remote: Ipv4Addr) -> Result<(OwnedFd, usize), ToolError> {
    let mut opened = None;
    for n in 0..256 {
        let path = std::ffi::CString::new(format!("/dev/bpf{n}")).unwrap();
        let raw = unsafe {
            libc::open(
                path.as_ptr(),
                libc::O_RDWR | libc::O_NONBLOCK | libc::O_CLOEXEC,
            )
        };
        if raw >= 0 {
            opened = Some(unsafe { OwnedFd::from_raw_fd(raw) });
            break;
        }
        let err = io::Error::last_os_error();
        if matches!(err.raw_os_error(), Some(libc::EACCES | libc::EPERM)) {
            return Err(ToolError::new(
                "PERMISSION_DENIED",
                "Active ARP requires read/write access to /dev/bpf* on macOS. Grant BPF access or run the CLI with administrator privileges; ICMP/TCP can continue. / macOS 主动 ARP 需要 BPF 读写权限；ICMP/TCP 可继续。",
            ));
        }
        if err.raw_os_error() != Some(libc::EBUSY) {
            break;
        }
    }
    let fd = opened
        .ok_or_else(|| ToolError::new("RESOURCE_EXHAUSTED", "No available BPF descriptor"))?;
    let raw = fd.as_raw_fd();
    let mut size = 4096u32;
    ioctl(raw, 0xc0044266, &mut size)?; // BIOCSBLEN
    if iface.len() >= libc::IFNAMSIZ {
        return Err(bad("ARP interface name too long"));
    }
    let mut req = [0u8; 32];
    req[..iface.len()].copy_from_slice(iface.as_bytes());
    ioctl(raw, 0x8020426c, &mut req)?; // BIOCSETIF, struct ifreq = 32 bytes on Darwin
    let mut dlt = 0u32;
    ioctl(raw, 0x4004426a, &mut dlt)?;
    if dlt != 1 {
        return Err(ToolError::new(
            "UNSUPPORTED_CAPABILITY",
            "ARP requires an Ethernet link; tunnel interfaces are skipped",
        ));
    }
    // Capture only ARP replies for these exact IPv4 endpoints, at most 42 bytes. No promiscuous mode.
    let mut insns = [
        Insn {
            code: 0x28,
            jt: 0,
            jf: 0,
            k: 12,
        },
        Insn {
            code: 0x15,
            jt: 0,
            jf: 7,
            k: 0x0806,
        },
        Insn {
            code: 0x28,
            jt: 0,
            jf: 0,
            k: 20,
        },
        Insn {
            code: 0x15,
            jt: 0,
            jf: 5,
            k: 2,
        },
        Insn {
            code: 0x20,
            jt: 0,
            jf: 0,
            k: 28,
        },
        Insn {
            code: 0x15,
            jt: 0,
            jf: 3,
            k: u32::from(remote),
        },
        Insn {
            code: 0x20,
            jt: 0,
            jf: 0,
            k: 38,
        },
        Insn {
            code: 0x15,
            jt: 0,
            jf: 1,
            k: u32::from(local),
        },
        Insn {
            code: 0x06,
            jt: 0,
            jf: 0,
            k: 42,
        },
        Insn {
            code: 0x06,
            jt: 0,
            jf: 0,
            k: 0,
        },
    ];
    ioctl(
        raw,
        0x80104267,
        &mut Program {
            len: insns.len() as u32,
            insns: insns.as_mut_ptr(),
        },
    )?;
    ioctl(raw, 0x80044270, &mut 1u32)?; // BIOCIMMEDIATE
    ioctl(raw, 0x80044275, &mut 1u32)?; // BIOCSHDRCMPLT
    ioctl(raw, 0x80044277, &mut 0u32)?; // BIOCSSEESENT
    ioctl(raw, 0x40044266, &mut size)?;
    if !(32..=524288).contains(&size) {
        return Err(ToolError::new(
            "LOCAL_PROBE_FAILED",
            "Invalid BPF buffer size",
        ));
    }
    Ok((fd, size as usize))
}
fn request(mac: [u8; 6], local: Ipv4Addr, dest: Ipv4Addr) -> [u8; 60] {
    let mut b = [0u8; 60];
    b[..6].fill(255);
    b[6..12].copy_from_slice(&mac);
    b[12..22].copy_from_slice(&[8, 6, 0, 1, 8, 0, 6, 4, 0, 1]);
    b[22..28].copy_from_slice(&mac);
    b[28..32].copy_from_slice(&local.octets());
    b[38..42].copy_from_slice(&dest.octets());
    b
}
fn response(b: &[u8], local_mac: [u8; 6], local: Ipv4Addr, dest: Ipv4Addr) -> Option<[u8; 6]> {
    if b.len() < 42
        || b[12..22] != [8, 6, 0, 1, 8, 0, 6, 4, 0, 2]
        || b[28..32] != dest.octets()
        || b[38..42] != local.octets()
        || b[..6] != local_mac
        || b[32..38] != local_mac
        || b[6..12] != b[22..28]
    {
        return None;
    }
    let mac: [u8; 6] = b[22..28].try_into().ok()?;
    (mac[0] & 1 == 0 && mac != [0; 6] && mac != local_mac).then_some(mac)
}
fn frames(buf: &[u8], mut visit: impl FnMut(&[u8])) {
    let mut offset = 0;
    while offset + 18 <= buf.len() {
        // LP64 Darwin BPF uses timeval32 (8 bytes), and 4-byte record alignment.
        let cap = u32::from_ne_bytes(buf[offset + 8..offset + 12].try_into().unwrap()) as usize;
        let hdr = u16::from_ne_bytes(buf[offset + 16..offset + 18].try_into().unwrap()) as usize;
        if hdr < 18 || hdr > buf.len() - offset || cap > buf.len() - offset - hdr {
            break;
        }
        visit(&buf[offset + hdr..offset + hdr + cap]);
        offset += (hdr + cap + 3) & !3;
    }
}
pub(super) fn probe(
    ip: Ipv4Addr,
    iface: &interfaces::Interface,
    deadline: Instant,
    c: &Cancel,
) -> Result<neighbors::Neighbor, ToolError> {
    let local: Ipv4Addr = iface
        .address
        .parse()
        .map_err(|_| bad("ARP source must be IPv4"))?;
    let (fd, size) = open(&iface.name, local, ip)?;
    let mac = local_mac(&iface.name)?;
    cancelled(c)?;
    if Instant::now() >= deadline {
        return Err(ToolError::new("TIMEOUT", "ARP setup timeout"));
    }
    write(fd.as_raw_fd(), &request(mac, local, ip))?;
    let mut buf = vec![0u8; size];
    loop {
        wait(fd.as_raw_fd(), deadline, c)?;
        let n = unsafe { libc::read(fd.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
        if n < 0 {
            if matches!(
                io::Error::last_os_error().kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
            ) {
                continue;
            }
            return Err(os_error("ARP BPF read"));
        }
        let mut found = None;
        frames(&buf[..n as usize], |frame| {
            if let Some(m) = response(frame, mac, local, ip) {
                found = Some(m);
            }
        });
        if let Some(m) = found {
            return reply(ip, iface, &m, "macos-bpf-arp");
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gateway_routes_never_qualify_even_inside_interface_prefix() {
        assert_eq!(route_index(7, libc::RTF_UP), Some(7));
        for f in [
            libc::RTF_GATEWAY,
            libc::RTF_LOCAL,
            libc::RTF_REJECT,
            libc::RTF_BLACKHOLE,
        ] {
            assert_eq!(route_index(7, f | libc::RTF_UP), None);
        }
        assert_eq!(route_index(0, libc::RTF_UP), None);
    }
    #[test]
    fn exact_arp_reply_and_bpf_record_validation() {
        let local = "192.0.2.20".parse().unwrap();
        let remote = "192.0.2.1".parse().unwrap();
        let our = [2, 0, 0, 0, 0, 20];
        let peer = [2, 0, 0, 0, 0, 1];
        let mut b = request(peer, remote, local);
        b[..6].copy_from_slice(&our);
        b[32..38].copy_from_slice(&our);
        b[21] = 2;
        assert_eq!(response(&b, our, local, remote), Some(peer));
        assert!(response(&b, our, local, "192.0.2.2".parse().unwrap()).is_none());
        let mut unrelated = b;
        unrelated[32] = 4;
        assert!(response(&unrelated, our, local, remote).is_none());
        assert!(response(&request(our, local, remote), our, local, remote).is_none());
        let mut buf = vec![0u8; 20 + 60];
        buf[8..12].copy_from_slice(&60u32.to_ne_bytes());
        buf[16..18].copy_from_slice(&20u16.to_ne_bytes());
        buf[20..].copy_from_slice(&b);
        let mut found = 0;
        frames(&buf, |f| {
            assert_eq!(response(f, our, local, remote), Some(peer));
            found += 1;
        });
        assert_eq!(found, 1);
        buf[8..12].copy_from_slice(&u32::MAX.to_ne_bytes());
        frames(&buf, |_| panic!("malformed BPF record"));
    }
}
