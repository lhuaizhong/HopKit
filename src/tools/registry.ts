export const categories = [
 {id:'common', label:['常用工具','Common']},
 {id:'files', label:['文件传输','File Transfer']},
 {id:'expert', label:['专家工具','Expert']},
 {id:'utilities', label:['小工具','Utilities']},
] as const
export type CategoryId = typeof categories[number]['id']
export type ToolId = 'mtr'|'socket'|'connections'|'wol'|'discovery'|'scan'|'subnet'|'check'|'monitor'|'ping'|'trace'|'tcping'|'dns'|'ntp'|'iperf3'|'tftp'|'ftp'|'http'|'wormhole'|'syslog'
export type ToolDefinition = {id:ToolId;name:string;glyph:string;sub:[string,string];desc:[string,string];category:CategoryId;aliases:string[];view:'mtr'|'socket'|'connections'|'wol'|'check'|'monitor'|'wormhole'|'discovery'|'scan'|'subnet'|'legacy';requirements:string[]}
const entries:Omit<ToolDefinition,'category'|'aliases'|'view'|'requirements'>[]=[
 {id:'ping',name:'Ping',glyph:'↗',sub:['连通性测试','ICMP echo'],desc:['检测目标可达性、延迟与丢包。','Measure reachability, latency and packet loss.']},
 {id:'trace',name:'Traceroute',glyph:'⌁',sub:['路由追踪','Route tracing'],desc:['逐跳查看 ICMP 路径，识别响应与超时。','Inspect ICMP responses along the route.']},
 {id:'tcping',name:'TCPing',glyph:'⇄',sub:['TCP 端口测试','TCP connection'],desc:['测量 TCP 延迟，区分拒绝与超时。','Measure TCP latency, refusals and timeouts.']},
 {id:'dns',name:'DNS',glyph:'◎',sub:['域名解析','Name resolution'],desc:['使用系统解析器，或查询指定 DNS 服务器。','Resolve with the system or query a specific DNS server.']},
 {id:'ntp',name:'NTP',glyph:'◷',sub:['时间服务','Time tools'],desc:['测量时间偏差，或使用本机时钟提供临时 NTP 服务。','Measure clock offset or serve the local clock as a temporary NTP server.']},
 {id:'iperf3',name:'iperf3',glyph:'↔',sub:['网络测速','Network speed test'],desc:['内置 TCP/UDP 测速客户端与服务端，无需安装 iperf3。','Built-in TCP/UDP throughput client and server. No iperf3 installation required.']},
 {id:'tftp',name:'TFTP',glyph:'⇅',sub:['文件传输','File transfer'],desc:['标准 octet 文件传输与临时只读服务。','Standard octet transfers and a temporary file server.']},
 {id:'ftp',name:'FTP',glyph:'⇆',sub:['文件传输与服务','File client / server'],desc:['浏览远端目录、上传下载文件，或启动临时 FTP 服务。','List remote files, upload and download, or start a temporary FTP server.']},
 {id:'http',name:'HTTP 文件服务',glyph:'▤',sub:['文件服务','Browser file sharing'],desc:['使用浏览器下载、上传文件，无需安装其他软件。','Download and upload files in a browser. No extra software required.']},
 {id:'wormhole',name:'Wormhole',glyph:'⇌',sub:['端到端文件传输','Encrypted file transfer'],desc:['通过一次性配对码安全收发单个文件。','Send and receive a file securely using a one-time pairing code.']},
 {id:'syslog',name:'Syslog',glyph:'≡',sub:['临时日志','Temporary logs'],desc:['接收、保存并预览 UDP 设备日志。','Receive, save and preview device logs over UDP.']},
 {id:'check',name:'HopKit',glyph:'◈',sub:['网络排障','Troubleshooting'],desc:['确认本机网络与目标，逐项检查并关联证据和下一步。','Confirm the local network and target, then review staged evidence and next steps.']},
 {id:'monitor',name:'Monitor',glyph:'◴',sub:['多目标持续监测','Multi-target monitoring'],desc:['同时观察网关、外部目标和业务服务器，记录失败与恢复时间。','Observe gateway, external and business targets together, with failure and recovery times.']}
]
const order:ToolId[]=['check','monitor','ping','trace','tcping','dns','iperf3','ntp','tftp','ftp','http','wormhole','syslog']
const fileTools:ToolId[]=['tftp','ftp','http','wormhole']
const expertTools:ToolId[]=['monitor','syslog']
const aliases:Partial<Record<ToolId,string[]>>={check:['网络排障','troubleshooting','诊断'],monitor:['持续监测','monitor'],trace:['路由追踪','traceroute'],http:['HTTP','文件共享','file sharing'],ftp:['文件传输'],syslog:['日志','日志接收']}
const legacyTool=(id:ToolId):ToolDefinition=>({...entries.find(t=>t.id===id)!,id,category:fileTools.includes(id)?'files':expertTools.includes(id)?'expert':'common',aliases:aliases[id]??[],view:['check','monitor','wormhole'].includes(id)?id as 'check'|'monitor'|'wormhole':'legacy',requirements:[]})
export const tools:ToolDefinition[]=order.filter(id=>!expertTools.includes(id)).map(legacyTool)
tools.push({id:'scan',name:'TCP Scan',glyph:'▦',sub:['TCP 端口扫描','TCP port scanner'],desc:['对明确目标做有界 TCP 建连扫描。','Bounded TCP connect scanning of explicit targets.'],category:'expert',aliases:['扫端口','端口扫描','port scan','scanner'],view:'scan',requirements:[]},{id:'discovery',name:'Discovery',glyph:'⌘',sub:['设备发现','Device discovery'],desc:['发现有限候选的响应设备，分别查看邻居缓存与名称来源。','Discover responsive candidates with separate neighbor cache and name evidence.'],category:'expert',aliases:['设备发现','局域网','邻居','ARP','ND','discover','lan','neighbor'],view:'discovery',requirements:[]},{id:'subnet',name:'Subnet',glyph:'⊞',sub:['子网计算器','Subnet calculator'],desc:['离线计算 IPv4/IPv6 网段、包含关系、拆分与精确聚合。','Offline IPv4/IPv6 subnet arithmetic, containment, splitting and exact aggregation.'],category:'utilities',aliases:['子网','掩码','CIDR','subnet calculator'],view:'subnet',requirements:[]})

