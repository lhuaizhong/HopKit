//! Linux ping sockets. Echo replies arrive as datagrams; router errors arrive
//! through IP_RECVERR/IPV6_RECVERR, including the responding router's address.
use crate::runtime::*;
use serde_json::{Value, json};
use socket2::{Domain, Protocol, Socket, Type};
use std::{
    io,
    mem::{size_of, size_of_val, zeroed},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket},
    os::fd::AsRawFd,
    time::{Duration, Instant},
};

#[derive(Debug, PartialEq)]
struct Reply {
    status: &'static str,
    code: u16,
    address: IpAddr,
}

impl Reply {
    fn sample(&self, start: Instant) -> Value {
        json!({"ok":self.status=="reply","responded":true,"status":self.status,
            "status_code":self.code,"address":self.address.to_string(),
            "rtt_ms":start.elapsed().as_secs_f64()*1000.0})
    }
}

fn echo_request(v6: bool, payload_size: usize, id: u16) -> Vec<u8> {
    let nonce = uuid::Uuid::new_v4();
    let mut request = vec![0x6e; 8 + payload_size];
    request[..4].copy_from_slice(&[if v6 { 128 } else { 8 }, 0, 0, 0]);
    // Linux replaces the echo identifier with the bound ping socket's port.
    request[4..6].copy_from_slice(&id.to_be_bytes());
    request[6..8].copy_from_slice(&nonce.as_bytes()[..2]);
    let random_len = payload_size.min(16);
    request[8..8 + random_len].copy_from_slice(&nonce.as_bytes()[..random_len]);
    // Both checksums (including the IPv6 pseudo-header) are supplied by Linux.
    request
}

fn echo_reply(packet: &[u8], request: &[u8], target: IpAddr, from: IpAddr) -> Option<Reply> {
    (request.len() >= 8
        && packet.len() == request.len()
        && from == target
        && packet[..2] == [if target.is_ipv6() { 129 } else { 0 }, 0]
        && packet[4..] == request[4..])
        .then_some(Reply {
            status: "reply",
            code: 0,
            address: from,
        })
}

fn quoted_request(packet: &[u8], request: &[u8]) -> bool {
    // Routers may quote only the ICMP header. Match id and sequence, plus any
    // quoted payload, but ignore the checksum that the kernel filled in.
    packet.len() >= 8
        && request.len() >= 8
        && packet.len() <= request.len()
        && packet[..2] == request[..2]
        && packet[4..] == request[4..packet.len()]
}

fn read_struct<T: Copy>(bytes: &[u8]) -> Option<T> {
    (bytes.len() >= size_of::<T>()).then(|| {
        // Byte slices need not have the alignment of a native socket structure.
        unsafe { std::ptr::read_unaligned(bytes.as_ptr().cast::<T>()) }
    })
}

fn sockaddr_ip(bytes: &[u8]) -> Option<IpAddr> {
    match i32::from(read_struct::<libc::sa_family_t>(bytes)?) {
        libc::AF_INET => {
            let address = read_struct::<libc::sockaddr_in>(bytes)?;
            Some(Ipv4Addr::from(address.sin_addr.s_addr.to_ne_bytes()).into())
        }
        libc::AF_INET6 => {
            let address = read_struct::<libc::sockaddr_in6>(bytes)?;
            Some(Ipv6Addr::from(address.sin6_addr.s6_addr).into())
        }
        _ => None,
    }
}

