<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

type Row = Record<string, any>
type Task = {id:string;tool:string;target:string;status:string;started_at:string;seq:number;events:Row[];result:Row|null}
const lang=ref(localStorage.getItem('netbox-language')||'zh-CN')
const en=computed(()=>lang.value==='en-US')
const t=(zh:string,english:string)=>en.value?english:zh
const tools=[
 {id:'ping',name:'Ping',glyph:'↗',sub:['连通性测试','ICMP echo'],desc:['检测目标可达性、延迟与丢包。','Measure reachability, latency and packet loss.']},
 {id:'trace',name:'Traceroute',glyph:'⌁',sub:['路由追踪','Route tracing'],desc:['逐跳查看 ICMP 路径，识别响应与超时。','Inspect ICMP responses along the route.']},
 {id:'tcping',name:'TCPing',glyph:'⇄',sub:['TCP 端口测试','TCP connection'],desc:['测量 TCP 建连耗时，区分拒绝与超时。','Measure TCP connections, refusals and timeouts.']},
 {id:'dns',name:'DNS',glyph:'◎',sub:['域名解析','Name resolution'],desc:['使用系统解析器，或查询指定 DNS 服务器。','Resolve with Windows or query a specific DNS server.']},
 {id:'ntp',name:'NTP',glyph:'◷',sub:['时间偏差','Clock offset'],desc:['测量服务器时间偏差，不修改系统时间。','Measure server clock offset without changing system time.']},
 {id:'tftp',name:'TFTP',glyph:'⇅',sub:['文件传输','File transfer'],desc:['标准 octet 文件传输与临时只读服务。','Standard octet transfers and a temporary file server.']},
 {id:'syslog',name:'Syslog',glyph:'≡',sub:['临时日志','Temporary logs'],desc:['接收、保存并预览 UDP 设备日志。','Receive, save and preview device logs over UDP.']}
]
const selected=ref('ping'),active=ref(''),tasks=ref<Task[]>([]),rows=ref<Row[]>([]),error=ref(''),loading=ref(false),filter=ref(''),expanded=ref(false),copied=ref(false)
const form=reactive({target:'127.0.0.1',port:443,count:4,timeout_ms:1000,interval_ms:1000,ipv6:false,max_hops:30,record_type:'A',dns_server:'',dns_mode:'system',path:'',remote_name:'config.cfg',action:'get',listen:'127.0.0.1',allow:'',writable:false,overwrite:false,message:'netbox test',continuous:false})
const current=computed(()=>tools.find(x=>x.id===selected.value)!)
const task=computed(()=>tasks.value.find(x=>x.id===active.value))
const running=computed(()=>task.value?.status==='running'||task.value?.status==='cancelling')
const service=computed(()=>selected.value==='tftp'&&form.action==='serve'||selected.value==='syslog'&&form.action==='listen')
const isTransfer=computed(()=>selected.value==='tftp'&&form.action!=='serve')
const countActive=computed(()=>tasks.value.filter(x=>['running','cancelling'].includes(x.status)).length)
const sampleRows=computed(()=>rows.value.filter(x=>x.event_type==='sample'))
const displayRows=computed(()=>rows.value.filter(x=>!['target','stats'].includes(x.event_type)).filter(x=>!filter.value||JSON.stringify(x).toLowerCase().includes(filter.value.toLowerCase())).slice(-200))
const data=computed(()=>task.value?.result?.data)
const stats=computed(()=>{
 const all=sampleRows.value,ok=all.filter(x=>x.ok),rtt=ok.map(x=>Number(x.rtt_ms)).filter(Number.isFinite)
 return {sent:data.value?.sent??all.length,received:data.value?.received??ok.length,loss:data.value?.loss_percent??(all.length?(all.length-ok.length)*100/all.length:0),avg:data.value?.rtt_avg_ms??(rtt.length?rtt.reduce((a,b)=>a+b,0)/rtt.length:null)}
})
const latestStats=computed(()=>[...rows.value].reverse().find(x=>x.event_type==='stats'))
const statusText=(s:string)=>({running:t('运行中','Running'),cancelling:t('正在停止','Stopping'),succeeded:t('已完成','Complete'),failed:t('失败','Failed'),cancelled:t('已停止','Stopped')}[s]||s)
const number=(n:any,suffix='')=>n==null?'—':`${Number(n).toFixed(2)}${suffix}`
function pickTool(id:string){selected.value=id;active.value='';rows.value=[];error.value='';expanded.value=false;if(id==='syslog'){form.action='listen';form.port=514;form.timeout_ms=1000}else if(id==='tftp'){form.action='get';form.port=69}else if(id==='ntp'){form.port=123;form.timeout_ms=2000;form.interval_ms=2000}else{form.port=443;form.timeout_ms=1000;form.interval_ms=1000}}
function language(){lang.value=en.value?'zh-CN':'en-US';localStorage.setItem('netbox-language',lang.value)}
async function choose(){try{const kind=service.value?'directory':form.action==='get'?'save':'file';const path=await invoke<string|null>('choose_path',{kind});if(path)form.path=path}catch(e){error.value=String(e)}}
async function start(){
 error.value='';loading.value=true
 try{
  const request={tool:selected.value,target:form.target.trim(),port:selected.value==='dns'?0:Number(form.port),count:form.continuous&&['ping','tcping'].includes(selected.value)?0:Number(form.count),timeout_ms:Number(form.timeout_ms),interval_ms:Number(form.interval_ms),ipv6:form.ipv6,max_hops:Number(form.max_hops),record_type:form.dns_mode==='system'?'A':form.record_type,dns_server:selected.value==='dns'&&form.dns_mode==='direct'?form.dns_server.trim():'',path:form.path,remote_name:form.remote_name,action:form.action,listen:form.listen.trim(),allow:form.allow.trim(),writable:form.writable,overwrite:form.overwrite,message:form.message}
  if(selected.value==='dns'&&form.dns_mode==='direct'&&!request.dns_server)throw new Error(t('请输入 DNS 服务器 IP。','Enter a DNS server IP.'))
  active.value=await invoke<string>('start_task',{request});rows.value=[];await poll()
 }catch(e){error.value=String(e)}finally{loading.value=false}
}
async function stop(){try{await invoke('cancel_task',{id:active.value});await poll()}catch(e){error.value=String(e)}}
async function stopAll(){try{await invoke('stop_all');await poll()}catch(e){error.value=String(e)}}
async function openTask(item:Task){selected.value=item.tool;active.value=item.id;rows.value=[];await poll()}
let polling=false
async function poll(){if(polling)return;polling=true;try{
 tasks.value=(await invoke<Task[]>('list_tasks')).sort((a,b)=>b.started_at.localeCompare(a.started_at))
 if(active.value){const after=rows.value.length?Number(rows.value[rows.value.length-1]?.cursor||0):0;const detail=await invoke<Task>('task_poll',{id:active.value,after});if(detail.events.length){if(after&&detail.events[0].cursor>after+1)error.value=t('部分实时预览已淘汰；接收文件仍独立保存。','Some preview events expired; service files are independent.');rows.value=[...rows.value,...detail.events].slice(-300)}const i=tasks.value.findIndex(x=>x.id===active.value);if(i>=0)tasks.value[i]=detail}
 }catch(e){error.value=String(e)}finally{polling=false}}
