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
 await page.waitForSelector('nav .nav-item');await pause(500)
 await check('Native WebView loads bundled assets and all 7 tools',async()=>{assert.equal(await page.$$eval('nav .nav-item',x=>x.length),7);assert((await page.content()).includes('NETWORK TOOLKIT'));assert(await page.evaluate(()=>!!window.__TAURI_INTERNALS__))})
 await page.screenshot({path:path.join(out,'gui-home.png'),fullPage:true})
 await check('GUI Ping uses Rust backend; cursors deliver 4 samples once',async()=>{await field('#count',4);await field('#interval',50);const t=await uiTask();const final=await taskFinished(t.id);assert.equal(final.result.data.received,4);const cursors=final.events.map(x=>x.cursor);assert.equal(new Set(cursors).size,cursors.length);await pause(600);assert.equal(await page.$$eval('tbody tr',x=>x.length),4);await page.screenshot({path:path.join(out,'gui-ping.png'),fullPage:true})})
 await check('English UI and navigation',async()=>{await clickText('.header-actions button','English');await clickText('nav .nav-item','DNS');assert((await page.$eval('h1',e=>e.textContent)).includes('Name resolution'));await field('#target','localhost');const t=await uiTask();const s=await taskFinished(t.id);assert.equal(s.result.data.source,'system');assert(s.result.data.addresses.length>0)})
 await check('GUI TCPing connects to independent local TCP server',async()=>{const server=net.createServer(s=>s.end());await new Promise(r=>server.listen(0,'127.0.0.1',r));try{await clickText('nav .nav-item','TCPing');await field('#target','127.0.0.1');await field('#port',server.address().port);await field('#count',2);await field('#interval',50);const t=await uiTask();assert.equal((await taskFinished(t.id)).result.data.received,2)}finally{server.close()}})
 await check('GUI starts/stops Syslog and renders external text without HTML',async()=>{await clickText('nav .nav-item','Syslog');const logPath=path.join(out,'gui-syslog');await field('#path',logPath);await field('#port',15514);const t=await uiTask();await pause(300);const socket=dgram.createSocket('udp4');await new Promise((res,rej)=>socket.send(Buffer.from('<13><script>alert(1)</script> GUI日志'),15514,'127.0.0.1',e=>e?rej(e):res()));socket.close();await pause(800);assert((await page.$eval('tbody',e=>e.textContent)).includes('<script>alert(1)</script>'));assert.equal(await page.$$('tbody script').then(x=>x.length),0);await page.screenshot({path:path.join(out,'gui-syslog.png'),fullPage:true});await clickText('form .stop','Stop');assert.equal((await taskFinished(t.id)).status,'cancelled');assert(fs.readdirSync(path.join(logPath,'netbox-syslog')).some(x=>x.endsWith('.jsonl')))})
 await check('GUI starts and cancels continuous diagnostic',async()=>{await clickText('nav .nav-item','Ping');await page.click('label.check input');const t=await uiTask();await pause(400);await clickText('form .stop','Stop');assert.equal((await taskFinished(t.id)).status,'cancelled')})
 await check('TFTP server lifecycle through the native frontend',async()=>{await clickText('nav .nav-item','TFTP');await clickText('.segmented button','Server');const dir=path.join(out,'gui-tftp');fs.mkdirSync(dir,{recursive:true});await field('#path',dir);await field('#port',15669);const t=await uiTask();await pause(300);const s=await invoke('task_poll',{id:t.id,after:0});assert(s.events.some(e=>e.event_type==='ready'));await clickText('form .stop','Stop');assert.equal((await taskFinished(t.id)).status,'cancelled')})
 await check('Frontend JavaScript has no uncaught errors',async()=>assert.deepEqual(errors,[]))
 await invoke('stop_all')
 await clickText('.header-actions button','中文');await clickText('nav .nav-item','Ping');await pause(300)
 const metrics=await page.evaluate(()=>({width:innerWidth,height:innerHeight,documentWidth:document.documentElement.scrollWidth,bodyHeight:document.body.scrollHeight}))
 assert(metrics.documentWidth<=metrics.width,'Horizontal overflow')
 fs.writeFileSync(path.join(out,'gui.json'),JSON.stringify({url:page.url(),report,metrics,errors},null,2))
 console.log(JSON.stringify({passed:report.length,total:report.length,metrics,report},null,2))
}catch(e){fs.writeFileSync(path.join(out,'gui.json'),JSON.stringify({url:page.url(),report,errors},null,2));await page.screenshot({path:path.join(out,'gui-failure.png'),fullPage:true});throw e}
finally{await browser.disconnect()}
