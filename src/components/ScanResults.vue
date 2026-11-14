<script setup lang="ts">
import {computed,ref} from 'vue'
import type {ScanResult,ScanRow,ScanEvent} from '../tools/m2'
const props=defineProps<{en:boolean;data:ScanResult|null;events:ScanEvent[]}>()
const emit=defineEmits<{use:[row:ScanRow]}>()
const query=ref(''),page=ref(0)
const rows=computed<ScanRow[]>(()=>props.data?.rows??props.events.filter(e=>e.event_type==='scan_row').map(e=>e.row))
const filtered=computed(()=>rows.value.filter(r=>!query.value||JSON.stringify(r).toLowerCase().includes(query.value.toLowerCase())))
const pages=computed(()=>Math.max(1,Math.ceil(filtered.value.length/100)))
const visible=computed(()=>filtered.value.slice(Math.min(page.value,pages.value-1)*100,(Math.min(page.value,pages.value-1)+1)*100))
const candidates=computed(()=>props.data?.candidates??props.events.filter(e=>e.event_type==='scan_target').flatMap(e=>e.candidates))
const t=(zh:string,en:string)=>props.en?en:zh
</script>
<template><div class="scan-results structured">
 <p v-if="data">{{t('已完成 / 总数','Completed / total')}}: {{data.stats.completed}} / {{data.stats.total??t('未知','unknown')}} · {{t('剩余（含中断）','Remaining (including interrupted)')}}: {{data.stats.remaining??t('未知','unknown')}} · {{t('部分结果','Partial')}}: {{data.partial}}</p>
 <p v-else>{{t('运行时仅显示近期预览，结束后可分页查看全量结果。','Live view shows recent preview; all results are paged when finished.')}}</p>
 <p>{{t('固定候选地址','Fixed candidate addresses')}}: {{candidates.join(', ')||'—'}}</p>
 <p v-if="data?.resolution_complete===false">{{t('在地址解析阶段停止，尚未发起探测；总数与剩余数量未知。','Stopped during resolution before probing; total and remaining counts are unknown.')}}</p>
 <p v-if="data">{{Object.entries(data.stats.counts).map(([state,count])=>`${state}: ${count}`).join(' · ')}}</p>
 <p>{{t('open 开放；refused 拒绝；timeout 超时；unreachable 不可达；local_error 本机/建连错误；cancelled 中断。','open / refused / timeout / unreachable / local_error / cancelled.')}}</p>
 <label for="scan-filter">{{t('筛选结果','Filter results')}}</label><input id="scan-filter" v-model="query" @input="page=0">
 <div class="table-scroll"><table><thead><tr><th>IP : {{t('端口','port')}}</th><th>{{t('状态 / ms','Status / ms')}}</th><th>{{t('实际源地址 / 证据','Actual source / evidence')}}</th><th></th></tr></thead><tbody><tr v-for="r in visible" :key="r.remote_endpoint"><td>{{r.remote_endpoint}}</td><td>{{r.status}} · {{r.elapsed_ms.toFixed(2)}}</td><td>{{r.local_endpoint??'—'}}<small v-if="r.error">{{r.error.code}} · {{r.error.message}}</small></td><td><button v-if="r.status==='open'" @click="emit('use',r)">TCPing →</button></td></tr></tbody></table></div>
 <button :disabled="page===0" @click="page--">←</button> {{Math.min(page+1,pages)}} / {{pages}} <button :disabled="page>=pages-1" @click="page++">→</button>
 <p>{{t('超时不等同 filtered；端口号不代表实际服务身份。','Timeout is not equivalent to filtered; port numbers do not identify actual services.')}}</p>
</div></template>
<style scoped>.scan-results{overflow-wrap:anywhere}.scan-results input{width:100%;padding:8px}td{white-space:normal;overflow-wrap:anywhere}td small{display:block}button{padding:6px}table{min-width:0;width:100%}th:first-child{width:29%}th:nth-child(2){width:22%}th:last-child{width:85px}</style>
