<script setup lang="ts">
import {onMounted,ref,watch,computed} from 'vue'
import {invoke} from '@tauri-apps/api/core'
import type {DiscoveryConfig,DiscoveryInterface} from '../tools/discovery'
const props=defineProps<{en:boolean;config:DiscoveryConfig;disabled:boolean;running:boolean}>()
defineEmits<{run:[];stop:[]}>()
const t=(zh:string,en:string)=>props.en?en:zh
const refreshing=ref(false)
const interfaces=ref<DiscoveryInterface[]>([]),loadError=ref(''),text=ref(props.config.targets.join('\n'))
const parse=(s:string)=>s.split(/[\s,]+/).filter(Boolean)
watch(()=>props.config.targets,v=>{if(JSON.stringify(v)!==JSON.stringify(parse(text.value)))text.value=v.join('\n')},{deep:true})
const selected=computed(()=>interfaces.value.filter(i=>i.name===props.config.interface||String(i.index)===props.config.interface))
async function refresh(){if(refreshing.value)return;refreshing.value=true;try{interfaces.value=await invoke('discovery_interfaces');loadError.value=''}catch(e){loadError.value=String(e)}finally{refreshing.value=false}}
onMounted(refresh)
function edit(e:Event){text.value=(e.target as HTMLTextAreaElement).value;props.config.targets=parse(text.value)}
</script>
<template><div class="panel-title"><h2>{{t('设备发现配置','Device discovery settings')}}</h2></div>
<form @submit.prevent="$emit('run')">
<label for="discovery-interface">{{t('接口（可选）','Interface (optional)')}}</label><select id="discovery-interface" v-model="config.interface"><option value="">{{t('系统路由 / 全部邻居','System route / all neighbors')}}</option><option v-for="i in interfaces.filter((v,n,a)=>a.findIndex(x=>x.index===v.index)===n)" :key="i.index" :value="i.name">{{i.name}} · #{{i.index}}</option><option v-if="config.interface&&!selected.length" :value="config.interface">{{config.interface}} · {{t('当前不可用','unavailable')}}</option></select>
<button type="button" class="quiet" :disabled="refreshing" @click="refresh">{{t('刷新接口','Refresh interfaces')}}</button><p v-if="loadError" class="error">{{loadError}}</p>
<div v-for="i in selected" :key="i.address" class="suggestion"><span>{{i.address}}/{{i.prefix}}</span><button v-if="!i.address.includes(':')" type="button" @click="config.targets=[`${i.address}/${i.prefix}`]">{{t('填入此网段','Use this network')}}</button></div>
<label for="discovery-targets">{{t('明确候选（留空仅读邻居缓存）','Explicit candidates (blank reads cache only)')}}</label><textarea id="discovery-targets" :value="text" @input="edit" rows="3" maxlength="16384" placeholder="192.168.1.0/24, 192.168.2.10-192.168.2.20"></textarea>
<small class="field-help">{{t('IPv4 CIDR、起止地址或多个 IP；最多 4096 个候选。IPv6 仅明确地址，链路本地地址须带数字 scope 或选择接口。填入网段不会启动探测。','IPv4 CIDR, start-end or IPs; at most 4096 candidates. IPv6 requires explicit addresses, with numeric scope or an interface for link-local addresses. Filling a network does not start probes.')}}</small>
<label>{{t('主动探测方法','Active probe methods')}}</label><div class="methods"><label><input v-model="config.methods" value="arp" type="checkbox"> ARP</label><label><input v-model="config.methods" value="icmp" type="checkbox"> ICMP</label><label><input v-model="config.methods" value="tcp" type="checkbox"> TCP</label></div>
<label for="discovery-ports">{{t('TCP 候选端口（最多 8 个）','TCP ports (up to 8)')}}</label><input id="discovery-ports" v-model="config.ports" required>
<label for="discovery-timeout">{{t('单次超时 / ms','Probe timeout / ms')}}</label><input id="discovery-timeout" v-model.number="config.timeout_ms" type="number" min="100" max="1000" required>
<label for="discovery-rate">{{t('每秒探测上限','Probes per second')}}</label><input id="discovery-rate" v-model.number="config.rate" type="number" min="1" max="100" required>
<label for="discovery-concurrency">{{t('并发探测','Concurrent probes')}}</label><input id="discovery-concurrency" v-model.number="config.concurrency" type="number" min="1" max="128" required>
<label for="discovery-source">{{t('本机源 IP（可选）','Local source IP (optional)')}}</label><input id="discovery-source" v-model.trim="config.source" :placeholder="t('留空按所选接口或系统路由','Selected interface or system route when blank')">
<small class="field-help">{{t('接口选择用于源 IP 和 IPv6 scope，不强制固定接口路由；源地址随应用重启清空。扫描和发现共用 128 连接、200 次/秒预算。','Interface selection chooses a source IP and IPv6 scope; it does not force interface routing. Source IP clears on app restart. Scans and discovery share 128 connections and 200 probes/s.')}}</small>
<label class="inline"><input v-model="config.name_lookup" type="checkbox">{{t('响应后查询 PTR 名称（可选）','Look up PTR names after responses (optional)')}}</label>
<template v-if="config.name_lookup"><label for="discovery-dns">{{t('PTR DNS 服务器 IP 或 IP:端口','PTR DNS server IP or IP:port')}}</label><input id="discovery-dns" v-model.trim="config.dns_server" required><small>{{t('最多 4 个并发查询；使用系统路由。名称不代表设备身份。','Up to 4 lookups at once, using system routing. Names do not establish device identity.')}}</small></template>
<p class="info-note">{{t('未响应不代表离线或空闲 IP。邻居缓存不证明本次在线；TCP 拒绝也是响应证据。ARP 只探测 IPv4 直连网段且实际路由不经网关的目标。macOS 需 BPF 权限；无主动 ND 验证和厂商数据库。','No response does not prove offline or an available IP. Cached neighbors do not prove current liveness; TCP refusal is response evidence. ARP probes only directly connected IPv4 targets whose route has no gateway. macOS requires BPF access. No active ND verification or vendor database.')}}</p>
<button class="primary" type="submit" :disabled="disabled">{{config.targets.length?t('开始发现','Start discovery'):t('读取邻居缓存','Read neighbor cache')}}</button><button v-if="running" type="button" class="stop" @click="$emit('stop')">{{t('停止发现','Stop discovery')}}</button>
</form></template>
<style scoped>.methods{display:flex;gap:20px}.methods label,.inline{display:flex;align-items:center;gap:8px}.methods input,.inline input{width:auto}.suggestion{overflow-wrap:anywhere;font-size:12px}.suggestion button{padding:4px;margin:4px}</style>
