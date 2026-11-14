export type ScanConfig={targets:string[];ports:string;timeout_ms:number;concurrency:number;rate:number;family:'auto'|'ipv4'|'ipv6';source:string}
export type SubnetConfig={operation:'inspect'|'contains'|'overlap'|'split'|'aggregate';input:string;other:string;prefix:number}
export type ScanRow={address:string;port:number;status:'open'|'refused'|'timeout'|'unreachable'|'local_error'|'cancelled';elapsed_ms:number;local_endpoint:string|null;remote_endpoint:string;error?:{code:string;message:string}|null}
export type ScanResult={schema_version:1;resolution_complete:boolean;candidates:string[];ports:number[];rows:ScanRow[];stats:{total:number|null;completed:number;remaining:number|null;interrupted:number;counts:Record<string,number>;duration_ms:number};partial:boolean;notes:string[]}
export type ScanEvent=
 |{event_type:'scan_target';input:string;candidates:string[]}
 |{event_type:'scan_row';row:ScanRow}
 |{event_type:'stats';total:number;observed:number;remaining:number}
export type SubnetInfo={network:string;prefix:number;family:'ipv4'|'ipv6';mask:string;wildcard:string;first:string;last:string;address_count:string;broadcast:string|null;host_first:string;host_last:string;host_count:string;host_semantics:string}
export type SubnetResult=
 |{operation:'inspect';subnet:SubnetInfo}
 |{operation:'contains';network:string;address:string;contains:boolean;success:boolean}
 |{operation:'overlap';network:string;other:string;overlaps:boolean;contains_other:boolean;contained_by_other:boolean}
 |{operation:'split';network:string;networks:string[]}
 |{operation:'aggregate';networks:string[];exact:true}
export const scanDefaults=():ScanConfig=>({targets:['127.0.0.1'],ports:'22,80,443,3389',timeout_ms:1000,concurrency:32,rate:100,family:'auto',source:''})
export const subnetDefaults=():SubnetConfig=>({operation:'inspect',input:'192.168.10.25/24',other:'192.168.10.1',prefix:26})
export function cleanScan(input:unknown):ScanConfig {
 const s=input as ScanConfig
 if(!s||!Array.isArray(s.targets)||s.targets.length<1||s.targets.length>4096||s.targets.some(v=>typeof v!=='string'||!v.trim()||v.length>253||/[\x00-\x20]/.test(v))||typeof s.ports!=='string'||s.ports.length>65536||!/^\s*\d+(?:\s*-\s*\d+)?(?:\s*,\s*\d+(?:\s*-\s*\d+)?)*\s*$/.test(s.ports)||!Number.isInteger(s.timeout_ms)||s.timeout_ms<100||s.timeout_ms>10000||!Number.isInteger(s.concurrency)||s.concurrency<1||s.concurrency>128||!Number.isInteger(s.rate)||s.rate<1||s.rate>100||!['auto','ipv4','ipv6'].includes(s.family)||typeof s.source!=='string'||s.source.length>128||/[\x00-\x20]/.test(s.source))throw Error('Invalid scan settings / 扫描配置无效')
 for(const item of s.ports.split(',')){const [a,b=a]=item.split('-').map(v=>Number(v.trim()));if(!Number.isInteger(a)||!Number.isInteger(b)||a<1||a>b||b>65535)throw Error('Invalid port range / 端口范围无效')}
 return {targets:[...s.targets],ports:s.ports.trim(),timeout_ms:s.timeout_ms,concurrency:s.concurrency,rate:s.rate,family:s.family,source:s.source}
}
export function cleanSubnet(input:unknown):SubnetConfig {
 const s=input as SubnetConfig
 if(!s||!['inspect','contains','overlap','split','aggregate'].includes(s.operation)||typeof s.input!=='string'||s.input.length>16384||typeof s.other!=='string'||s.other.length>128||!Number.isInteger(s.prefix)||s.prefix<0||s.prefix>128)throw Error('Invalid subnet settings / 子网配置无效')
 return {operation:s.operation,input:s.input,other:s.other,prefix:s.prefix}
}