fn extended_error(bytes: &[u8], v6: bool) -> io::Result<Option<Reply>> {
    let Some(error) = read_struct::<libc::sock_extended_err>(bytes) else {
        return Ok(None);
    };
    if error.ee_origin == libc::SO_EE_ORIGIN_LOCAL {
        return Err(io::Error::from_raw_os_error(error.ee_errno as i32));
    }
    let origin = if v6 {
        libc::SO_EE_ORIGIN_ICMP6
    } else {
        libc::SO_EE_ORIGIN_ICMP
    };
    if error.ee_origin != origin {
        return Ok(None);
    }
    let status = match (v6, error.ee_type, error.ee_code) {
        (false, 11, _) | (true, 3, _) => "ttl_expired",
        (false, 3, 4) | (true, 2, _) => "packet_too_big",
        (false, 3, _) | (true, 1, _) => "unreachable",
        (false, 12, _) | (true, 4, _) => "parameter_problem",
        _ => "icmp_error",
    };
    Ok(
        sockaddr_ip(&bytes[size_of::<libc::sock_extended_err>()..]).map(|address| Reply {
            status,
            code: u16::from_be_bytes([error.ee_type, error.ee_code]),
            address,
        }),
    )
}

fn receive_error(
    socket: &UdpSocket,
    target: IpAddr,
    request: &[u8],
    packet: &mut [u8],
) -> io::Result<Option<Reply>> {
    loop {
        // Aligned storage for native control headers, extended errors and both
        // sockaddr families. All pointers remain valid throughout recvmsg.
        let mut control = [0_usize; 64];
        let mut destination: libc::sockaddr_storage = unsafe { zeroed() };
        let mut iov = libc::iovec {
            iov_base: packet.as_mut_ptr().cast(),
            iov_len: packet.len(),
        };
        let mut message: libc::msghdr = unsafe { zeroed() };
        message.msg_name = (&mut destination as *mut libc::sockaddr_storage).cast();
        message.msg_namelen = size_of::<libc::sockaddr_storage>() as _;
        message.msg_iov = &mut iov;
        message.msg_iovlen = 1;
        message.msg_control = control.as_mut_ptr().cast();
        message.msg_controllen = size_of_val(&control);
        let n = unsafe {
            libc::recvmsg(
                socket.as_raw_fd(),
                &mut message,
                libc::MSG_ERRQUEUE | libc::MSG_DONTWAIT,
            )
        };
        if n < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::WouldBlock {
                return Ok(None);
            }
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        let destination_bytes = unsafe {
            std::slice::from_raw_parts(
                (&destination as *const libc::sockaddr_storage).cast::<u8>(),
                (message.msg_namelen as usize).min(size_of_val(&destination)),
            )
        };
        if message.msg_flags & (libc::MSG_TRUNC | libc::MSG_CTRUNC) != 0
            || sockaddr_ip(destination_bytes) != Some(target)
            || !quoted_request(&packet[..n as usize], request)
        {
            continue;
        }
        // These headers come from the kernel, but still check each length before
        // constructing a slice for the family-specific decoder.
        let mut header = unsafe { libc::CMSG_FIRSTHDR(&message) };
        while !header.is_null() {
            let cmsg = unsafe { &*header };
            let offset = header as usize - message.msg_control as usize;
            let prefix = unsafe { libc::CMSG_LEN(0) } as usize;
            if cmsg.cmsg_len < prefix || cmsg.cmsg_len > message.msg_controllen - offset {
                break;
            }
            let expected = if target.is_ipv6() {
                (libc::IPPROTO_IPV6, libc::IPV6_RECVERR)
            } else {
                (libc::IPPROTO_IP, libc::IP_RECVERR)
            };
            if (cmsg.cmsg_level, cmsg.cmsg_type) == expected {
                let bytes = unsafe {
                    std::slice::from_raw_parts(libc::CMSG_DATA(header), cmsg.cmsg_len - prefix)
                };
                if let Some(reply) = extended_error(bytes, target.is_ipv6())? {
                    return Ok(Some(reply));
                }
            }
            header = unsafe { libc::CMSG_NXTHDR(&message, header) };
        }
    }
}

