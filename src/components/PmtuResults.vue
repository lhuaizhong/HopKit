<script setup lang="ts">
import type {PmtuResult} from '../tools/pmtu'
const props=defineProps<{en:boolean;data:PmtuResult|null;error?:{code:string;message:string}|null}>();const t=(zh:string,en:string)=>props.en?en:zh
const outcome=(s:string)=>({bounded:t('相邻边界已验证','Adjacent bounds verified'),at_least:t('至少支持已验证长度','At least the verified length'),unknown:t('区间尚不能确定','Interval undetermined')}[s]??s)
</script>
<template><div class="pmtu-results structured"><p v-if="!data">{{t('手动开始后显示证据；打开页面不会发出探测。','Start manually to collect evidence. Opening this page sends no probes.')}}</p><template v-if="data">
<p><strong>ICMP PMTU · {{data.target}}</strong> → {{data.resolved_target??'—'}}</p><p>{{outcome(data.diagnostic_outcome)}} <strong v-if="data.partial"> · {{t('部分结果','Partial result')}}</strong></p>
<div class="pmtu-bounds"><p>{{t('已验证成功 IP 总长度','Confirmed successful IP length')}}: <strong>{{data.confirmed_lower??'—'}}</strong> B</p><p>{{t('最小明确拒绝 IP 总长度','Smallest explicitly rejected IP length')}}: <strong>{{data.confirmed_upper??'—'}}</strong> B</p><p>{{t('最近报告 MTU（独立提示）','Latest reported MTU (separate hint)')}}: {{data.reported_mtu??'—'}} B</p></div>
<p>{{t('上界是被拒绝长度，不是可用 MTU；全部成功仅表示至少支持已测上限。超时不代表 MTU 太小。','The upper bound is a rejected length, not a usable MTU. All sizes succeeding means at least the tested maximum. Timeout does not mean the MTU is too small.')}}</p>
<p>{{data.family}} · {{data.adapter}} · {{data.fragmentation_control}} · {{t('源 IP 配置','Requested source IP')}} {{data.source||t('系统选择','OS selected')}} · {{t('实际绑定源 IP','Bound local IP')}} {{data.local_endpoint??'—'}}</p>
<p>{{data.started_at}} → {{data.ended_at}} · {{data.duration_ms}} ms</p><p>{{t('尝试 / 回包 / 明确过大 / 未知','Attempts / replies / explicit too big / unknown')}}: {{data.stats.attempts}} / {{data.stats.replies}} / {{data.stats.too_big}} / {{data.stats.unknown}}</p>
<p v-if="data.unknown_reasons.length">{{t('未知或中断原因','Unknown or interruption reasons')}}: {{data.unknown_reasons.join(', ')}}</p>
<div class="table-scroll"><table class="pmtu-table"><thead><tr><th>#</th><th>{{t('阶段','Phase')}}</th><th>IP / {{t('IP 头','IP header')}} / ICMP / payload</th><th>{{t('状态 / 代码','Status / code')}}</th><th>{{t('响应者','Responder')}}</th><th>{{t('报告 MTU','Reported MTU')}}</th></tr></thead><tbody><tr v-for="p in data.probes" :key="p.seq"><td>{{p.seq}}</td><td>{{p.phase}}</td><td>{{p.ip_bytes}} / {{p.ip_header_bytes}} / {{p.icmp_header_bytes}} / {{p.payload_bytes}}</td><td>{{p.status}} / {{p.status_code??'—'}}</td><td>{{p.responder??'—'}}</td><td>{{p.reported_mtu??'—'}}</td></tr></tbody></table></div>
<p>{{t('仅为当前目标、源地址和观测窗口的 ICMP 结果，不保证所有 TCP/UDP 流量的载荷上限。源 IP 在系统路由选择后明确绑定；不固定接口。','ICMP evidence for this target, source and observation window; no TCP/UDP payload guarantee. Local IP is explicitly bound after OS route selection; no fixed-interface guarantee.')}}</p>
<details><summary>{{t('能力与限制','Capabilities and limitations')}}</summary><p v-for="l in data.limitations" :key="l">{{l}}</p></details>
</template><p v-if="error" class="result-error">{{error.code}} · {{error.message}}</p></div></template>
<style scoped>.pmtu-results{min-width:0;overflow-wrap:anywhere}.pmtu-table{min-width:620px}.pmtu-table th,.pmtu-table td{white-space:normal}.pmtu-bounds{padding:8px 12px;background:var(--surface-soft);border-radius:8px}</style>
