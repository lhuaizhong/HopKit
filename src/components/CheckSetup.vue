<script setup lang="ts">
import {computed,onMounted,onUnmounted,ref,watch} from 'vue'
import {invoke} from '@tauri-apps/api/core'
type Row=Record<string,any>
const props=defineProps<{en:boolean;config:Row;disabled:boolean}>()
const emit=defineEmits<{run:[request:Row]}>()
const t=(zh:string,en:string)=>props.en?en:zh
const network=ref<Row|null>(null),failure=ref(''),refreshing=ref(false),parsed=ref<Row|null>(null),parseError=ref(''),parsing=ref(false)
let parseTimer:ReturnType<typeof setTimeout>,refreshTimer:ReturnType<typeof setInterval>,disposed=false,generation=0
async function refresh(){if(refreshing.value)return;refreshing.value=true;try{const value=await invoke<Row>('network_overview');if(!disposed){network.value=value;failure.value=''}}catch(e){if(!disposed)failure.value=String(e)}finally{refreshing.value=false}}
watch(()=>props.config.target,()=>{const gen=++generation;parsing.value=true;parsed.value=null;parseError.value='';clearTimeout(parseTimer);if(!props.config.target.trim()){parsing.value=false;return;}parseTimer=setTimeout(async()=>{try{const value=await invoke<Row>('parse_target',{input:props.config.target});if(gen===generation)parsed.value=value}catch(e){if(gen===generation)parseError.value=String(e)}finally{if(gen===generation)parsing.value=false}},200)},{immediate:true})
const sources=computed(()=>network.value?.interfaces?.flatMap((i:Row)=>(i.addresses??[]).map((a:Row)=>({value:a.scope&&i.index?`${a.address.split('%')[0]}%${i.index}`:a.address,label:`${i.name} · ${a.address}/${a.prefix??'?'}`})))??[])
onMounted(()=>{refresh();refreshTimer=setInterval(refresh,15000)})
onUnmounted(()=>{disposed=true;generation++;clearTimeout(parseTimer);clearInterval(refreshTimer)})
</script>
<template>
 <div class="check-setup">
  <div class="panel-title"><h2>{{t('本机网络概览','Local network')}}</h2><button type="button" class="quiet" :disabled="refreshing" @click="refresh">{{t('刷新','Refresh')}}</button></div>
  <small class="field-help">{{t('每 15 秒更新本机状态；打开页面不发送公网探测。','Refreshes local state every 15 seconds. Opening this page sends no public probes.')}}</small>
  <p v-if="failure" class="error" role="alert">{{failure}}</p>
  <template v-if="network">
   <small>{{network.captured_at}}</small>
   <details class="interface-list"><summary>{{t('接口与地址','Interfaces and addresses')}} · {{network.interfaces.length}}</summary>
    <article v-for="i in network.interfaces" :key="i.name"><b>{{i.label||i.name}}</b> · {{i.name}} · {{i.enabled?t('启用','Enabled'):t('停用','Disabled')}} · {{({physical:t('物理','Physical'),virtual:t('虚拟','Virtual'),vpn:'VPN',loopback:t('回环','Loopback'),unknown:t('类型未知','Unknown type'),virtual_or_unknown:t('虚拟或未知','Virtual or unknown')} as Row)[i.kind]||i.kind}}<code v-for="a in i.addresses" :key="a.address">{{a.address}}/{{a.prefix??'?'}}<span v-if="a.scope"> · scope {{a.scope}}</span></code><small>{{t('网关','Gateway')}}: {{i.gateways?.join(', ')||'—'}}<br>DNS: {{i.dns?.join(', ')||'—'}}</small></article>
   </details>
   <p class="field-help">{{t('系统默认路由','System default routes')}}: <span v-if="!network.default_routes.length">{{t('未发现','None found')}}</span><code v-for="(r,i) in network.default_routes" :key="i">{{r.gateway}} · {{r.interface??r.interface_index}}</code></p>
   <p class="field-help">{{t('系统 DNS','System DNS')}}: {{network.dns?.join(', ')||t('未取得；请核对系统设置','Unavailable; check system settings')}}</p>
   <small v-if="network.errors?.length" class="error">{{t('部分系统信息未取得','Some system information is unavailable')}}: {{network.errors}}</small>
  </template>
  <form @submit.prevent="emit('run',{tool:'check',...config})">
   <label for="check-target">{{t('故障目标','Troubleshooting target')}}</label><input id="check-target" v-model="config.target" required maxlength="4096" placeholder="example.com · 192.168.1.1:443 · https://example.com/path" spellcheck="false">
   <p v-if="parseError" class="error" role="alert">{{parseError}}</p>
   <div v-if="parsed" class="target-preview"><b>{{t('测试目标','Test target')}}</b><code>{{parsed.normalized}}</code><small>{{parsed.scheme||t('未指定应用协议','No application protocol')}} · {{parsed.host}} · {{parsed.port??t('未指定端口','No port')}}<span v-if="parsed.path"> · {{parsed.path}}</span></small><small v-if="parsed.query_redacted">{{t('请求将使用原 URL；查询参数与片段在任务记录、重跑参数及报告中脱敏。重跑时须重新输入原 URL。','Requests use the original URL; query and fragment are redacted from history and reports. Re-enter the original URL to rerun.')}}</small></div>
   <details><summary>{{t('专业参数','Advanced options')}}</summary>
    <label for="check-source">{{t('源网络','Source network')}}</label><select id="check-source" v-model="config.source_address"><option v-if="config.source_address&&!sources.some((s:any)=>s.value===config.source_address)" :value="config.source_address">{{t('原源地址已离线：','Previous source is offline: ')}}{{config.source_address}}</option><option value="">{{t('系统默认路由','System default route')}}</option><option v-for="s in sources" :key="s.value" :value="s.value">{{s.label}}</option></select>
    <small class="field-help">{{t('指定地址会实际绑定 TCP/HTTP。系统 DNS 与路由查询使用系统策略；ICMP 和路由追踪不支持源绑定，选定源时跳过。','TCP/HTTP bind the selected source. System DNS and route queries follow OS policy. ICMP/trace source binding is unavailable and those checks are skipped when a source is selected.')}}</small>
    <label for="check-timeout">{{t('单次连接或每跳 HTTP 总超时 / ms','TCP attempt / total HTTP hop timeout / ms')}}</label><input id="check-timeout" v-model.number="config.timeout_ms" type="number" min="100" max="10000" step="100" required>
    <label for="check-dns">{{t('对比指定 DNS（可选 IP 或 IP:端口）','Compare DNS (optional IP or IP:port)')}}</label><input id="check-dns" v-model="config.dns_server"><small class="field-help">{{t('指定 DNS 仅作对比，使用系统路由，不绑定所选源地址。','Direct DNS is for comparison only; it uses system routing and is not source-bound.')}}</small>
    <label class="check"><input v-model="config.ipv6" type="checkbox">{{t('仅使用 IPv6 解析地址','Use only IPv6 resolver addresses')}}</label>
    <label class="check"><input v-model="config.include_trace" type="checkbox">{{t('补充路由追踪（可能较慢）','Include traceroute (may take longer)')}}</label>
    <label for="check-ca">{{t('附加 CA 文件（可选 PEM）','Additional CA file (optional PEM)')}}</label><input id="check-ca" v-model="config.tls_ca_file" :placeholder="t('本机 CA 文件路径','Local CA file path')"><small class="field-help">{{t('默认使用随程序内置的 Mozilla 根证书，不读取系统证书或环境变量；企业内网可显式添加 CA，不会关闭证书校验。','Uses bundled Mozilla roots, without system certificates or environment overrides. Explicitly add an enterprise CA while retaining certificate validation.')}}</small>
    <label class="check"><input v-model="config.insecure_tls" type="checkbox">{{t('跳过 TLS 证书校验（无法证明证书正常）','Skip TLS validation (cannot establish certificate validity)')}}</label>
   </details>
   <p v-if="config.insecure_tls" class="error" role="alert">{{t('已跳过 TLS 校验：结果不能用于判断证书正常。','TLS validation is skipped: results cannot establish certificate validity.')}}</p>
   <p class="info-note">{{t('内置 HTTP/TLS 直连；仅发 HEAD，不读取正文，最多 5 次跳转。不使用系统代理。未指定端口或 URL 的主机只检查 DNS、ICMP 和路由。','Built-in HTTP/TLS, direct HEAD requests, no body, up to 5 redirects. System proxies are not used. A host without a port or URL runs DNS, ICMP and route checks.')}}</p>
   <button class="primary" type="submit" :disabled="disabled||!parsed||parsing">▶ {{t('开始排障','Start troubleshooting')}}</button>
  </form>
 </div>
</template>
<style scoped>
.check-setup>small,.check-setup>p,.check-setup>.interface-list{margin-left:18px;margin-right:18px}.interface-list article{border-bottom:1px solid #8884;padding:10px 0}.check-setup code,.target-preview small{display:block;overflow-wrap:anywhere}.check-setup details{margin:14px 0}.check-setup summary{cursor:pointer;font-weight:600}.target-preview{padding:12px;background:#80808010;border-radius:8px;margin:12px 0}.target-preview small{margin-top:6px}.check-setup small{overflow-wrap:anywhere}.check-setup button.primary{width:100%}
</style>
