<script setup lang="ts">
import {ref} from 'vue'
import {invoke} from '@tauri-apps/api/core'
import type {SubnetConfig,SubnetResult} from '../tools/m2'
import SubnetResults from './SubnetResults.vue'
const props=defineProps<{en:boolean;config:SubnetConfig}>()
const calculatedConfig=ref<SubnetConfig|null>(null)
const result=ref<SubnetResult|null>(null),error=ref(''),busy=ref(false),notice=ref('')
const t=(zh:string,en:string)=>props.en?en:zh
async function calculate(){busy.value=true;error.value='';notice.value='';try{const config={...props.config};result.value=await invoke('calculate_subnet',{config});calculatedConfig.value=config}catch(e){error.value=String(e)}finally{busy.value=false}}
async function exportData(){try{if(await invoke('export_result',{content:JSON.stringify({schema_version:1,command:'subnet',request:calculatedConfig.value,data:result.value},null,2)}))notice.value=t('已导出','Exported')}catch(e){error.value=String(e)}}
async function copy(){try{await navigator.clipboard.writeText(JSON.stringify({request:calculatedConfig.value,data:result.value},null,2));notice.value=t('已复制','Copied')}catch(e){error.value=String(e)}}
</script>
<template><section class="panel subnet-tool"><div class="panel-title"><h2>{{t('离线子网计算','Offline subnet calculator')}}</h2></div>
<form @submit.prevent="calculate"><label for="subnet-operation">{{t('操作','Operation')}}</label><select id="subnet-operation" v-model="config.operation"><option value="inspect">{{t('网段信息','Inspect')}}</option><option value="contains">{{t('包含地址','Contains address')}}</option><option value="overlap">{{t('网段包含 / 重叠','Containment / overlap')}}</option><option value="split">{{t('拆分子网','Split')}}</option><option value="aggregate">{{t('精确聚合','Exact aggregation')}}</option></select>
<label for="subnet-input">{{t('地址 / 前缀（聚合时空白或逗号分隔）','Address / prefix (space or comma separated for aggregation)')}}</label><textarea id="subnet-input" v-model="config.input" required rows="2" maxlength="16384"></textarea>
<template v-if="['contains','overlap'].includes(config.operation)"><label for="subnet-other">{{t('比较地址或网段','Comparison address or subnet')}}</label><input id="subnet-other" v-model="config.other" required></template>
<template v-if="config.operation==='split'"><label for="subnet-prefix">{{t('拆分后前缀','Child prefix')}}</label><input id="subnet-prefix" v-model.number="config.prefix" type="number" min="0" max="128" required></template>
<p class="info-note">{{t('纯本机计算，不发送网络请求，不占四任务名额。IPv4 支持点分掩码；/31 为点到点双地址，/32 单地址；IPv6 无广播。拆分和聚合最多 1024 项。地址数量使用精确十进制字符串。','Local calculation with no network I/O or network task slot. IPv4 accepts dotted masks; /31 has two point-to-point addresses, /32 one address. IPv6 has no broadcast. Split/aggregation: up to 1024 networks. Counts use exact decimal strings.')}}</p>
<button type="submit" class="primary" :disabled="busy">{{t('计算','Calculate')}}</button></form>
<p v-if="error" class="error" role="alert">{{error}}</p>
<div v-if="result&&calculatedConfig" class="subnet-result structured">
 <h2>{{t('计算结果','Calculation results')}}</h2>
 <div class="subnet-result-actions"><button type="button" @click="copy">{{t('复制表达式与结果','Copy expressions and results')}}</button><button type="button" @click="exportData">{{t('导出 JSON','Export JSON')}}</button><p role="status">{{notice}}</p></div>
 <SubnetResults :en="en" :result="result" :request="calculatedConfig"/>
 <details class="subnet-raw"><summary>{{t('查看原始 JSON','View raw JSON')}}</summary><pre>{{JSON.stringify(result,null,2)}}</pre></details>
</div></section></template>
<style scoped>
.subnet-tool{max-width:950px;overflow-wrap:anywhere}.subnet-result{padding:20px;border-top:1px solid #dce6f4}.subnet-result h2{font-size:17px;margin-bottom:12px}.subnet-result-actions{display:flex;flex-wrap:wrap;align-items:center;gap:10px;margin-bottom:20px}.subnet-result-actions button{padding:8px 12px;border:1px solid #a8b4c3;background:white;color:#174b94}.subnet-raw{margin-top:22px;border-top:1px solid #dce6f4;padding-top:14px}.subnet-raw summary{cursor:pointer;color:#43536a}.subnet-raw pre{max-height:360px;overflow:auto;white-space:pre-wrap;overflow-wrap:anywhere;font-size:12px}:root[data-theme=readable] .subnet-result h2{font-size:20px}:root[data-theme=readable] .subnet-result-actions button{font-size:15px;min-height:42px}
</style>
