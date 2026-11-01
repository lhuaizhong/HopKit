<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { appearance } from './appearance'
import { resultImage } from './result-image'
import { preference, savePreference } from './preferences'
import CheckSetup from './components/CheckSetup.vue'
import CheckResults from './components/CheckResults.vue'
import MonitorSetup from './components/MonitorSetup.vue'
import MonitorResults from './components/MonitorResults.vue'

type Row = Record<string, any>
type TraceHop = {hop:number;probes:(Row|null)[];addresses:string[]}
type Task = {id:string;tool:string;action:string;target:string;port?:number;status:string;started_at:string;seq:number;events:Row[];result:Row|null;request?:Row|null;latest_stats?:Row|null}
const appInfo=ref({version:'',platform:'',os:''})
const savedLanguage=preference('language')
const lang=ref(['zh-CN','en-US'].includes(savedLanguage??'')?savedLanguage!:(navigator.language.toLowerCase().startsWith('zh')?'zh-CN':'en-US'))
const en=computed(()=>lang.value==='en-US')
const t=(zh:string,english:string)=>en.value?english:zh
const projectUrl='https://github.com/lhuaizhong/HopKit'
const projectLinkError=ref('')
async function openProject(){try{projectLinkError.value='';await invoke('open_project')}catch(e){projectLinkError.value=t('无法打开浏览器，请复制链接访问：','Could not open the browser. Copy this link: ')+projectUrl}}
const tools=[
 {id:'ping',name:'Ping',glyph:'↗',sub:['连通性测试','ICMP echo'],desc:['检测目标可达性、延迟与丢包。','Measure reachability, latency and packet loss.']},
 {id:'trace',name:'Traceroute',glyph:'⌁',sub:['路由追踪','Route tracing'],desc:['逐跳查看 ICMP 路径，识别响应与超时。','Inspect ICMP responses along the route.']},
 {id:'tcping',name:'TCPing',glyph:'⇄',sub:['TCP 端口测试','TCP connection'],desc:['测量 TCP 建连耗时，区分拒绝与超时。','Measure TCP connections, refusals and timeouts.']},
 {id:'dns',name:'DNS',glyph:'◎',sub:['域名解析','Name resolution'],desc:['使用系统解析器，或查询指定 DNS 服务器。','Resolve with the system or query a specific DNS server.']},
 {id:'ntp',name:'NTP',glyph:'◷',sub:['时间服务','Time tools'],desc:['测量时间偏差，或使用本机时钟提供临时 NTP 服务。','Measure clock offset or serve the local clock as a temporary NTP server.']},
 {id:'iperf3',name:'iperf3',glyph:'↔',sub:['网络测速','Network speed test'],desc:['内置 TCP/UDP 测速客户端与服务端，无需安装 iperf3。','Built-in TCP/UDP throughput client and server. No iperf3 installation required.']},
 {id:'tftp',name:'TFTP',glyph:'⇅',sub:['文件传输','File transfer'],desc:['标准 octet 文件传输与临时只读服务。','Standard octet transfers and a temporary file server.']},
 {id:'ftp',name:'FTP',glyph:'⇆',sub:['文件传输与服务','File client / server'],desc:['浏览远端目录、上传下载文件，或启动临时 FTP 服务。','List remote files, upload and download, or start a temporary FTP server.']},
 {id:'syslog',name:'Syslog',glyph:'≡',sub:['临时日志','Temporary logs'],desc:['接收、保存并预览 UDP 设备日志。','Receive, save and preview device logs over UDP.']},
 {id:'check',name:'HopKit',glyph:'◈',sub:['网络排障','Troubleshooting'],desc:['确认本机网络与目标，逐项检查并关联证据和下一步。','Confirm the local network and target, then review staged evidence and next steps.']},
 {id:'monitor',name:'Monitor',glyph:'◴',sub:['多目标持续监测','Multi-target monitoring'],desc:['同时观察网关、外部目标和业务服务器，记录失败与恢复时间。','Observe gateway, external and business targets together, with failure and recovery times.']}
]
const monitorConfig=reactive({targets:[{label:'Gateway',protocol:'ping',target:'',port:0},{label:'External',protocol:'ping',target:'',port:0},{label:'Business',protocol:'tcping',target:'',port:443}],interval_ms:1000,timeout_ms:1000,duration_secs:0,log_directory:''})
const selected=ref('check'),active=ref(''),tasks=ref<Task[]>([]),rows=ref<Row[]>([]),error=ref(''),loading=ref(false),filter=ref(''),expanded=ref(false),copied=ref(false)
// List responses contain summaries only. Keep the selected snapshot separately
// so refreshing the task center cannot remove its result or reset its scroll.
const activeDetail=ref<Task|null>(null)
const exportingImage=ref(false),imageSaved=ref(false),imageCopied=ref(false)
const imageError=ref('')
watch(active,()=>{imageSaved.value=false;imageCopied.value=false;imageError.value=''})
const reusableTarget=ref('')
const defaults=()=>({source_address:'',tls_ca_file:'',insecure_tls:false,include_trace:false,target:'127.0.0.1',port:443,count:4,timeout_ms:1000,interval_ms:1000,payload_size_bytes:32,ipv6:false,max_hops:30,record_type:'A',dns_server:'',dns_mode:'system',path:'',remote_name:'config.cfg',action:'',listen:'0.0.0.0',allow:'',writable:false,overwrite:false,message:'HopKit test',continuous:false,ntp_stratum:10,duration_secs:0,ftp_username:'anonymous',ftp_password:'',ftp_remote_path:'',iperf_protocol:'tcp',iperf_parallel:1,iperf_reverse:false,iperf_bitrate_mbps:0})
const form=reactive({...defaults(),target:''})
const configurations=new Map<string,ReturnType<typeof defaults>>()
const modes=new Map<string,string>()
const configKey=()=>`${selected.value}:${selected.value==='dns'?form.dns_mode:form.action}`
function saveConfig(){configurations.set(configKey(),{...form,ftp_password:''})}
function loadConfig(id:string,mode:string){
 const key=`${id}:${mode}`
 const saved=configurations.get(key)
 Object.assign(form,saved??{...defaults(),target:id==='check'?'':'127.0.0.1',action:mode,dns_mode:id==='dns'?mode:'system',port:({ntp:123,ftp:21,iperf3:5201,tftp:69,syslog:514} as Row)[id]??443,timeout_ms:['ftp','iperf3'].includes(id)?5000:id==='ntp'?2000:1000,interval_ms:id==='ntp'?2000:1000,duration_secs:id==='iperf3'&&mode!=='serve'?10:0})
 form.ftp_password=''
}
function setMode(mode:string){if(mode===(selected.value==='dns'?form.dns_mode:form.action))return;saveConfig();modes.set(selected.value,mode);loadConfig(selected.value,mode)}

