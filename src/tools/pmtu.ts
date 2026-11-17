export type PmtuConfig={target:string;family:'ipv4'|'ipv6';source:string;min_ip_bytes:number;max_ip_bytes:number;retries:number;timeout_ms:number;total_timeout_ms:number}
export const pmtuDefaults=():PmtuConfig=>({target:'127.0.0.1',family:'ipv4',source:'',min_ip_bytes:576,max_ip_bytes:1500,retries:1,timeout_ms:1000,total_timeout_ms:30000})
export type PmtuProbe={seq:number;observed_at:string;phase:string;ip_bytes:number;ip_header_bytes:number;icmp_header_bytes:number;payload_bytes:number;status:string;status_code:number|null;responder:string|null;reported_mtu:number|null;rtt_ms:number|null}
export type PmtuResult={schema_version:1;target:string;resolved_target:string|null;source:string;local_endpoint?:string|null;family:string;adapter:string;fragmentation_control:string;started_at:string;ended_at:string;duration_ms:number;confirmed_lower:number|null;confirmed_upper:number|null;reported_mtu:number|null;probes:PmtuProbe[];stats:{attempts:number;replies:number;too_big:number;unknown:number};unknown_reasons:string[];diagnostic_outcome:string;success:boolean;partial:boolean;limitations:string[]}
export function cleanPmtu(input:unknown):PmtuConfig {
 if(!input||typeof input!=='object'||Array.isArray(input))throw Error('Invalid PMTU configuration')
 const c=pmtuDefaults();for(const key of Object.keys(c) as (keyof PmtuConfig)[]){const v=(input as PmtuConfig)[key];if(v===undefined)continue;if(typeof v!==typeof c[key]||typeof v==='string'&&(v.length>253||/[\s\x00-\x1f/@?#]/.test(v))||typeof v==='number'&&!Number.isInteger(v))throw Error('Invalid PMTU field: '+key);Object.assign(c,{[key]:v})}
 if(!c.target||!['ipv4','ipv6'].includes(c.family)||c.min_ip_bytes<(c.family==='ipv6'?1280:68)||c.min_ip_bytes>c.max_ip_bytes||c.max_ip_bytes>65535||c.retries<0||c.retries>3||c.timeout_ms<100||c.timeout_ms>1000||c.total_timeout_ms<1000||c.total_timeout_ms>120000)throw Error('Invalid PMTU size/time budget')
 return c
}
