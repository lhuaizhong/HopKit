//! Local-only platform queries. Opening the overview sends no network probes.
use crate::{process, runtime::*};
use serde_json::{Value, json};

pub fn overview(c: &Cancel) -> ToolResult {
    #[cfg(target_os = "macos")]
    {
        let mut errors = Vec::new();
        let mut read = |p: &str, a: &[&str]| -> Result<String, ToolError> {
            match process::text(p, a, c) {
                Ok(s) => Ok(s),
                Err(e) if e.code == "CANCELLED" => Err(e),
                Err(e) => {
                    errors.push(json!({"adapter":p,"code":e.code}));
                    Ok(String::new())
                }
            }
        };
        let interfaces = read("/sbin/ifconfig", &["-a"])?;
        let routes = read("/usr/sbin/netstat", &["-rn"])?;
        let dns = read("/usr/sbin/scutil", &["--dns"])?;
        let hardware = read("/usr/sbin/networksetup", &["-listallhardwareports"])?;
        let mut result = parse_macos(&interfaces, &routes, &dns, &hardware);
        result["errors"] = json!(errors);
        Ok(result)
    }
    #[cfg(windows)]
    {
        // CIM/NetTCPIP cmdlets return structured data independent of display language.
        let script = r#"$ErrorActionPreference='Stop'; [Console]::OutputEncoding=[Text.UTF8Encoding]::new(); $ad=@(Get-NetAdapter -IncludeHidden); $dns=@(Get-DnsClientServerAddress); $rt=@(Get-NetRoute | Where-Object {$_.DestinationPrefix -in @('0.0.0.0/0','::/0')} | ForEach-Object { @{destination=$_.DestinationPrefix;gateway=$_.NextHop;interface_index=$_.InterfaceIndex;metric=$_.RouteMetric} }); $ifs=@(Get-NetIPInterface | Group-Object InterfaceIndex | ForEach-Object { $idx=[int]$_.Name; $a=$ad | Where-Object ifIndex -eq $idx | Select-Object -First 1; @{name=$_.Group[0].InterfaceAlias;index=$idx;enabled=($a.AdminStatus -eq 'Up');link_status=[string]$a.Status;kind=$(if($a.HardwareInterface){'physical'}else{'virtual_or_unknown'});addresses=@(Get-NetIPAddress -InterfaceIndex $idx | ForEach-Object { @{address=$_.IPAddress;prefix=$_.PrefixLength;scope=$(if($_.IPAddress -like 'fe80:*'){[string]$idx}else{$null})} });dns=@($dns | Where-Object InterfaceIndex -eq $idx | ForEach-Object ServerAddresses);gateways=@($rt | Where-Object interface_index -eq $idx | ForEach-Object gateway)} }); @{platform='windows';interfaces=$ifs;default_routes=$rt;dns=@($dns | ForEach-Object ServerAddresses);captured_at=[DateTime]::UtcNow.ToString('o');errors=@()} | ConvertTo-Json -Depth 8 -Compress"#;
        let text = process::text(
            "powershell.exe",
            &[
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                script,
            ],
            c,
        )?;
        serde_json::from_str(text.trim_start_matches('\u{feff}'))
            .map_err(|_| ToolError::new("SYSTEM_QUERY_FAILED", "Invalid platform network response"))
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = c;
        Err(ToolError::new(
            "UNSUPPORTED_CAPABILITY",
            "Network overview supports Windows/macOS / 网络概览支持 Windows/macOS",
        ))
    }
}

