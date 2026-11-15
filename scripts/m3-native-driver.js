// Debug-only WKWebView/Rust IPC validation with an independent loopback peer.
(async()=>{
 const invoke=(cmd,args={})=>window.__TAURI_INTERNALS__.invoke(cmd,args),config=window.__HOPKIT_TEST_CONFIG__,reports=[]
 const sleep=ms=>new Promise(r=>setTimeout(r,ms)),assert=(v,m)=>{if(!v)throw Error(m)}
 async function wait(fn){for(let i=0;i<300;i++){const r=await fn();if(r)return r;await sleep(50)}throw Error('Native M3 timed out')}
 function field(id,value){const el=document.getElementById(id);assert(el,'Missing '+id);el.value=value;el.dispatchEvent(new Event('input',{bubbles:true}));el.dispatchEvent(new Event('change',{bubbles:true}))}
 function button(text,root=document){const el=[...root.querySelectorAll('button')].find(b=>b.textContent.trim()===text);assert(el,'Missing '+text);return el}
 async function select(id){field('tool-search',id);document.getElementById('tool-search').dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true,cancelable:true}));await sleep(100)}
 let result
 try{
  await wait(()=>document.getElementById('tool-search'));if([...document.querySelectorAll('header button')].some(b=>b.textContent.trim()==='中文'))button('中文').click()
  await select('discovery');await wait(()=>document.getElementById('discovery-targets'))
  assert((await invoke('list_tasks')).length===0,'Navigation started probes');assert((await invoke('discovery_interfaces')).some(i=>i.address==='127.0.0.1'),'No loopback interface')
  button('读取邻居缓存').click();const cached=await wait(async()=>{const t=(await invoke('list_tasks')).find(t=>t.tool==='discovery');return t&&t.status!=='running'&&await invoke('task_poll',{id:t.id,after:0})});assert(cached.result.data.stats.planned===0&&cached.result.data.stats.responders===0,'Cache inferred liveness');reports.push('Native interface list and read-only neighbor query with no active probes/liveness inference')
  field('discovery-targets','127.0.0.1');field('discovery-ports',String(config.open_port));field('discovery-source','127.0.0.1');await sleep(80);button('开始发现').click()
  const task=await wait(async()=>{const tasks=await invoke('list_tasks');for(const t of tasks)if(t.tool==='discovery'&&t.id!==cached.id&&t.status!=='running')return await invoke('task_poll',{id:t.id,after:0})});assert(task.status==='succeeded',JSON.stringify(task.result));const row=task.result.data.rows[0];assert(row.responded&&row.observations.some(o=>o.method==='tcp'&&o.status==='open')&&row.observations.some(o=>o.method==='icmp'&&o.status==='reply'),'Missing active evidence')
  await wait(()=>document.querySelector('.device'));button('端口扫描 →',document.querySelector('.device')).click();await wait(()=>document.getElementById('scan-targets'));assert(document.getElementById('scan-targets').value==='127.0.0.1'&&document.getElementById('scan-source').value==='127.0.0.1','Handoff lost address/source');assert((await invoke('list_tasks')).length===2,'Handoff started a task');reports.push('Real source-bound ICMP/TCP, native discovery result and explicit scan handoff without execution')
  const scan={tool:'scan',action:'tcp',scan:{targets:['127.0.0.1'],ports:'1-100',timeout_ms:1000,concurrency:32,rate:1,family:'ipv4',source:''}},discovery={tool:'discovery',action:'discover',discovery:{targets:['127.0.0.1-127.0.0.10'],interface:'',methods:['tcp'],ports:'80',timeout_ms:500,concurrency:32,rate:1,source:'',name_lookup:false,dns_server:''}}
  const ids=[];for(const request of [scan,discovery,scan,discovery])ids.push(await invoke('start_task',{request}));let rejected=false;try{await invoke('start_task',{request:discovery})}catch{rejected=true}assert(rejected,'Fifth task accepted');await sleep(300)
  await wait(async()=>{for(const id of ids){const t=await invoke('task_poll',{id,after:0});if(!t.seq)return false}return true})
  await select('ping');const began=Date.now();await invoke('stop_all');await wait(async()=>{const tasks=await invoke('list_tasks');return ids.every(id=>tasks.some(t=>t.id===id&&t.status==='cancelled'))});const elapsed=Date.now()-began;assert(elapsed<1500,'Slow cancellation');for(const id of ids){const t=await invoke('task_poll',{id,after:0});assert(t.result.data.partial,'Missing partial result')}
  reports.push('Two scans + two discoveries share admission, fifth rejected, cross-category cancellation with partial evidence')
  result={scenario:'m3',status:'passed',reports,stop_all_ms:elapsed,version:(await invoke('app_info')).version}
 }catch(e){await invoke('stop_all').catch(()=>{});await wait(async()=>!(await invoke('list_tasks')).some(t=>['running','cancelling'].includes(t.status))).catch(()=>{});result={scenario:'m3',status:'failed',error:String(e),reports}}
 await invoke('native_test_report',{value:result})
})()
