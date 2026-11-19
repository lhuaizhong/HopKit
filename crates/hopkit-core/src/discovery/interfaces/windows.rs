//! Discovery needs local addresses, prefixes and aliases, not the full CIM network overview.
use super::*;
use std::net::{Ipv4Addr, Ipv6Addr};
use windows_sys::Win32::{
    NetworkManagement::{IpHelper::*, Ndis::IfOperStatusUp},
    Networking::WinSock::*,
};

pub(super) fn list(c: &Cancel) -> Result<Vec<Interface>, ToolError> {
    cancelled(c)?;
    unsafe {
        let mut table = std::ptr::null_mut();
        let code = GetUnicastIpAddressTable(AF_UNSPEC, &mut table);
        if code == 1168 {
            return Ok(vec![]);
        }
        if code != 0 {
            return Err(ToolError::new(
                "SYSTEM_QUERY_FAILED",
                format!("GetUnicastIpAddressTable: {code}"),
            ));
        }
        struct Guard(*mut MIB_UNICASTIPADDRESS_TABLE);
        impl Drop for Guard {
            fn drop(&mut self) {
                unsafe { FreeMibTable(self.0.cast()) }
            }
        }
        let _guard = Guard(table);
        if table.is_null() {
            return Err(ToolError::new("SYSTEM_QUERY_FAILED", "Null address table"));
        }
        if (*table).NumEntries > 16384 {
            return Err(ToolError::new(
                "RESOURCE_EXHAUSTED",
                "Address table exceeds 16384 entries",
            ));
        }
        // Table is aligned independently of NumEntries; do not assume a 4-byte offset.
        let addresses = std::slice::from_raw_parts(
            std::ptr::addr_of!((*table).Table).cast::<MIB_UNICASTIPADDRESS_ROW>(),
            (*table).NumEntries as usize,
        );
        let mut links = BTreeMap::new();
        let mut rows = Vec::new();
        for address in addresses {
            cancelled(c)?;
            let link = match links.entry(address.InterfaceIndex) {
                std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
                std::collections::btree_map::Entry::Vacant(entry) => {
                    let mut link = MIB_IF_ROW2::default();
                    link.InterfaceIndex = address.InterfaceIndex;
                    let code = GetIfEntry2(&mut link);
                    if code != 0 {
                        return Err(ToolError::new(
                            "SYSTEM_QUERY_FAILED",
                            format!("GetIfEntry2({}): {code}", address.InterfaceIndex),
                        ));
                    }
                    entry.insert(link)
                }
            };
            if let Some(row) = interface(address, link) {
                rows.push(row);
            }
        }
        Ok(rows)
    }
}

fn interface(address: &MIB_UNICASTIPADDRESS_ROW, link: &MIB_IF_ROW2) -> Option<Interface> {
    if address.InterfaceIndex == 0
        || link.OperStatus != IfOperStatusUp
        || address.DadState != IpDadStatePreferred && address.DadState != IpDadStateDeprecated
    {
        return None;
    }
    let ip = unsafe {
        match address.Address.si_family {
            AF_INET => IpAddr::V4(Ipv4Addr::from(
                address.Address.Ipv4.sin_addr.S_un.S_addr.to_ne_bytes(),
            )),
            AF_INET6 => IpAddr::V6(Ipv6Addr::from(address.Address.Ipv6.sin6_addr.u.Byte)),
            _ => return None,
        }
    };
    if ip.is_unspecified()
        || ip.is_multicast()
        || address.OnLinkPrefixLength > if ip.is_ipv4() { 32 } else { 128 }
    {
        return None;
    }
    let end = link
        .Alias
        .iter()
        .position(|&v| v == 0)
        .unwrap_or(link.Alias.len());
    Some(Interface {
        name: String::from_utf16_lossy(&link.Alias[..end]),
        index: address.InterfaceIndex,
        address: if matches!(ip, IpAddr::V6(v) if v.is_unicast_link_local()) {
            format!("{ip}%{}", address.InterfaceIndex)
        } else {
            ip.to_string()
        },
        prefix: address.OnLinkPrefixLength as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_addresses_preserve_alias_prefix_and_ipv6_scope() {
        let mut link = MIB_IF_ROW2::default();
        link.OperStatus = IfOperStatusUp;
        for (slot, ch) in link.Alias.iter_mut().zip("以太网".encode_utf16()) {
            *slot = ch;
        }
        let mut address = MIB_UNICASTIPADDRESS_ROW::default();
        address.InterfaceIndex = 8;
        address.DadState = IpDadStatePreferred;
        address.OnLinkPrefixLength = 24;
        address.Address.Ipv4.sin_family = AF_INET;
        address.Address.Ipv4.sin_addr.S_un.S_addr = u32::from_ne_bytes([10, 232, 129, 25]);
        let row = interface(&address, &link).unwrap();
        assert_eq!(
            (
                row.name.as_str(),
                row.index,
                row.address.as_str(),
                row.prefix
            ),
            ("以太网", 8, "10.232.129.25", 24)
        );
        address.Address.Ipv6.sin6_family = AF_INET6;
        address.Address.Ipv6.sin6_addr.u.Byte = "fe80::1234".parse::<Ipv6Addr>().unwrap().octets();
        address.OnLinkPrefixLength = 64;
        assert_eq!(interface(&address, &link).unwrap().address, "fe80::1234%8");
        address.DadState = IpDadStateDeprecated;
        assert!(interface(&address, &link).is_some());
        for state in [IpDadStateInvalid, IpDadStateTentative, IpDadStateDuplicate] {
            address.DadState = state;
            assert!(interface(&address, &link).is_none());
        }
        address.DadState = IpDadStatePreferred;
        link.OperStatus = 2;
        assert!(interface(&address, &link).is_none());
    }

    #[test]
    fn native_query_needs_no_cim_and_honors_cancellation() {
        let c = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let rows = list(&c).unwrap();
        assert!(rows.iter().any(|r| r.address == "127.0.0.1"));
        c.store(true, std::sync::atomic::Ordering::Relaxed);
        assert_eq!(list(&c).unwrap_err().code, "CANCELLED");
    }
}