fn enable_error_queue(socket: &Socket, v6: bool) -> io::Result<()> {
    let enabled: libc::c_int = 1;
    let (level, option) = if v6 {
        (libc::IPPROTO_IPV6, libc::IPV6_RECVERR)
    } else {
        (libc::IPPROTO_IP, libc::IP_RECVERR)
    };
    // setsockopt copies the integer before returning.
    if unsafe {
        libc::setsockopt(
            socket.as_raw_fd(),
            level,
            option,
            (&enabled as *const libc::c_int).cast(),
            size_of_val(&enabled) as _,
        )
    } < 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub(crate) fn icmp(
    mut addr: SocketAddr,
    ttl: u8,
    timeout: u32,
    payload_size: usize,
    cancel: &Cancel,
) -> Result<Value, ToolError> {
    cancelled(cancel)?;
    let v6 = addr.is_ipv6();
    let socket = Socket::new(
        Domain::for_address(addr),
        Type::DGRAM,
        Some(if v6 { Protocol::ICMPV6 } else { Protocol::ICMPV4 }),
    ).map_err(|error| {
        if error.kind() == io::ErrorKind::PermissionDenied {
            ToolError::new("PERMISSION_DENIED", format!(
                "Linux ICMP ping socket: {error}; net.ipv4.ping_group_range must include a process group / ping_group_range 需要包含进程所属的用户组"
            ))
        } else {
            error.into()
        }
    })?;
    socket.set_send_buffer_size((payload_size + 128).max(8192))?;
    socket.set_recv_buffer_size(((payload_size + 128) * 2).max(8192))?;
    if v6 {
        socket.set_unicast_hops_v6(u32::from(ttl))?;
    } else {
        socket.set_ttl_v4(u32::from(ttl))?;
    }
    enable_error_queue(&socket, v6)?;
    let local: SocketAddr = if v6 { "[::]:0" } else { "0.0.0.0:0" }.parse().unwrap();
    socket.bind(&local.into())?;
    let socket: UdpSocket = socket.into();
    socket.set_nonblocking(true)?;
    let request = echo_request(v6, payload_size, socket.local_addr()?.port());
    // Keep IPv6 scope information while removing the irrelevant service port.
    addr.set_port(0);
    let start = Instant::now();
    cancelled(cancel)?;
    socket.send_to(&request, addr)?;
    let budget = Duration::from_millis(u64::from(timeout));
    let mut packet = vec![0; (request.len() + 1).max(2048)];
    loop {
        cancelled(cancel)?;
        let remaining = budget.saturating_sub(start.elapsed());
        if remaining.is_zero() {
            return Ok(json!({"ok":false,"responded":false,"status":"timeout",
                "status_code":null,"rtt_ms":null,"address":addr.ip().to_string()}));
        }
        if let Some(reply) = receive_error(&socket, addr.ip(), &request, &mut packet)? {
            return Ok(reply.sample(start));
        }
        match socket.recv_from(&mut packet) {
            Ok((n, from)) => {
                if let Some(reply) = echo_reply(&packet[..n], &request, addr.ip(), from.ip()) {
                    return Ok(reply.sample(start));
                }
                continue;
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(error) => {
                // An error may have arrived between the queue read and recv_from.
                if let Some(reply) = receive_error(&socket, addr.ip(), &request, &mut packet)? {
                    return Ok(reply.sample(start));
                }
                return Err(error.into());
            }
        }
        let mut descriptor = libc::pollfd {
            fd: socket.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // POLLERR also wakes poll when router errors enter the error queue.
        // Short waits bound cancellation latency independently of probe timeout.
        let wait_ms = remaining.as_millis().clamp(1, 50) as i32;
        if unsafe { libc::poll(&mut descriptor, 1, wait_ms) } < 0 {
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error.into());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, atomic::AtomicBool};

    #[test]
    fn echo_matches_kernel_identifier_sequence_source_and_full_payload() {
        for v6 in [false, true] {
            let target = if v6 { "::1" } else { "127.0.0.1" }.parse().unwrap();
            for size in [0, 1, 15, 16, 32, 1472, 65500] {
                let request = echo_request(v6, size, 0x1234);
                assert_eq!(request.len(), size + 8);
                assert_eq!(&request[4..6], &[0x12, 0x34]);
                let mut reply = request.clone();
                reply[0] = if v6 { 129 } else { 0 };
                reply[2..4].copy_from_slice(&[0xab, 0xcd]);
                assert_eq!(
                    echo_reply(&reply, &request, target, target).unwrap().status,
                    "reply"
                );
                assert!(echo_reply(&request, &request, target, target).is_none());
                assert!(echo_reply(&reply[..reply.len() - 1], &request, target, target).is_none());
                let other = if v6 { "::2" } else { "127.0.0.2" }.parse().unwrap();
                assert!(echo_reply(&reply, &request, target, other).is_none());
                for index in [1, 4, 6, reply.len() - 1] {
                    let mut unrelated = reply.clone();
                    unrelated[index] ^= 1;
                    assert!(echo_reply(&unrelated, &request, target, target).is_none());
                }
            }
        }
    }

    // Construct the native Linux ancillary payload, including its offender.
    fn error_bytes(v6: bool, kind: u8, code: u8) -> Vec<u8> {
        let length = size_of::<libc::sock_extended_err>()
            + if v6 {
                size_of::<libc::sockaddr_in6>()
            } else {
                size_of::<libc::sockaddr_in>()
            };
        let mut bytes = vec![0_u8; length];
        let mut error: libc::sock_extended_err = unsafe { zeroed() };
        error.ee_errno = libc::EHOSTUNREACH as u32;
        error.ee_origin = if v6 {
            libc::SO_EE_ORIGIN_ICMP6
        } else {
            libc::SO_EE_ORIGIN_ICMP
        };
        error.ee_type = kind;
        error.ee_code = code;
        unsafe { std::ptr::write_unaligned(bytes.as_mut_ptr().cast(), error) };
        let offender = unsafe { bytes.as_mut_ptr().add(size_of::<libc::sock_extended_err>()) };
        if v6 {
            let mut address: libc::sockaddr_in6 = unsafe { zeroed() };
            address.sin6_family = libc::AF_INET6 as _;
            address.sin6_addr.s6_addr = "2001:db8::abcd".parse::<Ipv6Addr>().unwrap().octets();
            unsafe { std::ptr::write_unaligned(offender.cast(), address) };
        } else {
            let mut address: libc::sockaddr_in = unsafe { zeroed() };
            address.sin_family = libc::AF_INET as _;
            address.sin_addr.s_addr = u32::from_ne_bytes([192, 0, 2, 254]);
            unsafe { std::ptr::write_unaligned(offender.cast(), address) };
        }
        bytes
    }

    #[test]
    fn router_errors_preserve_status_and_offender_in_both_families() {
        for (v6, kind, code, status) in [
            (false, 11, 0, "ttl_expired"),
            (true, 3, 0, "ttl_expired"),
            (false, 3, 1, "unreachable"),
            (true, 1, 3, "unreachable"),
            (false, 3, 4, "packet_too_big"),
            (true, 2, 0, "packet_too_big"),
            (false, 12, 0, "parameter_problem"),
            (true, 4, 0, "parameter_problem"),
        ] {
            let reply = extended_error(&error_bytes(v6, kind, code), v6)
                .unwrap()
                .unwrap();
            assert_eq!(reply.status, status);
            assert_eq!(reply.code, u16::from_be_bytes([kind, code]));
            assert_eq!(
                reply.address.to_string(),
                if v6 { "2001:db8::abcd" } else { "192.0.2.254" }
            );
            let sample = reply.sample(Instant::now());
            assert_eq!(sample["responded"], true);
            assert_eq!(sample["ok"], false);
            assert!(sample["rtt_ms"].as_f64().unwrap() >= 0.0);
        }
    }

    #[test]
    fn malformed_ancillary_data_and_unrelated_quotations_are_ignored() {
        for v6 in [false, true] {
            let bytes = error_bytes(v6, if v6 { 3 } else { 11 }, 0);
            for length in 0..bytes.len() {
                assert!(extended_error(&bytes[..length], v6).unwrap().is_none());
            }
            assert!(extended_error(&bytes, !v6).unwrap().is_none());
            let mut unknown = bytes.clone();
            unknown[4] = 255; // ee_origin
            assert!(extended_error(&unknown, v6).unwrap().is_none());
            let mut local = bytes.clone();
            local[4] = libc::SO_EE_ORIGIN_LOCAL;
            assert_eq!(
                extended_error(&local, v6).unwrap_err().raw_os_error(),
                Some(libc::EHOSTUNREACH)
            );
            let request = echo_request(v6, 32, 1234);
            for length in 0..8 {
                assert!(!quoted_request(&request[..length], &request));
            }
            assert!(quoted_request(&request[..8], &request));
            assert!(quoted_request(&request, &request));
            for index in [0, 1, 4, 6, 10] {
                let mut unrelated = request.clone();
                unrelated[index] ^= 1;
                assert!(!quoted_request(&unrelated, &request));
            }
        }
    }

    #[test]
    fn cancelled_probe_does_not_open_a_socket() {
        let cancel = Arc::new(AtomicBool::new(true));
        let error = icmp("127.0.0.1:0".parse().unwrap(), 1, 1000, 32, &cancel).unwrap_err();
        assert_eq!(error.code, "CANCELLED");
    }

    #[test]
    fn error_queue_decodes_real_ipv4_and_ipv6_unreachable_messages() {
        for host in ["127.0.0.1", "::1"] {
            let closed = UdpSocket::bind((host, 0)).unwrap();
            let destination = closed.local_addr().unwrap();
            drop(closed);
            let socket = Socket::new(
                Domain::for_address(destination),
                Type::DGRAM,
                Some(Protocol::UDP),
            )
            .unwrap();
            socket
                .bind(&SocketAddr::new(destination.ip(), 0).into())
                .unwrap();
            enable_error_queue(&socket, destination.is_ipv6()).unwrap();
            let socket: UdpSocket = socket.into();
            let request = echo_request(
                destination.is_ipv6(),
                32,
                socket.local_addr().unwrap().port(),
            );
            socket.send_to(&request, destination).unwrap();
            let start = Instant::now();
            let mut packet = vec![0; 2048];
            loop {
                if let Some(reply) =
                    receive_error(&socket, destination.ip(), &request, &mut packet).unwrap()
                {
                    assert_eq!(reply.status, "unreachable");
                    assert_eq!(reply.address, destination.ip());
                    assert_eq!(
                        reply.code,
                        if destination.is_ipv6() {
                            0x0104
                        } else {
                            0x0303
                        }
                    );
                    break;
                }
                assert!(
                    start.elapsed() < Duration::from_secs(2),
                    "No ICMP error for {destination}"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }

    #[test]
    #[ignore = "requires Linux ping socket permission and IPv4/IPv6 loopback"]
    fn linux_loopback_ping_and_trace() {
        let cancel = Arc::new(AtomicBool::new(false));
        for target in ["127.0.0.1", "::1"] {
            for size in [0, 1, 32, 1472, 65500] {
                let request = Request {
                    target: target.into(),
                    count: 1,
                    payload_size_bytes: size,
                    ..Request::default()
                };
                let result = run(&request, &cancel, &|_| {}).unwrap();
                assert_eq!(result["received"], 1, "{target}, size {size}: {result}");
                assert_eq!(result["samples"][0]["status"], "reply");
            }
            let request = Request {
                tool: "trace".into(),
                target: target.into(),
                max_hops: 3,
                ..Request::default()
            };
            let result = run(&request, &cancel, &|_| {}).unwrap();
            assert_eq!(result["reached"], true, "{result}");
            assert_eq!(result["sent"], 3);
            assert_eq!(result["samples"][0]["hop"], 1);
        }
    }
}
