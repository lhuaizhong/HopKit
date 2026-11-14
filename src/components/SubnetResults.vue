<script setup lang="ts">
import {computed,ref,watch} from 'vue'
import type {SubnetConfig,SubnetInfo,SubnetResult} from '../tools/m2'
const props=defineProps<{en:boolean;result:SubnetResult;request:SubnetConfig}>()
const t=(zh:string,en:string)=>props.en?en:zh
const page=ref(0),pageSize=50
watch(()=>props.result,()=>{page.value=0})
const networks=computed(()=>('networks' in props.result?props.result.networks:[]))
const pages=computed(()=>Math.max(1,Math.ceil(networks.value.length/pageSize)))
const visible=computed(()=>networks.value.slice(page.value*pageSize,(page.value+1)*pageSize))
const inputCount=computed(()=>props.request.input.split(/[\s,]+/).filter(Boolean).length)
function networkInfo(network:string){const [address,prefix]=network.split('/');return {family:address!.includes(':')?'IPv6':'IPv4',prefix:Number(prefix),count:(1n<<BigInt((address!.includes(':')?128:32)-Number(prefix))).toString()}}
function semantics(info:SubnetInfo){if(props.en)return info.host_semantics;if(info.family==='ipv6')return 'IPv6 无广播，不自动扣除首尾地址';if(info.prefix===31)return '点到点 /31：两个地址均计入；实际可用性取决于配置';if(info.prefix===32)return '单地址 /32';return '常规广播子网主机范围；实际可用性取决于配置'}
const relationship=computed(()=>{
 const r=props.result;if(r.operation!=='overlap')return ''
 if(!r.overlaps)return t('两个网段互不重叠','The networks do not overlap')
 if(r.contains_other&&r.contained_by_other)return t('两个网段相同','The networks are identical')
 if(r.contains_other)return t('网段 A 包含网段 B','Network A contains network B')
 if(r.contained_by_other)return t('网段 B 包含网段 A','Network B contains network A')
 return t('两个网段存在重叠','The networks overlap')
})
</script>
<template>
 <div class="subnet-overview">
  <template v-if="result.operation==='inspect'">
   <div class="subnet-cards" :class="{'wide-counts':result.subnet.address_count.length>24}">
    <div class="subnet-card network-card"><span>{{t('标准化网络','Network')}}</span><strong>{{result.subnet.network}}</strong></div>
    <div class="subnet-card"><span>{{t('地址总数','Total addresses')}}</span><strong>{{result.subnet.address_count}}</strong></div>
    <div class="subnet-card"><span>{{t('主机地址数','Host addresses')}}</span><strong>{{result.subnet.host_count}}</strong></div>
   </div>
   <section class="subnet-section"><h3>{{t('地址范围','Address range')}}</h3>
    <dl><dt>{{t('首地址 → 末地址','First → last address')}}</dt><dd><code>{{result.subnet.first}}</code><span class="range-separator">→</span><code>{{result.subnet.last}}</code></dd>
     <dt>{{t('主机范围','Host range')}}</dt><dd><code>{{result.subnet.host_first}}</code><span class="range-separator">→</span><code>{{result.subnet.host_last}}</code></dd>
     <dt>{{t('广播地址','Broadcast address')}}</dt><dd>{{result.subnet.broadcast??(result.subnet.family==='ipv6'?t('无广播（IPv6）','No broadcast (IPv6)'):t('不适用（/31 或 /32）','Not applicable (/31 or /32)'))}}</dd>
    </dl><p class="subnet-note">{{semantics(result.subnet)}}</p>
   </section>
   <section class="subnet-section"><h3>{{t('网段参数','Network details')}}</h3><dl>
    <dt>{{t('地址族 / 前缀','Family / prefix')}}</dt><dd>{{result.subnet.family.toUpperCase()}} /{{result.subnet.prefix}}</dd>
    <dt>{{t('子网掩码','Netmask')}}</dt><dd><code>{{result.subnet.mask}}</code></dd>
    <dt>{{t('反掩码','Wildcard mask')}}</dt><dd><code>{{result.subnet.wildcard}}</code></dd>
   </dl></section>
  </template>
  <template v-else-if="result.operation==='contains'">
   <div class="subnet-verdict" :class="result.contains?'positive':'negative'" role="status"><span aria-hidden="true">{{result.contains?'✓':'×'}}</span><div><h3>{{result.contains?t('地址属于该网段','Address belongs to the network'):t('地址不属于该网段','Address is outside the network')}}</h3><p>{{t('判断地址是否位于网段范围内','Address membership in the network range')}}</p></div></div>
   <dl><dt>{{t('待判断地址','Address')}}</dt><dd><code>{{result.address}}</code></dd><dt>{{t('标准化网络','Network')}}</dt><dd><code>{{result.network}}</code></dd></dl>
  </template>
  <template v-else-if="result.operation==='overlap'">
   <div class="subnet-verdict" :class="result.overlaps?'positive':'neutral'" role="status"><span aria-hidden="true">{{result.overlaps?'✓':'○'}}</span><div><h3>{{relationship}}</h3><p>{{result.overlaps?t('网段之间存在共同地址','The networks share addresses'):t('网段之间没有共同地址','The networks share no addresses')}}</p></div></div>
   <div class="subnet-cards"><div class="subnet-card"><span>{{t('网段 A','Network A')}}</span><strong>{{result.network}}</strong></div><div class="subnet-card"><span>{{t('网段 B','Network B')}}</span><strong>{{result.other}}</strong></div></div>
   <dl><dt>{{t('是否重叠','Overlap')}}</dt><dd>{{result.overlaps?t('是','Yes'):t('否','No')}}</dd><dt>{{t('A 包含 B','A contains B')}}</dt><dd>{{result.contains_other?t('是','Yes'):t('否','No')}}</dd><dt>{{t('B 包含 A','B contains A')}}</dt><dd>{{result.contained_by_other?t('是','Yes'):t('否','No')}}</dd></dl>
  </template>
  <template v-else>
   <div class="subnet-cards" v-if="result.operation==='split'" :class="{'wide-counts':networkInfo(networks[0]!).count.length>24}">
    <div class="subnet-card network-card"><span>{{t('原网段','Parent network')}}</span><strong>{{result.network}}</strong></div>
    <div class="subnet-card"><span>{{t('拆分后子网数','Child networks')}}</span><strong>{{networks.length}}</strong></div>
    <div class="subnet-card"><span>{{t('每个子网地址数','Addresses per child')}}</span><strong>{{networkInfo(networks[0]!).count}}</strong></div>
   </div>
   <template v-else><div class="subnet-verdict positive" role="status"><span aria-hidden="true">✓</span><div><h3>{{t('精确聚合完成','Exact aggregation complete')}}</h3><p>{{t('仅合并可精确覆盖的网段，保留地址空洞。','Merges preserve the exact address set, including gaps.')}}</p></div></div>
    <div class="subnet-cards"><div class="subnet-card"><span>{{t('输入网段数','Input networks')}}</span><strong>{{inputCount}}</strong></div><div class="subnet-card"><span>{{t('聚合后网段数','Result networks')}}</span><strong>{{networks.length}}</strong></div></div>
   </template>
   <section class="subnet-section"><h3>{{result.operation==='split'?t('拆分后的子网','Child networks'):t('聚合后的网段','Aggregated networks')}}</h3>
    <div class="subnet-table-scroll"><table class="subnet-networks"><thead><tr><th>#</th><th>{{t('网段 / 前缀','Network / prefix')}}</th><th>{{t('地址族','Family')}}</th><th>{{t('地址数量','Addresses')}}</th></tr></thead><tbody><tr v-for="(network,index) in visible" :key="network"><td>{{page*pageSize+index+1}}</td><td><code>{{network}}</code></td><td>{{networkInfo(network).family}}</td><td>{{networkInfo(network).count}}</td></tr></tbody></table></div>
    <div v-if="pages>1" class="subnet-pagination"><button type="button" :disabled="page===0" @click="page--">{{t('上一页','Previous')}}</button><span>{{page+1}} / {{pages}} · {{t('共','Total')}} {{networks.length}}</span><button type="button" :disabled="page===pages-1" @click="page++">{{t('下一页','Next')}}</button></div>
   </section>
  </template>
 </div>
