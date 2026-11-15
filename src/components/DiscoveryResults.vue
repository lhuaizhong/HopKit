<script setup lang="ts">
import {computed,ref} from 'vue'
import {compareDiscoveryIp,discoveryMacs,type DiscoveryDevice,type DiscoveryEvent,type DiscoveryResult} from '../tools/discovery'
const props=defineProps<{en:boolean;data:DiscoveryResult|null;events:DiscoveryEvent[];devices?:DiscoveryDevice[]}>()
defineEmits<{use:[tool:'scan'|'ping'|'tcping'|'check',row:DiscoveryDevice]}>()
const t=(zh:string,en:string)=>props.en?en:zh
const query=ref(''),page=ref(0)
const rows=computed(()=>props.data?.rows??props.devices??[...new Map(props.events.filter(e=>e.event_type==='discovery_device').map(e=>[e.device.address,e.device])).values()])
const cacheOnly=computed(()=>props.data?.stats.planned===0)
const filtered=computed(()=>rows.value.filter(r=>(r.responded||cacheOnly.value&&r.neighbors.some(n=>n.mac))&&(!query.value||JSON.stringify(r).toLowerCase().includes(query.value.toLowerCase()))).sort((a,b)=>compareDiscoveryIp(a.address,b.address)))
const pages=computed(()=>Math.max(1,Math.ceil(filtered.value.length/50)))
const visible=computed(()=>filtered.value.slice(Math.min(page.value,pages.value-1)*50,(Math.min(page.value,pages.value-1)+1)*50))
</script>
<template><div class="discovery-results structured">
<p class="info-note">{{t('主动发现只显示有响应的设备，按 IP 排序；未响应仍记为未知。MAC 标注 ARP 响应或缓存来源，跨网关目标不使用网关 MAC。','Active discovery shows responsive devices sorted by IP; no response remains unknown. MAC evidence identifies ARP replies or cache sources; routed targets never inherit gateway MACs.')}}</p>
<p v-if="data">{{t('候选 / 响应 / 未知','Candidates / responders / unknown')}}: {{data.stats.candidates}} / {{data.stats.responders}} / {{data.stats.unknown}} · {{t('探测完成 / 总数','Probes completed / planned')}}: {{data.stats.completed}} / {{data.stats.planned}} · {{t('部分结果','Partial')}}: {{data.partial}}</p><p v-else>{{t('已发现设备按 IP 保留并排序；未响应事件不会移除已发现设备。','Discovered devices are retained and sorted by IP; later timeouts do not remove discovered devices.')}}</p>
<label for="discovery-filter">{{t('筛选 IP、MAC、名称或状态','Filter IP, MAC, name or state')}}</label><input id="discovery-filter" v-model="query" @input="page=0">
<p v-if="!filtered.length" class="empty-discovery">{{t(cacheOnly?'没有可显示的 MAC 缓存条目。':'尚未发现有响应的设备。',cacheOnly?'No cached MAC entries to display.':'No responsive devices found yet.')}}</p>
<article v-for="row in visible" :key="row.address" class="device"><strong>{{row.address}}</strong> · <span>{{row.responded?t('本次观察到响应','Response observed this run'):t('未知 / 仅缓存或未响应','Unknown / cache only or no response')}}</span>
<p class="device-mac"><strong>MAC</strong>: {{discoveryMacs(row).join(' / ')||t('未知（仅直连设备可通过 ARP 获取）','Unknown (ARP applies only to on-link devices)')}}</p>
<p v-if="row.first_response_at">{{t('首次 / 最近响应','First / last response')}}: {{row.first_response_at}} / {{row.last_response_at}}</p>
<div v-for="(o,i) in row.observations" :key="i">{{o.method.toUpperCase()}}{{o.port?`:${o.port}`:''}} · {{o.status}} · {{o.elapsed_ms.toFixed(2)}} ms · {{o.observed_at}}<small>{{t('源','Source')}}: {{o.local_endpoint??o.source}} <span v-if="o.error">· {{o.error.code}} · {{o.error.message}}</span></small></div>
<details v-if="row.neighbors.length"><summary>{{t('MAC 来源与邻居证据','MAC sources and neighbor evidence')}} ({{row.neighbors.length}})</summary><div v-for="(n,i) in row.neighbors" :key="i">{{n.mac??t('无 MAC','No MAC')}} · {{n.state}} · {{n.interface}} (#{{n.interface_index}})<small>{{n.source}} · {{n.phase}} · {{n.observed_at}}</small></div></details>
<p v-else>{{t('无对应邻居 MAC；跨路由目标不使用网关 MAC。','No matching neighbor MAC; routed targets do not inherit a gateway MAC.')}}</p>
<p v-if="row.name">PTR · {{row.name.names.join(', ')||'—'}}<small>{{row.name.source}} · {{row.name.server}} · {{row.name.observed_at}}<span v-if="row.name.error"> · {{row.name.error.code}} · {{row.name.error.message}}</span></small></p>
<p v-if="row.mac_change_hint" class="error">{{t('同链路 MAC 变化 / 冲突线索：可能为 Proxy ARP、VRRP 或网卡切换，不能直接判定冲突。','MAC change / conflict clue on the same link: Proxy ARP, VRRP or interface changes are possible; this is not a conflict verdict.')}}</p>
<div class="actions"><button v-for="tool in ['scan','ping','tcping','check'] as const" :key="tool" @click="$emit('use',tool,row)">{{({scan:t('端口扫描','Scan'),ping:'Ping',tcping:'TCPing',check:t('网络排障 / Web','Check / Web')} as const)[tool]}} →</button></div>
</article>
<button :disabled="page===0" @click="page--">←</button> {{Math.min(page+1,pages)}} / {{pages}} <button :disabled="page>=pages-1" @click="page++">→</button>
<p v-for="(e,i) in data?.errors" :key="i" class="error">{{e.code}} · {{e.message}}</p>
<details v-if="data"><summary>{{t('能力与限制','Capabilities and limitations')}}</summary><p v-for="line in data.limitations" :key="line">{{line}}</p></details>
</div></template>
<style scoped>.discovery-results{overflow-wrap:anywhere}.discovery-results input{width:100%;padding:8px}.device{border:1px solid var(--line,#d4dce5);padding:12px;margin:10px 0;border-radius:8px}.device p{margin:8px 0}.device small{display:block;font-size:11px;color:#53677e}.actions{display:flex;flex-wrap:wrap;gap:6px;margin-top:10px}button{padding:6px 9px}</style>
