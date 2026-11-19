use super::*;
#[cfg(windows)]
mod windows;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Interface {
    pub name: String,
    pub index: u32,
    pub address: String,
    pub prefix: u32,
}
pub(super) fn contains(i: &Interface, ip: IpAddr) -> bool {
    let Some(local) = target::literal_socket(&i.address, 0) else {
        return false;
    };
    ipnet::IpNet::new(local.ip(), i.prefix as u8).is_ok_and(|n| n.contains(&ip))
}
pub fn list(c: &Cancel) -> Result<Vec<Interface>, ToolError> {
    cancelled(c)?;
    #[cfg(unix)]
    unsafe {
        use std::{ffi::CStr, ptr};
        let mut head = ptr::null_mut();
        if libc::getifaddrs(&mut head) != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        struct Guard(*mut libc::ifaddrs);
        impl Drop for Guard {
            fn drop(&mut self) {
                unsafe {
                    libc::freeifaddrs(self.0);
                }
            }
        }
        let _guard = Guard(head);
        let mut rows = Vec::new();
        let mut item = head;
        while !item.is_null() {
            let a = &*item;
            item = a.ifa_next;
            if a.ifa_addr.is_null()
                || a.ifa_netmask.is_null()
                || a.ifa_flags & libc::IFF_UP as u32 == 0
            {
                continue;
            }
            let name = CStr::from_ptr(a.ifa_name).to_string_lossy().into_owned();
            let index = libc::if_nametoindex(a.ifa_name);
            let (ip, prefix) = match (*a.ifa_addr).sa_family as i32 {
                libc::AF_INET => {
                    let ip = &*a.ifa_addr.cast::<libc::sockaddr_in>();
                    let mask = &*a.ifa_netmask.cast::<libc::sockaddr_in>();
                    (
                        IpAddr::V4(std::net::Ipv4Addr::from(ip.sin_addr.s_addr.to_ne_bytes())),
                        mask.sin_addr.s_addr.count_ones(),
                    )
                }
                libc::AF_INET6 => {
                    let ip = &*a.ifa_addr.cast::<libc::sockaddr_in6>();
                    let mask = &*a.ifa_netmask.cast::<libc::sockaddr_in6>();
                    (
                        IpAddr::V6(std::net::Ipv6Addr::from(ip.sin6_addr.s6_addr)),
                        mask.sin6_addr.s6_addr.iter().map(|b| b.count_ones()).sum(),
                    )
                }
                _ => continue,
            };
            let address = if matches!(ip,IpAddr::V6(v) if v.is_unicast_link_local()) {
                format!("{ip}%{index}")
            } else {
                ip.to_string()
            };
            rows.push(Interface {
                name,
                index,
                address,
                prefix,
            });
        }
        Ok(rows)
    }
    #[cfg(windows)]
    {
        windows::list(c)
    }
    #[cfg(not(any(unix, windows)))]
    Err(ToolError::new(
        "UNSUPPORTED_CAPABILITY",
        "Interface enumeration unavailable",
    ))
}
