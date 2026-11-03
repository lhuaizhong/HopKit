// Compiled ONLY with the debug-only native-test feature; real WKWebView + IPC.
(async()=>{
 const invoke=(cmd,args={})=>window.__TAURI_INTERNALS__.invoke(cmd,args),c=window.__HOPKIT_TEST_CONFIG__,reports=[]
 const sleep=ms=>new Promise(r=>setTimeout(r,ms));const assert=(x,m)=>{if(!x)throw Error(m)}
 async function wait(fn){for(let i=0;i<400;i++){const result=await fn();if(result)return result;await sleep(50)}throw Error('Native wait timed out')}
 const request=(operation)=>({tool:'wormhole',action:operation,wormhole:{operation,send_path:c.source,mailbox_url:c.mailbox,transit_relay:c.relay,connection_mode:'relay'}})
 try{
  await wait(()=>document.querySelector('.nav-item'))
  const nav=[...document.querySelectorAll('.nav-item')];nav.find(b=>b.querySelector('b').textContent==='Wormhole').click();await sleep(100)
  const input=document.querySelector('#wormhole-file');input.value=c.source;input.dispatchEvent(new Event('input',{bubbles:true}))
  // Set advanced network options through actual bound inputs.
  document.querySelector('.wormhole-setup details').open=true
  for(const [id,value]of [['wormhole-mailbox',c.mailbox],['wormhole-relay',c.relay]]){const el=document.getElementById(id);el.value=value;el.dispatchEvent(new Event('input',{bubbles:true}))}
  const mode=document.getElementById('wormhole-connection');mode.value='relay';mode.dispatchEvent(new Event('change',{bubbles:true}))
  document.querySelector('.wormhole-setup button[type=submit]').click()
  const send=await wait(async()=>(await invoke('list_tasks')).find(t=>t.tool==='wormhole'))
  const secret=await wait(async()=>{const s=await invoke('wormhole_private_state',{id:send.id});return s.code})
  await wait(()=>document.querySelector('.wormhole-secret'))
  reports.push('native Vue send form and private IPC')
  const receiveReq=request('receive');receiveReq.wormhole.code=secret
  const receive=await invoke('start_task',{request:receiveReq})
  const offer=await wait(async()=>{const s=await invoke('task_poll',{id:receive,after:0});return s.latest_stats?.decision_id&&s})
  await invoke('wormhole_action',{id:receive,decisionId:offer.latest_stats.decision_id,action:'accept',savePath:c.dest})
  const completed=await wait(async()=>{const s=await invoke('task_poll',{id:receive,after:0});return s.result&&s})
  assert(completed.status==='succeeded',JSON.stringify(completed.result));assert(completed.result.data.local_saved,'saved')
  const sent=await wait(async()=>{const s=await invoke('task_poll',{id:send.id,after:0});return s.result&&s})
  assert(sent.status==='succeeded',JSON.stringify(sent.result));assert(sent.result.data.acknowledged,'receipt')
  for(const snapshot of [completed,sent])assert(!JSON.stringify(snapshot).includes(secret),'Secret leaked into snapshot')
  assert(!(await invoke('wormhole_private_state',{id:send.id})).code,'Secret retained after completion')
  reports.push('native IPC receive decision, filesystem commit and sender receipt','public snapshot redaction and terminal private-state clearing')
  // Full private verifier, rejection, and retry after a local path conflict.
  const verifiedReq=request('send');verifiedReq.wormhole.verify=true
  const verifySend=await invoke('start_task',{request:verifiedReq})
  const verifyCode=await wait(async()=>(await invoke('wormhole_private_state',{id:verifySend})).code)
  const verifyReceive=request('receive');verifyReceive.wormhole.verify=true;verifyReceive.wormhole.code=verifyCode
  const verifyRecv=await invoke('start_task',{request:verifyReceive})
  const a=await wait(async()=>{const s=await invoke('wormhole_private_state',{id:verifySend});return s.verifier&&s.verifier})
  const b=await wait(async()=>{const s=await invoke('wormhole_private_state',{id:verifyRecv});return s.verifier&&s.verifier})
  assert(a===b&&a.length===64,'Full verifier mismatch')
  for(const id of [verifySend,verifyRecv]){const s=await wait(async()=>{const s=await invoke('task_poll',{id,after:0});return s.latest_stats?.phase==='waiting_verification'&&s.latest_stats?.decision_id&&s});await invoke('wormhole_action',{id,decisionId:s.latest_stats.decision_id,action:'verify_ok'})}
  const offered=await wait(async()=>{const s=await invoke('task_poll',{id:verifyRecv,after:0});return s.latest_stats?.phase==='waiting_acceptance'&&s.latest_stats?.decision_id&&s})
  await invoke('wormhole_action',{id:verifyRecv,decisionId:offered.latest_stats.decision_id,action:'accept',savePath:c.source})
  const retry=await wait(async()=>{const s=await invoke('task_poll',{id:verifyRecv,after:0});return s.latest_stats?.path_error&&s.latest_stats?.decision_id!==offered.latest_stats.decision_id&&s})
  assert(retry.latest_stats.path_error_code==='ALREADY_EXISTS','Conflict error')
  await invoke('wormhole_action',{id:verifyRecv,decisionId:retry.latest_stats.decision_id,action:'reject'})
  const refused=await wait(async()=>{const s=await invoke('task_poll',{id:verifySend,after:0});return s.result&&s})
  assert(refused.result.error.code==='TRANSFER_REJECTED',JSON.stringify(refused.result));reports.push('full verifier equality, path conflict retains offer, explicit rejection received by sender')
  // Four waiting tasks occupy all slots; fifth is refused; cancel releases them.
  const ids=[];for(let i=0;i<4;i++)ids.push(await invoke('start_task',{request:request('send')}))
  let rejected=false;try{await invoke('start_task',{request:request('send')})}catch{rejected=true}assert(rejected,'Fifth concurrent task accepted')
  const start=Date.now();await invoke('stop_all');await wait(async()=>{const list=await invoke('list_tasks');return ids.every(id=>list.some(t=>t.id===id&&!['running','cancelling'].includes(t.status)))})
  assert(Date.now()-start<2000,'Cancellation exceeded 2s');reports.push('4-task limit, fifth rejected, stop_all <2s')
  const next=await invoke('start_task',{request:request('send')});await invoke('cancel_task',{id:next});await wait(async()=>(await invoke('task_poll',{id:next,after:0})).result)
  reports.push('new transfer after cancellation')
  await invoke('native_test_report',{value:{status:'passed',platform:navigator.userAgent,reports,receiver:completed.result,sender:sent.result}})
 }catch(e){await invoke('native_test_report',{value:{status:'failed',error:String(e),reports}})}
})()
