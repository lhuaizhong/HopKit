<script setup lang="ts">
import {computed,ref,watch} from 'vue'
import {invoke} from '@tauri-apps/api/core'
import {range,average,type MonitorData,type MonitorEvent} from '../monitor'
const props=defineProps<{en:boolean;data?:Record<string,any>|null;taskId:string;running:boolean}>()
defineEmits<{stop:[]}>()
const t=(zh:string,en:string)=>props.en?en:zh
const model=computed(()=>props.data as MonitorData|undefined)
const from=ref(''),to=ref(''),split=ref(''),note=ref(''),action=ref('note'),error=ref(''),busy=ref(false)
watch(()=>props.taskId,()=>{from.value='';to.value='';split.value='';error.value=''})
const end=computed(()=>to.value?new Date(to.value).getTime():(model.value?.ended_at_ms??Date.now())+1)
const begin=computed(()=>from.value?new Date(from.value).getTime():Math.max(model.value?.started_at_ms??0,end.value-900000))
const pivot=computed(()=>split.value?new Date(split.value).getTime():Math.floor((begin.value+end.value)/2))
const valid=computed(()=>Number.isFinite(begin.value)&&Number.isFinite(end.value)&&begin.value<end.value&&pivot.value>begin.value&&pivot.value<end.value)
const view=computed(()=>model.value&&valid.value?range(model.value,begin.value,end.value):null)
const before=computed(()=>model.value&&valid.value?range(model.value,begin.value,pivot.value):null)
const after=computed(()=>model.value&&valid.value?range(model.value,pivot.value,end.value):null)
const max=computed(()=>Math.max(1,...(view.value?.points.flatMap(b=>b.targets.map(c=>c.max_ms??0))??[])))
const x=(ms:number)=>Math.max(0,Math.min(1000,1000*(ms-begin.value)/(end.value-begin.value)))
const y=(ms:number)=>95-85*ms/max.value
const time=(ms:number|null|undefined)=>ms==null?'—':new Date(ms).toLocaleString(props.en?'en-US':'zh-CN',{hour12:false})
const number=(n:number|null|undefined)=>n==null?'—':n.toFixed(2)
const events=computed(()=>model.value?.records.filter(e=>(e.to_ms??e.timestamp_ms??0)>=begin.value&&(e.from_ms??e.timestamp_ms??0)<end.value).slice(-100)??[])
const gaps=computed(()=>{const list=(model.value?.records.filter(e=>e.kind==='measurement_gap')??[]).map(e=>({from:e.from_ms!,to:e.to_ms!}));if(model.value?.gap_start_ms!=null)list.push({from:model.value.gap_start_ms,to:model.value.ended_at_ms});return list.filter(g=>g.to>=begin.value&&g.from<end.value)})
const failures=(i:number)=>{const a=(model.value?.records.filter(e=>e.kind==='failure_interval'&&e.target_index===i)??[]).map(e=>({from:e.from_ms!,to:e.to_ms!}));const open=model.value?.open_failures[i];if(open!=null)a.push({from:open,to:model.value!.ended_at_ms});return a.filter(e=>e.to>=begin.value&&e.from<end.value)}
const kind=(e:MonitorEvent)=>({failure_interval:e.end_reason==='observed_recovery'?t('失败 → 首次观测恢复','Failure → first observed recovery'):t('失败 → 测量中断，恢复未知','Failure → gap; recovery unknown'),measurement_gap:t('测量中断','Measurement gap'),note:t('备注','Note'),network_change:t('网络变更','Network change'),network_baseline:t('网络基线','Network baseline'),network_query_error:t('网络概览读取失败','Network overview unavailable'),pause:t('暂停','Paused'),resume:t('继续','Resumed')}[e.kind]??e.kind)
async function control(command:string){busy.value=true;error.value='';try{await invoke('monitor_action',{id:props.taskId,action:command,text:['note','network_change'].includes(command)?note.value:''});note.value=''}catch(e){error.value=String(e)}finally{busy.value=false}}
</script>
<template>
 <div class="monitor-results">
  <p v-if="!model">{{t('配置三个目标并开始监测。','Configure three targets and start monitoring.')}}</p>
  <template v-else>
   <div class="monitor-actions" v-if="running"><button :disabled="busy" @click="control(model.paused?'resume':'pause')">{{model.paused?t('继续监测','Resume monitoring'):t('暂停监测','Pause monitoring')}}</button><button class="stop" @click="$emit('stop')">■ {{t('停止监测','Stop monitoring')}}</button></div>
   <p class="statistics-note">{{t('累计探测','Cumulative completed probes')}}: {{model.sample_count}} · {{time(model.started_at_ms)}} → {{time(model.ended_at_ms)}} · {{model.paused?t('已暂停（不产生失败样本）','Paused (no failure samples)'):t('按完成探测统计','Completed probes only')}}</p>
   <div class="monitor-range"><label>{{t('开始（本地时间）','From (local time)')}}<input v-model="from" type="datetime-local" step="1"></label><label>{{t('结束（空为最新）','To (blank = latest)')}}<input v-model="to" type="datetime-local" step="1"></label><label>{{t('对比点（空为中点）','Compare at (blank = midpoint)')}}<input v-model="split" type="datetime-local" step="1"></label><button @click="from='';to='';split=''">{{t('最近 15 分钟','Last 15 minutes')}}</button><button @click="from=new Date(model.started_at_ms-new Date().getTimezoneOffset()*60000).toISOString().slice(0,19);to='';split=''">{{t('全程','Entire run')}}</button></div>
   <p v-if="!valid" class="error">{{t('请设置有效的开始、对比点和结束时间。','Set valid start, comparison and end times.')}}</p>
   <template v-if="view&&before&&after">
    <p class="field-help">{{view.raw?t('最近原始样本；范围 [开始, 结束)。','Recent raw samples; range [from, to).'):t('历史聚合；仅包含完整落入范围的桶，边缘部分不估算。','Historical aggregates; includes only whole buckets inside the range. Partial edges are excluded.')}} · {{t('有效范围','Effective range')}}: {{time(view.from)}} → {{time(view.to)}} · {{t('历史桶宽','Historical bucket width')}}: {{model.bucket_width_ms/1000}} s</p>
    <article v-for="(target,i) in model.config.targets" :key="i" class="monitor-track">
     <h3>{{target.label}} · {{target.target}}{{target.protocol==='tcping'?':'+target.port:''}}</h3>
     <p>{{target.protocol==='tcping'?t('TCP 建连耗时 / ms；建连失败','TCP connect time / ms; connection failures'):t('ICMP RTT / ms；Echo 未响应','ICMP RTT / ms; Echo failures')}} · 0–{{number(max)}} ms</p>
     <svg class="monitor-chart" viewBox="0 0 1000 110" role="img" :aria-label="target.label+' '+t('延迟和失败时间线','latency and failure timeline')">
      <line x1="0" y1="95" x2="1000" y2="95" stroke="currentColor" opacity=".3"/>
      <rect v-for="(g,j) in gaps" :key="'gap'+j" :x="x(g.from)" y="0" :width="Math.max(2,x(g.to)-x(g.from))" height="105" fill="#94a3b8" opacity=".35"><title>{{t('测量中断，不计失败','Measurement gap, excluded from failures')}} · {{time(g.from)}} → {{time(g.to)}}</title></rect>
      <rect v-for="(f,j) in failures(i)" :key="'failure'+j" :x="x(f.from)" y="0" :width="Math.max(2,x(f.to)-x(f.from))" height="105" fill="#ef4444" opacity=".12"><title>{{t('观测失败区段','Observed failure interval')}} · {{time(f.from)}} → {{time(f.to)}}</title></rect>
      <g v-for="(b,j) in view.points" :key="j"><title>{{time(b.from_ms)}} → {{time(b.to_ms)}} · {{t('成功 / 失败','Success / failure')}}: {{b.targets[i].succeeded}} / {{b.targets[i].failed}} · {{t('平均 / 最大 ms','Mean / max ms')}}: {{number(average(b.targets[i]))}} / {{number(b.targets[i].max_ms)}}</title>
       <line v-if="b.targets[i].succeeded" :x1="x(b.from_ms)" :x2="x(b.from_ms)" :y1="y(b.targets[i].min_ms??0)" :y2="y(b.targets[i].max_ms??0)" stroke="#0d9488" stroke-width="2"/>
       <circle v-if="b.targets[i].succeeded" :cx="x(b.from_ms)" :cy="y(average(b.targets[i])??0)" r="3" fill="#0d9488"/>
       <path v-if="b.targets[i].failed" :d="`M${x(b.from_ms)-4},99 l8,8 m-8,0 l8,-8`" stroke="#dc2626" stroke-width="2"/>
      </g>
     </svg>
     <div class="monitor-axis"><span>{{time(begin)}}</span><span>{{time(end)}}</span></div>
    </article>
    <p class="field-help">{{t('绿点为成功耗时均值，竖线为最小至最大；红叉为失败，灰带为测量中断。每目标最多绘制 240 点。区段起止为观测时间，不是精确断网时间。','Green dots: successful mean, lines: min–max; red crosses: failures; gray: measurement gaps. At most 240 points per target. Interval boundaries are observation times, not exact outage times.')}}</p>
    <div class="monitor-table"><table><thead><tr><th>{{t('目标 / 协议','Target / protocol')}}</th><th>{{t('全程成功 / 失败','Cumulative success / failure')}}</th><th>{{t('范围成功 / 失败','Range success / failure')}}</th><th>{{t('前段平均 ms / 失败','Before mean ms / failures')}}</th><th>{{t('后段平均 ms / 失败','After mean ms / failures')}}</th></tr></thead><tbody><tr v-for="(target,i) in model.config.targets" :key="i"><td>{{target.label}} · {{target.protocol==='ping'?'ICMP':'TCP'}}</td><td>{{model.totals[i].succeeded}} / {{model.totals[i].failed}}</td><td>{{view.totals[i].succeeded}} / {{view.totals[i].failed}}</td><td>{{number(average(before.totals[i]))}} / {{before.totals[i].failed}}</td><td>{{number(average(after.totals[i]))}} / {{after.totals[i].failed}}</td></tr></tbody></table></div>
    <p class="field-help">{{t('前段有效范围','Before effective range')}}: {{time(before.from)}} → {{time(before.to)}} · {{t('后段有效范围','After effective range')}}: {{time(after.from)}} → {{time(after.to)}}. {{t('平均值仅含成功样本，至少 1 个；失败分类保留在 JSON。暂不计算 P95 或抖动。','Mean uses successful samples only, minimum 1. Failure categories remain in JSON. P95 and jitter are not computed.')}}</p>
   </template>
   <form v-if="running" class="monitor-note" @submit.prevent="control(action)"><label for="monitor-event-kind">{{t('记录事件','Record event')}}</label><select id="monitor-event-kind" v-model="action"><option value="note">{{t('备注','Note')}}</option><option value="network_change">{{t('网络变更','Network change')}}</option></select><label for="monitor-note">{{t('备注正文（不填写密码或令牌）','Note text (exclude passwords and tokens)')}}</label><input id="monitor-note" v-model="note" maxlength="160" required><button type="submit" :disabled="busy||!note.trim()">{{t('添加事件','Add event')}}</button></form>
   <p v-if="error" class="error" role="alert">{{error}}</p>
   <h3>{{t('区段与事件（当前范围最近 100 条）','Intervals and events (last 100 in range)')}}</h3>
   <p v-if="model.record_count>model.records.length" class="field-help">{{t('较早事件已淘汰；全程统计与聚合独立保留。','Older events have expired; cumulative counters and aggregates remain available.')}}</p>
   <ol class="monitor-events"><li v-for="e in events" :key="e.id"><b>{{kind(e)}}</b> · {{e.target_index!=null?model.config.targets[e.target_index].label:''}} · {{time(e.from_ms??e.timestamp_ms)}}<span v-if="e.to_ms"> → {{time(e.to_ms)}}</span><p v-if="e.text">{{e.text}}</p><small v-if="e.code">{{e.code}}</small><details v-if="e.network"><summary>{{t('本机网络证据','Local network evidence')}}</summary><pre>{{JSON.stringify(e.network,null,2)}}</pre></details></li></ol>
   <p class="statistics-note">{{t('全程计数独立累计；内存保留最近 900 条样本、300 条事件，聚合最多 1024 桶并逐步加宽；本机网络每 30 秒查询一次。休眠、调度延迟和时钟跳变仅能识别为测量中断。','Independent cumulative counters; memory keeps 900 recent samples, 300 events and at most 1024 progressively wider aggregate buckets. Local network is checked every 30 seconds. Sleep, scheduling delays and clock jumps are identified as measurement gaps.')}}</p>
   <p v-if="model.journal_path" class="field-help">{{model.journal_path}}</p>
  </template>
 </div>
