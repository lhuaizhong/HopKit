//! Read current SystemConfiguration dynamic-store values, never lease files.
use super::*;
use std::{
    ffi::{CString, c_void},
    ptr,
};
type CF = *const c_void;
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(v: CF);
    fn CFGetTypeID(v: CF) -> usize;
    fn CFStringCreateWithCString(a: CF, s: *const i8, encoding: u32) -> CF;
    fn CFStringGetCString(v: CF, b: *mut i8, size: isize, encoding: u32) -> bool;
    fn CFStringGetTypeID() -> usize;
    fn CFArrayGetTypeID() -> usize;
    fn CFDictionaryGetTypeID() -> usize;
    fn CFArrayGetCount(v: CF) -> isize;
    fn CFArrayGetValueAtIndex(v: CF, i: isize) -> CF;
    fn CFDictionaryGetValue(d: CF, key: CF) -> CF;
    fn CFDataGetTypeID() -> usize;
    fn CFDataGetLength(v: CF) -> isize;
    fn CFDataGetBytePtr(v: CF) -> *const u8;
    fn CFDateGetTypeID() -> usize;
    fn CFDateGetAbsoluteTime(v: CF) -> f64;
}
#[link(name = "SystemConfiguration", kind = "framework")]
unsafe extern "C" {
    fn SCDynamicStoreCreate(a: CF, name: CF, callback: CF, ctx: CF) -> CF;
    fn SCDynamicStoreCopyKeyList(s: CF, pattern: CF) -> CF;
    fn SCDynamicStoreCopyValue(s: CF, key: CF) -> CF;
}
struct Owned(CF);
impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0) }
        }
    }
}
fn string(s: &str) -> Owned {
    let c = CString::new(s).unwrap();
    Owned(unsafe { CFStringCreateWithCString(ptr::null(), c.as_ptr(), 0x08000100) })
}
fn text(v: CF) -> Option<String> {
    unsafe {
        if v.is_null() || CFGetTypeID(v) != CFStringGetTypeID() {
            return None;
        }
        let mut b = vec![0i8; 8192];
        if !CFStringGetCString(v, b.as_mut_ptr(), b.len() as isize, 0x08000100) {
            return None;
        }
        Some(
            std::ffi::CStr::from_ptr(b.as_ptr())
                .to_string_lossy()
                .into_owned(),
        )
    }
}
fn value(store: CF, key: &str) -> Owned {
    Owned(unsafe { SCDynamicStoreCopyValue(store, string(key).0) })
}
fn get(d: CF, key: &str) -> CF {
    unsafe {
        if d.is_null() || CFGetTypeID(d) != CFDictionaryGetTypeID() {
            return ptr::null();
        }
        CFDictionaryGetValue(d, string(key).0)
    }
}
fn array(v: CF) -> Vec<CF> {
    unsafe {
        if v.is_null() || CFGetTypeID(v) != CFArrayGetTypeID() {
            return vec![];
        }
        let n = CFArrayGetCount(v).clamp(0, 128);
        (0..n).map(|i| CFArrayGetValueAtIndex(v, i)).collect()
    }
}
fn strings(d: CF, key: &str) -> Vec<String> {
    array(get(d, key)).into_iter().filter_map(text).collect()
}
fn date(v: CF) -> Option<String> {
    unsafe {
        if v.is_null() || CFGetTypeID(v) != CFDateGetTypeID() {
            return None;
        }
        chrono::DateTime::from_timestamp((CFDateGetAbsoluteTime(v) + 978307200.0) as i64, 0)
            .map(|d| d.to_rfc3339())
    }
}
pub(super) fn inspect(c: &Cancel) -> Result<Vec<Interface>, ToolError> {
    unsafe {
        let store = Owned(SCDynamicStoreCreate(
            ptr::null(),
            string("HopKit DHCP inspection").0,
            ptr::null(),
            ptr::null(),
        ));
        if store.0.is_null() {
            return Err(ToolError::new(
                "SYSTEM_QUERY_FAILED",
                "SystemConfiguration store unavailable",
            ));
        }
        let keys = Owned(SCDynamicStoreCopyKeyList(
            store.0,
            string("Setup:/Network/Service/[^/]+/IPv4").0,
        ));
        if keys.0.is_null() {
            return Err(ToolError::new(
                "SYSTEM_QUERY_FAILED",
                "SystemConfiguration service enumeration failed",
            ));
        }
        if CFArrayGetCount(keys.0) > 128 {
            return Err(ToolError::new(
                "RESOURCE_EXHAUSTED",
                "More than 128 IPv4 services",
            ));
        }
        let mut rows = Vec::new();
        for key in array(keys.0).into_iter().filter_map(text) {
            cancelled(c)?;
            let base = key.trim_end_matches("/IPv4");
            let setup = value(store.0, &key);
            let dev = value(store.0, &format!("{base}/Interface"));
            let Some(name) = text(get(dev.0, "DeviceName")) else {
                continue;
            };
            let statebase = base.replacen("Setup:", "State:", 1);
            let state = value(store.0, &format!("{statebase}/IPv4"));
            let dns = value(store.0, &format!("{statebase}/DNS"));
            let lease = value(store.0, &format!("{statebase}/DHCP"));
            let method = text(get(setup.0, "ConfigMethod"));
            let dhcp = match method.as_deref() {
                Some("DHCP") => Some(true),
                Some("Manual") => Some(false),
                _ => None,
            };
            let cs = CString::new(name.as_str()).unwrap();
            let index = libc::if_nametoindex(cs.as_ptr());
            let v = get(lease.0, "Option_54");
            let server =
                if !v.is_null() && CFGetTypeID(v) == CFDataGetTypeID() && CFDataGetLength(v) == 4 {
                    let bytes = std::slice::from_raw_parts(CFDataGetBytePtr(v), 4);
                    Some(Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]).to_string())
                } else {
                    None
                };
            let start = if dhcp == Some(true) {
                date(get(lease.0, "LeaseStartTime"))
            } else {
                None
            };
            let expires = if dhcp == Some(true) {
                date(get(lease.0, "LeaseExpirationTime"))
            } else {
                None
            };
            let mac = crate::process::text("/sbin/ifconfig", &[&name], c)
                .ok()
                .and_then(|s| {
                    s.lines().find_map(|l| {
                        l.trim()
                            .strip_prefix("ether ")
                            .and_then(|s| s.split_whitespace().next())
                            .map(String::from)
                    })
                });
            rows.push(Interface{id:base.trim_start_matches("Setup:/Network/Service/").into(),name,index,mac,dhcp_enabled:dhcp,server:if dhcp==Some(true){server}else{None},lease_start:start,lease_expires:expires,addresses:strings(state.0,"Addresses").into_iter().zip(strings(state.0,"SubnetMasks")).map(|(address,mask)|Address{address,mask}).collect(),gateways:text(get(state.0,"Router")).into_iter().collect(),dns:strings(dns.0,"ServerAddresses"),search_domains:strings(dns.0,"SearchDomains"),config_source:ADAPTER.into(),field_sources:std::collections::BTreeMap::from([("method".into(),key),("effective_ipv4_dns_lease".into(),statebase)]),limitations:vec!["SystemConfiguration fields may be unavailable; unknown remains null. Native macOS acceptance pending. / 系统未公开字段为 unknown；macOS 原生待验收。".into()]});
        }
        Ok(rows)
    }
}
