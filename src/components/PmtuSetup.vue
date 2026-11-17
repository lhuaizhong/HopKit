<script setup lang="ts">
import type {PmtuConfig} from '../tools/pmtu'
const props=defineProps<{en:boolean;config:PmtuConfig;disabled:boolean;running:boolean}>()
const emit=defineEmits<{run:[];stop:[]}>();const t=(zh:string,en:string)=>props.en?en:zh
function familyChanged(){if(props.config.family==='ipv6'&&props.config.min_ip_bytes<1280)props.config.min_ip_bytes=1280}
</script>
<template><div class="panel-title"><h2>{{t('路径 MTU 探测','Path MTU probing')}}</h2><span>ICMP</span></div><form @submit.prevent="emit('run')">
<label for="pmtu-target">{{t('目标 IP / 域名','Target IP / hostname')}}</label><input id="pmtu-target" v-model="config.target" required maxlength="253" spellcheck="false">
<label for="pmtu-family">{{t('地址族','Address family')}}</label><select id="pmtu-family" v-model="config.family" @change="familyChanged"><option value="ipv4">IPv4</option><option value="ipv6">IPv6</option></select>
<label for="pmtu-source">{{t('本机源 IP（可选）','Local source IP (optional)')}}</label><input id="pmtu-source" v-model="config.source" spellcheck="false">
<div class="field-pair"><div><label for="pmtu-min">{{t('最小 IP 总字节','Minimum IP bytes')}}</label><input id="pmtu-min" v-model.number="config.min_ip_bytes" type="number" :min="config.family==='ipv6'?1280:68" :max="config.max_ip_bytes" required></div><div><label for="pmtu-max">{{t('最大 IP 总字节','Maximum IP bytes')}}</label><input id="pmtu-max" v-model.number="config.max_ip_bytes" type="number" :min="config.min_ip_bytes" max="65535" required></div></div>
<small>{{t('IP 总长度 = IP 头 + ICMP 头 8 + payload。','IP total = IP header + 8 ICMP bytes + payload.')}} IP {{config.family==='ipv6'?40:20}} + ICMP 8 + payload {{config.min_ip_bytes-(config.family==='ipv6'?48:28)}}…{{config.max_ip_bytes-(config.family==='ipv6'?48:28)}}</small>
<label for="pmtu-retries">{{t('额外重试（需两次回包确认成功长度）','Extra retries (two replies confirm a length)')}}</label><input id="pmtu-retries" v-model.number="config.retries" type="number" min="0" max="3" required>
<div class="field-pair"><div><label for="pmtu-timeout">{{t('单探测超时 / ms','Probe timeout / ms')}}</label><input id="pmtu-timeout" v-model.number="config.timeout_ms" type="number" min="100" max="1000" required></div><div><label for="pmtu-total">{{t('总期限 / ms','Total deadline / ms')}}</label><input id="pmtu-total" v-model.number="config.total_timeout_ms" type="number" min="1000" max="120000" required></div></div>
<p>{{t('超时保留为未知，仅明确的过大证据收紧上界。源 IP 不固定接口。Windows IPv6 禁止源端分片尚未验证，返回不支持；macOS/Linux 待原生报文验收。','Timeout remains unknown; only explicit too-big evidence reduces the upper bound. Source IP does not fix an interface. Windows IPv6 source fragmentation control is unverified and returns unsupported; macOS/Linux require native wire acceptance.')}}</p>
<button class="primary" type="submit" :disabled="disabled">{{t('开始 MTU 探测','Start MTU probe')}}</button><button v-if="running" class="stop" type="button" @click="emit('stop')">{{t('停止并保留证据','Stop and retain evidence')}}</button>
</form></template>
