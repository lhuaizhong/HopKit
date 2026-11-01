// Run against a test instance with WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=
// --remote-debugging-port=9238. Uses the actual desktop backend and result table.
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import {createRequire} from 'node:module'
const require=createRequire(import.meta.url)
const {chromium}=require(process.env.HOPKIT_PLAYWRIGHT??'C:/Users/lhz/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright')
const browser=await chromium.connectOverCDP(process.env.HOPKIT_CDP??'http://127.0.0.1:9238')
const page=browser.contexts()[0].pages()[0]
const target=process.env.HOPKIT_TCPING_TARGET??'10.246.128.3'
const port=process.env.HOPKIT_TCPING_PORT??'80'
const invoke=(command,args={})=>page.evaluate(({command,args})=>window.__TAURI_INTERNALS__.invoke(command,args),{command,args})
const reports=[]
try {
 await page.waitForSelector('.nav-item')
 await page.locator('.nav-item').filter({hasText:'TCPing'}).click()
 await page.locator('#target').fill(target)
 await page.locator('#port').fill(port)
 await page.locator('#count').fill('5')
 await page.locator('#interval').fill('1000')
 await page.locator('#timeout').fill('1000')
 for(let run=0;run<3;run++) {
  const old=new Set((await invoke('list_tasks')).map(t=>t.id))
  await page.locator('button[type="submit"]').click()
  let task
  for(let i=0;i<150;i++) {
   const created=(await invoke('list_tasks')).find(t=>!old.has(t.id)&&t.tool==='tcping')
   if(created)task=await invoke('task_poll',{id:created.id,after:0})
   if(task?.result)break
   await new Promise(r=>setTimeout(r,100))
  }
  assert.equal(task?.result?.status,'succeeded')
  const data=task.result.data
  assert.equal(data.sent,5);assert.equal(data.received,5)
  assert.equal(data.samples.length,5)
  for(const [i,s] of data.samples.entries()) {
   assert.equal(s.seq,i+1)
   assert.equal(s.rtt_source,'tcp_info','Windows TCP_INFO unavailable; investigate fallback')
   assert(s.rtt_ms>0);assert(s.connect_elapsed_ms>0)
  }
  reports.push(task)
  console.log(JSON.stringify(data.samples.map(s=>({seq:s.seq,rtt_ms:s.rtt_ms,connect_elapsed_ms:s.connect_elapsed_ms,source:s.rtt_source}))))
 }
 await page.waitForFunction(()=>document.querySelectorAll('table tbody tr').length===5,null,{timeout:10000})
 assert.equal(await page.locator('table tbody tr').count(),5)
 await page.screenshot({path:'test-output/tcping-native-fixed.png',fullPage:true})
 await fs.writeFile('test-output/tcping-native-fixed.json',JSON.stringify(reports,null,2))
} finally {await browser.close()}
