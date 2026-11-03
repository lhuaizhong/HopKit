<script setup lang="ts">
import {ref,watch} from 'vue'
import {invoke} from '@tauri-apps/api/core'
import type {WormholeConfig} from '../wormhole'
const props=defineProps<{en:boolean;config:WormholeConfig;disabled:boolean}>()
const emit=defineEmits<{run:[]}>()
const error=ref(''),file=ref<{name:string;size:string}|null>(null)
const t=(zh:string,en:string)=>props.en?en:zh
watch(()=>props.config.send_path,()=>file.value=null)
async function choose(){try{const path=await invoke<string|null>('choose_path',{kind:'file'});if(path){props.config.send_path=path;file.value=await invoke('wormhole_file_info',{path})}}catch(e){error.value=String(e)}}
</script>
<template>
 <div class="panel-title"><h2>{{t('文件传输','File transfer')}}</h2><span>01</span></div>
 <form class="wormhole-setup" @submit.prevent="emit('run')">
  <div class="segmented"><button v-for="op in (['send','receive'] as const)" :key="op" type="button" :class="{chosen:config.operation===op}" @click="config.operation=op;config.code=''">{{op==='send'?t('发送文件','Send file'):t('接收文件','Receive file')}}</button></div>
  <template v-if="config.operation==='send'"><label for="wormhole-file">{{t('普通文件','Regular file')}}</label><div class="path-input"><input id="wormhole-file" v-model="config.send_path" required><button type="button" class="browse" @click="choose">···</button></div><small v-if="file">{{file.name}} · {{file.size}} B</small></template>
  <template v-else><label for="wormhole-code">{{t('一次性配对码','One-time pairing code')}}</label><input id="wormhole-code" v-model="config.code" required autocomplete="off" spellcheck="false" data-sensitive maxlength="1024"><small>{{t('连接后查看文件信息，再选择保存位置并确认接收。','Review the offer after pairing, then choose a save location and accept.')}}</small></template>
  <details><summary>{{t('高级选项','Advanced options')}}</summary>
   <label for="wormhole-mailbox">Mailbox URL</label><input id="wormhole-mailbox" v-model="config.mailbox_url" required>
   <small v-if="config.mailbox_url.startsWith('ws:')">{{t('WS 信令未启用传输层 TLS。','WS signaling has no transport TLS.')}}</small>
   <label for="wormhole-relay">Transit Relay</label><input id="wormhole-relay" v-model="config.transit_relay" required>
   <label for="wormhole-connection">{{t('连接策略','Connection policy')}}</label><select id="wormhole-connection" v-model="config.connection_mode"><option value="auto">{{t('自动','Automatic')}}</option><option value="direct">{{t('仅直连','Direct only')}}</option><option value="relay">{{t('仅中继','Relay only')}}</option></select>
   <label v-if="config.operation==='send'">{{t('配对码单词数','Code word count')}}<input v-model.number="config.code_length" type="number" min="2" max="8"></label>
   <label>{{t('等待对方 / 秒','Peer wait / seconds')}}<input v-model.number="config.peer_wait_secs" type="number" min="60" max="3600"></label>
   <label v-if="config.operation==='receive'">{{t('接收上限 / 字节（可选）','Receive limit / bytes (optional)')}}<input :value="config.max_receive_bytes??''" inputmode="numeric" pattern="[0-9]*" @input="config.max_receive_bytes=($event.target as HTMLInputElement).value||null"></label>
   <label class="check"><input v-model="config.verify" type="checkbox">{{t('比对完整校验字符串','Compare the full verification string')}}</label>
  </details>
  <p class="info-note">{{t('双方需保持在线；中继传输仍为端到端加密；配对码请仅交给接收人。压缩包按文件保存，不自动解压。','Both peers must stay online. Relayed files remain end-to-end encrypted. Share the code only with your recipient. Archives are saved without extraction.')}}</p>
  <p v-if="error" class="error" role="alert">{{error}}</p>
  <button class="primary" type="submit" :disabled="disabled">{{config.operation==='send'?t('开始发送','Start sending'):t('连接发送方','Connect to sender')}}</button>
 </form>
</template>
