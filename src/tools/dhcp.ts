export type DhcpConfig={operation:'inspect'|'discover';interface:string;duration_secs:number;attempts:number;expected_servers:string[]}
export const dhcpDefaults=():DhcpConfig=>({operation:'inspect',interface:'',duration_secs:5,attempts:3,expected_servers:[]})
export type DhcpInterface={id:string;name:string;index:number;mac:string|null;dhcp_enabled:boolean|null;server:string|null;lease_start:string|null;lease_expires:string|null;addresses:{address:string;mask:string}[];gateways:string[];dns:string[];search_domains:string[];config_source:string;field_sources:Record<string,string>;limitations:string[]}
export type DhcpOffer={observed_at:string;server_id:string;packet_source:string;yiaddr:string;siaddr:string;relay:string;relay_agent_hex:string|null;lease_secs:number|null;mask:string|null;routers:string[];dns:string[];search_domains:string[];outside_expected:boolean|null}
export type DhcpResult={schema_version:1;operation:string;config_source:string;started_at:string;ended_at:string;duration_ms:number;interfaces:DhcpInterface[];offers:DhcpOffer[];server_ids:string[];stats:{sent:number;received:number;ignored:number;malformed:number;duplicates:number;offers_dropped:number};transaction_id:string|null;client_mac:string|null;local_endpoint:string|null;remote_endpoint:string|null;diagnostic_outcome:string;success:boolean;partial:boolean;limitations:string[]}
export function cleanDhcp(input:unknown):DhcpConfig{
 if(!input||typeof input!=='object'||Array.isArray(input))throw Error('Invalid DHCP configuration')
 const d=input as DhcpConfig,c={...dhcpDefaults(),...Object.fromEntries(Object.keys(dhcpDefaults()).filter(k=>k in d).map(k=>[k,d[k as keyof DhcpConfig]]))} as DhcpConfig
 const ip=(s:unknown)=>typeof s==='string'&&/^\d{1,3}(\.\d{1,3}){3}$/.test(s)&&s.split('.').every(n=>Number(n)<=255)&&Number(s.split('.')[0])<224&&!['0.0.0.0','255.255.255.255'].includes(s)
 if(!['inspect','discover'].includes(c.operation)||typeof c.interface!=='string'||c.interface.length>256||/[\x00-\x1f\x7f]/.test(c.interface)||!Number.isInteger(c.duration_secs)||c.duration_secs<1||c.duration_secs>30||!Number.isInteger(c.attempts)||c.attempts<1||c.attempts>3||!Array.isArray(c.expected_servers)||c.expected_servers.length>32||!c.expected_servers.every(ip))throw Error('Invalid DHCP interface/time/server list / DHCP 参数无效')
 return {...c,expected_servers:[...c.expected_servers]}
}
