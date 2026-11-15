import {cleanScan} from './m2'
export type DiscoveryConfig={targets:string[];interface:string;methods:('arp'|'icmp'|'tcp')[];ports:string;timeout_ms:number;concurrency:number;rate:number;source:string;name_lookup:boolean;dns_server:string}
export type Neighbor={address:string;interface:string;interface_index:number;mac:string|null;state:string;source:string;observed_at:string;phase:string}
export type DiscoveryObservation={method:string;status:string;responded:boolean;port:number|null;elapsed_ms:number;observed_at:string;source:string;local_endpoint:string|null;error:{code:string;message:string}|null}
export type DiscoveryDevice={address:string;responded:boolean;first_response_at:string|null;last_response_at:string|null;observations:DiscoveryObservation[];neighbors:Neighbor[];name:{source:string;server:string;observed_at:string;names:string[];error:{code:string;message:string}|null}|null;mac_change_hint:boolean}
export type DiscoveryResult={schema_version:1;rows:DiscoveryDevice[];stats:{candidates:number;responders:number;unknown:number;planned:number;completed:number;remaining:number;duration_ms:number};neighbor_sources:string[];errors:{code:string;message:string}[];partial:boolean;limitations:string[]}
export type DiscoveryEvent={event_type:'discovery_device';device:DiscoveryDevice}|{event_type:'stats';total:number;observed:number;remaining:number}
export type DiscoveryInterface={name:string;index:number;address:string;prefix:number}
export const discoveryDefaults=():DiscoveryConfig=>({targets:[],interface:'',methods:['arp','icmp','tcp'],ports:'80,443',timeout_ms:500,concurrency:32,rate:100,source:'',name_lookup:false,dns_server:''})
export function cleanDiscovery(input:unknown):DiscoveryConfig{
 const c=input as DiscoveryConfig
 if(!c||!Array.isArray(c.targets)||!Array.isArray(c.methods)||c.methods.length<1||c.methods.length>3||new Set(c.methods).size!==c.methods.length||c.methods.some(m=>!['arp','icmp','tcp'].includes(m))||typeof c.interface!=='string'||c.interface.length>128||/[\x00-\x1f]/.test(c.interface)||typeof c.name_lookup!=='boolean'||typeof c.dns_server!=='string'||c.dns_server.length>128||/[\x00-\x20]/.test(c.dns_server)||c.name_lookup&&!c.dns_server||c.timeout_ms>1000)throw Error('Invalid discovery settings / 设备发现配置无效')
 cleanScan({targets:c.targets.length?c.targets:['127.0.0.1'],ports:c.ports,timeout_ms:c.timeout_ms,concurrency:c.concurrency,rate:c.rate,source:c.source,family:'auto'})
 const ports=new Set<number>();for(const item of c.ports.split(',')){const [a,b=a]=item.split('-').map(Number);if(b-a>7)throw Error('Discovery accepts at most 8 ports / 设备发现最多 8 个端口');for(let n=a;n<=b;n++){ports.add(n);if(ports.size>8)throw Error('Discovery accepts at most 8 ports / 设备发现最多 8 个端口')}}
 return {targets:[...c.targets],interface:c.interface,methods:[...c.methods],ports:c.ports,timeout_ms:c.timeout_ms,concurrency:c.concurrency,rate:c.rate,source:c.source,name_lookup:c.name_lookup,dns_server:c.dns_server}
}

// Compare address bytes, keeping IPv4 before IPv6 and numeric scopes distinct.
export function compareDiscoveryIp(a:string,b:string):number{
 const key=(text:string):[number,bigint,number]=>{
  const [ip,scope='0']=text.split('%')
  const v4=(v:string)=>v.split('.').reduce((n,p)=>(n<<8n)+BigInt(p),0n)
  if(!ip.includes(':'))return [0,v4(ip),0]
  const expanded=ip.replace(/(\d+\.\d+\.\d+\.\d+)$/,v=>{const n=v4(v);return `${(n>>16n).toString(16)}:${(n&65535n).toString(16)}`})
  const [left,right]=expanded.split('::'),l=left?left.split(':'):[],r=right?right.split(':'):[]
  const parts=right===undefined?l:[...l,...Array(8-l.length-r.length).fill('0'),...r]
  return [1,parts.reduce((n,p)=>(n<<16n)+BigInt(`0x${p}`),0n),Number(scope)]
 }
 try{const x=key(a),y=key(b);return x[0]-y[0]||(x[1]<y[1]?-1:x[1]>y[1]?1:0)||x[2]-y[2]}catch{return a.localeCompare(b)}
}
export function discoveryMacs(row:DiscoveryDevice):string[]{
 return [...new Set([...row.neighbors].sort((a,b)=>b.observed_at.localeCompare(a.observed_at)).map(n=>n.mac).filter((m):m is string=>!!m))]
}
