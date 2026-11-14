//! Offline subnet arithmetic. Counts are decimal strings, including 2^128.
use crate::{ToolError, ToolResult};
use ipnet::IpNet;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
pub const MAX_OUTPUT: usize = 1024;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub operation: String,
    pub input: String,
    pub other: String,
    pub prefix: u8,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            operation: "inspect".into(),
            input: "192.168.10.25/24".into(),
            other: String::new(),
            prefix: 26,
        }
    }
}
#[derive(Debug, Serialize)]
pub struct Info {
    pub network: String,
    pub prefix: u8,
    pub family: &'static str,
    pub mask: String,
    pub wildcard: String,
    pub first: String,
    pub last: String,
    pub address_count: String,
    pub broadcast: Option<String>,
    pub host_first: String,
    pub host_last: String,
    pub host_count: String,
    pub host_semantics: &'static str,
}
#[derive(Debug, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum ResultData {
    Inspect {
        subnet: Info,
    },
    Contains {
        network: String,
        address: String,
        contains: bool,
        success: bool,
    },
    Overlap {
        network: String,
        other: String,
        overlaps: bool,
        contains_other: bool,
        contained_by_other: bool,
    },
    Split {
        network: String,
        networks: Vec<String>,
    },
    Aggregate {
        networks: Vec<String>,
        exact: bool,
    },
}
fn encode(result: ResultData) -> ToolResult {
    serde_json::to_value(result)
        .map_err(|_| ToolError::new("INTERNAL_ERROR", "Cannot encode subnet result"))
}
fn bad(message: &str) -> ToolError {
    ToolError::new("INVALID_ARGUMENT", message)
}
pub fn parse(input: &str) -> Result<IpNet, ToolError> {
    let input = input.trim();
    if input.len() > 128 {
        return Err(bad("Subnet input too long / 子网输入过长"));
    }
    let (address, mask) = input
        .split_once('/')
        .ok_or_else(|| bad("Use address/prefix or IPv4/netmask / 请输入地址/前缀或点分掩码"))?;
    let ip: IpAddr = address
        .parse()
        .map_err(|_| bad("Invalid numeric IP / IP 地址无效"))?;
    let prefix = if mask.contains('.') {
        if !ip.is_ipv4() {
            return Err(bad("Dotted masks require IPv4"));
        }
        let mask: Ipv4Addr = mask.parse().map_err(|_| bad("Invalid netmask"))?;
        let n = u32::from(mask);
        let bits = n.leading_ones();
        if n != if bits == 0 {
            0
        } else {
            u32::MAX << (32 - bits)
        } {
            return Err(bad("Noncontiguous netmask / 掩码不连续"));
        }
        bits as u8
    } else {
        mask.parse::<u8>()
            .map_err(|_| bad("Invalid prefix / 前缀无效"))?
    };
    IpNet::new(ip, prefix).map_err(|_| bad("Prefix out of range / 前缀超出范围"))
}
pub fn bounds(net: IpNet) -> (u128, u128, u8) {
    let (start, bits) = match net {
        IpNet::V4(n) => (u32::from(n.network()) as u128, 32),
        IpNet::V6(n) => (u128::from(n.network()), 128),
    };
    let host = bits - net.prefix_len();
    let mask = if host == 128 {
        u128::MAX
    } else if host == 0 {
        0
    } else {
        (1u128 << host) - 1
    };
    (start, start | mask, bits)
}
fn address(n: u128, bits: u8) -> String {
    if bits == 32 {
        Ipv4Addr::from(n as u32).to_string()
    } else {
        Ipv6Addr::from(n).to_string()
    }
}
fn power(n: u8) -> String {
    if n == 128 {
        "340282366920938463463374607431768211456".into()
    } else {
        (1u128 << n).to_string()
    }
}
pub fn inspect(net: IpNet) -> Info {
    let (first, last, bits) = bounds(net);
    let host_bits = bits - net.prefix_len();
    let normal = bits == 32 && net.prefix_len() < 31;
    let mask = if bits == 32 {
        u32::MAX as u128 ^ (last - first)
    } else {
        u128::MAX ^ (last - first)
    };
    Info {
        network: net.trunc().to_string(),
        prefix: net.prefix_len(),
        family: if bits == 32 { "ipv4" } else { "ipv6" },
        mask: address(mask, bits),
        wildcard: address(last - first, bits),
        first: address(first, bits),
        last: address(last, bits),
        address_count: power(host_bits),
        broadcast: if normal {
            Some(address(last, bits))
        } else {
            None
        },
        host_first: address(first + u128::from(normal), bits),
        host_last: address(last - u128::from(normal), bits),
        host_count: if normal {
            ((1u128 << host_bits) - 2).to_string()
        } else {
            power(host_bits)
        },
        host_semantics: if bits == 128 {
            "IPv6 has no broadcast; addresses are not automatically excluded"
        } else if net.prefix_len() == 31 {
            "Point-to-point /31: both addresses; availability depends on configuration"
        } else if net.prefix_len() == 32 {
            "Single address /32"
        } else {
            "Conventional broadcast subnet host range; availability depends on configuration"
        },
    }
}
pub fn calculate(config: &Config) -> ToolResult {
    if config.input.len() > 16384 || config.other.len() > 128 {
        return Err(bad("Input too large"));
    }
    if config.operation == "aggregate" {
        let inputs: Vec<_> = config
            .input
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .collect();
        if inputs.is_empty() || inputs.len() > MAX_OUTPUT {
            return Err(bad("Aggregate requires 1..1024 networks"));
        }
        let nets = inputs
            .into_iter()
            .map(parse)
            .map(|r| r.map(|n| n.trunc()))
            .collect::<Result<Vec<_>, _>>()?;
        let result = IpNet::aggregate(&nets);
        return encode(ResultData::Aggregate {
            networks: result.iter().map(ToString::to_string).collect(),
            exact: true,
        });
    }
    let net = parse(&config.input)?;
    let (first, last, bits) = bounds(net);
    match config.operation.as_str() {
        "inspect" => encode(ResultData::Inspect {
            subnet: inspect(net),
        }),
        "contains" => {
            let ip: IpAddr = config
                .other
                .trim()
                .parse()
                .map_err(|_| bad("Invalid comparison IP"))?;
            let contains = net.contains(&ip);
            encode(ResultData::Contains {
                network: net.trunc().to_string(),
                address: ip.to_string(),
                contains,
                success: contains,
            })
        }
        "overlap" => {
            let other = parse(&config.other)?;
            let (a, b, other_bits) = bounds(other);
            encode(ResultData::Overlap {
                network: net.trunc().to_string(),
                other: other.trunc().to_string(),
                overlaps: bits == other_bits && first <= b && a <= last,
                contains_other: bits == other_bits && first <= a && last >= b,
                contained_by_other: bits == other_bits && a <= first && b >= last,
            })
        }
        "split" => {
            if config.prefix < net.prefix_len() || config.prefix > bits {
                return Err(bad(
                    "Split prefix must be within parent prefix and family width",
                ));
            }
            let delta = config.prefix - net.prefix_len();
            if delta > 10 {
                return Err(bad("Split exceeds 1024 networks / 拆分超过 1024 项"));
            }
            let count = 1usize << delta;
            let mut networks = Vec::with_capacity(count);
            for index in 0..count {
                let offset = if index == 0 {
                    0
                } else {
                    (index as u128) << (bits - config.prefix)
                };
                networks.push(format!(
                    "{}/{}",
                    address(first + offset, bits),
                    config.prefix
                ));
            }
            encode(ResultData::Split {
                network: net.trunc().to_string(),
                networks,
            })
        }
        _ => Err(bad("Unknown subnet operation")),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn boundaries_and_masks() {
        for (input, count) in [
            ("0.0.0.0/0", "4294967296"),
            ("192.0.2.0/31", "2"),
            ("192.0.2.1/32", "1"),
            ("::/0", "340282366920938463463374607431768211456"),
            ("2001:db8::/64", "18446744073709551616"),
            ("::/127", "2"),
            ("::1/128", "1"),
        ] {
            assert_eq!(inspect(parse(input).unwrap()).address_count, count);
        }
        assert_eq!(parse("192.0.2.9/255.255.255.0").unwrap().prefix_len(), 24);
        for input in [
            "1.2.3.4/255.0.255.0",
            "1.2.3.4/33",
            "::/129",
            "999.0.0.1/24",
        ] {
            assert!(parse(input).is_err());
        }
        assert!(inspect(parse("::/0").unwrap()).broadcast.is_none());
    }
    #[test]
    fn split_aggregate_and_relations() {
        for prefix in 17..25 {
            let c = Config {
                operation: "split".into(),
                input: format!("10.12.34.56/{prefix}"),
                prefix: prefix + 3,
                ..Default::default()
            };
            let result = calculate(&c).unwrap();
            let nets = result["networks"].as_array().unwrap();
            assert_eq!(nets.len(), 8);
            let parts = nets
                .iter()
                .map(|n| parse(n.as_str().unwrap()).unwrap())
                .collect();
            assert_eq!(
                IpNet::aggregate(&parts),
                vec![parse(&c.input).unwrap().trunc()]
            );
            for pair in nets.windows(2) {
                assert_eq!(
                    bounds(parse(pair[0].as_str().unwrap()).unwrap()).1 + 1,
                    bounds(parse(pair[1].as_str().unwrap()).unwrap()).0
                );
            }
        }
        assert!(
            calculate(&Config {
                operation: "split".into(),
                input: "::/0".into(),
                prefix: 128,
                ..Default::default()
            })
            .is_err()
        );
        let exact = calculate(&Config {
            operation: "aggregate".into(),
            input: "10.0.0.0/24,10.0.2.0/24".into(),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(exact["networks"].as_array().unwrap().len(), 2);
    }
}