</template>
<style scoped>
.monitor-results{padding:16px;overflow-wrap:anywhere}.monitor-actions,.monitor-range{display:flex;gap:10px;flex-wrap:wrap;align-items:end}.monitor-range label{flex:1;min-width:180px}.monitor-range input{width:100%;margin-top:6px}.monitor-track{border-top:1px solid #8884;margin-top:18px;padding-top:8px}.monitor-track h3{margin-bottom:6px}.monitor-track p{font-size:12px}.monitor-chart{width:100%;height:120px;display:block}.monitor-axis{display:flex;justify-content:space-between;font-size:11px;gap:12px}.monitor-table{overflow-x:auto;margin-top:18px}.monitor-table table{min-width:640px}.monitor-note{padding:16px 0;display:grid;gap:8px}.monitor-events{padding-left:20px;max-height:340px;overflow:auto}.monitor-events li{padding:10px 0;border-bottom:1px solid #8883}.monitor-events p{white-space:pre-wrap}.monitor-events pre{max-height:200px;overflow:auto;font-size:11px}.monitor-results .field-help{display:block;margin:10px 0}
.monitor-table table{table-layout:auto;font-size:12px}.monitor-table th{position:static;font-size:11px;letter-spacing:0}.monitor-table th:first-child{width:22%;min-width:125px}.monitor-table th:nth-child(2),.monitor-table th:last-child{width:auto}.monitor-table td{font-size:12px}.monitor-actions button,.monitor-range button,.monitor-note button{padding:6px 10px;border:1px solid #8885;background:#80808008;border-radius:6px}.monitor-results>p,.monitor-results>h3{margin:12px 0}
</style>
