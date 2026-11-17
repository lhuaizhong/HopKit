use super::*;
#[cfg(windows)]
pub(super) const ADAPTER: &str = "windows-iphelper";
#[cfg(target_os = "macos")]
pub(super) const ADAPTER: &str = "macos-systemconfiguration";
#[cfg(not(any(windows, target_os = "macos")))]
pub(super) const ADAPTER: &str = "unavailable";

#[cfg(windows)]
pub(super) fn inspect(c: &Cancel) -> Result<Vec<Interface>, ToolError> {
    use std::{ffi::CStr, ptr};
    use windows_sys::Win32::{NetworkManagement::IpHelper::*, Networking::WinSock::*};
    // u64 allocation provides native alignment. Reallocate only before reading any pointers.
    fn buffer(mut call: impl FnMut(*mut u8, &mut u32) -> u32) -> Result<Vec<u64>, ToolError> {
        let mut size = 16384u32;
        for _ in 0..3 {
            if size > 2 * 1024 * 1024 {
                return Err(ToolError::new(
                    "RESOURCE_EXHAUSTED",
                    "Adapter query too large",
                ));
            }
            let mut b = vec![0u64; (size as usize).div_ceil(8)];
            let code = call(b.as_mut_ptr().cast(), &mut size);
            if code == 0 {
                return Ok(b);
            }
            if code == 232 {
                return Ok(vec![]);
            }
            if code != 111 {
                return Err(ToolError::new(
                    "SYSTEM_QUERY_FAILED",
                    format!("IP Helper status {code}"),
                ));
            }
        }
        Err(ToolError::new(
            "SYSTEM_QUERY_FAILED",
            "Adapter list changed repeatedly",
        ))
    }
    unsafe fn wide(p: *const u16) -> String {
        if p.is_null() {
            return String::new();
        }
        let mut n = 0;
        unsafe {
            while n < 32768 && *p.add(n) != 0 {
                n += 1
            }
            String::from_utf16_lossy(std::slice::from_raw_parts(p, n))
        }
    }
    unsafe fn addr(a: &SOCKET_ADDRESS) -> Option<String> {
        unsafe {
            if a.lpSockaddr.is_null()
                || a.iSockaddrLength < (std::mem::size_of::<SOCKADDR_IN>() as i32)
                || (*a.lpSockaddr).sa_family != AF_INET
            {
                return None;
            }
            let ip = Ipv4Addr::from(
                (*(a.lpSockaddr.cast::<SOCKADDR_IN>()))
                    .sin_addr
                    .S_un
                    .S_addr
                    .to_ne_bytes(),
            );
            (!ip.is_unspecified() && !ip.is_broadcast()).then(|| ip.to_string())
        }
    }
    unsafe fn strings(mut p: *const IP_ADDR_STRING, mask: bool) -> Vec<String> {
        unsafe {
            let mut out = Vec::new();
            while !p.is_null() && out.len() < 128 {
                let s = CStr::from_ptr(if mask {
                    (*p).IpMask.String.as_ptr()
                } else {
                    (*p).IpAddress.String.as_ptr()
                })
                .to_string_lossy()
                .into_owned();
                out.push(s);
                p = (*p).Next
            }
            out
        }
    }
    unsafe {
        cancelled(c)?;
        let info = buffer(|b, n| GetAdaptersInfo(b.cast(), n))?;
        let addresses =
            buffer(|b, n| GetAdaptersAddresses(AF_INET as u32, 0, ptr::null(), b.cast(), n))?;
        let mut rows = Vec::new();
        let mut item = if info.is_empty() {
            ptr::null()
        } else {
            info.as_ptr().cast::<IP_ADAPTER_INFO>()
        };
        while !item.is_null() {
            cancelled(c)?;
            if rows.len() >= 128 {
                return Err(ToolError::new(
                    "RESOURCE_EXHAUSTED",
                    "More than 128 IPv4 adapters",
                ));
            }
            let a = &*item;
            let id = CStr::from_ptr(a.AdapterName.as_ptr())
                .to_string_lossy()
                .into_owned();
            let index = a.Index;
            let dhcp = a.DhcpEnabled != 0;
            let date = |v: i64| {
                if dhcp && v > 0 {
                    chrono::DateTime::from_timestamp(v, 0).map(|d| d.to_rfc3339())
                } else {
                    None
                }
            };
            let mut row=Interface{id:id.clone(),name:CStr::from_ptr(a.Description.as_ptr()).to_string_lossy().into_owned(),index,mac:(a.AddressLength==6).then(||a.Address[..6].iter().map(|b|format!("{b:02X}")).collect::<Vec<_>>().join(":")),dhcp_enabled:Some(dhcp),server:if dhcp{strings(&a.DhcpServer,false).into_iter().find(|s|s.parse::<Ipv4Addr>().is_ok_and(|ip|!ip.is_unspecified()&&!ip.is_broadcast()))}else{None},lease_start:date(a.LeaseObtained),lease_expires:date(a.LeaseExpires),addresses:strings(&a.IpAddressList,false).into_iter().zip(strings(&a.IpAddressList,true)).filter(|(s,_)|s!="0.0.0.0").map(|(address,mask)|Address{address,mask}).collect(),gateways:strings(&a.GatewayList,false).into_iter().filter(|s|s!="0.0.0.0").collect(),dns:vec![],search_domains:vec![],config_source:ADAPTER.into(),field_sources:std::collections::BTreeMap::from([("configuration_and_lease".into(),"GetAdaptersInfo (current OS configuration)".into()),("dns_and_suffix".into(),"GetAdaptersAddresses (current effective adapter configuration, not necessarily DHCP-provided)".into())]),limitations:vec!["Sequential OS snapshots; effective DNS/gateway may include manual overrides. Unknown lease fields remain null. / 顺序系统快照；有效 DNS/网关可能含手动覆盖。".into()]};
            let mut p = if addresses.is_empty() {
                ptr::null()
            } else {
                addresses.as_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>()
            };
            while !p.is_null() {
                let x = &*p;
                if x.Anonymous1.Anonymous.IfIndex == index {
                    let name = wide(x.FriendlyName);
                    if !name.is_empty() {
                        row.name = name
                    }
                    let suffix = wide(x.DnsSuffix);
                    if !suffix.is_empty() {
                        row.search_domains.push(suffix)
                    }
                    let mut dns = x.FirstDnsServerAddress;
                    while !dns.is_null() && row.dns.len() < 128 {
                        if let Some(ip) = addr(&(*dns).Address) {
                            row.dns.push(ip)
                        }
                        dns = (*dns).Next
                    }
                    break;
                }
                p = x.Next;
            }
            rows.push(row);
            item = a.Next;
        }
        Ok(rows)
    }
}