export function isToolId(value:unknown):value is ToolId {return tools.some(t=>t.id===value)}
export function displayName(tool:ToolDefinition,en:boolean){return en&&tool.id==='http'?'HTTP File Sharing':tool.name}
export function searchTools(query:string){const normalized=query.trim().toLocaleLowerCase(),terms=normalized.split(/\s+/);return tools.filter(t=>terms.every(term=>[t.name,...t.sub,...t.desc,...t.aliases,t.id].join(' ').toLocaleLowerCase().includes(term))).sort((a,b)=>Number([b.id,b.name,...b.aliases].some(s=>s.toLocaleLowerCase()===normalized))-Number([a.id,a.name,...a.aliases].some(s=>s.toLocaleLowerCase()===normalized)))}
export function categoryOf(id:ToolId):CategoryId{return tools.find(t=>t.id===id)!.category}

 tools.push({id:'socket',name:'TCP / UDP',glyph:'⇄',sub:['调试助手','Socket debug'],desc:['两端收发字节流与数据报，验证网络规则。','Exchange TCP bytes and UDP datagrams to inspect network rules.'],category:'expert',aliases:['TCP/UDP 调试助手','收发','socket','UDP','nc'],view:'socket',requirements:[]},{id:'connections',name:'Connections',glyph:'☷',sub:['本机端口与连接','Local ports & connections'],desc:['只读查看本机 TCP/UDP 端点和进程。','Inspect local TCP/UDP endpoints and available process names.'],category:'utilities',aliases:['端口占用','监听端口','netstat','connections','PID'],view:'connections',requirements:[]},{id:'wol',name:'Wake-on-LAN',glyph:'⏻',sub:['网络唤醒','Wake-on-LAN'],desc:['发送 IPv4 Magic Packet，发出不代表已唤醒。','Send IPv4 Magic Packets; sending does not prove wake-up.'],category:'utilities',aliases:['唤醒','WOL','magic packet'],view:'wol',requirements:[]})
tools.push(...expertTools.map(legacyTool))

tools.push({id:'mtr',name:'MTR',glyph:'⌁',sub:['持续路由诊断','Continuous route diagnosis'],desc:['持续 ICMP 逐跳观测、目标到达与多响应者统计。','Continuous ICMP hop observations, target reach and multiple responders.'],category:'expert',aliases:['mtr','持续路由','路由诊断'],view:'mtr',requirements:['icmp']})
