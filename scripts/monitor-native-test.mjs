import fs from 'node:fs/promises'
import assert from 'node:assert/strict'
import {createRequire} from 'node:module'
import net from 'node:net'
const require=createRequire(import.meta.url)
const {chromium}=require(process.env.HOPKIT_PLAYWRIGHT??'C:/Users/lhz/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright')
for(let i=0;i<100;i++){try{await fetch('http://127.0.0.1:9237/json/version');break}catch{await new Promise(r=>setTimeout(r,200))}}
const browser=await chromium.connectOverCDP('http://127.0.0.1:9237')
const page=browser.contexts()[0].pages()[0];await page.setViewportSize({width:1440,height:1050})
const invoke=(command,args={})=>page.evaluate(({command,args})=>window.__TAURI_INTERNALS__.invoke(command,args),{command,args})
const wait=ms=>new Promise(r=>setTimeout(r,ms))
const errors=[];page.on('pageerror',e=>errors.push(String(e)))
const report=[]
const listener=net.createServer(socket=>socket.end());await new Promise(resolve=>listener.listen(0,'127.0.0.1',resolve))
const port=listener.address().port
try{
 await page.waitForSelector('.nav-item')
 await page.evaluate(()=>localStorage.setItem('hopkit-language','zh-CN'));await page.reload();await page.waitForSelector('.nav-item')
 await page.locator('.nav-item').filter({hasText:'Monitor'}).click()
 for(let i=0;i<3;i++)await page.locator('#monitor-target-'+i).fill('127.0.0.1')
 await page.locator('#monitor-port-2').fill(String(port))
 await page.locator('#monitor-timeout').fill('200')
 await page.getByRole('button',{name:'开始监测',exact:false}).click()
 await page.waitForSelector('.monitor-chart',{timeout:20000})
 let tasks=await invoke('list_tasks');const monitor=tasks.find(t=>t.tool==='monitor'&&t.status==='running');assert(monitor)
 const deadline=Date.now()+20000
 while((await invoke('task_poll',{id:monitor.id,after:0})).latest_stats?.sample_count<3){assert(Date.now()<deadline);await wait(200)}
 report.push('Native GUI creates three-target monitor with real ICMP and TCP evidence')
 // A monitor and integrated checks share the existing four top-level slots.
 const request={tool:'tcping',target:'127.0.0.1',port,count:0,interval_ms:1000,timeout_ms:200}
 const ids=[];for(let i=0;i<3;i++)ids.push(await invoke('start_task',{request}))
 let rejected=false;try{await invoke('start_task',{request:{tool:'check',target:'127.0.0.1'}})}catch(e){rejected=String(e).includes('4')}
 assert(rejected,'The fifth task bypassed the shared budget')
 await page.getByRole('button',{name:'暂停监测',exact:true}).click()
 await page.getByRole('button',{name:'继续监测',exact:true}).waitFor()
 const paused=(await invoke('task_poll',{id:monitor.id,after:0})).latest_stats.sample_count
 await wait(1400)
 assert.equal((await invoke('task_poll',{id:monitor.id,after:0})).latest_stats.sample_count,paused)
 await page.locator('#monitor-note').fill('切换接口核查 <script>literal</script>')
 await page.getByRole('button',{name:'添加事件',exact:true}).click()
 await page.getByText('切换接口核查 <script>literal</script>',{exact:true}).waitFor()
 await page.getByRole('button',{name:'继续监测',exact:true}).click()
 await page.getByRole('button',{name:'暂停监测',exact:true}).waitFor()
 report.push('Four-task admission, pause without invented failures, resume and escaped notes')
 // Navigation must recover the backend timeline, not reconstruct counters from UI events.
 await page.locator('.nav-item').filter({hasText:'TCPing'}).click()
 const row=page.locator('.task-row').filter({hasText:'monitor'})
 await row.getByRole('button',{name:'打开',exact:true}).click()
 await page.waitForSelector('.monitor-chart')
 assert.equal(await page.locator('.monitor-chart').count(),3)
 await page.screenshot({path:'test-output/monitor-native.png',fullPage:true})
 await invoke('cancel_task',{id:monitor.id})
 for(const id of ids)await invoke('cancel_task',{id})
 let detail
 for(let i=0;i<60;i++){detail=await invoke('task_poll',{id:monitor.id,after:0});if(detail.result)break;await wait(200)}
 assert.equal(detail.result.status,'cancelled');assert(detail.result.data.sample_count>=paused)
 assert(detail.result.data.records.some(e=>e.reason==='manual_pause'))
 assert.equal(detail.events.length,0,'Large monitor snapshots must not accumulate in event queues')
 report.push('Switch away/reopen retains timeline; Stop retains partial evidence; no duplicated snapshot queue')
 await fs.writeFile('test-output/monitor-native.json',JSON.stringify({status:'passed',report,errors,totals:detail.result.data.totals},null,2))
 assert.equal(errors.length,0,errors.join('\n'))
 console.log(report.join('\n'))
}finally{await invoke('stop_all').catch(()=>{});listener.close();await browser.close()}
