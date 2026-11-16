<script setup lang="ts">
import type {SocketConfig} from '../tools/m4'
defineProps<{en:boolean;config:SocketConfig;disabled:boolean;running:boolean}>();defineEmits<{run:[];stop:[]}>()
</script>
<template><form class="m4-setup" @submit.prevent="$emit('run')">
 <h2>{{en?'TCP / UDP Debug':'TCP / UDP 调试助手'}}</h2>
 <label for="socket-protocol">{{en?'Protocol':'协议'}}</label><select id="socket-protocol" v-model="config.protocol" @change="config.mode=config.protocol==='tcp'?'connect':'send'"><option>tcp</option><option>udp</option></select>
 <label for="socket-mode">{{en?'Mode':'模式'}}</label><select id="socket-mode" v-model="config.mode" @change="config.mode==='listen'&&(config.source='')"><option :value="config.protocol==='tcp'?'connect':'send'">{{en?'Client / send':'连接 / 发送'}}</option><option value="listen">{{en?'Listen':'监听'}}</option></select>
 <template v-if="config.mode==='listen'"><label for="socket-listen">{{en?'Listen IP':'监听 IP'}}</label><input id="socket-listen" v-model="config.listen" required><label for="socket-allow">{{en?'Allowed source CIDRs':'允许来源 CIDR'}}</label><input id="socket-allow" v-model="config.allow" placeholder="127.0.0.0/8, ::1/128"><small>{{en?'Blank accepts all sources.':'留空允许所有来源。'}}</small></template>
 <template v-else><label for="socket-target">{{en?'Target':'目标'}}</label><input id="socket-target" v-model="config.target" required><label for="socket-source">{{en?'Source IP (optional)':'源 IP（可选）'}}</label><input id="socket-source" v-model="config.source"></template>
 <label for="socket-port">{{en?'Port':'端口'}}</label><input id="socket-port" v-model.number="config.port" type="number" min="1" max="65535" required>
 <label for="socket-family">{{en?'Address family':'地址族'}}</label><select id="socket-family" v-model="config.family"><option value="auto">Auto</option><option value="ipv4">IPv4</option><option value="ipv6">IPv6</option></select>
 <label for="socket-duration">{{en?'Duration / s (0 until stopped)':'运行秒数（0 持续运行）'}}</label><input id="socket-duration" v-model.number="config.duration_secs" type="number" min="0" max="86400">
 <details><summary>{{en?'Limits and payload recording':'限制与正文记录'}}</summary>
 <label for="socket-timeout">{{en?'Connect / send timeout ms':'连接 / 发送超时 ms'}}</label><input id="socket-timeout" v-model.number="config.timeout_ms" type="number" min="100" max="10000">
 <label for="socket-sessions">{{en?'Maximum TCP sessions':'最多 TCP 会话'}}</label><input id="socket-sessions" v-model.number="config.max_sessions" type="number" min="1" max="16">
 <label for="socket-preview">{{en?'Preview byte budget':'预览字节上限'}}</label><input id="socket-preview" v-model.number="config.preview_bytes" type="number" min="1024" max="1048576">
 <label for="socket-rate">{{en?'Maximum sends / second':'每秒最多发送次数'}}</label><input id="socket-rate" v-model.number="config.rate" type="number" min="1" max="100">
 <label v-if="config.mode==='listen'" class="check"><input v-model="config.echo" type="checkbox">{{en?'Echo (requires source CIDRs)':'回显（须填写来源 CIDR）'}}</label>
 <label for="socket-record">{{en?'Record full payload to new JSONL file (optional)':'完整正文写入新 JSONL 文件（可选）'}}</label><input id="socket-record" v-model="config.record_path" :placeholder="en?'Blank: memory preview only':'留空：仅内存预览'">
 <label for="socket-record-limit">{{en?'File byte limit':'文件字节上限'}}</label><input id="socket-record-limit" v-model.number="config.record_limit_bytes" type="number" min="1024" max="1073741824"><small>{{en?'Contains raw payload. Recording stops at the limit; no overwrite.':'文件含原始正文；达到上限停止记录，不覆盖已有文件。'}}</small>
 </details>
 <button class="primary" :disabled="disabled">{{en?'Start channel':'启动通道'}}</button><button v-if="running" class="stop" type="button" @click="$emit('stop')">{{en?'Stop channel':'停止通道'}}</button>
</form></template>