</template>
<style scoped>
.subnet-overview{display:grid;gap:18px;color:#26364a;font-size:13px;min-width:0}.subnet-cards{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,210px),1fr));gap:12px}.subnet-cards.wide-counts{grid-template-columns:minmax(0,1fr)}.network-card{grid-column:1/-1}.subnet-card{background:#f4f8fe;border:1px solid #dce6f4;border-radius:8px;padding:14px;min-width:0}.subnet-card span{display:block;color:#43536a;font-size:12px;margin-bottom:8px}.subnet-card strong{display:block;color:#174b94;font-size:20px;line-height:1.5;overflow-wrap:anywhere;font-variant-numeric:tabular-nums}.subnet-section{min-width:0}.subnet-section h3{font-size:14px;color:#26364a;margin:0 0 10px}.subnet-overview dl{display:grid;grid-template-columns:minmax(100px,1fr) minmax(0,3fr);gap:12px;margin:0}.subnet-overview dt{color:#43536a}.subnet-overview dd{margin:0;overflow-wrap:anywhere}.subnet-overview code{display:inline;font-size:inherit;overflow-wrap:anywhere}.range-separator{display:inline-block;margin:0 12px;color:#43536a}.subnet-note{margin-top:12px;color:#43536a}.subnet-verdict{display:flex;align-items:flex-start;gap:12px;border:1px solid #b8dcc9;border-radius:8px;background:#edf8f3;padding:16px;color:#176346}.subnet-verdict>span{font-size:28px;line-height:1.2}.subnet-verdict h3{font-size:18px;line-height:1.5}.subnet-verdict p{margin-top:4px}.subnet-verdict.negative{background:#fff0ed;border-color:#e5c3bd;color:#982b24}.subnet-verdict.neutral{background:#f4f7fb;border-color:#cbd5e1;color:#43536a}.subnet-table-scroll{overflow:auto;max-height:460px}.subnet-networks{table-layout:fixed;font-size:13px;width:100%}.subnet-networks th{font-size:12px;color:#43536a}.subnet-networks th:first-child{width:48px}.subnet-networks th:nth-child(2){width:42%}.subnet-networks th:nth-child(3){width:62px}.subnet-networks td{white-space:normal;overflow-wrap:anywhere;color:#26364a}.subnet-pagination{display:flex;align-items:center;justify-content:flex-end;gap:10px;margin-top:12px}.subnet-pagination button{padding:7px 10px;border:1px solid #a8b4c3;background:white;color:#174b94}
:root[data-theme=readable] .subnet-overview{font-size:16px}:root[data-theme=readable] .subnet-card span,:root[data-theme=readable] .subnet-networks th{font-size:14px}:root[data-theme=readable] .subnet-card strong{font-size:23px}:root[data-theme=readable] .subnet-section h3{font-size:18px}:root[data-theme=readable] .subnet-networks{font-size:15px}:root[data-theme=readable] .subnet-card{border-color:#a8b4c3}:root[data-theme=readable] .subnet-pagination button{min-height:42px}
</style>
