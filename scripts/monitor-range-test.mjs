import fs from 'node:fs/promises'
import assert from 'node:assert/strict'
import ts from 'typescript'
const source=await fs.readFile(new URL('../src/monitor.ts',import.meta.url),'utf8')
const {range,emptyCounters}=await import('data:text/javascript;base64,'+Buffer.from(ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2020,module:ts.ModuleKind.ES2020}}).outputText).toString('base64'))
const samples=Array.from({length:1000},(_,i)=>({target_index:0,timestamp_ms:i*1000,ok:i%5!==0,rtt_ms:i%5!==0?30:null,status:i%5!==0?'connected':'refused',seq:i+1}))
const data={config:{targets:[{label:'local',protocol:'tcping',target:'127.0.0.1',port:443}]},samples,sample_count:1000,buckets:[],records:[]}
const whole=range(data,0,1000000),before=range(data,0,500000),after=range(data,500000,1000000)
assert.equal(whole.totals[0].completed,1000);assert.equal(whole.totals[0].failed,200)
assert.equal(before.totals[0].completed+after.totals[0].completed,1000)
assert(whole.points.length<=240)
const totals=emptyCounters();Object.assign(totals,{completed:60,succeeded:50,failed:10,latency_sum_ms:1500})
data.samples=samples.slice(-100);data.buckets=Array.from({length:10},(_,i)=>({from_ms:i*60000,to_ms:(i+1)*60000,targets:[totals]}))
const historical=range(data,1,599999)
assert.equal(historical.raw,false);assert.equal(historical.totals[0].completed,480);assert.equal(historical.from,60000);assert.equal(historical.to,540000)
const left=range(data,0,300000),right=range(data,300000,600000)
assert.equal(left.totals[0].completed+right.totals[0].completed,600)
assert.equal(range(data,20,40).totals[0].completed,0)
assert.equal(range(data,999000,1000000).totals[0].completed,1)
console.log('PASS: exact half-open ranges, comparison boundary, historical partial exclusion, empty ranges, 240-point rendering budget')
