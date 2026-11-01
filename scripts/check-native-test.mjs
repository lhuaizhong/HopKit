// Real WebView + Rust backend regression for Windows connect completion.
// Start a test desktop instance with --remote-debugging-port=9239 in
// WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS, then run this script.
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import {createRequire} from 'node:module'
const require=createRequire(import.meta.url)
const {chromium}=require(process.env.HOPKIT_PLAYWRIGHT??'C:/Users/lhz/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright')
const browser=await chromium.connectOverCDP(process.env.HOPKIT_CDP??'http://127.0.0.1:9239')
const page=browser.contexts()[0].pages()[0]
const target=process.env.HOPKIT_TLS_TARGET??'https://www.qq.com/'
const invoke=(command,args={})=>page.evaluate(({command,args})=>window.__TAURI_INTERNALS__.invoke(command,args),{command,args})
const reports=[]
try {
 await page.waitForSelector('#check-target')
 await page.locator('#check-target').fill(target)
 if(!await page.locator('#check-timeout').isVisible())await page.locator('.check-setup form details summary').click()
 await page.locator('#check-timeout').fill('5000')
 await page.locator('#check-source').selectOption('')
 await page.locator('.check-setup input[type="checkbox"]').first().uncheck()
 for(let run=0;run<3;run++) {
  if(run===2) {
   await page.locator('#check-source').selectOption('')
   await page.locator('.check-setup input[type="checkbox"]').first().check()
  }
  await page.locator('.check-setup button[type="submit"]').waitFor({state:'visible'})
  const old=new Set((await invoke('list_tasks')).map(t=>t.id))
  await page.locator('.check-setup button[type="submit"]').click()
  let task
  for(let i=0;i<300;i++) {
   const created=(await invoke('list_tasks')).find(t=>!old.has(t.id)&&t.tool==='check')
   if(created)task=await invoke('task_poll',{id:created.id,after:0})
   if(task?.result)break
   await new Promise(r=>setTimeout(r,100))
  }
  assert.equal(task?.result?.status,'succeeded',JSON.stringify(task))
  const evidence=task.result.data.evidence
  const tls=evidence.find(e=>e.id==='tls_0'),http=evidence.find(e=>e.id==='http_0')
  assert.equal(tls?.status,'observed',JSON.stringify(evidence))
  assert(tls.data.certificate_validation.startsWith('passed_by_rustls'))
  assert.equal(task.request.insecure_tls,false)
  assert.equal(http?.data?.status_code,200,JSON.stringify(evidence))
  const connection=evidence.find(e=>e.id==='connect_0')
  assert(!['::','0.0.0.0'].includes(connection.data.local_address))
  if(task.request.source_address)assert.equal(connection.data.local_address,task.request.source_address)
  if(run===2)assert(connection.data.remote_address.includes(':'))
  assert(!JSON.stringify(task).includes('os error 10057'))
  reports.push(task)
  await page.locator('#evidence-http_0').waitFor()
  console.log(JSON.stringify({run,ipv6:task.request.ipv6,source:task.request.source_address,tls:tls.status,protocol:tls.data.protocol,http:http.data.status_code}))
 }
 await page.screenshot({path:'test-output/check/native-tls-fixed.png',fullPage:true})
 await fs.writeFile('test-output/check/native-tls-fixed.json',JSON.stringify(reports,null,2))
} finally {await browser.close()}