const current=computed(()=>tools.find(x=>x.id===selected.value)!)
const task=computed(()=>activeDetail.value?.id===active.value?activeDetail.value:tasks.value.find(x=>x.id===active.value))
const running=computed(()=>task.value?.status==='running'||task.value?.status==='cancelling')
const service=computed(()=>['ftp','tftp','ntp','iperf3'].includes(selected.value)&&form.action==='serve'||selected.value==='syslog'&&form.action==='listen')
const serviceTask=computed(()=>service.value?tasks.value.find(x=>x.tool===selected.value&&x.action===form.action&&['running','cancelling'].includes(x.status)):undefined)
const stopTask=computed(()=>service.value?serviceTask.value:running.value?task.value:undefined)
const startDisabled=computed(()=>loading.value||countActive.value>=4||!!serviceTask.value)
const startLabel=computed(()=>loading.value?t('正在启动…','Starting…'):serviceTask.value?serviceTask.value.status==='cancelling'?t('正在停止…','Stopping…'):t('服务运行中','Service running'):service.value?t('启动服务','Start service'):t('开始测试','Run test'))
const isTransfer=computed(()=>['ftp','tftp'].includes(selected.value)&&['get','put'].includes(form.action))
const countActive=computed(()=>tasks.value.filter(x=>['running','cancelling'].includes(x.status)).length)
const sampleRows=computed(()=>rows.value.filter(x=>x.event_type==='sample'))
const displayRows=computed(()=>rows.value.filter(x=>!['target','stats'].includes(x.event_type)).filter(x=>!filter.value||JSON.stringify(x).toLowerCase().includes(filter.value.toLowerCase())).slice(-200))
const data=computed(()=>task.value?.result?.data)
const traceHops=computed(()=>{
 const hops=new Map<number,TraceHop>()
 const samples:Row[]=Array.isArray(data.value?.samples)?data.value.samples:task.value?.latest_stats?.samples??sampleRows.value
 for(const sample of samples){
  const hop=Number(sample.hop),slot=(Number(sample.seq)-1)%3
  if(!Number.isInteger(hop)||hop<1||!Number.isInteger(slot)||slot<0)continue
  if(!hops.has(hop))hops.set(hop,{hop,probes:[null,null,null],addresses:[]})
  const row=hops.get(hop)!;row.probes[slot]=sample
 }
 return [...hops.values()].sort((a,b)=>a.hop-b.hop).map(hop=>({...hop,addresses:[...new Set(hop.probes.filter(p=>p?.address&&p.rtt_ms!=null&&p.status!=='timeout').map(p=>String(p!.address)))]})).filter(hop=>!filter.value||JSON.stringify(hop).toLowerCase().includes(filter.value.toLowerCase()))
})
const traceStatus=(probe:Row)=>({reply:t('已到达','Reached'),ttl_expired:t('TTL 到期','TTL expired'),timeout:t('超时','Timeout'),unreachable:t('不可达','Unreachable'),packet_too_big:t('包过大','Packet too big'),parameter_problem:t('参数错误','Parameter problem')}[probe.status as string]||probe.status)
const probeSize=computed(()=>data.value?.payload_size_bytes??[...rows.value].reverse().find(x=>x.payload_size_bytes!=null)?.payload_size_bytes)
const latestStats=computed(()=>task.value?.latest_stats??[...rows.value].reverse().find(x=>['stats','stopped'].includes(x.event_type)))
const cumulative=computed(()=>data.value??latestStats.value)
const speedRows=computed(()=>{const start=rows.value.map(x=>x.event_type).lastIndexOf('connected');return rows.value.slice(Math.max(0,start)).filter(x=>x.event_type==='stats'&&x.mbps!=null&&x.has_samples).slice(-80)})
const speedMax=computed(()=>Math.max(1,...speedRows.value.map(x=>Number(x.mbps))))
const stats=computed(()=>({sent:cumulative.value?.sent??0,received:cumulative.value?.received??0,loss:cumulative.value?.loss_percent,avg:cumulative.value?.rtt_avg_ms}))
const resultTool=computed(()=>task.value?.tool??selected.value)
const resultService=computed(()=>['serve','listen'].includes(task.value?.action??''))
const diagnosticText=computed(()=>{
 if(!cumulative.value?.has_samples&&['ping','tcping','trace'].includes(resultTool.value))return t('尚无样本','No samples yet')
 if(resultTool.value==='tcping')return cumulative.value?.success?t('至少一次 TCP 建连成功','At least one TCP connection succeeded'):t('目标端口未连接成功','No connection to the target port succeeded')
 if(resultTool.value==='ping')return cumulative.value?.success?t('至少一次收到 Echo Reply','At least one Echo Reply received'):t('未收到 Echo Reply','No Echo Reply received')
 if(resultTool.value==='trace')return cumulative.value?.reached?t('已到达目标','Target reached'):t('尚未到达目标','Target not reached')
 if(resultTool.value==='iperf3')return resultService.value?t('服务已停止，已保留测速统计','Server stopped; test statistics retained'):cumulative.value?.success?t('测速完成','Throughput test complete'):t('已保留已传输数据','Transferred data retained')
 return t('请查看证据','Review evidence')
})
const historyFilter=ref('')
const visibleTasks=computed(()=>tasks.value.filter(x=>['running','cancelling'].includes(x.status)||!historyFilter.value||JSON.stringify([x.tool,x.target,x.status]).toLowerCase().includes(historyFilter.value.toLowerCase())).sort((a,b)=>Number(['running','cancelling'].includes(b.status))-Number(['running','cancelling'].includes(a.status))))
const chartWindow=ref(80)
const chartSamples=computed(()=>sampleRows.value.filter(x=>!filter.value||JSON.stringify(x).toLowerCase().includes(filter.value.toLowerCase())).slice(-chartWindow.value))
const chartPosition=(sample:Row)=>{
 const values=chartSamples.value
 const time=(x:Row)=>Number(x.elapsed_ms??x.timestamp_ms??x.seq)
 const first=values[0],last=values[values.length-1]
 const span=first&&last?time(last)-time(first):0
 return span>0?3+94*(time(sample)-time(first))/span:50
}
const chartMax=computed(()=>Math.max(1,...chartSamples.value.filter(x=>x.ok&&x.rtt_ms!=null).map(x=>Number(x.rtt_ms))))
const chartTime=(s:Row)=>s.timestamp_ms?new Date(Number(s.timestamp_ms)).toLocaleTimeString(undefined,{hour:'2-digit',minute:'2-digit',second:'2-digit',fractionalSecondDigits:3} as Intl.DateTimeFormatOptions):`#${s.seq}`
const statusText=(s:string)=>({running:t('运行中','Running'),cancelling:t('正在停止','Stopping'),succeeded:t('已完成','Complete'),failed:t('失败','Failed'),cancelled:t('已停止','Stopped')}[s]||s)
const number=(n:any,suffix='')=>n==null?'—':`${Number(n).toFixed(2)}${suffix}`
// Retain newly accepted tasks until a poll observes them, including when an
// older poll was already in flight while start_task was being submitted.
const pendingTasks=new Map<string,Task>()
function pickTool(id:string){saveConfig();modes.set(selected.value,selected.value==='dns'?form.dns_mode:form.action);selected.value=id;loadConfig(id,modes.get(id)??({ftp:'list',tftp:'get',ntp:'query',iperf3:'client',syslog:'listen',dns:'system'} as Row)[id]??'');active.value='';activeDetail.value=null;rows.value=[];error.value='';expanded.value=false}
function useCheckTarget(tool:string,target:Row){pickTool(tool);form.target=target.host;if(tool==='tcping'&&target.port)form.port=target.port}
function reuseTarget(){if(reusableTarget.value)form.target=reusableTarget.value}
async function rerun(item:Task){try{
 const detail=await invoke<Task>('task_poll',{id:item.id,after:0})
 if(!detail.request)return
 if(detail.tool==='monitor'&&detail.request.monitor)Object.assign(monitorConfig,JSON.parse(JSON.stringify(detail.request.monitor)))
 pickTool(detail.tool);setMode(detail.tool==='dns'?(detail.request.dns_server?'direct':'system'):detail.action)
 Object.assign(form,detail.request,{continuous:detail.request.count===0,dns_mode:detail.request.dns_server?'direct':'system',ftp_remote_path:detail.action==='list'?detail.request.remote_name:'',ftp_password:''})
 if(detail.tool==='check'&&detail.request.target.includes('?[redacted]')){form.target='';error.value=t('URL 参数已脱敏，请重新输入原 URL。','URL parameters were redacted. Re-enter the original URL.');return}
 active.value='';activeDetail.value=null
 error.value=detail.tool==='ftp'?t('已载入请求快照，请重新填写 FTP 凭据后启动。','Request loaded. Re-enter FTP credentials and run.'):''
 }catch(e){error.value=String(e)}
}
async function stopItem(id:string){try{await invoke('cancel_task',{id});await poll()}catch(e){error.value=String(e)}}

