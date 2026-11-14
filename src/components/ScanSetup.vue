<script setup lang="ts">
import {ref,watch} from 'vue'
import type {ScanConfig} from '../tools/m2'
const props=defineProps<{en:boolean;config:ScanConfig;disabled:boolean;running:boolean}>()
defineEmits<{run:[];stop:[]}>()
const t=(zh:string,en:string)=>props.en?en:zh
const targetsText=ref(props.config.targets.join('\n'))
const parseTargets=(value:string)=>value.split(/[\s,]+/).filter(Boolean)
watch(()=>props.config.targets,targets=>{if(JSON.stringify(targets)!==JSON.stringify(parseTargets(targetsText.value)))targetsText.value=targets.join('\n')},{deep:true})
function editTargets(event:Event){targetsText.value=(event.target as HTMLTextAreaElement).value;props.config.targets=parseTargets(targetsText.value)}
</script>
<template>
 <div class="panel-title"><h2>{{t('TCP 扫描配置','TCP scan settings')}}</h2></div>
 <form @submit.prevent="$emit('run')">
  <label for="scan-targets">{{t('目标（逗号或空白分隔）','Targets (commas or whitespace)')}}</label>
  <textarea id="scan-targets" :value="targetsText" @input="editTargets" rows="3" required maxlength="16384"></textarea>
  <small class="field-help">{{t('IP、域名、IPv4 CIDR 或起止地址。IPv6 仅支持明确地址。最多 4096 个候选、65536 个主机×端口组合。','IP, hostname, IPv4 CIDR or start-end. IPv6 requires explicit addresses. Up to 4096 candidates and 65536 host×port pairs.')}}</small>
  <label for="scan-preset">{{t('常见端口用途预设','Common port uses')}}</label><select id="scan-preset" value="" @change="config.ports=($event.target as HTMLSelectElement).value;($event.target as HTMLSelectElement).value=''"><option disabled value="">{{t('选择预设以填入端口…','Choose a preset to fill ports…')}}</option><option value="22,80,443,3389">Web / SSH / RDP</option><option value="80,443,8080,8443">Web</option><option value="22">SSH</option><option value="3389">RDP</option><option value="53">DNS / TCP</option><option value="25,110,143,465,587,993,995">{{t('邮件','Mail')}}</option></select>
  <label for="scan-ports">{{t('端口列表与范围','Ports and ranges')}}</label><input id="scan-ports" v-model="config.ports" required placeholder="22,80,443,8000-8010">
  <small class="field-help">{{t('预设仅表示常见用途，不识别实际服务；不读取 Banner。','Presets describe common uses, not detected services. No banner reads.')}}</small>
  <label for="scan-timeout">{{t('建连超时 / ms','Connect timeout / ms')}}</label><input id="scan-timeout" v-model.number="config.timeout_ms" type="number" min="100" max="10000" required>
  <label for="scan-concurrency">{{t('并发连接','Concurrent connections')}}</label><input id="scan-concurrency" v-model.number="config.concurrency" type="number" min="1" max="128" required>
  <label for="scan-rate">{{t('每秒探测上限','Probes per second')}}</label><input id="scan-rate" v-model.number="config.rate" type="number" min="1" max="100" required>
  <label for="scan-family">{{t('地址类型','Address family')}}</label><select id="scan-family" v-model="config.family"><option value="auto">{{t('自动','Auto')}}</option><option value="ipv4">IPv4</option><option value="ipv6">IPv6</option></select>
  <label for="scan-source">{{t('本机源 IP（可选）','Local source IP (optional)')}}</label><input id="scan-source" v-model.trim="config.source" :placeholder="t('留空，使用系统默认网络地址','Leave blank to use the system default source')" aria-describedby="scan-source-help" autocomplete="off">
  <small id="scan-source-help" class="field-help">{{t('默认由系统路由选择源地址。应用启动时留空；需要固定出口时手动填写本机 IP，或主动载入现场配置。','System routing selects the source by default. Blank on app startup; enter a local IP or load a site profile to bind explicitly.')}}</small>
  <button v-if="config.source" type="button" class="source-reset" @click="config.source=''">{{t('使用默认网络地址','Use default source')}}</button>
  <p class="info-note">{{t('TCP connect；每个域名仅解析一次，候选在结果中展示。超时不证明被过滤，拒绝可能来自中间设备。GUI 共享 128 个扫描连接、200 次/秒预算。','TCP connect; names resolve once and candidates are shown in results. Timeout does not prove filtering; refusal may come from a middlebox. GUI scans share 128 connections and 200 probes/s.')}}</p>
  <button type="submit" class="primary" :disabled="disabled">{{t('开始扫描','Start scan')}}</button><button v-if="running" type="button" class="stop" @click="$emit('stop')">{{t('停止扫描','Stop scan')}}</button>
 </form>
</template>
<style scoped>.source-reset{margin-top:8px;padding:6px 10px;border:1px solid #a8b4c3;background:#fff;color:#174b94}:root[data-theme=readable] .source-reset{min-height:42px;font-size:15px}</style>
