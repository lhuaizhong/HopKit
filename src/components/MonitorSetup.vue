<script setup lang="ts">
import {ref} from 'vue'
import {invoke} from '@tauri-apps/api/core'
const props=defineProps<{en:boolean;config:{targets:{label:string;protocol:string;target:string;port:number}[];interval_ms:number;timeout_ms:number;duration_secs:number;log_directory:string};disabled:boolean}>()
defineEmits<{run:[]}>()
const t=(zh:string,en:string)=>props.en?en:zh
const error=ref(''),busy=ref(false)
async function gateway(){busy.value=true;error.value='';try{const n=await invoke<Record<string,any>>('network_overview');const routes=n.default_routes??[];const all=routes.map((r:any)=>r.gateway).filter((g:string)=>g&&g!=='0.0.0.0'&&g!=='::'&&!g.includes('%'));const ipv4=all.filter((g:string)=>!g.includes(':'));const gateways=[...new Set(ipv4.length?ipv4:all)];if(gateways.length!==1)throw new Error(t('默认网关不唯一或无法取得，请核对本机网络后手动输入。','No unique default gateway. Inspect the local network and enter one.'));props.config.targets[0].target=String(gateways[0])}catch(e){error.value=String(e)}finally{busy.value=false}}
async function directory(){try{const path=await invoke<string|null>('choose_path',{kind:'directory'});if(path)props.config.log_directory=path}catch(e){error.value=String(e)}}
</script>
<template>
 <div class="panel-title"><h2>{{t('监测配置','Monitor setup')}}</h2></div>
 <form @submit.prevent="$emit('run')">
  <p class="field-help">{{t('默认系统路由；每轮依次测量三个目标，共用同一时间轴。打开页面不会自动开始探测。','Uses system routes. Targets are sampled sequentially on one timeline. Opening this page does not start probes.')}}</p>
  <fieldset v-for="(target,i) in config.targets" :key="i" class="monitor-target">
   <legend>{{t(['网关','外部目标','业务服务器'][i],'Target '+(i+1))}}</legend>
   <label :for="'monitor-label-'+i">{{t('名称','Label')}}</label><input :id="'monitor-label-'+i" v-model="target.label" maxlength="60" required>
   <label :for="'monitor-target-'+i">{{t('IP 或域名','IP or hostname')}}</label><input :id="'monitor-target-'+i" v-model.trim="target.target" required maxlength="253" spellcheck="false">
   <button v-if="i===0" type="button" class="quiet" :disabled="busy" @click="gateway">{{t('读取默认网关','Read default gateway')}}</button>
   <label :for="'monitor-protocol-'+i">{{t('探测协议','Probe protocol')}}</label><select :id="'monitor-protocol-'+i" v-model="target.protocol"><option value="ping">ICMP Echo</option><option value="tcping">TCP</option></select>
   <template v-if="target.protocol==='tcping'"><label :for="'monitor-port-'+i">{{t('TCP 端口','TCP port')}}</label><input :id="'monitor-port-'+i" v-model.number="target.port" type="number" min="1" max="65535" required></template>
  </fieldset>
  <label for="monitor-interval">{{t('轮次间等待 / ms','Wait between rounds / ms')}}</label><input id="monitor-interval" v-model.number="config.interval_ms" type="number" min="1000" max="60000" required>
  <label for="monitor-timeout">{{t('单目标超时 / ms','Per-target timeout / ms')}}</label><input id="monitor-timeout" v-model.number="config.timeout_ms" type="number" min="100" max="3000" required>
  <label for="monitor-duration">{{t('运行秒数（0 为手动停止）','Duration in seconds (0 until stopped)')}}</label><input id="monitor-duration" v-model.number="config.duration_secs" type="number" min="0" max="604800" required>
  <label for="monitor-directory">{{t('日志目录（可选）','Journal directory (optional)')}}</label><div class="path-input"><input id="monitor-directory" v-model="config.log_directory"><button type="button" @click="directory">…</button></div>
  <small class="field-help">{{t('每次监测保留两份轮转 JSONL，合计最多 16 MiB；不含全部历史原始样本。目录留空则不落盘。','Two rotating JSONL files per monitor, at most 16 MiB total; older raw samples expire. Leave blank for no disk writes.')}}</small>
  <p class="info-note">{{t('共用四任务上限，监测占一个名额、最多一个在途探测。失败区段只说明观测结果；多目标相关性不能直接确定故障归属。','Shares the four-task limit; one slot and one active probe per monitor. Failure intervals describe observations. Correlation cannot establish responsibility.')}}</p>
  <p v-if="error" class="error" role="alert">{{error}}</p><button class="primary" type="submit" :disabled="disabled">▶ {{t('开始监测','Start monitoring')}}</button>
 </form>
</template>
<style scoped>.monitor-target{border:1px solid #8885;border-radius:8px;padding:12px;margin:16px 0;min-width:0}.monitor-target legend{font-weight:650;padding:0 8px}.monitor-target input,.monitor-target select{width:100%}</style>
