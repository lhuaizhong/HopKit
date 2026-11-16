(async()=>{
 const invoke=window.__TAURI_INTERNALS__.invoke,checks=[];let phase='setup';const sleep=ms=>new Promise(r=>setTimeout(r,ms)),assert=(v,s)=>{if(!v)throw Error(s)},wait=async fn=>{for(let i=0;i<150;i++){const v=await fn();if(v)return v;await sleep(50)}throw Error('Timeout: '+phase)};
 const button=text=>[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===text),field=(id,value)=>{const e=document.getElementById(id);assert(e,'Missing '+id);e.value=String(value);e.dispatchEvent(new Event('input',{bubbles:true}));e.dispatchEvent(new Event('change',{bubbles:true}))};
 const select=async id=>{field('tool-search',id);await sleep(30);document.getElementById('tool-search').dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true}));await sleep(70)};
 let report;
 try{
  if(button('中文'))button('中文').click();await select('mtr');assert((await invoke('list_tasks')).length===0,'Implicit probe');field('mtr-target','127.0.0.1');field('mtr-cycles',3);field('mtr-interval',100);button('开始 MTR').click();phase='native form';
  const task=await wait(async()=>{const t=(await invoke('list_tasks')).find(t=>t.tool==='mtr');return t?.status==='succeeded'&&t});const result=await invoke('task_poll',{id:task.id,after:0});assert(result.result.data.stats.target_reached_cycles===3,'Wrong native count');await wait(()=>document.querySelector('.mtr-table tbody tr'));assert(document.querySelector('.mtr-results').textContent.includes('100.00%'),'Missing reach rate');checks.push('Real WebView MTR form, Rust IPC, OS ICMP, dedicated table and target rate');
  const ids=[];for(const target of ['127.0.0.1','::1','127.0.0.1','::1'])ids.push(await invoke('start_task',{request:{tool:'mtr',action:'icmp',mtr:{target,cycles:0,interval_ms:100,timeout_ms:1000,max_hops:4}}}));
  phase='four concurrent tasks';await wait(async()=>{const snapshots=await Promise.all(ids.map(id=>invoke('task_poll',{id,after:0})));return snapshots.every(t=>t.latest_stats?.stats.responses>0)});let rejected=false;try{await invoke('start_task',{request:{tool:'mtr',action:'icmp',mtr:{target:'127.0.0.1',cycles:0}}})}catch{rejected=true}assert(rejected,'Fifth task allowed');
  for(const id of ids){const d=await invoke('task_poll',{id,after:0});assert(d.latest_stats.hops[0].responders.every(r=>r.address===d.request.mtr.target),'Cross-task ICMP mismatch')}
  checks.push('Four concurrent mixed-family MTR tasks share one process; fifth rejected; no cross-task responder');
  await select('dns');document.querySelector('.navigation-tasks').click();await wait(()=>button('停止全部'));const start=Date.now();button('停止全部').click();phase='stop all';await wait(async()=>{const all=await invoke('list_tasks');return ids.every(id=>all.some(t=>t.id===id&&t.status==='cancelled'))});const stop_ms=Date.now()-start;assert(stop_ms<1500,'Slow cancel');
  for(const id of ids){const d=await invoke('task_poll',{id,after:0});assert(d.result.data.stats.responses>0,'Cancelled stats missing')}
  checks.push('Global cross-category stop retains all four cumulative results');report={scenario:'m5',status:'passed',checks,stop_ms,version:(await invoke('app_info')).version};
 }catch(e){await invoke('stop_all').catch(()=>{});report={scenario:'m5',status:'failed',checks,phase,error:String(e),body:document.body.innerText}}
 await invoke('native_test_report',{value:report});
})()
