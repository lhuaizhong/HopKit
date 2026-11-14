import {defaults, type ToolSnapshot} from './config'
import {isToolId, type ToolId} from './registry'
export type TargetBookmark={id:string;name:string;target:string}
export type Profile={id:string;name:string;snapshot:ToolSnapshot}
export type Library={schema_version:1;bookmarks:TargetBookmark[];profiles:Profile[];configurations:ToolSnapshot[]}
export const emptyLibrary=():Library=>({schema_version:1,bookmarks:[],profiles:[],configurations:[]})
export const defaultMode=(id:ToolId)=>({ftp:'list',http:'serve',tftp:'get',ntp:'query',iperf3:'client',syslog:'listen',dns:'system',wormhole:'send'} as Partial<Record<ToolId,string>>)[id]??''
const modes:Partial<Record<ToolId,string[]>>={ftp:['list','get','put','serve'],http:['serve'],tftp:['get','put','serve'],ntp:['query','serve'],iperf3:['client','serve'],syslog:['listen','send'],dns:['system','direct'],wormhole:['send','receive']}
const validMode=(tool:ToolId,mode:unknown)=>typeof mode==='string'&&(modes[tool]??['']).includes(mode)
const bounded=(v:unknown,max=4096):v is string=>typeof v==='string'&&v.length<=max&&!/[\x00-\x1f]/.test(v)
export function safeTarget(target:string):string{
 if(!bounded(target))throw Error('Invalid target / 目标无效')
 if(target.includes('://')){
  const url=new URL(target)
  if(!['http:','https:'].includes(url.protocol)||url.username||url.password)throw Error('URL credentials are not allowed / URL 不允许携带凭据')
  url.search='';url.hash='';return url.toString()
 }
 return target.trim()
}
// Explicit allowlist: payloads, codes, credentials and future unknown fields are never persisted.
export function sanitizeSnapshot(input:unknown):ToolSnapshot{
 const data=input as ToolSnapshot
 if(!data||data.schema_version!==1||!isToolId(data.tool)||!validMode(data.tool,data.mode)||!data.values||typeof data.values!=='object'||Array.isArray(data.values))throw Error('Unsupported configuration / 配置格式或版本无效')
 const values:ToolSnapshot['values']={}
 for(const [key,baseline] of Object.entries(defaults())){
  if(['ftp_password','message','action','dns_mode'].includes(key))continue
  const value=data.values[key as keyof typeof data.values]
  if(value===undefined)continue
  if(typeof value!==typeof baseline||typeof value==='number'&&(!Number.isFinite(value)||value<0)||typeof value==='string'&&!bounded(value))throw Error('Invalid configuration field / 配置字段无效: '+key)
  const bounds:Record<string,[number,number]>={port:[0,65535],count:[0,10000],timeout_ms:[100,10000],interval_ms:[0,60000],payload_size_bytes:[0,65500],max_hops:[1,64],ntp_stratum:[1,15],duration_secs:[0,4294967295],iperf_parallel:[1,16],iperf_bitrate_mbps:[0,10000]}
  if(typeof value==='number'&&(!Number.isInteger(value)||bounds[key]&&(value<bounds[key][0]||value>bounds[key][1])))throw Error('Configuration number out of range: '+key)
  Object.assign(values,{[key]:key==='target'?safeTarget(String(value)):value})
 }
 if(values.ipv4&&values.ipv6||values.iperf_protocol!==undefined&&!['tcp','udp'].includes(values.iperf_protocol)||values.record_type!==undefined&&!['A','AAAA','CNAME','MX','TXT','NS','PTR','SOA'].includes(values.record_type))throw Error('Invalid configuration option / 配置选项无效')
 const snapshot:ToolSnapshot={schema_version:1,tool:data.tool,mode:data.mode,values}
 if(data.tool==='monitor'&&data.monitor){
  const m=data.monitor
  if(!Array.isArray(m.targets)||m.targets.length<1||m.targets.length>3)throw Error('Invalid monitor targets')
  const targets=m.targets.map(t=>{
   if(!bounded(t.label,120)||!['ping','tcping'].includes(t.protocol)||!bounded(t.target)||!Number.isInteger(t.port)||t.port<0||t.port>65535)throw Error('Invalid monitor target')
   return {label:t.label,protocol:t.protocol,target:safeTarget(t.target),port:t.port}
  })
  if(![m.interval_ms,m.timeout_ms,m.duration_secs].every(v=>Number.isInteger(v)&&v>=0)||m.interval_ms<1000||m.interval_ms>60000||m.timeout_ms<100||m.timeout_ms>3000||m.duration_secs>604800||!bounded(m.log_directory))throw Error('Invalid monitor settings')
  snapshot.monitor={targets,interval_ms:m.interval_ms,timeout_ms:m.timeout_ms,duration_secs:m.duration_secs,log_directory:m.log_directory}
 }
 if(data.tool==='wormhole'&&data.wormhole){
  const w=data.wormhole
  if(typeof w!=='object'||Array.isArray(w))throw Error('Invalid Wormhole settings')
  const saved:NonNullable<ToolSnapshot['wormhole']>={operation:data.mode as 'send'|'receive'}
  if(w.send_path!==undefined){if(!bounded(w.send_path))throw Error('Invalid send path');saved.send_path=w.send_path}
  for(const key of ['mailbox_url','transit_relay'] as const){
   const value=w[key];if(value===undefined)continue
   if(!bounded(value))throw Error('Invalid Wormhole endpoint')
   if(value===''){saved[key]='';continue}
   const url=new URL(value)
   if(!url.hostname||(key==='mailbox_url'?!['ws:','wss:'].includes(url.protocol):url.protocol!=='tcp:'||!url.port||!['','/'].includes(url.pathname)))throw Error('Invalid Wormhole endpoint')
   // Never persist credentials or query tokens, even inside public endpoint fields.
   saved[key]=url.username||url.password||url.search||url.hash?'':value
  }
  if(w.connection_mode!==undefined){if(!['auto','direct','relay'].includes(w.connection_mode))throw Error('Invalid connection mode');saved.connection_mode=w.connection_mode}
  for(const [key,min,max]of [['code_length',2,8],['peer_wait_secs',60,3600]] as const){const value=w[key];if(value===undefined)continue;if(!Number.isInteger(value)||value<min||value>max)throw Error('Invalid Wormhole numeric setting');saved[key]=value}
  if(w.verify!==undefined){if(typeof w.verify!=='boolean')throw Error('Invalid verification setting');saved.verify=w.verify}
  if(w.max_receive_bytes!==undefined){const value=w.max_receive_bytes;if(value!==null&&(typeof value!=='string'||!/^\d{1,20}$/.test(value)||BigInt(value)>18446744073709551615n))throw Error('Invalid receive limit');saved.max_receive_bytes=value}
  snapshot.wormhole=saved
 }
 return snapshot
}
export function parseLibrary(text:string):Library{
 if(new TextEncoder().encode(text).length>262144)throw Error('Configuration exceeds 256 KiB / 配置超过 256 KiB')
 const data=JSON.parse(text)
 if(!data||data.schema_version!==1||!['bookmarks','profiles','configurations'].every(k=>Array.isArray(data[k])&&data[k].length<=100))throw Error('Unsupported library / 配置库格式或版本无效')
 const name=(v:unknown)=>{if(!bounded(v,120)||!v.trim())throw Error('Invalid name / 名称无效');return v.trim()}
 const id=(v:unknown)=>{if(!bounded(v,120)||!v)throw Error('Invalid ID');return v}
 const bookmarks=data.bookmarks.map((b:TargetBookmark)=>({id:id(b.id),name:name(b.name),target:safeTarget(b.target)}))
 const profiles=data.profiles.map((p:Profile)=>({id:id(p.id),name:name(p.name),snapshot:sanitizeSnapshot(p.snapshot)}))
 const configurations=data.configurations.map(sanitizeSnapshot)
 for(const list of [bookmarks,profiles])if(new Set(list.map((e:{id:string})=>e.id)).size!==list.length)throw Error('Duplicate ID')
 if(new Set(configurations.map((s:ToolSnapshot)=>s.tool+':'+s.mode)).size!==configurations.length)throw Error('Duplicate configuration')
 return {schema_version:1,bookmarks,profiles,configurations}
}
export function loadLibrary():{library:Library;error:string}{try{const raw=localStorage.getItem('hopkit-library');return {library:raw?parseLibrary(raw):emptyLibrary(),error:''}}catch{return {library:emptyLibrary(),error:'配置库无法读取；当前会话仍可使用。 / Configuration library unavailable; this session remains usable.'}}}
export function mergeLibraries(current:Library,incoming:Library):Library{
 const merge=<T extends {id:string}>(a:T[],b:T[])=>[...new Map([...a,...b].map(x=>[x.id,x])).values()]
 return parseLibrary(JSON.stringify({schema_version:1,bookmarks:merge(current.bookmarks,incoming.bookmarks),profiles:merge(current.profiles,incoming.profiles),configurations:[...new Map([...current.configurations,...incoming.configurations].map(c=>[c.tool+':'+c.mode,c])).values()]}))
}
export function saveLibrary(library:Library):boolean{try{localStorage.setItem('hopkit-library',JSON.stringify(parseLibrary(JSON.stringify(library))));return true}catch{return false}}
