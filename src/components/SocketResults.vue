<script setup lang="ts">
import {computed,ref,watch} from 'vue'
import type {SocketState,SocketConfig,SendCommand} from '../tools/m4'
const props=defineProps<{en:boolean;state:SocketState|null;config:SocketConfig;running:boolean;taskId:string}>();const emit=defineEmits<{send:[command:SendCommand]}>()
const session=ref<number|null>(null),destination=ref(''),encoding=ref<'utf8'|'hex'>('utf8'),body=ref(''),newline=ref('none'),count=ref(1),interval=ref(1000),display=ref('hex'),problem=ref(''),include=ref(false),viewCount=ref(50)
const live=computed(()=>props.state?.sessions?.filter(s=>!s.closed)??[])
watch(live,()=>{if(!live.value.some(s=>s.id===session.value))session.value=live.value[0]?.id??null},{immediate:true})
watch(()=>props.taskId,()=>{body.value='';include.value=false;problem.value='';destination.value='';session.value=null})
function send(operation:SendCommand['operation']='send'){
 problem.value='';const hex=body.value.replace(/\s/g,'');if(operation==='send'&&encoding.value==='hex'&&(!/^[0-9a-f]*$/i.test(hex)||hex.length%2)){problem.value=props.en?'Invalid HEX: use complete byte pairs.':'HEX 无效：请填写完整字节对。';return}
 emit('send',{operation,session_id:props.config.protocol==='tcp'?session.value:null,destination:props.config.protocol==='udp'&&props.config.mode==='listen'?destination.value:null,encoding:encoding.value,data:operation==='send'?body.value:'',newline:operation==='send'?newline.value:'none',count:operation==='send'?count.value:1,interval_ms:interval.value})
}
function download(){const data=props.state;if(!data)return;const visible={...data,records:data.records?.slice(-viewCount.value)};const value=include.value?visible:{...visible,records:visible.records?.map(({hex,text,...metadata})=>metadata)};const a=document.createElement('a'),url=URL.createObjectURL(new Blob([JSON.stringify(value,null,2)],{type:'application/json'}));a.href=url;a.download='hopkit-socket-preview.json';a.click();URL.revokeObjectURL(url)}
</script>
<template><div class="socket-results m4-results">
 <p><strong>{{en?'Local endpoint':'本机端点'}}</strong> <code>{{state?.local_endpoint??'…'}}</code> <span v-if="state?.remote_endpoint">→ {{state.remote_endpoint}}</span></p>
 <p v-if="config.mode==='listen'">{{en?'Peers connect to this port using an appropriate interface address. Wildcard addresses are not remote destinations.':'对端须使用本机相应接口地址和此端口连接；通配监听地址不能作为远端目标。'}}</p>
 <p class="info-note">{{en?'TCP displays byte-stream chunks, not messages. HEX preserves raw bytes; UTF-8 previews may split characters. UDP submission does not prove delivery.':'TCP 展示字节流片段，不代表业务消息；HEX 保留原始字节，UTF-8 预览可能截断字符。UDP 发出不代表送达。'}}</p>
 <div class="m4-counters" v-if="state?.stats"><span>{{en?'Sent':'已发送'}} {{state.stats.sent_bytes}} B</span><span>{{en?'Received':'已接收'}} {{state.stats.received_bytes}} B</span><span>{{en?'Send errors':'发送错误'}} {{state.stats.send_errors}}</span><span>{{en?'Preview dropped':'预览淘汰'}} {{state.stats.preview_dropped}}</span><span>{{en?'Sessions':'累计会话'}} {{state.stats.sessions_total}}</span><span>UDP ↑{{state.stats.datagrams_sent}} ↓{{state.stats.datagrams_received}}</span></div>
 <p v-if="state?.stats?.last_error" class="error">{{state.stats.last_error.code}} · {{state.stats.last_error.message}}</p>
 <p v-if="state?.stats?.recording_stopped" class="error">{{en?'Payload recording stopped: capacity or I/O error.':'正文记录已停止：容量上限或文件错误。'}}</p>
 <details v-if="state?.sessions?.length"><summary>{{en?'Sessions and half-close state':'会话与半关闭状态'}}</summary><p v-for="s in state.sessions" :key="s.id">#{{s.id}} {{s.remote_endpoint}} · {{s.closed?'closed':s.read_closed?'read half-closed':'read open'}} / {{s.write_closed?'write closed':'write open'}}</p></details>
 <form @submit.prevent="send()" v-if="running">
 <label v-if="config.protocol==='tcp'" for="socket-session">{{en?'TCP session':'TCP 会话'}}</label><select v-if="config.protocol==='tcp'" id="socket-session" v-model="session"><option v-for="s in live" :key="s.id" :value="s.id">#{{s.id}} {{s.remote_endpoint}}</option></select>
 <template v-if="config.protocol==='udp'&&config.mode==='listen'"><label for="socket-destination">{{en?'Destination IP:port':'目标 IP:端口'}}</label><input id="socket-destination" v-model="destination" placeholder="127.0.0.1:9001 / [::1]:9001"></template>
 <label for="socket-encoding">{{en?'Input encoding':'输入编码'}}</label><select id="socket-encoding" v-model="encoding"><option value="utf8">UTF-8</option><option value="hex">HEX</option></select>
 <label for="socket-body">{{en?'Payload (empty UDP allowed)':'发送正文（UDP 允许空报文）'}}</label><textarea id="socket-body" v-model="body" maxlength="196608" rows="3"></textarea>
 <div class="m4-fields"><label>{{en?'Newline':'换行'}}<select v-model="newline"><option value="none">None</option><option value="lf">LF</option><option value="crlf">CRLF</option></select></label><label>{{en?'Count':'次数'}}<input v-model.number="count" type="number" min="1" max="1000"></label><label>{{en?'Interval ms':'间隔 ms'}}<input v-model.number="interval" type="number" min="10" max="60000"></label></div>
 <button class="primary" :disabled="config.protocol==='tcp'&&(session===null||live.find(s=>s.id===session)?.write_closed===true)">{{en?'Send':'发送'}}</button><button v-if="config.protocol==='tcp'" type="button" :disabled="session===null" @click="send('shutdown_write')">{{en?'Half-close write':'半关闭发送'}}</button><button v-if="config.protocol==='tcp'" type="button" :disabled="session===null" @click="send('close')">{{en?'Close session':'关闭会话'}}</button><p role="alert" v-if="problem" class="error">{{problem}}</p>
 </form>
 <label for="socket-display">{{en?'Preview':'预览'}}</label><select id="socket-display" v-model="display"><option value="hex">HEX</option><option value="text">UTF-8</option></select>
 <small>{{en?'Recent bounded preview; cumulative counters are independent.':'有限实时预览；累计统计独立保留。'}}</small>
 <article class="socket-record" v-for="r in (state?.records??[]).slice(-viewCount)" :key="r.seq"><small>#{{r.seq}} +{{r.elapsed_ms}}ms · {{r.kind}} · {{r.session_id===null?'UDP':'#'+r.session_id}} · {{r.bytes}} B</small><small>{{r.local_endpoint}} ↔ {{r.remote_endpoint}}</small><pre>{{display==='hex'?r.hex:r.text}}</pre><small v-if="display==='text'&&!r.utf8_valid">{{en?'Invalid or split UTF-8; inspect HEX.':'非完整 UTF-8，请查看 HEX。'}}</small></article>
 <button v-if="(state?.records?.length??0)>viewCount" @click="viewCount+=50">{{en?'Show 50 more':'再显示 50 条'}}</button>
 <details><summary>{{en?'Export preview':'导出预览'}}</summary><label class="check"><input v-model="include" type="checkbox">{{en?'Include visible raw payload (may contain secrets)':'包含可见原始正文（可能含秘密）'}}</label><p>{{include?(en?'Export includes the HEX and text shown above.':'导出包含上方显示的 HEX 与文本。'):(en?'Metadata only; raw payload excluded.':'仅导出元数据，不含报文正文。')}}</p><button @click="download">{{en?'Save preview JSON':'保存预览 JSON'}}</button></details>
</div></template>