function language(){lang.value=en.value?'zh-CN':'en-US';savePreference('language',lang.value)}
async function choose(){try{const kind=service.value?'directory':form.action==='get'?'save':'file';const path=await invoke<string|null>('choose_path',{kind});if(path)form.path=path}catch(e){error.value=String(e)}}
async function browseLogs(){try{await invoke('browse_syslog_logs')}catch(e){error.value=String(e)}}
async function start(){
 if(startDisabled.value)return
 error.value='';loading.value=true
 try{
  const request={source_address:selected.value==='check'?form.source_address:'',tls_ca_file:selected.value==='check'?form.tls_ca_file:'',insecure_tls:form.insecure_tls,include_trace:form.include_trace,tool:selected.value,target:form.target.trim(),port:selected.value==='dns'?0:Number(form.port),count:form.continuous&&['ping','tcping'].includes(selected.value)?0:Number(form.count),timeout_ms:Number(form.timeout_ms),interval_ms:Number(form.interval_ms),payload_size_bytes:selected.value==='ping'?Number(form.payload_size_bytes):32,ipv6:form.ipv6,max_hops:Number(form.max_hops),record_type:form.dns_mode==='system'?'A':form.record_type,dns_server:selected.value==='check'||selected.value==='dns'&&form.dns_mode==='direct'?form.dns_server.trim():'',path:form.path,remote_name:selected.value==='ftp'&&form.action==='list'?form.ftp_remote_path:form.remote_name,action:form.action,listen:form.listen.trim(),allow:form.allow.trim(),writable:form.writable,overwrite:form.overwrite,message:form.message,ntp_stratum:Number(form.ntp_stratum),duration_secs:selected.value==='iperf3'||['ntp','ftp'].includes(selected.value)&&service.value?Number(form.duration_secs):0,iperf_protocol:form.iperf_protocol,iperf_parallel:selected.value==='iperf3'&&form.iperf_protocol==='udp'?1:Number(form.iperf_parallel),iperf_reverse:form.iperf_reverse,iperf_bitrate_mbps:Number(form.iperf_bitrate_mbps),ftp_username:selected.value==='ftp'?form.ftp_username:'anonymous',ftp_password:selected.value==='ftp'?form.ftp_password:''}
  if(selected.value==='dns'&&form.dns_mode==='direct'&&!request.dns_server)throw new Error(t('请输入 DNS 服务器 IP。','Enter a DNS server IP.'))
  if(selected.value==='monitor'){
   Object.assign(request,{monitor:JSON.parse(JSON.stringify(monitorConfig)),target:monitorConfig.targets.map(x=>x.target).join(' · ')})
  }
  const normalized=selected.value==='check'?await invoke<Row>('parse_target',{input:request.target}):null
  if(normalized)request.port=normalized.port??0
  const id=await invoke<string>('start_task',{request})
  const safeRequest={...request,target:normalized?.normalized??request.target}
  form.ftp_password=''
  const started:Task={id,tool:request.tool,action:request.action,target:['serve','listen'].includes(request.action)?`${request.listen}:${request.port}`:safeRequest.target,port:request.port,status:'running',started_at:String(Date.now()).padStart(20,'0'),seq:0,events:[],result:null,request:{...safeRequest,ftp_password:undefined}}
  pendingTasks.set(id,started);tasks.value=[started,...tasks.value.filter(x=>x.id!==id)]
  if(selected.value===request.tool){active.value=id;activeDetail.value=started;rows.value=[];reusableTarget.value=normalized?.host??request.target}await poll()
 }catch(e){error.value=String(e)}finally{loading.value=false}
}
async function stop(){const id=stopTask.value?.id;if(!id)return;try{if(active.value!==id){active.value=id;rows.value=[]}await invoke('cancel_task',{id});await poll()}catch(e){error.value=String(e)}}
async function stopAll(){try{await invoke('stop_all');await poll()}catch(e){error.value=String(e)}}
async function openTask(item:Task){pickTool(item.tool);active.value=item.id;activeDetail.value=null;rows.value=[];await poll()}
let polling=false
async function poll(){if(polling)return;polling=true;try{
 const listed=await invoke<Task[]>('list_tasks')
 for(const [id,pending] of pendingTasks){if(listed.some(x=>x.id===id))pendingTasks.delete(id);else listed.push(pending)}
 const previous=new Map(tasks.value.map(item=>[item.id,item]))
 const summaries=listed.sort((a,b)=>b.started_at.localeCompare(a.started_at)).map(item=>{
  const old=previous.get(item.id)
  return old&&old.status===item.status&&old.seq===item.seq&&old.started_at===item.started_at?old:item
 })
 if(summaries.length!==tasks.value.length||summaries.some((item,i)=>item!==tasks.value[i]))tasks.value=summaries
 const id=active.value
 if(id){
  const cached=activeDetail.value,summary=tasks.value.find(item=>item.id===id)
  if(cached?.tool==='monitor'&&cached.seq===summary?.seq&&cached.status===summary?.status)return
  if(cached?.id===id&&cached.result&&!['running','cancelling'].includes(cached.status)&&(!summary||summary.status===cached.status&&summary.seq===cached.seq))return
  const preview=rows.value,after=cached?.tool==='monitor'?cached.seq:preview.length?Number(preview[preview.length-1]?.cursor||0):0
  const detail=await invoke<Task>('task_poll',{id,after})
  if(active.value!==id||rows.value!==preview)return
  if(detail.events.length){if(after&&detail.events[0].cursor>after+1)error.value=t('部分实时预览已淘汰；全程统计独立保留。','Some preview events expired; cumulative statistics are retained independently.');rows.value=[...preview,...detail.events].slice(-300)}
  activeDetail.value=detail
  if(detail.request?.target&&!['serve','listen'].includes(detail.action))reusableTarget.value=detail.tool==='check'?(detail.result?.data?.target?.host??''):detail.request.target
 }
 }catch(e){error.value=String(e)}finally{polling=false}}
async function exportData(){try{await invoke('export_result',{content:JSON.stringify({task:task.value,preview:rows.value,preview_scope:'bounded_recent_events',view:{filter:filter.value,chart_window:chartWindow.value,chart_scale:'linear_auto_max_ms',statistics_scope:'entire_task'}},null,2)})}catch(e){error.value=String(e)}}
async function exportImage(destination:'file'|'clipboard'='file'){
 if(exportingImage.value||!task.value?.result)return
 exportingImage.value=true;imageSaved.value=false;imageCopied.value=false;imageError.value=''
 try{
  const snapshot=JSON.parse(JSON.stringify(task.value)) as Task
  const source=document.querySelector<HTMLElement>('.results-column')!
  const image=resultImage(source,en.value)
  if(destination==='clipboard'){
   await navigator.clipboard.write([new ClipboardItem({'image/png':image})])
   if(task.value?.id===snapshot.id)imageCopied.value=true
   return
  }
  const blob=await image
  const png=Array.from(new Uint8Array(await blob.arrayBuffer()))
  const filename=`hopkit-${snapshot.tool}-${snapshot.id.slice(0,8)}.png`
  const saved=await invoke<boolean>('export_result_image',{png,filename})
  if(task.value?.id===snapshot.id)imageSaved.value=saved
 }catch(e){imageError.value=String(e)}finally{exportingImage.value=false}
}
async function copy(){try{await navigator.clipboard.writeText(JSON.stringify(task.value?.result||rows.value,null,2));copied.value=true;setTimeout(()=>copied.value=false,1500)}catch(e){error.value=String(e)}}
let timer:ReturnType<typeof setInterval>
onMounted(async()=>{poll();timer=setInterval(poll,350);try{appInfo.value=await invoke('app_info')}catch(e){error.value=String(e)}})
onUnmounted(()=>clearInterval(timer))
</script>

