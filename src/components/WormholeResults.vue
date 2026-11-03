<script setup lang="ts">
import {computed,ref,watch,onUnmounted} from 'vue'
import {invoke} from '@tauri-apps/api/core'
import {bytes,percentage,phases} from '../wormhole'
const props=defineProps<{en:boolean;task:any}>()
const emit=defineEmits<{stop:[]}>()
const t=(zh:string,en:string)=>props.en?en:zh
const state=computed(()=>props.task?.result?.data??props.task?.result?.error?.data??props.task?.latest_stats??{})
const privateState=ref<{code?:string;verifier?:string}>({}),path=ref(''),error=ref(''),busy=ref(false),now=ref(Date.now())
const running=computed(()=>props.task?.status==='running')
const phase=computed(()=>phases[state.value.phase]?.[props.en?1:0]??state.value.phase??t('准备就绪','Ready'))
let generation=0
watch(()=>props.task?.id,()=>{generation++;privateState.value={};path.value='';error.value=''})
watch(()=>props.task?.status,()=>{if(!running.value)privateState.value={}})
async function privatePoll(){const id=props.task?.id,g=generation;now.value=Date.now();if(!id||!running.value){privateState.value={};return}try{const v=await invoke<typeof privateState.value>('wormhole_private_state',{id});if(g===generation&&id===props.task?.id&&running.value)privateState.value=v}catch{privateState.value={}}}
const timer=setInterval(privatePoll,350);onUnmounted(()=>{generation++;clearInterval(timer);privateState.value={}})
async function choose(){const p=await invoke<string|null>('choose_path',{kind:'save',suggestedName:state.value.suggested_name??'received-file'});if(p)path.value=p}
async function action(action:string){if(busy.value)return;busy.value=true;error.value='';try{if(action==='accept'&&!path.value){await choose();if(!path.value)return}await invoke('wormhole_action',{id:props.task.id,decisionId:state.value.decision_id,action,savePath:action==='accept'?path.value:null})}catch(e){error.value=String(e)}finally{busy.value=false}}
async function copy(value:string){try{await navigator.clipboard.writeText(value)}catch(e){error.value=String(e)}}
async function openFolder(){try{await invoke('wormhole_open_folder',{id:props.task.id})}catch(e){error.value=String(e)}}
</script>
<template>
 <div class="wormhole-results structured" aria-live="polite">
  <h3>{{phase}}</h3>
  <p v-if="state.file_name" class="wormhole-filename">{{state.file_name}}</p>
  <div v-if="privateState.code" data-sensitive class="wormhole-secret"><label>{{t('一次性配对码','One-time pairing code')}}</label><code>{{privateState.code}}</code><button @click="copy(privateState.code!)">{{t('复制配对码','Copy code')}}</button></div>
  <p v-if="state.deadline_ms&&running">{{t('确认剩余','Decision expires in')}} {{Math.max(0,Math.ceil((state.deadline_ms-now)/1000))}} s</p>
  <div v-if="state.phase==='waiting_verification'&&running" data-sensitive class="wormhole-controls"><code class="wormhole-verifier">{{privateState.verifier}}</code><button :disabled="busy" @click="action('verify_ok')">{{t('已核对一致','Strings match')}}</button><button :disabled="busy" @click="emit('stop')">{{t('取消','Cancel')}}</button></div>
  <div v-if="state.phase==='waiting_acceptance'&&state.operation==='receive'&&running" class="wormhole-controls" data-sensitive>
   <p>{{t('声明大小','Declared size')}}: {{bytes(state.total_bytes)}} ({{state.total_bytes}} B)</p>
   <label>{{t('保存位置（禁止覆盖）','Save location (no overwrite)')}}<div class="path-input"><input v-model="path"><button @click="choose">···</button></div></label>
   <p v-if="state.path_error" class="error">{{state.path_error}}</p><button class="primary" :disabled="busy||!state.decision_id" @click="action('accept')">{{t('接受并保存','Accept and save')}}</button><button :disabled="busy||!state.decision_id" @click="action('reject')">{{t('拒绝','Reject')}}</button>
  </div>
  <progress :value="percentage(state.bytes,state.total_bytes)" max="100" :aria-label="t('传输进度','Transfer progress')"></progress>
  <p>{{bytes(state.bytes)}} / {{bytes(state.total_bytes)}} · {{percentage(state.bytes,state.total_bytes)}}%</p>
  <p>{{t('速度','Speed')}}: {{bytes(String(Math.round(state.speed_bps??0)))}}/s · ETA {{state.eta_secs==null?'—':Math.ceil(state.eta_secs)+' s'}}</p>
  <p v-if="state.connection">{{t('实际路径','Actual connection')}}: {{state.connection==='direct'?t('直连','Direct'):t('中继（端到端加密）','Relay (end-to-end encrypted)')}}</p>
  <p v-if="state.sha256" class="wormhole-hash">SHA-256: {{state.sha256}}</p>
  <div v-if="state.local_saved"><p>{{t('已保存','Saved')}}: {{state.save_path}}</p><button @click="copy(state.save_path)">{{t('复制路径','Copy path')}}</button><button @click="openFolder">{{t('打开所在文件夹','Show folder')}}</button></div>
  <p v-if="state.warning" class="info-note">{{state.warning}}</p>
  <details v-if="state.notice"><summary>{{t('服务器公告','Server notice')}} · {{state.notice_source}}</summary><p>{{state.notice}}</p></details>
  <p v-if="task?.result?.error" class="error">{{task.result.error.code}} · {{task.result.error.message}}</p>
  <p v-if="error" role="alert" class="error">{{error}}</p>
  <button v-if="running" class="stop" @click="emit('stop')">{{t('停止传输','Stop transfer')}}</button>
 </div>
</template>
