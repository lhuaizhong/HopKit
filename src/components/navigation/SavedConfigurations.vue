<script setup lang="ts">
import {ref} from 'vue'
import {invoke} from '@tauri-apps/api/core'
import {parseLibrary,type Library} from '../../tools/library'
import type {ToolSnapshot} from '../../tools/config'
const props=defineProps<{en:boolean;library:Library;storageError:string}>()
const emit=defineEmits<{saveProfile:[name:string];saveBookmark:[name:string];restore:[snapshot:ToolSnapshot];target:[target:string];remove:[kind:'profiles'|'bookmarks',id:string];import:[library:Library,complete:(error?:string)=>void]}>()
const name=ref(''),notice=ref(''),failure=ref('')
const t=(zh:string,en:string)=>props.en?en:zh
async function importFile(event:Event){failure.value='';notice.value='';const input=event.target as HTMLInputElement;try{const file=input.files?.[0];if(!file)return;if(file.size>262144)throw Error(t('配置文件超过 256 KiB','Configuration exceeds 256 KiB'));emit('import',parseLibrary(await file.text()),error=>{if(error)failure.value=error;else notice.value=t('已导入配置，尚未启动任何任务。','Imported. No tasks were started.')})}catch(e){failure.value=String(e)}finally{input.value=''}}
async function exportFile(){try{failure.value='';const saved=await invoke<boolean>('export_result',{content:JSON.stringify(parseLibrary(JSON.stringify(props.library)),null,2)});notice.value=saved?t('配置已导出','Configurations exported'):''}catch(e){failure.value=String(e)}}
</script>
<template>
 <details class="saved-configurations"><summary>{{t('收藏目标与现场配置','Saved targets and site profiles')}}</summary>
  <p>{{t('保存参数后可手动载入；凭据、配对码、报文正文和 URL 查询不保存。','Load saved settings manually. Credentials, pairing codes, message bodies and URL queries are excluded.')}}</p>
  <label for="profile-name">{{t('名称 / 现场','Name / site')}}</label><input id="profile-name" v-model="name" maxlength="120" :placeholder="t('例如：办公室交换机','For example: Office switch')">
  <div class="profile-actions"><button type="button" :disabled="!name.trim()" @click="emit('saveProfile',name.trim())">{{t('保存当前配置','Save current settings')}}</button><button type="button" :disabled="!name.trim()" @click="emit('saveBookmark',name.trim())">{{t('收藏当前目标','Save current target')}}</button><button type="button" @click="exportFile">{{t('导出配置','Export configurations')}}</button><label class="import-label">{{t('导入配置','Import configurations')}}<input type="file" accept=".json,application/json" @change="importFile"></label></div>
  <p>{{t('导入将合并同 ID 条目，同工具和模式的参数以导入值为准。','Import merges entries by ID and replaces settings for matching tool modes.')}}</p>
  <ul v-if="library.bookmarks.length"><li v-for="b in library.bookmarks" :key="b.id"><button type="button" @click="emit('target',b.target)">{{b.name}} · {{b.target}}</button><button type="button" :aria-label="t('删除目标 ','Delete target ')+b.name" @click="emit('remove','bookmarks',b.id)">×</button></li></ul>
  <ul v-if="library.profiles.length"><li v-for="p in library.profiles" :key="p.id"><button type="button" @click="emit('restore',p.snapshot)">{{p.name}} · {{p.snapshot.tool}} {{p.snapshot.mode}}</button><button type="button" :aria-label="t('删除配置 ','Delete profile ')+p.name" @click="emit('remove','profiles',p.id)">×</button></li></ul>
  <p v-if="storageError||failure" role="alert">{{storageError||failure}}</p><p v-if="notice" role="status">{{notice}}</p>
 </details>
</template>
<style scoped>
.saved-configurations{margin-bottom:18px;padding:12px 16px;border:1px solid #a8b4c3;border-radius:8px;background:#fff;overflow-wrap:anywhere}.saved-configurations summary{cursor:pointer;color:#174b94;font-weight:600}.saved-configurations p{font-size:12px;color:#43536a;margin:8px 0}.saved-configurations label{display:block;margin:8px 0}.saved-configurations input:not([type=file]){max-width:400px;width:100%;padding:8px;border:1px solid #8b9bb0}.profile-actions{display:flex;flex-wrap:wrap;align-items:center;gap:8px}.profile-actions button,li button{padding:6px 10px;background:#f5f7fb;border:1px solid #a8b4c3;color:#174b94}.import-label{font-size:12px;max-width:100%}.import-label input{display:block;max-width:100%;font-size:12px}ul{padding-left:0;list-style:none}li{display:flex;gap:8px;margin:6px 0}li button:first-child{text-align:left;overflow-wrap:anywhere;min-width:0} [role=alert]{color:#982b24!important}:root[data-theme=readable] p{font-size:14px}
</style>
