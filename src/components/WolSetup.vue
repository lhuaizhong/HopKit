<script setup lang="ts">
import {ref} from 'vue'
import {invoke} from '@tauri-apps/api/core'
import type {WolConfig} from '../tools/m4'
const props=defineProps<{en:boolean;config:WolConfig;disabled:boolean;running:boolean}>();defineEmits<{run:[];stop:[]}>()
const interfaces=ref<{address:string;prefix:number;name:string}[]>([]),error=ref('')
async function suggest(){try{interfaces.value=await invoke('discovery_interfaces')}catch(e){error.value=String(e)}}
function use(row:{address:string;prefix:number}){const f=row.address.split('.').map(Number);if(f.length!==4||row.prefix>30)return;const n=f.reduce((a,b)=>(a*256+b)>>>0,0),mask=row.prefix===0?0:(0xffffffff<<(32-row.prefix))>>>0,b=(n|~mask)>>>0;props.config.destination=[24,16,8,0].map(s=>(b>>>s)&255).join('.');props.config.source=row.address}
</script>
<template><form class="m4-setup" @submit.prevent="$emit('run')"><h2>Wake-on-LAN</h2>
 <label for="wol-mac">MAC</label><input id="wol-mac" v-model="config.mac" required placeholder="AA:BB:CC:DD:EE:02">
 <label for="wol-destination">{{en?'IPv4 destination':'IPv4 目的地址'}}</label><input id="wol-destination" v-model="config.destination" required>
 <button type="button" @click="suggest">{{en?'Suggest interface broadcasts':'读取接口广播建议'}}</button><button type="button" v-for="row in interfaces.filter(r=>r.address.includes('.')&&!r.address.includes(':')&&r.prefix<=30)" :key="row.address" @click="use(row)">{{row.name}} · {{row.address}}/{{row.prefix}}</button>
 <label for="wol-source">{{en?'Source IPv4 (optional)':'源 IPv4（可选）'}}</label><input id="wol-source" v-model="config.source">
 <label for="wol-port">{{en?'UDP port':'UDP 端口'}}</label><input id="wol-port" v-model.number="config.port" type="number" min="1" max="65535" required>
 <label for="wol-count">{{en?'Count':'发送次数'}}</label><input id="wol-count" v-model.number="config.count" type="number" min="1" max="100" required>
 <label for="wol-interval">{{en?'Interval ms':'间隔 ms'}}</label><input id="wol-interval" v-model.number="config.interval_ms" type="number" min="10" max="60000" required>
 <p class="info-note">{{en?'Sending does not prove delivery or wake-up. Enable WOL on the device. Directed broadcasts depend on router policy. Source binding does not force an interface.':'发出报文不证明送达或唤醒；设备须启用 WOL，定向广播受路由器策略影响，源绑定不等同固定接口。'}}</p><p class="error" v-if="error">{{error}}</p>
 <button class="primary" :disabled="disabled">{{en?'Send Magic Packet':'发送唤醒报文'}}</button><button v-if="running" type="button" class="stop" @click="$emit('stop')">{{en?'Stop':'停止'}}</button>
</form></template>
