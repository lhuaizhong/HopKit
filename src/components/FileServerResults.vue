<script setup lang="ts">
import {computed, onMounted, ref} from 'vue'
import {invoke} from '@tauri-apps/api/core'
const props=defineProps<{en:boolean;taskId:string;running:boolean;events:Record<string,any>[];data?:Record<string,any>|null;stats?:Record<string,any>|null}>()
const t=(zh:string,en:string)=>props.en?en:zh
const ready=computed(()=>props.events.find(e=>e.event_type==='ready')??props.data??(props.stats?.url?props.stats:null))
const totals=computed(()=>props.data??props.stats??{})
const addresses=ref<string[]>([]),error=ref(''),copied=ref(''),networkLoading=ref(true),addressError=ref(false)
onMounted(async()=>{try{addresses.value=await invoke<string[]>('http_file_addresses')}catch{addressError.value=true}finally{networkLoading.value=false}})
const urls=computed(()=>{
 const info=ready.value
 if(!info?.url)return []
 const url=new URL(info.url),port=url.port?':'+url.port:'',wildcard=info.listen?.startsWith('0.0.0.0:')||info.listen?.startsWith('[::]:')
 const loopback=url.hostname.startsWith('127.')||url.hostname==='[::1]'
 const result:{url:string}[]=wildcard||loopback?[]:[{url:info.url}]
 if(wildcard)for(const ip of addresses.value){
   if(!ip||ip.startsWith('127.')||ip==='::1'||ip.startsWith('fe80:')||ip.startsWith('169.254.')||ip.includes('%'))continue
   const ipv6=ip.includes(':');if(ipv6!==info.listen.startsWith('[::]:'))continue
   const address=`http://${ipv6?'['+ip+']':ip}${port}/`
   if(!result.some(r=>r.url===address))result.push({url:address})
 }
 return result
})
async function open(url:string){try{error.value='';await invoke('open_file_service',{id:props.taskId,url})}catch(e){error.value=String(e)}}
async function copy(url:string){try{await navigator.clipboard.writeText(url);copied.value=url;setTimeout(()=>copied.value='',2000)}catch{error.value=t('请选中并复制访问地址。','Select and copy the URL.')}}
const displayRoot=computed(()=>String(ready.value?.root??'').replace(/^\\\\\?\\/,''))
</script>
<template>
 <div class="file-service-results">
  <div class="file-counts"><div><small>{{t('下载完成','Downloads')}}</small><b>{{totals.downloads??0}}</b></div><div><small>{{t('上传完成','Uploads')}}</small><b>{{totals.uploads??0}}</b></div><div><small>{{t('传输字节','Bytes transferred')}}</small><b>{{totals.bytes??0}}</b></div><div><small>{{t('在线连接','Active connections')}}</small><b>{{totals.active??0}}</b></div></div>
  <template v-if="ready">
   <h3>{{t('浏览器访问地址','Browser access')}}</h3>
   <p>{{running?t('其他设备在浏览器打开以下网卡地址即可访问共享文件。','Open an interface URL below in a browser on another device to access shared files.'):t('服务已停止；重新启动后可访问文件。','Server stopped. Start it again to access files.')}}</p>
   <div v-for="item in urls" :key="item.url" class="file-url"><span>{{t('网卡地址','Interface')}}</span><code>{{item.url}}</code><button :disabled="!running" type="button" @click="open(item.url)">{{t('打开','Open')}}</button><button type="button" @click="copy(item.url)">{{copied===item.url?t('已复制','Copied'):t('复制地址','Copy URL')}}</button></div>
   <p v-if="networkLoading">{{t('正在读取本机网卡地址…','Loading interface addresses…')}}</p>
   <p v-else-if="addressError">{{t('未能读取网卡地址；其他设备可使用本机 IP 加服务端口访问。','Could not list interfaces. Other devices can use this computer’s IP and the service port.')}}</p>
   <p v-else-if="!urls.length">{{t('没有可供其他设备访问的网卡地址，请检查监听地址和网络连接。','No interface address is available for other devices. Check the listen address and network connection.')}}</p>
   <p class="file-root">{{t('共享目录：','Shared directory: ')}}<code>{{displayRoot}}</code></p>
   <p>{{ready.writable?t('允许文件上传 · 同名文件不覆盖','Uploads allowed · existing files are never overwritten'):t('仅供下载 · 文件上传已关闭','Download only · uploads disabled')}}</p>
  </template>
  <p v-else-if="running">{{t('正在启动 HTTP 文件服务…','Starting the HTTP file server…')}}</p>
  <p v-if="error" class="result-error" role="alert">{{error}}</p>
 </div>
</template>
<style scoped>
.file-service-results{padding:16px;font-size:13px;line-height:1.7}.file-service-results h3{font-size:15px;margin:20px 0 4px}.file-service-results p{color:#60758d;overflow-wrap:anywhere}.file-counts{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:12px}.file-counts>div{border:1px solid #dce5ef;border-radius:8px;padding:10px;min-width:0}.file-counts small{color:#60758d}.file-counts b{display:block;font-size:22px;overflow-wrap:anywhere}.file-url{display:flex;align-items:center;gap:8px;flex-wrap:wrap;margin:10px 0;padding:10px;background:#f4f8fe;border-radius:8px}.file-url>span{color:#60758d}.file-url code{flex:1;overflow-wrap:anywhere;min-width:180px}.file-url button{padding:5px 9px;background:#fff;border:1px solid #cddbec;color:#235c9a}.file-root code{display:block;overflow-wrap:anywhere}@media(max-width:1040px){.file-counts{grid-template-columns:repeat(2,minmax(0,1fr))}}
</style>