async function exportData(){try{await invoke('export_result',{content:JSON.stringify({task:task.value,preview:rows.value},null,2)})}catch(e){error.value=String(e)}}
async function copy(){try{await navigator.clipboard.writeText(JSON.stringify(task.value?.result||rows.value,null,2));copied.value=true;setTimeout(()=>copied.value=false,1500)}catch(e){error.value=String(e)}}
let timer:ReturnType<typeof setInterval>
onMounted(()=>{poll();timer=setInterval(poll,350)})
onUnmounted(()=>clearInterval(timer))
</script>

<template>
 <div class="shell">
  <aside class="sidebar">
   <div class="brand"><div class="brand-icon"><span></span><span></span><span></span></div><div><b>netbox<span class="brand-dot">.</span></b><small>NETWORK TOOLKIT</small></div></div>
   <div class="nav-label">{{ t('网络工具','NETWORK TOOLS') }}</div>
   <nav><button v-for="item in tools" :key="item.id" :class="['nav-item',{active:selected===item.id}]" @click="pickTool(item.id)"><span class="nav-glyph">{{item.glyph}}</span><span><b>{{item.name}}</b><small>{{en?item.sub[1]:item.sub[0]}}</small></span><i v-if="tasks.some(x=>x.tool===item.id&&['running','cancelling'].includes(x.status))" class="live-dot"></i></button></nav>
   <div class="sidebar-bottom"><div class="preview-badge">WINDOWS PREVIEW</div><p>v0.1.0-alpha.1</p><small>{{t('本地执行 · 离线可用','Local tools · Offline ready')}}</small></div>
  </aside>
  <div class="workspace">
   <header><div class="breadcrumb">{{t('工作台','Workspace')}} <span>/</span> <strong>{{current.name}}</strong></div><div class="header-actions"><span class="platform"><i></i> Windows x64</span><button class="quiet" @click="language">{{en?'中文':'English'}}</button></div></header>
   <main>
    <div class="page-heading"><div><div class="eyebrow">NETWORK DIAGNOSTICS</div><h1>{{current.name}}<span>{{en?current.sub[1]:current.sub[0]}}</span></h1><p>{{en?current.desc[1]:current.desc[0]}}</p></div><span class="local-pill"><span>◈</span>{{t('本机执行','LOCAL EXECUTION')}}</span></div>
    <div class="content-grid">
     <section class="panel config-panel">
      <div class="panel-title"><h2>{{t('任务配置','Configuration')}}</h2><span>01</span></div>
      <form @submit.prevent="start">
       <template v-if="selected==='tftp'||selected==='syslog'"><label>{{t('操作模式','Operation')}}</label><div class="segmented"><button v-for="a in selected==='tftp'?['get','put','serve']:['listen','send']" :key="a" type="button" :class="{chosen:form.action===a}" @click="form.action=a">{{({get:t('下载','Download'),put:t('上传','Upload'),serve:t('文件服务','Server'),listen:t('接收','Receive'),send:t('发送','Send')} as Row)[a]}}</button></div></template>
       <template v-if="!service"><label for="target">{{t('目标地址','Target address')}}</label><input id="target" v-model="form.target" required maxlength="253" placeholder="IP / hostname" spellcheck="false"><small class="field-help">{{t('支持 IP 地址或域名，使用本机网络路由。','An IP address or hostname, using local network routes.')}}</small></template>
       <template v-else><label for="listen">{{t('监听地址','Listen address')}}</label><input id="listen" v-model="form.listen" required placeholder="127.0.0.1" spellcheck="false"><label for="allow">{{t('允许来源 CIDR','Allowed source CIDRs')}}</label><input id="allow" v-model="form.allow" placeholder="192.168.1.0/24" spellcheck="false"><small class="field-help">{{t('默认仅本机。局域网监听需设置来源，可用逗号分隔。','Loopback by default. LAN listeners require source CIDRs, comma-separated.')}}</small></template>
       <template v-if="['tcping','ntp','tftp','syslog'].includes(selected)"><label for="port">{{t('端口','Port')}}</label><input id="port" v-model.number="form.port" type="number" min="1" max="65535" required></template>
       <template v-if="selected==='dns'"><label>{{t('查询模式','Query mode')}}</label><div class="segmented"><button type="button" :class="{chosen:form.dns_mode==='system'}" @click="form.dns_mode='system'">{{t('系统解析','System')}}</button><button type="button" :class="{chosen:form.dns_mode==='direct'}" @click="form.dns_mode='direct'">{{t('指定 DNS','Direct DNS')}}</button></div><template v-if="form.dns_mode==='direct'"><label for="dns-server">{{t('DNS 服务器','DNS server')}}</label><input id="dns-server" v-model="form.dns_server" placeholder="192.168.1.1 or 127.0.0.1:5353" required><label>{{t('记录类型','Record type')}}</label><select v-model="form.record_type"><option v-for="kind in ['A','AAAA','CNAME','MX','TXT','NS','PTR','SOA']">{{kind}}</option></select></template></template>
       <template v-if="['ping','tcping','ntp'].includes(selected)"><div class="field-pair"><div><label for="count">{{t('探测次数','Count')}}</label><input id="count" v-model.number="form.count" type="number" min="1" :max="selected==='ntp'?20:10000" :disabled="form.continuous&&selected!=='ntp'" required></div><div><label for="interval">{{t('间隔 / ms','Interval / ms')}}</label><input id="interval" v-model.number="form.interval_ms" type="number" min="50" max="60000" step="50" required></div></div><label v-if="selected!=='ntp'" class="check"><input v-model="form.continuous" type="checkbox">{{t('持续探测，直至手动停止','Continue until stopped')}}</label></template>
       <template v-if="selected==='trace'"><label for="hops">{{t('最大跳数','Maximum hops')}}</label><input id="hops" v-model.number="form.max_hops" type="number" min="1" max="64" required></template>
       <template v-if="['ping','trace','tcping','dns','ntp'].includes(selected)"><div class="field-pair"><div><label for="timeout">{{t('单次超时 / ms','Timeout / ms')}}</label><input id="timeout" v-model.number="form.timeout_ms" type="number" min="100" max="10000" step="100" required></div><div><label for="family">{{t('地址选择','Address family')}}</label><select id="family" v-model="form.ipv6"><option :value="false">{{t('自动 / IPv4','Auto / IPv4')}}</option><option :value="true">IPv6</option></select></div></div></template>
       <template v-if="selected==='tftp'||selected==='syslog'&&form.action==='listen'"><label for="path">{{service?t('服务目录','Service directory'):t('本地文件','Local file')}}</label><div class="path-input"><input id="path" v-model="form.path" required :placeholder="service?'D:\\netbox-files':'D:\\config.cfg'"><button type="button" class="browse" @click="choose" :aria-label="t('选择路径','Choose path')">···</button></div></template>
       <template v-if="isTransfer"><label for="remote">{{t('远端文件名','Remote filename')}}</label><input id="remote" v-model="form.remote_name" required placeholder="config.cfg"><label v-if="form.action==='get'" class="check"><input v-model="form.overwrite" type="checkbox">{{t('允许覆盖本地目标文件','Allow replacing the local destination')}}</label></template>
       <label v-if="selected==='tftp'&&form.action==='serve'" class="check"><input v-model="form.writable" type="checkbox">{{t('允许设备上传新文件（不覆盖）','Allow devices to create files (no overwrite)')}}</label>
       <template v-if="selected==='syslog'&&form.action==='send'"><label for="message">{{t('消息','Message')}}</label><textarea id="message" v-model="form.message" rows="3" required maxlength="60000"></textarea></template>
       <div v-if="selected==='tftp'" class="info-note">{{t('测试版：octet / 512 B / 单传输；文件小于 32 MiB。未启用块号回绕。','Preview: octet / 512 B / one transfer; file limit 33,553,919 bytes. No block rollover.')}}</div>
       <div v-if="selected==='syslog'" class="info-note">{{t('UDP 不保证送达。预览采样显示，完整消息独立写入 JSONL 文件。','UDP delivery is not guaranteed. The preview is sampled; full messages are saved as JSONL.')}}</div>
       <div v-if="error" class="error" role="alert">{{error}}</div>
       <div class="run-actions"><button class="primary" type="submit" :disabled="loading||countActive>=4"><span>▶</span>{{loading?t('正在启动…','Starting…'):service?t('启动服务','Start service'):t('开始测试','Run test')}}</button><button v-if="running" class="stop" type="button" @click="stop">■ {{t('停止','Stop')}}</button></div>
      </form>
     </section>
     <div class="results-column">
      <section class="metrics" v-if="['ping','trace','tcping'].includes(selected)"><div><small>{{t('已发送','SENT')}}</small><b>{{stats.sent}}<em>{{t('次','probes')}}</em></b></div><div><small>{{t('已响应','RECEIVED')}}</small><b>{{stats.received}}<em>{{t('次','replies')}}</em></b></div><div><small>{{t('平均延迟','AVG LATENCY')}}</small><b>{{number(stats.avg)}}<em>ms</em></b></div><div><small>{{t('丢包率','PACKET LOSS')}}</small><b :class="{warn:stats.loss>0}">{{number(stats.loss)}}<em>%</em></b></div></section>
      <section class="metrics" v-else-if="selected==='syslog'&&form.action==='listen'"><div><small>{{t('用户态收到','RECEIVED')}}</small><b>{{latestStats?.received??0}}</b></div><div><small>{{t('已写入','WRITTEN')}}</small><b>{{latestStats?.written??0}}</b></div><div><small>{{t('来源拒绝','DENIED')}}</small><b>{{latestStats?.denied??0}}</b></div><div><small>{{t('传输协议','TRANSPORT')}}</small><b>UDP</b></div></section>
      <section class="result-panel panel">
       <div class="panel-title"><h2>{{t('实时结果','Live results')}}<span v-if="task" :class="['status',task.status]">{{statusText(task.status)}}</span></h2><div class="result-tools"><button :disabled="!task" @click="copy">{{copied?t('已复制','Copied'):t('复制','Copy')}}</button><button :disabled="!task" @click="exportData">{{t('导出 JSON','Export JSON')}}</button></div></div>
       <div v-if="!active" class="empty-state"><div class="radar"><span></span><i></i><b>⌁</b></div><h3>{{t('准备就绪，开始第一次测试','Ready for your first test')}}</h3><p>{{t('配置左侧参数，结果将实时显示在这里。','Configure a task on the left. Results will appear here.')}}</p><div class="example">{{t('快速开始','QUICK START')}}<code>nb {{selected==='tftp'?'tftp --help':selected==='syslog'?'syslog --help':selected+' 127.0.0.1'+(selected==='tcping'?' 443':'')}}</code></div></div>
       <template v-else>
        <div class="result-meta"><code>{{task?.target}}</code><span>{{active.slice(0,8)}} · {{t('本地任务','local task')}}</span></div>
        <div v-if="sampleRows.length&&['ping','tcping'].includes(selected)" class="sparkline"><div v-for="(s,i) in sampleRows.slice(-80)" :key="i" :class="{lost:!s.ok}" :style="{height:s.ok?Math.max(8,Math.min(64,8+Number(s.rtt_ms)*2))+'px':'8px'}" :title="s.status+' '+s.rtt_ms+'ms'"></div><small>RTT / ms</small></div>
        <div class="filter-bar"><span>⌕</span><input v-model="filter" :placeholder="t('筛选当前预览…','Filter current preview…')"><small>{{rows.length}} {{t('条预览','events')}}</small></div>
        <div class="table-scroll"><table><thead><tr><th>#</th><th>{{t('状态 / 事件','STATUS / EVENT')}}</th><th>{{t('响应详情','DETAILS')}}</th><th>{{t('耗时','TIME')}}</th></tr></thead><tbody><tr v-for="(row,i) in displayRows" :key="row.seq||i"><td class="muted">{{row.hop?row.hop+'.'+((Number(row.seq)-1)%3+1):row.seq||i+1}}</td><td><span :class="['event-label',{bad:row.ok===false,good:row.ok===true}]">{{row.status||row.event_type}}</span></td><td class="detail">{{row.event_type==='log'?row.record?.source+' · '+row.record?.text:row.address||row.error?.message||row.data&&JSON.stringify(row.data)||row.directory||row.root||row.bytes!=null&&row.bytes+' bytes'||row.listen||'—'}}</td><td class="mono">{{row.rtt_ms!=null?number(row.rtt_ms,' ms'):row.offset_ms!=null?number(row.offset_ms,' ms'):'—'}}</td></tr><tr v-if="!displayRows.length"><td colspan="4" class="waiting">{{t('等待响应…','Waiting for results…')}}</td></tr></tbody></table></div>
        <div v-if="task?.result?.error" :class="['result-error',{cancelled:task.status==='cancelled'}]">{{task.result.error.code}} · {{task.result.error.message}}</div>
        <div v-if="data&&selected==='dns'" class="structured"><div v-if="data.addresses"><b>{{t('解析地址','Addresses')}}</b><code v-for="address in data.addresses">{{address}}</code></div><div v-for="(rr,i) in data.records" :key="i"><small>{{rr.section}} · TTL {{rr.ttl}}</small><code>{{rr.name}} → {{rr.value}}</code></div><p v-if="data.rcode!=null">RCODE: {{data.rcode}} · {{data.server}} / {{data.transport}}</p></div>
        <div v-if="data?.best_sample" class="ntp-result"><small>{{t('最低延迟样本的时钟偏差','OFFSET AT MINIMUM DELAY')}}</small><b>{{number(data.best_sample.offset_ms)}}<em>ms</em></b><p>{{t('正值代表远端比本机更快。未修改系统时间。','A positive value means the remote clock is ahead. System time was not changed.')}}</p></div>
        <div v-if="task?.result" class="raw-result"><button @click="expanded=!expanded">{{expanded?'▾':'▸'}} {{t('查看结构化结果','Structured result')}}</button><pre v-if="expanded">{{JSON.stringify(task.result,null,2)}}</pre></div>
       </template>
      </section>
      <section class="panel task-panel"><div class="panel-title"><h2>{{t('任务中心','Task center')}}<span class="task-count">{{countActive}} {{t('运行中','active')}}</span></h2><button :disabled="!countActive" class="quiet danger" @click="stopAll">{{t('停止全部','Stop all')}}</button></div><div v-if="!tasks.length" class="task-empty">{{t('本次会话的任务将在这里显示。','Tasks from this session will appear here.')}}</div><button v-for="item in tasks.slice(0,6)" :key="item.id" class="task-row" @click="openTask(item)"><span class="task-icon">{{tools.find(x=>x.id===item.tool)?.glyph}}</span><strong>{{item.tool}}</strong><code>{{item.target}}</code><span :class="['status',item.status]">{{statusText(item.status)}}</span><span>↗</span></button></section>
     </div>
    </div>
    <footer><span><i></i>{{t('所有工具在本机运行；未接入 AI 或云端遥测。','Tools run locally. No AI or cloud telemetry is connected.')}}</span><span>{{t('首次评估版','First evaluation build')}} <b>α.1</b></span></footer>
   </main>
  </div>
 </div>
</template>


