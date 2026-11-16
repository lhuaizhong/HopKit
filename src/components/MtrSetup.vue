<script setup lang="ts">
import type {MtrConfig} from '../tools/mtr'
const props=defineProps<{en:boolean;config:MtrConfig;disabled:boolean;running:boolean}>()
const emit=defineEmits<{run:[];stop:[]}>();const t=(zh:string,en:string)=>props.en?en:zh
</script>
<template><div class="panel-title"><h2>{{t('持续路由诊断','Continuous route diagnosis')}}</h2><span>ICMP</span></div><form @submit.prevent="emit('run')">
<label for="mtr-target">{{t('目标 IP / 域名','Target IP / hostname')}}</label><input id="mtr-target" v-model="config.target" required maxlength="253" spellcheck="false">
<label for="mtr-family">{{t('地址族','Address family')}}</label><select id="mtr-family" v-model="config.family"><option value="auto">Auto</option><option value="ipv4">IPv4</option><option value="ipv6">IPv6</option></select>
<label for="mtr-source">{{t('本机源 IP（可选）','Local source IP (optional)')}}</label><input id="mtr-source" v-model="config.source" spellcheck="false"><small>{{t('源 IP 绑定不固定出口接口；反向 DNS 默认关闭。','Source IP binding does not fix an interface. Reverse DNS is off by default.')}}</small>
<label for="mtr-ptr">{{t('PTR 解析器（可选数字 IP / 端点）','PTR resolver (optional numeric IP / endpoint)')}}</label><input id="mtr-ptr" v-model="config.ptr_server" spellcheck="false"><small>{{t('独立 UDP 反查，使用系统路由，不跟随 ICMP 源绑定；超时 300 ms，无 TCP 回退。','Independent UDP PTR via OS routing; ICMP source binding does not apply. 300 ms timeout; no TCP fallback.')}}</small>
<div class="field-pair"><div><label for="mtr-cycles">{{t('轮数（0 持续）','Cycles (0 continuous)')}}</label><input id="mtr-cycles" v-model.number="config.cycles" type="number" min="0" max="1000000" required></div><div><label for="mtr-duration">{{t('时长 / s（0 不限）','Duration / s (0 unlimited)')}}</label><input id="mtr-duration" v-model.number="config.duration_secs" type="number" min="0" max="86400" required></div></div>
<div class="field-pair"><div><label for="mtr-interval">{{t('轮次间隔 / ms','Cycle interval / ms')}}</label><input id="mtr-interval" v-model.number="config.interval_ms" type="number" min="100" max="60000" required></div><div><label for="mtr-timeout">{{t('单探测超时 / ms','Probe timeout / ms')}}</label><input id="mtr-timeout" v-model.number="config.timeout_ms" type="number" min="100" max="1000" required></div></div>
<div class="field-pair"><div><label for="mtr-hops">{{t('最大跳数','Maximum hops')}}</label><input id="mtr-hops" v-model.number="config.max_hops" type="number" min="1" max="64" required></div><div><label for="mtr-rate">{{t('探测上限 / 秒','Probe limit / second')}}</label><input id="mtr-rate" v-model.number="config.rate" type="number" min="1" max="100" required></div></div>
<p>{{t('逐 TTL 顺序探测；慢轮次不排队、不补造样本。所有扫描、发现及 MTR 任务共享每秒 200 次预算。','Sequential TTL probes; slow rounds never queue or fabricate samples. Scan, discovery and MTR share a 200/s process budget.')}}</p>
<button type="submit" class="primary" :disabled="disabled">{{t('开始 MTR','Start MTR')}}</button><button v-if="running" type="button" class="stop" @click="emit('stop')">{{t('停止并保留统计','Stop and retain statistics')}}</button>
</form></template>