<template>
 <div class="shell">
  <aside class="sidebar">
   <div class="brand"><div class="brand-icon"><span></span><span></span><span></span></div><div><b>HopKit<span class="brand-dot">.</span></b><small>NETWORK TOOLKIT</small></div></div>
   <div class="nav-label">{{ t('网络工具','NETWORK TOOLS') }}</div>
   <nav><button v-for="item in tools" :key="item.id" :class="['nav-item',{active:selected===item.id}]" @click="pickTool(item.id)"><span class="nav-glyph">{{item.glyph}}</span><span><b>{{item.name}}</b><small>{{en?item.sub[1]:item.sub[0]}}</small></span><i v-if="tasks.some(x=>x.tool===item.id&&['running','cancelling'].includes(x.status))" class="live-dot"></i></button></nav>
   <div class="sidebar-bottom"><div class="preview-badge">{{appInfo.os.toUpperCase()}} PREVIEW</div><p>v{{appInfo.version}}</p><small>{{t('本地执行 · 离线可用','Local tools · Offline ready')}}</small></div>
  </aside>
  <div class="workspace">
   <header>
    <div class="breadcrumb">{{t('工作台','Workspace')}} <span>/</span> <strong>{{current.name}}</strong></div>
    <div class="header-actions">
     <span class="platform"><i></i> {{appInfo.platform}}</span>
     <div class="appearance-switch" role="group" :aria-label="t('界面风格','Appearance')">
      <span class="appearance-label">{{t('界面风格','Appearance')}}</span>
      <button type="button" :aria-pressed="appearance==='fresh'" @click="appearance='fresh'">{{t('清新','Fresh')}}</button>
      <button type="button" :aria-pressed="appearance==='readable'" @click="appearance='readable'">{{t('视力友好','Easy to read')}}</button>
     </div>
     <button class="quiet" @click="language">{{en?'中文':'English'}}</button>
    </div>
   </header>
   <main>
    <div class="page-heading"><div><div class="eyebrow">NETWORK DIAGNOSTICS</div><h1>{{current.name}}<span>{{en?current.sub[1]:current.sub[0]}}</span></h1><p>{{en?current.desc[1]:current.desc[0]}}</p></div><span class="local-pill"><span>◈</span>{{t('本机执行','LOCAL EXECUTION')}}</span></div>
    <div class="content-grid">
     <section class="panel config-panel" v-if="selected==='monitor'"><MonitorSetup :en="en" :config="monitorConfig" :disabled="startDisabled" @run="start"/><p v-if="error" class="error" role="alert">{{error}}</p></section>
     <section class="panel config-panel" v-if="selected==='check'"><CheckSetup :en="en" :config="form" :disabled="startDisabled" @run="start"/><div v-if="error" class="error" role="alert">{{error}}</div><button v-if="running" class="stop" @click="stop">■ {{t('停止排障','Stop troubleshooting')}}</button></section>
     <section class="panel config-panel" v-else-if="selected!=='monitor'">
      <div class="panel-title"><h2>{{t('任务配置','Configuration')}}</h2><span>01</span></div>
      <form @submit.prevent="start">
       <template v-if="['ftp','tftp','syslog','ntp','iperf3'].includes(selected)"><label>{{t('操作模式','Operation')}}</label><div class="segmented"><button v-for="a in selected==='ftp'?['list','get','put','serve']:selected==='tftp'?['get','put','serve']:selected==='ntp'?['query','serve']:selected==='iperf3'?['client','serve']:['listen','send']" :key="a" type="button" :class="{chosen:form.action===a}" @click="setMode(a)">{{({client:t('客户端','Client'),list:t('目录','List'),get:t('下载','Download'),put:t('上传','Upload'),query:t('时间测量','Client'),serve:selected==='ntp'?t('时间服务','NTP Server'):selected==='iperf3'?t('服务端','Server'):t('文件服务','Server'),listen:t('接收','Receive'),send:t('发送','Send')} as Row)[a]}}</button></div></template>
       <template v-if="!service"><label for="target">{{t('目标地址','Target address')}}</label><input id="target" v-model="form.target" required maxlength="253" placeholder="IP / hostname" spellcheck="false"><small class="field-help">{{t('支持 IP 地址或域名，使用本机网络路由。','An IP address or hostname, using local network routes.')}}</small><button v-if="reusableTarget" type="button" class="quiet" @click="reuseTarget">{{t('沿用上一任务目标','Use last task target')}} · {{reusableTarget}}</button></template>
       <template v-else><label for="listen">{{t('监听地址','Listen address')}}</label><input id="listen" v-model="form.listen" required placeholder="0.0.0.0" spellcheck="false"><small class="field-help">{{t('默认所有 IPv4 网卡；可指定本机网卡 IP。IPv6 所有网卡请填 ::。','All IPv4 interfaces by default. Use a local interface IP to bind one, or :: for all IPv6 interfaces.')}}</small><label for="allow">{{t('允许来源 CIDR（可选）','Allowed source CIDRs (optional)')}}</label><input id="allow" v-model="form.allow" :placeholder="t('留空允许所有来源；如 192.168.1.0/24','Blank allows all sources, e.g. 192.168.1.0/24')" spellcheck="false"><small class="field-help">{{t('对外监听时留空允许所有来源；可填网段限制来源，多个网段用逗号分隔。','A non-loopback listener accepts all sources when blank. Enter comma-separated CIDRs to restrict access.')}}</small></template>
       <template v-if="['tcping','ntp','ftp','tftp','syslog','iperf3'].includes(selected)"><label for="port">{{t('端口','Port')}}</label><input id="port" v-model.number="form.port" type="number" min="1" max="65535" required></template>
       <small v-if="service&&appInfo.os==='macOS'&&form.port<1024" class="field-help">{{t('macOS 标准端口可能需要管理员权限；普通用户可手动选择 FTP 2121、NTP 1123、TFTP 1069 或 Syslog 1514。','Standard ports may need administrator privileges on macOS. Choose FTP 2121, NTP 1123, TFTP 1069 or Syslog 1514 for ordinary-user tests.')}}</small>
       <template v-if="selected==='iperf3'">
        <template v-if="!service">
         <label for="iperf-protocol">{{t('传输协议','Transport protocol')}}</label><select id="iperf-protocol" v-model="form.iperf_protocol" @change="form.iperf_parallel=1;form.iperf_bitrate_mbps=form.iperf_protocol==='udp'?10:0"><option value="tcp">TCP</option><option value="udp">UDP</option></select>
         <label for="iperf-direction">{{t('测速方向','Test direction')}}</label><select id="iperf-direction" v-model="form.iperf_reverse"><option :value="false">{{t('上传：客户端 → 服务端','Upload: client → server')}}</option><option :value="true">{{t('下载：服务端 → 客户端','Download: server → client')}}</option></select>
         <label for="iperf-parallel">{{t('并行连接数','Parallel streams')}}</label><input id="iperf-parallel" v-model.number="form.iperf_parallel" type="number" min="1" :max="form.iperf_protocol==='udp'?1:16" :disabled="form.iperf_protocol==='udp'" step="1" required>
         <label for="iperf-bitrate">{{t('每流目标速率 / Mbps','Target bitrate per stream / Mbps')}}</label><input id="iperf-bitrate" v-model.number="form.iperf_bitrate_mbps" type="number" :min="form.iperf_protocol==='udp'?1:0" max="10000" step="1" required><small class="field-help">{{t('TCP：0 表示不限速；UDP：1–10000 Mbps，使用单流。','TCP: 0 means unlimited. UDP: 1–10000 Mbps, one stream.')}}</small>
        </template>
        <label for="iperf-duration">{{service?t('服务运行时长 / 秒','Server duration / seconds'):t('测速时长 / 秒','Test duration / seconds')}}</label><input id="iperf-duration" v-model.number="form.duration_secs" type="number" :min="service?0:1" :max="service?86400:3600" step="1" required>
        <small v-if="service" class="field-help">{{t('0 表示持续运行；按客户端请求自动选择 TCP/UDP 和方向，每次处理一个测试。','0 runs until stopped. Protocol and direction follow the client; one test at a time.')}}</small>
        <div class="info-note">{{t('内置 iperf3 测速，无需安装其他程序。对端须运行 iperf3 服务或 HopKit；UDP 测速也需要开放同端口的 TCP 控制连接。不支持 iperf2、认证、双向同时测速或预热。','Built-in iperf3 testing; no other program required. Connect to an iperf3 or HopKit peer. UDP tests also need TCP on the same port. iperf2, authentication, simultaneous bidirectional tests and warm-up are unsupported.')}}</div>
       </template>
       <template v-if="selected==='ntp'&&service"><label for="ntp-stratum">{{t('服务层级 / Stratum','Server stratum')}}</label><input id="ntp-stratum" v-model.number="form.ntp_stratum" type="number" min="1" max="15" step="1" required><label for="duration">{{t('运行时长 / 秒','Duration / seconds')}}</label><input id="duration" v-model.number="form.duration_secs" type="number" min="0" max="4294967295" step="1" required><small class="field-help">{{t('0 表示持续运行，直至手动停止。','0 runs until you stop the server.')}}</small><div class="info-note">{{t('以本机系统时间响应 NTPv3/v4 请求，默认层级 10。精度取决于本机时钟；不校准本机时间，不校验上游同步，不支持认证。','Responds to NTPv3/v4 requests using the system clock, with stratum 10 by default. Accuracy depends on this clock. No clock adjustment, upstream verification or authentication.')}}</div></template>
       <template v-if="selected==='ftp'"><label for="ftp-user">{{t('用户名','Username')}}</label><input id="ftp-user" v-model="form.ftp_username" required maxlength="240" autocomplete="off"><label for="ftp-password">{{t('密码','Password')}}</label><input id="ftp-password" v-model="form.ftp_password" type="password" maxlength="2048" autocomplete="off" :required="service&&form.ftp_username!=='anonymous'"><small class="field-help">{{t('anonymous 为匿名登录；指定其他用户名时，服务器需要密码。','Use anonymous for guest access. A named server account requires a password.')}}</small><template v-if="form.action==='list'"><label for="ftp-directory">{{t('远端目录（可选）','Remote directory (optional)')}}</label><input id="ftp-directory" v-model="form.ftp_remote_path" placeholder="/ or /backups"></template><template v-if="service"><label for="ftp-duration">{{t('运行时长 / 秒','Duration / seconds')}}</label><input id="ftp-duration" v-model.number="form.duration_secs" type="number" min="0" max="4294967295" step="1" required><small class="field-help">{{t('0 表示持续运行，直至手动停止。','0 runs until you stop the server.')}}</small></template><div class="info-note">{{t('普通 FTP：账号和数据明文传输；使用被动模式，不支持 FTPS/SFTP。服务默认只读；启用上传后仅新建，不覆盖。数据端口由系统分配。','Plain FTP sends credentials and data without encryption. Passive mode; no FTPS/SFTP. Server is read-only by default; uploads only create new files. Data ports are assigned by the OS.')}}</div></template>
       <template v-if="selected==='dns'"><label>{{t('查询模式','Query mode')}}</label><div class="segmented"><button type="button" :class="{chosen:form.dns_mode==='system'}" @click="setMode('system')">{{t('系统解析','System')}}</button><button type="button" :class="{chosen:form.dns_mode==='direct'}" @click="setMode('direct')">{{t('指定 DNS','Direct DNS')}}</button></div><template v-if="form.dns_mode==='direct'"><label for="dns-server">{{t('DNS 服务器','DNS server')}}</label><input id="dns-server" v-model="form.dns_server" placeholder="192.168.1.1 or 127.0.0.1:5353" required><label>{{t('记录类型','Record type')}}</label><select v-model="form.record_type"><option v-for="kind in ['A','AAAA','CNAME','MX','TXT','NS','PTR','SOA']">{{kind}}</option></select></template></template>
       <template v-if="!service&&['ping','tcping','ntp'].includes(selected)"><div class="field-pair"><div><label for="count">{{t('探测次数','Count')}}</label><input id="count" v-model.number="form.count" type="number" min="1" :max="selected==='ntp'?20:10000" :disabled="form.continuous&&selected!=='ntp'" required></div><div><label for="interval">{{t('间隔 / ms','Interval / ms')}}</label><input id="interval" v-model.number="form.interval_ms" type="number" min="50" max="60000" step="50" required></div></div><label v-if="selected!=='ntp'" class="check"><input v-model="form.continuous" type="checkbox">{{t('持续探测，直至手动停止','Continue until stopped')}}</label></template>
       <template v-if="selected==='ping'"><label for="payload-size">{{t('测试包大小 / 字节','Packet size / bytes')}}</label><input id="payload-size" v-model.number="form.payload_size_bytes" type="number" min="0" max="65500" step="1" required aria-describedby="payload-help"><small id="payload-help" class="field-help">{{t('0–65500 字节，默认 32；仅 ICMP 数据部分，不含 IP 和 ICMP 头。','0–65500 bytes, default 32. ICMP data only; excludes IP and ICMP headers.')}}</small></template>
       <template v-if="selected==='trace'"><label for="hops">{{t('最大跳数','Maximum hops')}}</label><input id="hops" v-model.number="form.max_hops" type="number" min="1" max="64" required></template>
       <template v-if="!service&&['ping','trace','tcping','dns','ntp','ftp','iperf3'].includes(selected)"><div class="field-pair"><div><label for="timeout">{{t('单次超时 / ms','Timeout / ms')}}</label><input id="timeout" v-model.number="form.timeout_ms" type="number" min="100" max="10000" step="100" required></div><div><label for="family">{{t('地址选择','Address family')}}</label><select id="family" v-model="form.ipv6"><option :value="false">{{t('自动 / IPv4','Auto / IPv4')}}</option><option :value="true">IPv6</option></select></div></div></template>
       <template v-if="selected==='tftp'||selected==='ftp'&&form.action!=='list'||selected==='syslog'&&form.action==='listen'"><label for="path">{{service?t('服务目录','Service directory'):t('本地文件','Local file')}}</label><div class="path-input"><input id="path" v-model="form.path" required :placeholder="appInfo.os==='Windows'?(service?'D:\\hopkit-files':'D:\\config.cfg'):(service?'/Users/yourname/hopkit-files':'/Users/yourname/config.cfg')"><button type="button" class="browse" @click="choose" :aria-label="t('选择路径','Choose path')">···</button></div></template>
       <template v-if="isTransfer"><label for="remote">{{t('远端文件名','Remote filename')}}</label><input id="remote" v-model="form.remote_name" required placeholder="config.cfg"><label v-if="form.action==='get'" class="check"><input v-model="form.overwrite" type="checkbox">{{t('允许覆盖本地目标文件','Allow replacing the local destination')}}</label></template>
       <label v-if="['ftp','tftp'].includes(selected)&&form.action==='serve'" class="check"><input v-model="form.writable" type="checkbox">{{t('允许设备上传新文件（不覆盖）','Allow devices to create files (no overwrite)')}}</label>
       <template v-if="selected==='syslog'&&form.action==='send'"><label for="message">{{t('消息','Message')}}</label><textarea id="message" v-model="form.message" rows="3" required maxlength="60000"></textarea></template>
       <div v-if="selected==='tftp'" class="info-note">{{t('测试版：octet / 512 B / 单传输；文件小于 32 MiB。未启用块号回绕。','Preview: octet / 512 B / one transfer; file limit 33,553,919 bytes. No block rollover.')}}</div>
       <div v-if="selected==='syslog'" class="info-note">{{t('UDP 不保证送达。预览采样显示，完整消息写入 hopkit-syslog/。旧 netbox-syslog/ 日志保留原位。','UDP delivery is not guaranteed. Full messages are saved under hopkit-syslog/. Existing netbox-syslog/ logs stay in place.')}}<button type="button" class="browse" @click="browseLogs">{{t('浏览已有日志…','Browse existing logs…')}}</button></div>
       <div v-if="error" class="error" role="alert">{{error}}</div>
       <div class="run-actions"><button class="primary" type="submit" :disabled="startDisabled"><span>▶</span>{{startLabel}}</button><button v-if="stopTask" class="stop" type="button" :disabled="stopTask.status==='cancelling'" @click="stop">■ {{stopTask.status==='cancelling'?t('正在停止…','Stopping…'):t('停止','Stop')}}</button></div>
      </form>
     </section>
     <div class="results-column">
      <section class="metrics" v-if="['ping','tcping'].includes(resultTool)"><div><small>{{t('已发起探测','ATTEMPTED')}}</small><b>{{stats.sent}}<em>{{t('次','probes')}}</em></b></div><div><small>{{t('已响应','RECEIVED')}}</small><b>{{stats.received}}<em>{{t('次','replies')}}</em></b></div><div><small>{{resultTool==='tcping'?t('平均建连耗时','AVG CONNECT TIME'):t('平均 RTT','AVG RTT')}}</small><b>{{number(stats.avg)}}<em>ms</em></b></div><div><small>{{resultTool==='tcping'?t('建连失败率','CONNECTION FAILURE'):t('丢包率','PACKET LOSS')}}</small><b :class="{warn:stats.loss>0}">{{number(stats.loss)}}<em>%</em></b></div></section>
      <section class="metrics" v-else-if="resultTool==='trace'"><div><small>{{t('探测跳数','HOPS PROBED')}}</small><b>{{cumulative?.hops_probed??0}}</b></div><div><small>{{t('到达目标','TARGET REACHED')}}</small><b>{{cumulative?.reached?t('是','Yes'):t('尚未到达','Not reached')}}</b></div><div><small>{{t('实际耗时 / ms','DURATION / ms')}}</small><b>{{number(cumulative?.duration_ms)}}</b></div><div><small>{{t('超时探测数','TIMEOUT PROBES')}}</small><b>{{cumulative?.timeout_probes??0}}</b></div></section>
      <section class="metrics" v-else-if="resultTool==='iperf3'"><div><small>{{t('本端平均速率','LOCAL AVERAGE')}}</small><b>{{number(cumulative?.average_mbps)}}<em>Mbps</em></b></div><div><small>{{t('已传输数据','TRANSFERRED')}}</small><b>{{number(cumulative?.bytes==null?null:cumulative.bytes/1_000_000)}}<em>MB</em></b></div><div><small>{{t('UDP 抖动','UDP JITTER')}}</small><b>{{number(cumulative?.jitter_ms)}}<em>ms</em></b></div><div><small>{{t('UDP 丢包率','UDP PACKET LOSS')}}</small><b>{{number(cumulative?.loss_percent)}}<em>%</em></b></div></section>
      <section class="metrics" v-else-if="resultTool==='syslog'&&task?.action==='listen'"><div><small>{{t('用户态收到','RECEIVED')}}</small><b>{{latestStats?.received??0}}</b></div><div><small>{{t('已写入','WRITTEN')}}</small><b>{{latestStats?.written??0}}</b></div><div><small>{{t('来源拒绝','DENIED')}}</small><b>{{latestStats?.denied??0}}</b></div><div><small>{{t('传输协议','TRANSPORT')}}</small><b>UDP</b></div></section>
      <section class="metrics" v-else-if="resultTool==='ntp'&&resultService"><div><small>{{t('收到请求','RECEIVED')}}</small><b>{{data?.received??latestStats?.received??0}}</b></div><div><small>{{t('已响应','REPLIED')}}</small><b>{{data?.replied??latestStats?.replied??0}}</b></div><div><small>{{t('来源拒绝','DENIED')}}</small><b>{{data?.denied??latestStats?.denied??0}}</b></div><div><small>{{t('无效报文','INVALID')}}</small><b>{{data?.invalid??latestStats?.invalid??0}}</b></div></section>
      <section class="metrics" v-else-if="resultTool==='ftp'&&resultService"><div><small>{{t('连接数','CONNECTIONS')}}</small><b>{{data?.connections??latestStats?.connections??0}}</b></div><div><small>{{t('在线连接','ACTIVE')}}</small><b>{{data?.active??latestStats?.active??0}}</b></div><div><small>{{t('已传输','TRANSFERS')}}</small><b>{{data?.transfers??latestStats?.transfers??0}}</b></div><div><small>{{t('传输字节','BYTES')}}</small><b>{{data?.bytes??latestStats?.bytes??0}}</b></div></section>
      <section class="result-panel panel">
       <div class="panel-title"><h2>{{t('实时结果','Live results')}}<span v-if="task" :class="['status',task.status]">{{statusText(task.status)}}</span></h2><div class="result-tools"><button :disabled="!task?.result||exportingImage" @click="exportImage('file')">{{exportingImage?t('正在导出…','Exporting…'):t('导出图片','Export image')}}</button><button :disabled="!task?.result||exportingImage" @click="exportImage('clipboard')">{{imageCopied?t('图片已复制','Image copied'):t('复制图片','Copy image')}}</button><span v-if="imageSaved" role="status">{{t('图片已保存','Image saved')}}</span><button :disabled="!task" @click="copy">{{copied?t('已复制','Copied'):t('复制','Copy')}}</button><button :disabled="!task" @click="exportData">{{t('导出 JSON','Export JSON')}}</button></div></div>
       <div v-if="imageError" class="result-error" role="alert">{{imageError}}</div>
       <div v-if="resultTool==='check'"><CheckResults :data="data" :events="rows" :en="en" @use="useCheckTarget"/><p v-if="task?.result?.error" class="result-error">{{statusText(task.status)}} · {{task.result.error.code}}</p></div>
       <div v-else-if="resultTool==='monitor'"><MonitorResults :en="en" :data="data??latestStats" :task-id="task?.id??''" :running="running" @stop="stop"/><p v-if="task?.result?.error" :class="['result-error',{cancelled:task.status==='cancelled'}]">{{statusText(task.status)}} · {{task.result.error.code}} · {{task.result.error.message}}</p></div>
       <div v-else-if="!active" class="empty-state"><div class="radar"><span></span><i></i><b>⌁</b></div><h3>{{t('准备就绪，开始第一次测试','Ready for your first test')}}</h3><p>{{t('配置左侧参数，结果将实时显示在这里。','Configure a task on the left. Results will appear here.')}}</p><div class="example">{{t('快速开始','QUICK START')}}<code>hk {{resultTool==='ntp'&&resultService?'ntp serve --help':['ftp','tftp','iperf3'].includes(resultTool)?resultTool+' --help':resultTool==='syslog'?'syslog --help':resultTool+' 127.0.0.1'+(resultTool==='tcping'?' 443':'')}}</code></div></div>
       <template v-else>
        <div class="result-meta"><code>{{task?.target}}</code><span v-if="task?.tool==='tcping'&&task.port!=null" class="tcp-port">{{t('TCP 端口','TCP port')}} {{task.port}}</span><span v-if="resultTool==='ping'&&probeSize!=null">{{t('测试包','Packet')}} {{probeSize}} B</span><span>{{active.slice(0,8)}} · {{t('本地任务','local task')}}</span></div>
        <div v-if="['ping','tcping','trace'].includes(resultTool)" class="statistics-note">
         {{t('全程统计（独立累计）','Entire task (cumulative)')}} · {{cumulative?.has_samples?t('已有样本','Samples available'):t('尚无样本','No samples yet')}} · {{t('耗时与失败率仅含已完成探测','Latency and failure rate include completed probes only')}}<span v-if="cumulative?.interrupted_probes"> · {{t('中断的调用','Interrupted calls')}}: {{cumulative.interrupted_probes}}</span>
         <span v-if="resultTool!=='trace'"> · {{resultTool==='tcping'?t('建连最小 / 平均 / 最大 ms','Connect min / avg / max ms'):t('RTT 最小 / 平均 / 最大 ms','RTT min / avg / max ms')}}: {{number(cumulative?.rtt_min_ms)}} / {{number(cumulative?.rtt_avg_ms)}} / {{number(cumulative?.rtt_max_ms)}}</span>
         <span v-if="cumulative?.started_at_ms"> · {{new Date(Number(cumulative.started_at_ms)).toLocaleString()}} → {{cumulative.ended_at_ms?new Date(Number(cumulative.ended_at_ms)).toLocaleString():t('运行中','Running')}}</span><span> · {{t('实际持续 ms','Actual duration ms')}}: {{number(cumulative?.duration_ms)}}</span>
         <p v-if="cumulative?.failure_counts&&Object.keys(cumulative.failure_counts).length">{{t('失败分类','Failures by status')}}: {{Object.entries(cumulative.failure_counts).map(([key,value])=>`${key}: ${value}`).join(' · ')}}</p><p v-if="resultTool==='trace'">{{t('中间设备可能不响应或限制探测；单次超时不能证明链路断开。','Intermediate devices may ignore or limit probes; a timeout does not prove a broken link.')}}</p>
        </div>
        <div v-if="task?.request" class="request-summary"><small>{{t('任务参数','Task parameters')}}</small><code>{{task.tool}} {{task.action}} · {{resultService?task.target:task.request.target}}<template v-if="task.request.port&&!resultService&&!['ping','trace'].includes(resultTool)">:{{task.request.port}}</template><template v-if="resultTool==='iperf3'"> · {{resultService?'TCP / UDP':task.request.iperf_protocol.toUpperCase()}} · {{task.request.duration_secs||t('持续','Continuous')}} s<template v-if="!resultService"> · {{task.request.iperf_parallel}} {{t('流','streams')}} · {{task.request.iperf_reverse?t('下载','Download'):t('上传','Upload')}} · {{task.request.iperf_bitrate_mbps||t('不限速','Unlimited')}} Mbps</template></template><template v-else> · {{t('超时','Timeout')}} {{task.request.timeout_ms}} ms<template v-if="!['trace','dns'].includes(resultTool)"> · {{t('间隔','Interval')}} {{task.request.interval_ms}} ms · {{t('次数','Count')}} {{task.request.count||t('持续','Continuous')}}</template></template></code><button v-if="!resultService" @click="reuseTarget">{{t('沿用此目标','Use this target')}}</button></div>
        <p v-if="task?.result" class="diagnostic-outcome">{{statusText(task.status)}} · {{diagnosticText}}</p>
        <div v-if="resultTool==='iperf3'&&task" class="structured iperf-summary"><p>{{t('发送端 / 接收端平均速率','Sender / receiver average')}}: {{number(cumulative?.sent_mbps)}} / {{number(cumulative?.received_mbps)}} Mbps</p><p>{{t('本端角色','Local role')}}: {{cumulative?.local_role==='sender'?t('发送端','Sender'):cumulative?.local_role==='receiver'?t('接收端','Receiver'):'—'}} · {{t('实测时长','Measured duration')}}: {{number(cumulative?.duration_secs)}} s<span v-if="resultService"> · {{t('已完成测试','Completed tests')}}: {{cumulative?.completed_tests??0}}</span></p><small>{{t('MB 为十进制兆字节；实时图为本端最近采样，统计由后端独立累计。未完成结果交换时，对端速率显示为 —。','MB uses decimal units. The chart shows recent local samples; counters accumulate in the backend. Peer rates appear after result exchange.')}}</small></div>
        <div v-if="resultTool==='iperf3'&&speedRows.length" class="throughput-chart"><div>{{t('本端实时速率 / Mbps · 最近采样','Local throughput / Mbps · recent samples')}} · {{t('纵轴最大值','Axis maximum')}} {{number(speedMax)}}</div><div class="throughput-bars"><span v-for="(sample,i) in speedRows" :key="sample.cursor??i" :style="{height:Math.max(1,Number(sample.mbps)/speedMax*100)+'%'}" :title="`${number(sample.duration_secs)} s · ${number(sample.mbps)} Mbps`"></span></div><small>0 Mbps · {{t('每 0.5 秒采样','Sampled every 0.5 seconds')}}</small></div>
        <div v-if="sampleRows.length&&['ping','tcping'].includes(resultTool)" class="latency-chart">
         <label>{{resultTool==='tcping'?t('TCP 建连耗时 / ms','TCP connection time / ms'):t('ICMP RTT / ms','ICMP RTT / ms')}} · {{t('最近窗口（筛选后）','Recent window (filtered)')}} <select v-model.number="chartWindow"><option :value="20">20</option><option :value="80">80</option><option :value="200">200</option></select></label>
         <div class="chart-grid"><div class="chart-axis"><span>{{number(chartMax)}}</span><span>{{number(chartMax/2)}}</span><span>0 ms</span></div><div class="chart-bars"><div v-for="s in chartSamples" :key="s.seq" class="chart-slot" :style="{left:chartPosition(s)+'%',width:Math.min(6,85/chartSamples.length)+'%'}" :title="`${chartTime(s)} · ${s.ok?number(s.rtt_ms)+' ms':'×'} · ${s.status}`"><i v-if="s.ok&&s.rtt_ms!=null" :style="{height:Math.max(0,Number(s.rtt_ms)/chartMax*100)+'%'}"></i><b v-else class="failure-marker">×</b></div></div></div>
         <div class="chart-times"><span>{{chartSamples[0]?chartTime(chartSamples[0]):'—'}}</span><span>{{t('线性缩放 · × = 探测失败','Linear scale · × = failed probe')}}</span><span>{{chartSamples.length?chartTime(chartSamples[chartSamples.length-1]!):'—'}}</span></div>
        </div>
        <div class="filter-bar"><span>⌕</span><input v-model="filter" :placeholder="t('筛选当前预览…','Filter current preview…')"><small>{{resultTool==='trace'?traceHops.length:rows.length}} {{resultTool==='trace'?t('跳','hops'):t('条预览','events')}}</small></div>
        <div class="table-scroll">
         <table v-if="resultTool==='trace'" class="trace-table" :aria-label="t('路由追踪，每跳三次探测','Traceroute, three probes per hop')"><thead><tr><th scope="col">{{t('跳数','HOP')}}</th><th scope="col">{{t('响应地址','ADDRESS')}}</th><th v-for="n in 3" :key="n" scope="col">{{t('探测 ','PROBE ')}}{{n}}</th></tr></thead><tbody>
          <tr v-for="hop in traceHops" :key="hop.hop"><td class="muted">{{hop.hop}}</td><td class="trace-address"><code v-for="address in hop.addresses" :key="address">{{address}}</code><span v-if="!hop.addresses.length">—</span></td><td v-for="(probe,i) in hop.probes" :key="i">
           <template v-if="probe"><span class="mono trace-time">{{probe.status==='timeout'?'*':number(probe.rtt_ms,' ms')}}</span><span :class="['event-label','trace-status',{bad:!['reply','ttl_expired'].includes(probe.status),good:probe.ok===true}]">{{traceStatus(probe)}}</span><code v-if="hop.addresses.length>1&&probe.rtt_ms!=null&&probe.status!=='timeout'" class="trace-responder">{{probe.address}}</code></template>
           <span v-else class="muted" :title="running?t('等待本次探测','Waiting for this probe'):t('未完成探测','Probe not completed')">{{running?'…':'—'}}</span>
          </td></tr>
          <tr v-if="!traceHops.length"><td colspan="5" class="waiting">{{t('等待响应…','Waiting for results…')}}</td></tr>
         </tbody></table>
         <table v-else><thead><tr><th>#</th><th>{{t('状态 / 事件','STATUS / EVENT')}}</th><th>{{t('响应详情','DETAILS')}}</th><th>{{t('耗时','TIME')}}</th></tr></thead><tbody><tr v-for="(row,i) in displayRows" :key="row.cursor||i"><td class="muted">{{row.seq||i+1}}</td><td><span :class="['event-label',{bad:row.ok===false,good:row.ok===true}]">{{row.status||row.event_type}}</span></td><td class="detail">{{row.event_type==='log'?row.record?.source+' · '+row.record?.text:row.event_type==='transfer'?row.source+' · '+row.action+' '+row.remote_name+' · '+row.bytes+' bytes':row.event_type==='request'?row.source+' · NTPv'+row.version+' · stratum '+row.stratum:row.event_type==='stats'?number(row.mbps)+' Mbps':row.peer||row.address||row.error?.message||row.data&&JSON.stringify(row.data)||row.directory||row.root||row.bytes!=null&&row.bytes+' bytes'||row.listen||'—'}}</td><td class="mono">{{row.rtt_ms!=null?number(row.rtt_ms,' ms'):row.offset_ms!=null?number(row.offset_ms,' ms'):'—'}}</td></tr><tr v-if="!displayRows.length"><td colspan="4" class="waiting">{{t('等待响应…','Waiting for results…')}}</td></tr></tbody></table>
        </div>
        <div v-if="task?.result?.error" :class="['result-error',{cancelled:task.status==='cancelled'}]">{{task.result.error.code}} · {{task.result.error.message}}</div>
        <div v-if="data&&resultTool==='dns'" class="structured"><div v-if="data.addresses"><b>{{t('解析地址','Addresses')}}</b><code v-for="address in data.addresses">{{address}}</code></div><div v-for="(rr,i) in data.records" :key="i"><small>{{rr.section}} · TTL {{rr.ttl}}</small><code>{{rr.name}} → {{rr.value}}</code></div><p v-if="data.rcode!=null">RCODE: {{data.rcode}} · {{data.server}} / {{data.transport}}</p></div>
        <div v-if="resultTool==='ftp'&&data?.entries" class="structured"><p>{{t('目录列表','Directory listing')}} · {{data.entries.length}}</p><code v-for="(entry,i) in data.entries" :key="i">{{entry}}</code><p v-if="data.entries_truncated">{{t('列表已截断','Listing truncated')}}</p></div>
        <div v-if="resultTool==='ftp'&&data?.bytes!=null&&!resultService" class="structured"><p>{{t('传输完成：','Transfer complete: ')}}{{data.bytes}} B · {{data.mode}}</p></div>
        <div v-if="resultTool==='ftp'&&resultService&&task" class="structured"><p>{{t('来源或连接限制拒绝：','Source / connection limit rejections: ')}}{{data?.denied??latestStats?.denied??0}} · {{t('错误：','Errors: ')}}{{data?.errors??latestStats?.errors??0}}</p></div>
        <div v-if="data?.best_sample" class="ntp-result"><small>{{t('最低延迟样本的时钟偏差','OFFSET AT MINIMUM DELAY')}}</small><b>{{number(data.best_sample.offset_ms)}}<em>ms</em></b><p>{{t('正值代表远端比本机更快。未修改系统时间。','A positive value means the remote clock is ahead. System time was not changed.')}}</p></div>
        <div v-if="resultTool==='ntp'&&resultService&&task" class="structured"><p>{{t('本机系统时钟 · UDP · 发送失败：','Local system clock · UDP · Send errors: ')}}{{data?.send_errors??latestStats?.send_errors??0}}</p></div>
        <div v-if="task?.result" class="raw-result"><button @click="expanded=!expanded">{{expanded?'▾':'▸'}} {{t('查看结构化结果','Structured result')}}</button><pre v-if="expanded">{{JSON.stringify(task.result,null,2)}}</pre></div>
       </template>
      </section>
      <section class="panel task-panel"><div class="panel-title"><h2>{{t('任务中心','Task center')}}<span class="task-count">{{countActive}} {{t('运行中','active')}}</span></h2><button :disabled="!countActive" class="quiet danger" @click="stopAll">{{t('停止全部','Stop all')}}</button></div><div v-if="!tasks.length" class="task-empty">{{t('本次会话的任务将在这里显示。','Tasks from this session will appear here.')}}</div><input v-model="historyFilter" :placeholder="t('筛选会话历史（最多 40 项）','Filter session history (up to 40 tasks)')"><div v-for="item in visibleTasks" :key="item.id" class="task-row"><span class="task-icon">{{tools.find(x=>x.id===item.tool)?.glyph}}</span><strong>{{item.tool}}</strong><code>{{item.target}}</code><span :class="['status',item.status]">{{statusText(item.status)}}</span><button @click="openTask(item)">{{t('打开','Open')}}</button><button v-if="['running','cancelling'].includes(item.status)" @click="stopItem(item.id)">{{t('停止','Stop')}}</button><button v-else @click="rerun(item)">{{t('重跑…','Rerun…')}}</button></div></section>
     </div>
    </div>
    <footer><span><i></i>{{t('所有工具在本机运行；未接入 AI 或云端遥测。','Tools run locally. No AI or cloud telemetry is connected.')}}</span><a class="project-link" :href="projectUrl" :title="t('在默认浏览器打开项目，分享与获取更新：','Open the project in your default browser to share and get updates: ')+projectUrl" target="_blank" rel="noopener noreferrer" @click.prevent="openProject">GitHub · {{t('分享与更新','Share & updates')}} ↗</a><span>{{t('首次评估版','First evaluation build')}} <b>α.1</b></span></footer>
    <p v-if="projectLinkError" class="project-link-error" role="alert">{{projectLinkError}}</p>
   </main>
  </div>
 </div>
</template>