#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod macos;
#[cfg(target_os = "macos")]
pub(super) fn inspect(c: &Cancel) -> Result<Vec<Interface>, ToolError> {
    macos::inspect(c)
}
#[cfg(not(any(windows, target_os = "macos")))]
pub(super) fn inspect(_c: &Cancel) -> Result<Vec<Interface>, ToolError> {
    Err(ToolError::new(
        "UNSUPPORTED_CAPABILITY",
        "DHCP configuration inspection supports Windows/macOS; no stale lease-file fallback",
    ))
}

pub(super) fn socket(iface: &Interface) -> Result<UdpSocket, ToolError> {
    use socket2::{Domain, Protocol, Socket, Type};
    if iface.index == 0 {
        return Err(ToolError::new(
            "UNSUPPORTED_CAPABILITY",
            "Interface index unavailable",
        ));
    }
    let s = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    // No SO_REUSEADDR: never compete with the system DHCP client for port 68.
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawSocket;
        use windows_sys::Win32::Networking::WinSock::*;
        let set = |level, option, value: u32| -> Result<(), ToolError> {
            let v = value.to_ne_bytes();
            let rc =
                unsafe { setsockopt(s.as_raw_socket() as usize, level, option, v.as_ptr(), 4) };
            if rc != 0 {
                return Err(std::io::Error::from_raw_os_error(unsafe { WSAGetLastError() }).into());
            }
            Ok(())
        };
        set(SOL_SOCKET, SO_EXCLUSIVEADDRUSE, 1)?;
        set(IPPROTO_IP, IP_UNICAST_IF, iface.index.to_be())?;
        set(IPPROTO_IP, IP_IFLIST, 1)?;
        set(IPPROTO_IP, IP_ADD_IFLIST, iface.index)?;
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::fd::AsRawFd;
        let index = iface.index;
        if unsafe {
            libc::setsockopt(
                s.as_raw_fd(),
                libc::IPPROTO_IP,
                libc::IP_BOUND_IF,
                (&index as *const u32).cast(),
                4,
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    return Err(ToolError::new(
        "UNSUPPORTED_CAPABILITY",
        "DHCP interface-bound socket unavailable",
    ));
    #[cfg(any(windows, target_os = "macos"))]
    {
        s.set_broadcast(true)?;
        s.bind(&"0.0.0.0:68".parse::<SocketAddr>().unwrap().into()).map_err(|e|{let mut err=ToolError::from(e);err.message=format!("Cannot exclusively bind standard DHCP client UDP 68; do not stop the system DHCP client. / 无法独占标准端口 68，不要停止系统 DHCP 客户端。 {}",err.message);err})?;
        s.set_read_timeout(Some(Duration::from_millis(50)))?;
        Ok(s.into())
    }
}
