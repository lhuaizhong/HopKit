export type MtrConfig={ptr_server:string;target:string;family:'auto'|'ipv4'|'ipv6';source:string;cycles:number;duration_secs:number;interval_ms:number;timeout_ms:number;max_hops:number;rate:number}
export const mtrDefaults=():MtrConfig=>({ptr_server:'',target:'127.0.0.1',family:'auto',source:'',cycles:100,duration_secs:0,interval_ms:1000,timeout_ms:1000,max_hops:30,rate:20})
export type RttStats={count:number;last_ms:number|null;min_ms:number|null;avg_ms:number|null;max_ms:number|null;jitter_ms:number|null;jitter_pairs:number}
export type MtrHop={ttl:number;sent:number;responses:number;timeouts:number;interrupted:number;no_response_percent:number|null;rtt:RttStats;responders:{address:string;responses:number;rtt:RttStats}[];untracked_responses:number;last_status:string}
export type MtrSample={seq:number;cycle:number;ttl:number;observed_at:string;elapsed_ms:number;status:string;address:string|null;rtt_ms:number|null;target_reached:boolean}
export type MtrResult={names?:{address:string;names:string[];status:string}[];names_dropped?:number;schema_version:1;target:string;resolved_target:string|null;source:string;started_at:string;ended_at:string;duration_ms:number;stats:{cycles_started:number;cycles_completed:number;target_reached_cycles:number;target_reach_percent:number|null;sent:number;responses:number;timeouts:number;interrupted:number;gaps:number;path_changes:number};hops:MtrHop[];samples:MtrSample[];path_changes:{observed_at:string;cycle:number;ttl:number;previous:string;address:string}[];gaps:{observed_at:string;elapsed_ms:number;duration_ms:number;reason:string}[];samples_dropped:number;changes_dropped:number;gaps_dropped:number;target_outcome:string;success:boolean;partial:boolean;limitations:string[]}
export function cleanMtr(input:unknown):MtrConfig {
 if(!input||typeof input!=='object'||Array.isArray(input))throw Error('Invalid MTR configuration')
 const c=mtrDefaults();for(const key of Object.keys(c) as (keyof MtrConfig)[]){const v=(input as MtrConfig)[key];if(v===undefined)continue;if(typeof v!==typeof c[key]||typeof v==='string'&&(v.length>253||/[\s\x00-\x1f/@?#]/.test(v))||typeof v==='number'&&!Number.isInteger(v))throw Error('Invalid MTR field: '+key);Object.assign(c,{[key]:v})}
 if(!c.target||!['auto','ipv4','ipv6'].includes(c.family))throw Error('Invalid MTR target/family')
 for(const [key,min,max]of [['cycles',0,1000000],['duration_secs',0,86400],['interval_ms',100,60000],['timeout_ms',100,1000],['max_hops',1,64],['rate',1,100]] as const)if(c[key]<min||c[key]>max)throw Error('Invalid MTR setting: '+key)
 return c
}
