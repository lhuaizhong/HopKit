// Browser fixture only. Real backend/protocol coverage is separate.
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import {createRequire} from 'node:module'
import {createServer} from 'vite'
const require=createRequire(import.meta.url)
const {chromium}=require(process.env.HOPKIT_PLAYWRIGHT??'/Users/lhz/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright')
const server=await createServer({server:{host:'127.0.0.1',port:1420,strictPort:true}});await server.listen()
const browser=await chromium.launch({headless:true,executablePath:process.env.HOPKIT_BROWSER??'/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge'})
const context=await browser.newContext({locale:'zh-CN',permissions:['clipboard-read','clipboard-write']});const page=await context.newPage();const errors=[];page.on('pageerror',e=>errors.push(String(e)))
await fs.mkdir('test-output/wormhole',{recursive:true})
try{
 await page.addInitScript(()=>{
  let task;let op='';window.fixture={requests:[],exports:[],png:[],actions:[]};window.__TAURI_INTERNALS__={invoke:async(cmd,args)=>{
   if(cmd==='app_info')return{os:'macOS',version:'test'}
   if(cmd==='list_tasks')return task?[{...task,events:[],result:null}]:[]
   if(cmd==='task_poll')return structuredClone(task)
   if(cmd==='choose_path')return args.kind==='file'?'/tmp/中文 file.bin':'/tmp/received.bin'
   if(cmd==='wormhole_file_info')return{name:'中文 file.bin',size:'9007199254740993'}
   if(cmd==='start_task'){
    const req=structuredClone(args.request);window.fixture.requests.push(req);op=req.wormhole.operation;delete req.wormhole.code;
    task={id:'fixture-'+Date.now(),tool:'wormhole',action:op,target:op,status:'running',seq:1,request:req,events:[],result:null,started_at:'2026',latest_stats:{operation:op,phase:op==='send'?'waiting_peer':'waiting_acceptance',decision_id:op==='send'?null:'offer-1',deadline_ms:Date.now()+300000,file_name:'<script>bad()</script> 中文 file.bin',suggested_name:'safe.bin',bytes:'0',total_bytes:'9007199254740993',notice:'<script>bad()</script>',notice_source:'ws://local/v1'}};return task.id
   }
   if(cmd==='wormhole_private_state')return{code:op==='send'&&task.status==='running'?'42-sensitive-words-alpha-bravo':null}
   if(cmd==='wormhole_action'){window.fixture.actions.push(args);task.latest_stats={...task.latest_stats,phase:args.action==='reject'?'waiting_acceptance':'completed',bytes:'9007199254740993',local_saved:args.action!=='reject',save_path:args.savePath,sha256:'abc'};task.status=args.action==='reject'?'failed':'succeeded';task.result={data:task.latest_stats};task.seq++;return}
   if(cmd==='cancel_task'){task.status='cancelled';task.result={data:task.latest_stats};task.seq++;return}
   if(cmd==='export_result'){window.fixture.exports.push(args.content);return}
   if(cmd==='export_result_image'){window.fixture.png.push(args.png);return true}
   return null
  }}
 })
 await page.goto('http://127.0.0.1:1420/');await page.locator('.nav-item').first().waitFor()
 const names=await page.locator('.nav-item b').allTextContents();assert.equal(names[names.indexOf('HTTP')+1],'Wormhole');assert.equal(names[names.indexOf('Wormhole')+1],'Syslog')
 await page.locator('.nav-item').filter({has:page.getByText('Wormhole',{exact:true})}).click();assert.equal(await page.evaluate(()=>window.fixture.requests.length),0)
 await page.locator('#wormhole-file').fill('/tmp/中文 file.bin');await page.locator('.wormhole-setup button[type=submit]').click();await page.locator('.wormhole-secret').waitFor();assert((await page.locator('.wormhole-secret').innerText()).includes('42-sensitive'))
 await page.getByRole('button',{name:'导出 JSON',exact:true}).click();const exported=await page.evaluate(()=>window.fixture.exports[0]);assert(!exported.includes('42-sensitive'));assert(!exported.includes('verifier'))
 await page.locator('.wormhole-results .stop').click();await page.waitForFunction(()=>!document.querySelector('.wormhole-secret'))
 await page.getByRole('button',{name:'接收文件',exact:true}).click();await page.locator('#wormhole-code').fill('7-receive-secret');await page.locator('.wormhole-setup button[type=submit]').click();await page.waitForFunction(()=>document.querySelector('.wormhole-controls')?.textContent.includes('接受并保存'))
 assert.equal(await page.locator('#wormhole-code').inputValue(),'');assert.equal(await page.locator('.wormhole-results script').count(),0)
 await page.getByRole('button',{name:'接受并保存',exact:true}).click();await page.waitForFunction(()=>document.querySelector('.wormhole-results')?.textContent.includes('SHA-256'))
 assert.equal((await page.evaluate(()=>window.fixture.actions[0])).savePath,'/tmp/received.bin')
 await page.getByRole('button',{name:'导出 JSON',exact:true}).click();assert(!(await page.evaluate(()=>window.fixture.exports.at(-1))).includes('7-receive-secret'))
 await page.getByRole('button',{name:'导出图片',exact:true}).click();await page.waitForFunction(()=>window.fixture.png.length>0);await fs.writeFile('test-output/wormhole/result-export.png',Buffer.from(await page.evaluate(()=>window.fixture.png[0])))
 for(const width of [960,1220]){await page.setViewportSize({width,height:680});for(const appearance of ['fresh','readable']){await page.evaluate(a=>localStorage.setItem('hopkit-appearance',a),appearance);await page.reload();await page.locator('.nav-item').first().waitFor();await page.locator('.nav-item').filter({has:page.getByText('Wormhole',{exact:true})}).click();assert(await page.locator('.wormhole-setup').isVisible());await page.locator('.nav-item').filter({has:page.getByText('Wormhole',{exact:true})}).scrollIntoViewIfNeeded();await page.screenshot({path:`test-output/wormhole/ui-${width}-${appearance}.png`,fullPage:true});assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth))}}
 await page.getByRole('button',{name:'English',exact:true}).click();assert((await page.locator('.wormhole-setup').innerText()).includes('Start sending'));assert.equal(errors.length,0,errors.join('\n'))
 await fs.writeFile('test-output/wormhole/ui.json',JSON.stringify({status:'passed',cases:['navigation order','no startup networking','private code','JSON redaction','receive code cleared','escaped offer/notice','accept/save IPC','image export','960/1220 both appearances','English'],errors},null,2));console.log('Wormhole UI passed')
}finally{await browser.close();await server.close()}
