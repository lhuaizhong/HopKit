import fs from 'node:fs'
import path from 'node:path'
import assert from 'node:assert/strict'
import net from 'node:net'
import dgram from 'node:dgram'
import {createRequire} from 'node:module'
const require=createRequire(import.meta.url)
// The workspace provides Puppeteer for development only; it is not a product dependency.
const puppeteer=require('puppeteer-core')
const root=path.resolve(new URL('..',import.meta.url).pathname.replace(/^\/([A-Za-z]:)/,'$1'))
const out=path.join(root,'test-output');fs.mkdirSync(out,{recursive:true})
const browser=await puppeteer.connect({browserURL:'http://127.0.0.1:9227',defaultViewport:null})
const pages=await browser.pages();const page=pages.find(p=>/tauri\.localhost|localhost/.test(p.url()))||pages[0]
assert(page,'No WebView page')
// Hidden Windows test launches can report a zero viewport until first shown.
await page.setViewport({width:1220,height:820})
const errors=[];page.on('pageerror',e=>errors.push(e.message))
const report=[]
const pause=ms=>new Promise(r=>setTimeout(r,ms))
const check=async(name,fn)=>{try{await fn();report.push({name,status:'passed'})}catch(e){report.push({name,status:'failed',error:String(e)});throw e}}
const invoke=(command,args={})=>page.evaluate((command,args)=>window.__TAURI_INTERNALS__.invoke(command,args),command,args)
const clickText=(selector,text)=>page.evaluate((selector,text)=>{const b=[...document.querySelectorAll(selector)].find(b=>b.textContent.includes(text));if(!b)throw new Error('Missing '+text);b.click()},selector,text)
const field=async(selector,value)=>{await page.$eval(selector,(input,value)=>{input.value=String(value);input.dispatchEvent(new Event('input',{bubbles:true}));input.dispatchEvent(new Event('change',{bubbles:true}))},value)}
const taskFinished=async id=>{for(let i=0;i<80;i++){const s=await invoke('task_poll',{id,after:0});if(!['running','cancelling'].includes(s.status))return s;await pause(150)}throw new Error('Task did not finish')}
const uiTask=async()=>{const before=new Set((await invoke('list_tasks')).map(x=>x.id));await page.click('form button.primary');for(let i=0;i<30;i++){const tasks=await invoke('list_tasks');const task=tasks.find(x=>!before.has(x.id));if(task)return task;await pause(100)}throw new Error('UI did not start task')}
try{
 await page.waitForSelector('nav .nav-item')
 await page.evaluate(()=>localStorage.setItem('netbox-language','zh-CN'));await page.reload()
 await page.waitForSelector('nav .nav-item');await pause(500)
 await check('Native WebView loads bundled assets and all 8 tools, FTP immediately below TFTP',async()=>{assert.deepEqual(await page.$$eval('nav .nav-item b',x=>x.map(e=>e.textContent)),['Ping','Traceroute','TCPing','DNS','NTP','TFTP','FTP','Syslog']);assert((await page.content()).includes('NETWORK TOOLKIT'));assert(await page.evaluate(()=>!!window.__TAURI_INTERNALS__))})
 await check('Native appearance switch persists and keeps readable fields',async()=>{
  await page.click('.appearance-switch button:last-child')
  await page.waitForFunction(()=>document.documentElement.dataset.theme==='readable')
  assert.equal(await page.$eval('#target',e=>getComputedStyle(e).fontSize),'16px')
  await page.reload();await page.waitForSelector('nav .nav-item')
  assert.equal(await page.evaluate(()=>document.documentElement.dataset.theme),'readable')
  assert.equal(await page.$eval('.appearance-switch button:last-child',e=>e.getAttribute('aria-pressed')),'true')
 })
 await page.screenshot({path:path.join(out,'gui-home.png'),fullPage:true})
 await check('GUI Ping uses Rust backend; cursors deliver 4 samples once',async()=>{await field('#count',4);await field('#interval',50);const t=await uiTask();const final=await taskFinished(t.id);assert.equal(final.result.data.received,4);const cursors=final.events.map(x=>x.cursor);assert.equal(new Set(cursors).size,cursors.length);await pause(600);assert.equal(await page.$$eval('tbody tr',x=>x.length),4);await page.screenshot({path:path.join(out,'gui-ping.png'),fullPage:true})})
 await check('Completed native JSON result stays mounted and keeps its scroll',async()=>{
  await page.click('.raw-result > button');await page.waitForSelector('.raw-result pre')
  const before=await page.evaluate(()=>{
   const pre=document.querySelector('.raw-result pre');pre.scrollTop=150;pre.scrollIntoView({block:'center'})
   window.__stablePre=pre;window.__resultChanges=0
   window.__stableObserver=new MutationObserver(m=>window.__resultChanges+=m.length)
   window.__stableObserver.observe(document.querySelector('.result-panel'),{childList:true,subtree:true,characterData:true})
   return {top:pre.scrollTop,page:scrollY}
  })
  assert(before.top>0);await pause(1800)
  const after=await page.evaluate(()=>{
   window.__stableObserver.disconnect();const pre=document.querySelector('.raw-result pre')
   return {same:pre===window.__stablePre,top:pre?.scrollTop,page:scrollY,changes:window.__resultChanges}
  })
  assert(after.same);assert.equal(after.top,before.top);assert.equal(after.page,before.page);assert.equal(after.changes,0)
  await page.click('.raw-result > button')
 })
 await check('Native screenshot renderer captures result cards and validates PNG saving input',async()=>{
  const current=(await invoke('list_tasks'))[0]
  const detail=await invoke('task_poll',{id:current.id,after:0})
  assert(detail.result)
  let rejected=false
  try{await invoke('export_result_image',{png:[1,2,3],filename:'invalid.png'})}catch{rejected=true}
  assert(rejected,'Backend must reject invalid PNG before showing a dialog')
  await field('#target','changed-after-test.invalid')
  await page.evaluate(()=>{
   window.__originalCreateElement=document.createElement.bind(document)
   document.createElement=(tag,...args)=>{
    const element=window.__originalCreateElement(tag,...args)
    if(tag==='canvas'){
     const encode=element.toDataURL.bind(element)
     element.toDataURL=(...options)=>{window.__nativeImage=encode(...options);throw new Error('PNG captured by test before save dialog')}
    }
    return element
   }
  })
  try{
   await clickText('.result-tools button','导出图片')
   await page.waitForFunction(()=>!!window.__nativeImage)
   const data=await page.evaluate(()=>window.__nativeImage)
   const bytes=Buffer.from(data.split(',')[1],'base64')
   assert.deepEqual([...bytes.subarray(0,8)],[137,80,78,71,13,10,26,10])
   fs.writeFileSync(path.join(out,'gui-export-result.png'),bytes)
  }finally{await page.evaluate(()=>{document.createElement=window.__originalCreateElement})}
 })
 await check('English UI and navigation',async()=>{await clickText('.header-actions button','English');await clickText('nav .nav-item','DNS');assert((await page.$eval('h1',e=>e.textContent)).includes('Name resolution'));await field('#target','localhost');const t=await uiTask();const s=await taskFinished(t.id);assert.equal(s.result.data.source,'system');assert(s.result.data.addresses.length>0)})
 await check('GUI TCPing connects to independent local TCP server',async()=>{const server=net.createServer(s=>s.end());await new Promise(r=>server.listen(0,'127.0.0.1',r));try{await clickText('nav .nav-item','TCPing');await field('#target','127.0.0.1');await field('#port',server.address().port);await field('#count',2);await field('#interval',50);const t=await uiTask();const finished=await taskFinished(t.id);assert.equal(finished.result.data.received,2);assert.equal(finished.port,server.address().port)
 await page.waitForSelector('.tcp-port');const expected='TCP port '+server.address().port
 assert.equal(await page.$eval('.tcp-port',e=>e.textContent),expected)
 await field('#port',1);await pause(500);assert.equal(await page.$eval('.tcp-port',e=>e.textContent),expected)
 await clickText('nav .nav-item','Ping');await clickText('.task-row','tcping');await page.waitForSelector('.tcp-port')
 assert.equal(await page.$eval('.tcp-port',e=>e.textContent),expected)
 await page.screenshot({path:path.join(out,'gui-tcping-port.png'),fullPage:true})
 await page.evaluate(()=>{
  window.__tcpOriginalCreate=document.createElement.bind(document)
  document.createElement=(tag,...args)=>{
   const element=window.__tcpOriginalCreate(tag,...args)
   if(tag==='canvas'){
    const encode=element.toDataURL.bind(element)
    element.toDataURL=(...options)=>{window.__tcpImage=encode(...options);throw new Error('Screenshot captured by test')}
   }
   return element
  }
 })
 try{
  await clickText('.result-tools button','Export image');await page.waitForFunction(()=>!!window.__tcpImage)
  const uri=await page.evaluate(()=>window.__tcpImage)
  fs.writeFileSync(path.join(out,'gui-tcping-port-result.png'),Buffer.from(uri.split(',')[1],'base64'))
 }finally{await page.evaluate(()=>{document.createElement=window.__tcpOriginalCreate})}

 }finally{server.close()}})
 await check('GUI ICMP trace reaches loopback',async()=>{await clickText('nav .nav-item','Traceroute');await field('#target','127.0.0.1');await field('#hops',2);const t=await uiTask();assert.equal((await taskFinished(t.id)).result.data.reached,true)})
 await check('GUI NTP measures an independent responder',async()=>{const socket=dgram.createSocket('udp4');await new Promise(r=>socket.bind(0,'127.0.0.1',r));socket.on('message',(q,peer)=>{const b=Buffer.alloc(48);b[0]=0x24;b[1]=2;q.copy(b,24,40,48);const stamp=q.readBigUInt64BE(40)+214748365n;b.writeBigUInt64BE(stamp,32);b.writeBigUInt64BE(stamp,40);socket.send(b,peer.port,peer.address)});try{await clickText('nav .nav-item','NTP');await field('#target','127.0.0.1');await field('#port',socket.address().port);await field('#count',1);const t=await uiTask();assert.equal((await taskFinished(t.id)).result.data.valid_samples,1)}finally{socket.close()}})
 await check('GUI starts/stops Syslog and renders external text without HTML',async()=>{await clickText('nav .nav-item','Syslog');const logPath=path.join(out,'gui-syslog');await field('#path',logPath);await field('#port',15514);const t=await uiTask();await pause(300);const socket=dgram.createSocket('udp4');await new Promise((res,rej)=>socket.send(Buffer.from('<13><script>alert(1)</script> GUI日志'),15514,'127.0.0.1',e=>e?rej(e):res()));socket.close();await pause(800);assert((await page.$eval('tbody',e=>e.textContent)).includes('<script>alert(1)</script>'));assert.equal(await page.$$('tbody script').then(x=>x.length),0);await page.screenshot({path:path.join(out,'gui-syslog.png'),fullPage:true});await clickText('form .stop','Stop');assert.equal((await taskFinished(t.id)).status,'cancelled');assert(fs.readdirSync(path.join(logPath,'netbox-syslog')).some(x=>x.endsWith('.jsonl')))})
 await check('GUI starts and cancels continuous diagnostic',async()=>{await clickText('nav .nav-item','Ping');await page.click('label.check input');const t=await uiTask();await pause(400);await clickText('form .stop','Stop');assert.equal((await taskFinished(t.id)).status,'cancelled')})
 await check('FTP server, listing, binary upload/download and stop through the native frontend',async()=>{
  await clickText('nav .nav-item','FTP');await clickText('.segmented button','Server')
  const dir=path.join(out,`gui-ftp-${Date.now()}`);fs.mkdirSync(dir,{recursive:true})
  const content=Buffer.from('FTP GUI binary\x00\xff 中文');fs.writeFileSync(path.join(dir,'source.bin'),content)
  const reservation=net.createServer();await new Promise(r=>reservation.listen(0,'127.0.0.1',r));const port=reservation.address().port;await new Promise(r=>reservation.close(r))
  await field('#listen','127.0.0.1');await field('#port',port);await field('#path',dir)
  await page.click('form .check input');const server=await uiTask()
  try{
   await page.waitForFunction(()=>document.querySelector('tbody')?.textContent.includes('ready'))
   await clickText('.segmented button','List');const list=await uiTask();assert((await taskFinished(list.id)).result.data.entries.some(x=>x.includes('source.bin')))
   await clickText('.segmented button','Download');const dest=path.join(out,`gui-ftp-download-${Date.now()}.bin`);await field('#path',dest);await field('#remote','source.bin');const get=await uiTask();assert.equal((await taskFinished(get.id)).status,'succeeded');assert.deepEqual(fs.readFileSync(dest),content)
   await clickText('.segmented button','Upload');await field('#remote','uploaded.bin');const put=await uiTask();assert.equal((await taskFinished(put.id)).status,'succeeded');assert.deepEqual(fs.readFileSync(path.join(dir,'uploaded.bin')),content)
   await clickText('.segmented button','Server');assert(await page.$eval('form .primary',e=>e.disabled));await clickText('form .stop','Stop');assert.equal((await taskFinished(server.id)).status,'cancelled')
  }finally{await invoke('cancel_task',{id:server.id});await taskFinished(server.id)}
 })
 await check('TFTP server lifecycle through the native frontend',async()=>{await clickText('nav .nav-item','TFTP');await clickText('.segmented button','Server');const dir=path.join(out,'gui-tftp');fs.mkdirSync(dir,{recursive:true});await field('#path',dir);await field('#port',15669);const t=await uiTask();await pause(300);const s=await invoke('task_poll',{id:t.id,after:0});assert(s.events.some(e=>e.event_type==='ready'));await clickText('form .stop','Stop');assert.equal((await taskFinished(t.id)).status,'cancelled')})
 await check('Backend rejects a fifth concurrent task and stop-all cleans up',async()=>{const started=[];try{for(let i=0;i<4;i++)started.push(await invoke('start_task',{request:{tool:'ping',target:'127.0.0.1',count:0,interval_ms:1000}}));let rejected=false;try{await invoke('start_task',{request:{tool:'ping',target:'127.0.0.1'}})}catch{rejected=true}assert(rejected);await invoke('stop_all');for(const id of started)assert.equal((await taskFinished(id)).status,'cancelled')}finally{await invoke('stop_all')}})
 await check('GUI NTP Server responds, supports concurrent client mode and releases its port',async()=>{
  const reservation=dgram.createSocket('udp4')
  await new Promise(resolve=>reservation.bind(0,'127.0.0.1',resolve))
  const port=reservation.address().port
  await new Promise(resolve=>reservation.close(resolve))
  await clickText('nav .nav-item','NTP');await clickText('.segmented button','NTP Server')
  assert.equal(await page.$('#target'),null);assert.equal(await page.$('#count'),null)
  await field('#listen','127.0.0.1');await field('#port',port);await field('#ntp-stratum',7)
  const server=await uiTask()
  try{
   await page.waitForFunction(()=>document.querySelector('form .primary').disabled)
   for(let i=0;i<30;i++){const snapshot=await invoke('task_poll',{id:server.id,after:0});if(snapshot.events.some(e=>e.event_type==='ready'))break;assert.equal(snapshot.status,'running');await pause(100)}
   const socket=dgram.createSocket('udp4')
   try{
    const query=Buffer.alloc(48);query[0]=0x23;query.writeBigUInt64BE(123456789n,40)
    const response=await new Promise((resolve,reject)=>{
     const timer=setTimeout(()=>reject(new Error('No NTP server response')),2000)
     socket.once('error',error=>{clearTimeout(timer);reject(error)})
     socket.once('message',body=>{clearTimeout(timer);resolve(body)})
     socket.send(query,port,'127.0.0.1')
    })
    assert.equal(response.length,48);assert.equal(response[0],0x24);assert.equal(response[1],7)
    assert.equal(response.readBigUInt64BE(24),123456789n)
   }finally{socket.close()}
   await page.waitForFunction(()=>[...document.querySelectorAll('.metrics b')].slice(0,2).every(e=>e.textContent==='1'))
   await clickText('.segmented button','Client');await field('#target','127.0.0.1');await field('#count',1)
   const client=await uiTask();assert.equal((await taskFinished(client.id)).result.data.valid_samples,1)
   await clickText('.task-row',`127.0.0.1:${port}`)
   await page.waitForSelector('#ntp-stratum')
   assert.equal(await page.$eval('form .primary',e=>e.disabled),true)
   await page.screenshot({path:path.join(out,'gui-ntp-server.png'),fullPage:true})
   await clickText('form .stop','Stop');assert.equal((await taskFinished(server.id)).status,'cancelled')
   await page.waitForFunction(()=>!document.querySelector('form .primary').disabled)
   const released=dgram.createSocket('udp4');await new Promise((resolve,reject)=>{released.once('error',reject);released.bind(port,'127.0.0.1',resolve)});released.close()
  }finally{await invoke('cancel_task',{id:server.id})}
 })
 await check('Frontend JavaScript has no uncaught errors',async()=>assert.deepEqual(errors,[]))
 await invoke('stop_all')
 await clickText('.header-actions button','中文');await clickText('nav .nav-item','Ping');await pause(300)
 await page.screenshot({path:path.join(out,'gui-readable-home.png'),fullPage:true})
 await page.click('.appearance-switch button:first-of-type')
 await page.waitForFunction(()=>document.documentElement.dataset.theme==='fresh')
 const metrics=await page.evaluate(()=>({width:innerWidth,height:innerHeight,documentWidth:document.documentElement.scrollWidth,bodyHeight:document.body.scrollHeight}))
 assert(metrics.documentWidth<=metrics.width,'Horizontal overflow')
 fs.writeFileSync(path.join(out,'gui.json'),JSON.stringify({url:page.url(),report,metrics,errors},null,2))
 console.log(JSON.stringify({passed:report.length,total:report.length,metrics,report},null,2))
}catch(e){fs.writeFileSync(path.join(out,'gui.json'),JSON.stringify({url:page.url(),report,errors},null,2));await page.screenshot({path:path.join(out,'gui-failure.png'),fullPage:true});throw e}
finally{await browser.disconnect()}
