<script setup lang="ts">
import {computed} from 'vue'
type Row=Record<string,any>
const props=defineProps<{data?:Row;events:Row[];en:boolean}>()
const emit=defineEmits<{use:[tool:string,target:Row]}>()
const t=(zh:string,en:string)=>props.en?en:zh
const evidence=computed<Row[]>(()=>props.data?.evidence??props.events.filter(e=>e.event_type==='stage').map(e=>e.evidence))
const stage=(s:string)=>({network:t('本机网络','Local network'),dns:t('系统解析','System DNS'),dns_compare:t('指定 DNS 对比','Direct DNS comparison'),route:t('目标路由','Target route'),icmp:'ICMP',tcp:'TCP',connect:t('连接','Connection'),tls:'TLS',http:'HTTP',trace:t('路由追踪','Traceroute')} as Row)[s]??s
</script>
<template>
 <div class="check-results">
  <div v-if="data" class="check-context"><b>{{data.target.normalized}}</b><p>{{data.started_at}} · {{data.platform}} · v{{data.version}}</p><p>{{t('源地址','Source')}}: {{data.source_address||t('系统默认','System default')}} · {{data.proxy_mode}}</p><small>{{data.tls_trust}}</small><p>{{t('TLS 校验','TLS verification')}}: <strong :class="{unsafe:data.tls_verification?.startsWith('SKIPPED')}">{{data.tls_verification}}</strong></p><small>{{data.scope}}</small><p class="check-handoff"><button v-for="tool in ['ping','tcping','dns','trace']" :key="tool" type="button" :disabled="tool==='tcping'&&!data.target.port" @click="emit('use',tool,data.target)">{{t('沿用目标到 ','Use target in ')}}{{tool}}</button></p></div>
  <small v-if="evidence.length">{{t('阶段总耗时包含适配器开销；协议计时另列于证据中。','Stage elapsed includes adapter overhead; protocol timings are listed separately.')}}</small>
  <article v-for="e in evidence" :key="e.id" :id="'evidence-'+e.id" class="evidence-card">
   <div><b>{{stage(e.stage)}}</b> <code>{{e.id}}</code> · <span :class="{unsafe:e.status==='failed'}">{{e.status==='observed'?t('已观测','Observed'):e.status==='skipped'?t('已跳过','Skipped'):t('未通过','Failed')}}</span> · {{e.duration_ms.toFixed(2)}} ms</div>
   <p v-if="e.error" class="error">{{e.error.code}} · {{e.error.message}}</p><p v-if="e.data?.reason">{{e.data.reason}}</p>
   <p v-if="e.data?.status_code">HTTP {{e.data.status_code}} · {{e.data.method}} · {{e.data.url}}<br><span v-if="e.data.redirect_to">→ {{e.data.redirect_to}}</span></p>
   <p v-if="e.data?.local_address">{{t('本地地址','Local address')}}: {{e.data.local_address}} → {{e.data.remote_address}}</p>
   <p v-if="e.data?.connect_ms!=null">{{t('TCP 建连','TCP connect')}} {{e.data.connect_ms.toFixed(2)}} ms<span v-if="e.data.tls_ms!=null"> · TLS {{e.data.tls_ms.toFixed(2)}} ms</span><span v-if="e.data.response_wait_ms!=null"> · {{t('等待响应','Response wait')}} {{e.data.response_wait_ms.toFixed(2)}} ms</span></p>
   <p v-if="e.data?.handshake_ms!=null">TLS {{e.data.handshake_ms.toFixed(2)}} ms</p>
   <p v-if="e.data?.certificate_validation">{{t('证书校验','Certificate validation')}}: {{e.data.certificate_validation}}</p>
   <details v-if="e.data&&!e.data.reason"><summary>{{t('原始证据与测量口径','Raw evidence and measurement scope')}}</summary><pre>{{JSON.stringify(e.data,null,2)}}</pre></details>
  </article>
  <article v-for="f in data?.findings??[]" :key="f.rule" class="finding-card">
   <p><b>{{t('观测事实','Observed fact')}}</b> · {{f.fact[en?1:0]}}</p><p><b>{{t('可能原因','Possible cause')}}</b> · {{f.possible_cause[en?1:0]}}</p><p><b>{{t('建议验证','Next verification')}}</b> · {{f.next_step[en?1:0]}}</p><small>{{t('关联证据','Evidence')}}: <a v-for="id in f.evidence_ids" :key="id" :href="'#evidence-'+id">{{id}} </a></small>
  </article>
  <p v-if="!evidence.length">{{t('准备检查；尚无证据','Preparing checks; no evidence yet')}}</p>
 </div>
</template>
<style scoped>
.check-handoff{display:flex;gap:6px;flex-wrap:wrap}.check-handoff button{border:1px solid #8884;padding:5px 8px;background:transparent;color:inherit;font-size:11px}.check-results{padding:16px;overflow-wrap:anywhere}.check-context,.evidence-card,.finding-card{padding:12px;border-bottom:1px solid #8884}.finding-card{background:#80808010;margin-top:12px;border-radius:8px}.unsafe{color:#b94433}.check-results pre{white-space:pre-wrap;max-height:360px;overflow:auto;font-size:12px}.check-results summary{cursor:pointer}.check-results p{margin:8px 0}.check-results a{margin-right:8px}
</style>
