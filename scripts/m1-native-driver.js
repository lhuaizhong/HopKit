// Debug-only native-test driver: real WebView, Vue forms, local UDP services and Rust IPC.
(async()=>{
 const invoke=(command,args={})=>window.__TAURI_INTERNALS__.invoke(command,args)
 const config=window.__HOPKIT_TEST_CONFIG__,reports=[]
 const sleep=ms=>new Promise(r=>setTimeout(r,ms))
 const assert=(condition,message)=>{if(!condition)throw Error(message)}
 async function wait(predicate){for(let i=0;i<200;i++){const result=await predicate();if(result)return result;await sleep(50)}throw Error('Native wait timed out')}
 function button(text,root=document){const result=[...root.querySelectorAll('button')].find(b=>b.textContent.trim()===text);assert(result,'Missing button: '+text);return result}
 function field(id,value){const input=document.getElementById(id);assert(input,'Missing field: '+id);input.value=value;input.dispatchEvent(new Event('input',{bubbles:true}))}
 async function select(id){field('tool-search',id);document.getElementById('tool-search').dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true,cancelable:true}));await sleep(50)}
 const keys=['hopkit-navigation','hopkit-library','hopkit-language','hopkit-appearance']
 const backup=Object.fromEntries(keys.map(key=>[key,localStorage.getItem(key)]))
 let result
 try{
  await wait(()=>document.getElementById('tool-search'))
  if([...document.querySelectorAll('header button')].some(b=>b.textContent==='中文'))button('中文').click()
  button('常用工具').click();await sleep(50)
  assert(document.querySelectorAll('nav .tool-nav-row').length>=13,'Old tools missing')
  button('专家工具').click();await sleep(50)
  assert(document.querySelector('.category-placeholder'),'Expert placeholder missing')
  await select('ftp');assert(document.getElementById('ftp-password'),'FTP exact search selected another tool')
  await select('tcping');field('target','192.0.2.25');field('port','8443')
  document.querySelector('.saved-configurations').open=true;field('profile-name','Native M1');await sleep(50)
  button('保存当前配置').click();await sleep(50)
  assert(JSON.parse(localStorage.getItem('hopkit-library')).profiles.some(p=>p.name==='Native M1'),'Native profile not saved')
  await select('dns');await select('tcping');assert(document.getElementById('port').value==='8443','Parameters lost on navigation')
  assert((await invoke('list_tasks')).length===0,'Navigation started a task')
  reports.push('Native navigation, cross-category search, parameter restoration and local profile persistence')
  const app=await invoke('app_info');assert(app.capabilities.schema_version===1,'Missing capability schema')
  assert(app.capabilities.probe_budget.global_connections===128,'Missing shared budget contract')
  let unknownRejected=false;try{await invoke('start_task',{request:{tool:'scan'}})}catch{unknownRejected=true}assert(unknownRejected,'Unknown tool accepted')
  reports.push('Capability/budget query and independent Rust tool-ID validation')
  // Four real listeners on runner-selected unprivileged loopback UDP ports.
  const ids=[]
  for(const port of config.ports){
   const id=await invoke('start_task',{request:{tool:'ntp',action:'serve',listen:'127.0.0.1',allow:'127.0.0.1/32',port}})
   await wait(async()=>{const s=await invoke('task_poll',{id,after:0});if(s.result)throw Error(JSON.stringify(s.result));return s.events.some(e=>e.event_type==='ready')})
   ids.push(id)
  }
  await wait(()=>[...document.querySelectorAll('.navigation-tasks')].some(b=>b.textContent.includes('4')))
  let fifthRejected=false;try{await invoke('start_task',{request:{tool:'ntp',action:'serve',listen:'127.0.0.1',port:config.ports[0]}})}catch{fifthRejected=true}assert(fifthRejected,'Fifth task accepted')
  button('小工具').click();await sleep(50);document.querySelector('.navigation-tasks').click()
  await wait(()=>document.querySelectorAll('.task-row').length===4)
  const first=document.querySelector('.task-row');button('停止',first).click()
  await wait(async()=>(await invoke('list_tasks')).filter(t=>['running','cancelling'].includes(t.status)).length===3)
  const start=Date.now();button('停止全部').click()
  await wait(async()=>(await invoke('list_tasks')).every(t=>!['running','cancelling'].includes(t.status)))
  const stopMs=Date.now()-start;assert(stopMs<2000,'Local service cancellation exceeded 2 seconds')
  reports.push('Four real NTP services, fifth rejected, global task access, individual stop and stop-all')
  result={scenario:'m1',status:'passed',version:app.version,platform:navigator.userAgent,reports,stop_all_ms:stopMs}
 }catch(error){await invoke('stop_all').catch(()=>{});result={scenario:'m1',status:'failed',error:String(error),reports}}
 finally{await sleep(500);for(const [key,value]of Object.entries(backup)){if(value===null)localStorage.removeItem(key);else localStorage.setItem(key,value)}}
 await invoke('native_test_report',{value:result})
})()