#[cfg(any(target_os = "macos", test))]
fn parse_macos(interfaces: &str, routes: &str, dns: &str, hardware: &str) -> Value {
    let mut names = std::collections::BTreeMap::new();
    let mut label = "";
    for line in hardware.lines() {
        if let Some(s) = line.strip_prefix("Hardware Port: ") {
            label = s;
        }
        if let Some(s) = line.strip_prefix("Device: ") {
            names.insert(s, label);
        }
    }
    let mut list: Vec<Value> = Vec::new();
    for line in interfaces.lines() {
        if !line.starts_with(char::is_whitespace) {
            if let Some((name, rest)) = line.split_once(": flags=") {
                let kind = if name.starts_with("utun") {
                    "vpn"
                } else if name == "lo0" {
                    "loopback"
                } else if name.starts_with("bridge")
                    || name.starts_with("gif")
                    || name.starts_with("stf")
                {
                    "virtual"
                } else if names.contains_key(name) {
                    "physical"
                } else {
                    "unknown"
                };
                list.push(json!({"name":name,"label":names.get(name),"enabled":rest.contains("<UP,")||rest.contains(",UP,"),"link_status":"unknown","kind":kind,"addresses":[],"dns":[],"gateways":[]}));
            }
            continue;
        }
        let Some(iface) = list.last_mut() else {
            continue;
        };
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.first() == Some(&"status:") {
            iface["link_status"] = json!(fields.get(1));
        }
        if fields.first() == Some(&"inet") || fields.first() == Some(&"inet6") {
            let address = fields.get(1).copied().unwrap_or("");
            if let Some(index) = fields
                .iter()
                .position(|s| *s == "scopeid")
                .and_then(|n| fields.get(n + 1))
                .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok())
            {
                iface["index"] = json!(index);
            }

            let prefix = if fields[0] == "inet" {
                fields
                    .iter()
                    .position(|f| *f == "netmask")
                    .and_then(|i| fields.get(i + 1))
                    .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok())
                    .map(|m| m.count_ones())
            } else {
                fields
                    .iter()
                    .position(|f| *f == "prefixlen")
                    .and_then(|i| fields.get(i + 1))
                    .and_then(|s| s.parse().ok())
            };
            iface["addresses"].as_array_mut().unwrap().push(json!({"address":address,"prefix":prefix,"scope":address.split_once('%').map(|(_,s)|s)}));
        }
    }
    let mut defaults = Vec::new();
    for line in routes.lines() {
        let f: Vec<_> = line.split_whitespace().collect();
        if f.first() == Some(&"default") && f.len() >= 4 {
            let interface = f[f.len() - 1];
            defaults.push(
                json!({"destination":"default","gateway":f[1],"flags":f[2],"interface":interface}),
            );
            if let Some(i) = list.iter_mut().find(|i| i["name"] == interface) {
                i["gateways"].as_array_mut().unwrap().push(json!(f[1]));
            }
        }
    }
    let mut all_dns = Vec::new();
    let mut servers = Vec::new();
    let mut interface = None;
    let apply = |servers: &mut Vec<Value>,
                 interface: &mut Option<String>,
                 list: &mut Vec<Value>,
                 all: &mut Vec<Value>| {
        for s in servers.drain(..) {
            if !all.contains(&s) {
                all.push(s.clone());
            }
            if let Some(name) = interface.as_ref()
                && let Some(i) = list.iter_mut().find(|i| i["name"] == *name)
                && !i["dns"].as_array().unwrap().contains(&s)
            {
                i["dns"].as_array_mut().unwrap().push(s);
            }
        }
        *interface = None;
    };
    for line in dns.lines() {
        let l = line.trim();
        if l.starts_with("resolver #") {
            apply(&mut servers, &mut interface, &mut list, &mut all_dns);
        }
        if l.starts_with("nameserver[")
            && let Some((_, ip)) = l.split_once(" : ")
        {
            servers.push(json!(ip));
        }
        if l.starts_with("if_index") {
            interface = l
                .split_once('(')
                .and_then(|(_, s)| s.strip_suffix(')'))
                .map(String::from);
        }
    }
    apply(&mut servers, &mut interface, &mut list, &mut all_dns);
    json!({"platform":"macos","captured_at":chrono::Utc::now().to_rfc3339(),"interfaces":list,"default_routes":defaults,"dns":all_dns,"dns_scope":"System resolver list; interface DNS shown only when scoped / 系统解析器列表，仅可靠归属时列入接口","errors":[]})
}

pub fn route(address: &str, c: &Cancel) -> ToolResult {
    #[cfg(target_os = "macos")]
    {
        let raw = process::text("/sbin/route", &["-n", "get", address], c)?;
        let mut value = json!({"destination":address,"mode":"system_route_query"});
        for l in raw.lines() {
            if let Some((k, v)) = l.trim().split_once(':')
                && matches!(
                    k,
                    "gateway" | "interface" | "destination" | "mask" | "flags"
                )
            {
                value[k] = json!(v.trim());
            }
        }
        Ok(value)
    }
    #[cfg(windows)]
    {
        // Only a resolved numeric address reaches this boundary.
        if crate::target::literal_socket(address, 0).is_none() {
            return Err(ToolError::new(
                "INVALID_ARGUMENT",
                "Numeric route target required",
            ));
        }
        let script = format!(
            "$ErrorActionPreference='Stop'; [Console]::OutputEncoding=[Text.UTF8Encoding]::new(); Find-NetRoute -RemoteIPAddress '{address}' | Select-Object IPAddress,InterfaceAlias,InterfaceIndex,NextHop,DestinationPrefix | ConvertTo-Json -Compress"
        );
        let s = process::text(
            "powershell.exe",
            &["-NoProfile", "-NonInteractive", "-Command", &script],
            c,
        )?;
        Ok(
            json!({"destination":address,"mode":"system_route_query","route":serde_json::from_str::<Value>(s.trim_start_matches('\u{feff}')).map_err(|_|ToolError::new("SYSTEM_QUERY_FAILED","Invalid route response"))?}),
        )
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (address, c);
        Err(ToolError::new(
            "UNSUPPORTED_CAPABILITY",
            "Route query supports Windows/macOS",
        ))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn macos_multi_interface_scope_and_no_route() {
        let ifs = "lo0: flags=8049<UP,LOOPBACK>\n inet 127.0.0.1 netmask 0xff000000\nen0: flags=8863<UP,RUNNING>\n inet 192.0.2.2 netmask 0xffffff00\n inet6 fe80::1%en0 prefixlen 64 scopeid 0x4\n status: active\nutun0: flags=8051<UP,POINTOPOINT>\n inet6 fe80::2%utun0 prefixlen 64\n";
        let v = parse_macos(
            ifs,
            "default 192.0.2.1 UGSc en0\ndefault fe80::%utun0 UGcIg utun0",
            "resolver #1\n nameserver[0] : 192.0.2.53\n nameserver[1] : 192.0.2.54\n if_index : 4 (en0)",
            "Hardware Port: Wi-Fi\nDevice: en0\n",
        );
        assert_eq!(v["interfaces"].as_array().unwrap().len(), 3);
        assert_eq!(v["dns"].as_array().unwrap().len(), 2);
        assert_eq!(v["interfaces"][1]["addresses"][1]["scope"], "en0");
        assert_eq!(v["interfaces"][2]["kind"], "vpn");
        assert_eq!(v["interfaces"][1]["addresses"][0]["prefix"], 24);
        assert!(
            parse_macos(ifs, "", "", "")["default_routes"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
}
