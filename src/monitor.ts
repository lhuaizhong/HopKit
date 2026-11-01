export type Counters={completed:number;succeeded:number;failed:number;latency_sum_ms:number;min_ms:number|null;max_ms:number|null;failures:Record<string,number>}
export type Sample={target_index:number;timestamp_ms:number;ok:boolean;rtt_ms:number|null;status:string;seq:number}
export type MonitorEvent={id:number;kind:string;timestamp_ms?:number;from_ms?:number;to_ms?:number;target_index?:number;end_reason?:string;text?:string;reason?:string;network?:unknown;code?:string}
export type Bucket={from_ms:number;to_ms:number;targets:Counters[]}
export type MonitorData={config:{targets:{label:string;target:string;protocol:string;port:number}[]};started_at_ms:number;ended_at_ms:number;paused:boolean;gap_start_ms:number|null;totals:Counters[];samples:Sample[];sample_count:number;records:MonitorEvent[];record_count:number;open_failures:(number|null)[];buckets:Bucket[];bucket_width_ms:number;journal_path?:string}
export const emptyCounters=():Counters=>({completed:0,succeeded:0,failed:0,latency_sum_ms:0,min_ms:null,max_ms:null,failures:{}})
export function add(a:Counters,b:Counters){a.completed+=b.completed;a.succeeded+=b.succeeded;a.failed+=b.failed;a.latency_sum_ms+=b.latency_sum_ms;if(b.min_ms!=null)a.min_ms=a.min_ms==null?b.min_ms:Math.min(a.min_ms,b.min_ms);if(b.max_ms!=null)a.max_ms=a.max_ms==null?b.max_ms:Math.max(a.max_ms,b.max_ms);for(const [k,v] of Object.entries(b.failures))a.failures[k]=(a.failures[k]??0)+v}
// Half-open ranges [from,to) never count the comparison boundary twice.
// Old aggregate buckets must fit fully: partial buckets are not invented.
export function range(data:MonitorData,from:number,to:number){
 const raw=data.sample_count===data.samples.length||from>(data.samples[0]?.timestamp_ms??Infinity)
 const totals=data.config.targets.map(emptyCounters)
 let points:Bucket[]=[]
 if(raw){
  const grouped=new Map<number,Bucket>()
  const width=Math.max(1,Math.ceil((to-from)/240))
  for(const s of data.samples){if(s.timestamp_ms<from||s.timestamp_ms>=to)continue
   const c=emptyCounters();c.completed=1;c.succeeded=s.ok?1:0;c.failed=s.ok?0:1
   if(s.ok&&s.rtt_ms!=null){c.latency_sum_ms=s.rtt_ms;c.min_ms=s.rtt_ms;c.max_ms=s.rtt_ms}else if(!s.ok)c.failures[s.status]=1
   add(totals[s.target_index],c)
   const key=Math.floor((s.timestamp_ms-from)/width)
   if(!grouped.has(key))grouped.set(key,{from_ms:from+key*width,to_ms:Math.min(to,from+(key+1)*width),targets:data.config.targets.map(emptyCounters)})
   add(grouped.get(key)!.targets[s.target_index],c)
  }
  points=[...grouped.values()]
 }else{
  points=data.buckets.filter(b=>b.from_ms>=from&&b.to_ms<=to)
  for(const b of points)b.targets.forEach((c,i)=>add(totals[i],c))
 }
 const effectiveFrom=raw?from:points[0]?.from_ms??null,effectiveTo=raw?to:points[points.length-1]?.to_ms??null
 const group=Math.ceil(points.length/240)
 if(group>1){const merged:Bucket[]=[];for(let i=0;i<points.length;i+=group){const part=points.slice(i,i+group),b={from_ms:part[0].from_ms,to_ms:part[part.length-1].to_ms,targets:data.config.targets.map(emptyCounters)};for(const p of part)p.targets.forEach((c,j)=>add(b.targets[j],c));merged.push(b)}points=merged}
 return {totals,points,raw,from:effectiveFrom,to:effectiveTo}
}
export const average=(c:Counters)=>c.succeeded?c.latency_sum_ms/c.succeeded:null
