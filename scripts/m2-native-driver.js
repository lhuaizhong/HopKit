// Real Vue + WKWebView + Rust IPC; debug-only native-test feature.
(async()=>{
 const invoke=(cmd,args={})=>window.__TAURI_INTERNALS__.invoke(cmd,args),config=window.__HOPKIT_TEST_CONFIG__,reports=[]
 const sleep=ms=>new Promise(r=>setTimeout(r,ms)),assert=(v,m)=>{if(!v)throw Error(m)}
 async function wait(fn){for(let i=0;i<300;i++){const result=await fn();if(result)return result;await sleep(50)}throw Error('Native M2 wait timed out')}
 const keys=['hopkit-navigation','hopkit-library','hopkit-language','hopkit-appearance'],backup=Object.fromEntries(keys.map(k=>[k,localStorage.getItem(k)]))
 function field(id,value){const el=document.getElementById(id);assert(el,'Missing '+id);el.value=value;el.dispatchEvent(new Event('input',{bubbles:true}));el.dispatchEvent(new Event('change',{bubbles:true}))}
 function button(text,root=document){const el=[...root.querySelectorAll('button')].find(b=>b.textContent.trim()===text);assert(el,'Missing button '+text);return el}
 async function select(id){field('tool-search',id);document.getElementById('tool-search').dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true,cancelable:true}));await sleep(80)}
 let result
 try{
  await wait(()=>document.getElementById('tool-search'));if([...document.querySelectorAll('header button')].some(b=>b.textContent.trim()==='中文'))button('中文').click()
  await select('subnet');field('subnet-input','::/0');await sleep(50);button('计算').click();await wait(()=>document.querySelector('.subnet-result')?.textContent.includes('340282366920938463463374607431768211456'))
  assert((await invoke('list_tasks')).length===0,'Offline calculation used task slots');reports.push('Native offline IPv6 /0 and exact decimal count without task admission')
  await select('scan');assert(document.getElementById('scan-source').value==='','Initial scan source must be blank');field('scan-targets','127.0.0.1');field('scan-ports',`${config.open_port},${config.refused_port}`);await sleep(50);button('开始扫描').click()
  const task=await wait(async()=>(await invoke('list_tasks')).find(t=>t.tool==='scan'));const finished=await wait(async()=>{const t=await invoke('task_poll',{id:task.id,after:0});return t.result&&t})
  assert(finished.status==='succeeded',JSON.stringify(finished.result));assert(finished.request.scan.source==='','Default scan unexpectedly bound a source');assert(finished.result.data.stats.counts.open===1&&finished.result.data.stats.counts.refused===1,'Native scan outcomes wrong')
  await wait(()=>[...document.querySelectorAll('.scan-results button')].find(b=>b.textContent.trim()==='TCPing →'));button('TCPing →').click();await sleep(100);assert(document.getElementById('port').value===String(config.open_port),'Port handoff lost');assert((await invoke('list_tasks')).length===1,'Handoff started a task');reports.push('Native scan form with blank source/default routing, real open/refused, full result and explicit TCPing handoff')
  const request={tool:'scan',action:'tcp',scan:{targets:['127.0.0.1'],ports:'1-100',timeout_ms:1000,concurrency:32,rate:1,family:'ipv4',source:''}}
  const ids=[];for(let i=0;i<4;i++)ids.push(await invoke('start_task',{request}))
  let rejected=false;try{await invoke('start_task',{request})}catch{rejected=true}assert(rejected,'Fifth scan accepted')
  await select('subnet');field('subnet-input','192.0.2.1/31');await sleep(50);button('计算').click();await wait(()=>document.querySelector('.subnet-result')?.textContent.includes('点到点 /31'))
  assert((await invoke('list_tasks')).filter(t=>t.status==='running').length===4,'Offline calculation disturbed scans')
  document.querySelector('.navigation-tasks').click();await wait(()=>document.querySelector('.task-panel'));const stop=Date.now();button('停止全部').click()
  const stopped=await wait(async()=>{const tasks=await invoke('list_tasks');return ids.every(id=>tasks.some(t=>t.id===id&&t.status==='cancelled'))&&tasks})
  assert(Date.now()-stop<1500,'Cancellation too slow')
  for(const id of ids){const t=await invoke('task_poll',{id,after:0});assert(t.result.data.partial&&t.result.data.stats.remaining>0,'Partial scan missing');assert(t.result.data.rows.length>0,'No partial rows')}
  reports.push('Four scans, fifth rejection, offline subnet at capacity, cross-category stop-all and partial results')
  result={scenario:'m2',status:'passed',reports,stop_all_ms:Date.now()-stop,version:(await invoke('app_info')).version}
 }catch(error){await invoke('stop_all').catch(()=>{});result={scenario:'m2',status:'failed',error:String(error),reports}}
 finally{await sleep(500);for(const [key,value]of Object.entries(backup)){if(value===null)localStorage.removeItem(key);else localStorage.setItem(key,value)}}
 await invoke('native_test_report',{value:result})
})()
