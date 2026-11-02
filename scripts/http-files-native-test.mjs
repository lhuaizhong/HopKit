// End-to-end checks against a real desktop WebView and a separate web browser.
// Start a test app with WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9243.
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import path from 'node:path'
import net from 'node:net'
import {createRequire} from 'node:module'
const require=createRequire(import.meta.url)
const {chromium}=require(process.env.HOPKIT_PLAYWRIGHT??'C:/Users/lhz/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright')
const root=path.resolve('test-output/http-files/shared'),out=path.resolve('test-output/http-files')
await fs.mkdir(path.join(root,'子目录'),{recursive:true})
await fs.writeFile(path.join(root,'中文 & file.txt'),'hello from HopKit\n')
await fs.writeFile(path.join(root,'子目录','nested.txt'),'nested file')
const unique=Date.now().toString()
const payloads=[{name:`上传-${unique}.txt`,mimeType:'text/plain',buffer:Buffer.from('browser upload 中文')},{name:`empty-${unique}.bin`,mimeType:'application/octet-stream',buffer:Buffer.alloc(0)},{name:`large-${unique}.bin`,mimeType:'application/octet-stream',buffer:Buffer.alloc(8*1024*1024,0x5a)}]
const desktop=await chromium.connectOverCDP(process.env.HOPKIT_CDP??'http://127.0.0.1:9243')
const page=desktop.contexts()[0].pages()[0]
const browser=await chromium.launch({headless:true,executablePath:process.env.HOPKIT_BROWSER??'C:/Users/lhz/AppData/Local/ms-playwright/chromium-1223/chrome-win64/chrome.exe'})
const context=await browser.newContext({locale:'zh-CN',acceptDownloads:true})
const web=await context.newPage(),errors=[],reports=[]
page.on('pageerror',e=>errors.push(String(e)));web.on('pageerror',e=>errors.push(String(e)))
const invoke=(command,args={})=>page.evaluate(({command,args})=>window.__TAURI_INTERNALS__.invoke(command,args),{command,args})
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms))
async function waitTask(id,done=false){for(let i=0;i<100;i++){const task=await invoke('task_poll',{id,after:0});if(done?task.result:task.events.some(e=>e.event_type==='ready'))return task;await delay(100)}throw Error('Task did not become '+(done?'finished':'ready'))}
async function start(){const old=new Set((await invoke('list_tasks')).map(t=>t.id));await page.locator('button[type=submit]').click();for(let i=0;i<50;i++){const task=(await invoke('list_tasks')).find(t=>t.tool==='http'&&!old.has(t.id));if(task)return waitTask(task.id);await delay(100)}throw Error('No HTTP task')}
async function stop(id){await page.locator('.run-actions button.stop').click();const task=await waitTask(id,true);assert.equal(task.status,'cancelled');reports.push(task);return task}
let active=''
try{
 await page.waitForSelector('.nav-item')
 await page.evaluate(()=>localStorage.setItem('hopkit-language','zh-CN'));await page.reload();await page.waitForSelector('.nav-item')
 const names=await page.locator('nav .nav-item b').allTextContents();assert.equal(names[names.indexOf('FTP')+1],'HTTP')
 await page.locator('nav .nav-item').filter({has:page.getByText('HTTP',{exact:true})}).click()
 assert.equal(await page.locator('#port').inputValue(),'80');assert.equal(await page.locator('#http-writable').isChecked(),false)
 await page.locator('#path').fill(root);await page.locator('#listen').fill('0.0.0.0')
 let task=await start();active=task.id;assert.equal(task.request.port,80);assert.equal(task.request.writable,false)
 await page.locator('.file-url').first().waitFor();assert.equal(await page.locator('.run-actions button.primary').isDisabled(),true)
 await web.goto('http://127.0.0.1/');await web.locator('#files a').filter({hasText:'中文 & file.txt'}).waitFor()
 assert.equal(await web.locator('#upload').isVisible(),false)
 assert.equal(await web.locator('#empty').isVisible(),false)
 const download=web.waitForEvent('download');await web.getByRole('link',{name:'中文 & file.txt',exact:true}).click();const file=await download
 assert.equal(file.suggestedFilename(),'中文 & file.txt');await file.saveAs(path.join(out,'download.txt'));assert.equal(await fs.readFile(path.join(out,'download.txt'),'utf8'),'hello from HopKit\n')
 await web.getByRole('link',{name:'▣ 子目录/'}).click();await web.getByRole('link',{name:'nested.txt',exact:true}).waitFor();await web.locator('#parent').click();await web.getByRole('link',{name:'中文 & file.txt',exact:true}).waitFor()
 await page.screenshot({path:path.join(out,'desktop-read-only.png'),fullPage:true});await web.screenshot({path:path.join(out,'browser-read-only.png'),fullPage:true})
 const readOnly=await stop(task.id);active='';assert.equal(readOnly.result.data.downloads,1)
 console.log('PASS default port 80, read-only browsing, UTF-8 download, subdirectories, stop')
 const port=await new Promise(resolve=>{const s=net.createServer();s.listen(0,'127.0.0.1',()=>{const p=s.address().port;s.close(()=>resolve(p))})})
 await page.locator('#port').fill(String(port));await page.locator('#http-writable').check()
 task=await start();active=task.id;const url=`http://127.0.0.1:${port}/`;await web.goto(url);await web.locator('#upload:not([hidden])').waitFor()
 assert.equal(await web.locator('#cancel').isVisible(),false)
 await web.locator('#file-input').setInputFiles(payloads);await web.locator('#send').click();await web.locator('#message').filter({hasText:'上传完成：3 / 3'}).waitFor({timeout:30000})
 for(const item of payloads)assert.deepEqual(await fs.readFile(path.join(root,item.name)),item.buffer)
 assert.equal(await web.locator('#cancel').isVisible(),false)
 await web.locator('#file-input').setInputFiles([{...payloads[0],buffer:Buffer.from('must not overwrite')}]);await web.locator('#send').click();await web.locator('#queue').filter({hasText:'同名文件已存在'}).waitFor();assert.deepEqual(await fs.readFile(path.join(root,payloads[0].name)),payloads[0].buffer)
 // Throttle a real browser upload, cancel it, and verify no partial file remains.
 const cdp=await context.newCDPSession(web);await cdp.send('Network.enable')
 await cdp.send('Network.emulateNetworkConditions',{offline:false,latency:100,downloadThroughput:-1,uploadThroughput:32768})
 const cancelledName=`cancelled-${unique}.bin`
 await web.locator('#file-input').setInputFiles([{name:cancelledName,mimeType:'application/octet-stream',buffer:Buffer.alloc(2*1024*1024)}]);await web.locator('#send').click()
 await web.waitForFunction(()=>document.querySelector('#progress').value>0);await web.locator('#cancel').click()
 await web.locator('#queue').filter({hasText:'已取消'}).waitFor()
 await cdp.send('Network.emulateNetworkConditions',{offline:false,latency:0,downloadThroughput:-1,uploadThroughput:-1});await cdp.detach()
 await delay(500);assert.equal((await fs.readdir(root)).some(name=>name===cancelledName||name.endsWith('.part')),false)
 await web.locator('#language').click();assert.equal(await web.locator('h1').textContent(),'File sharing')
 await web.getByRole('link',{name:'▣ 子目录/'}).click();await web.getByRole('link',{name:'nested.txt',exact:true}).waitFor()
 const nested={name:`nested-upload-${unique}.txt`,mimeType:'text/plain',buffer:Buffer.from('nested browser upload')}
 await web.locator('#file-input').setInputFiles([nested]);await web.locator('#send').click();await web.locator('#message').filter({hasText:'Uploaded: 1 / 1'}).waitFor();assert.deepEqual(await fs.readFile(path.join(root,'子目录',nested.name)),nested.buffer)
 await web.locator('#language').click();await web.locator('#parent').click();await web.getByRole('link',{name:'中文 & file.txt',exact:true}).waitFor()
 await web.setViewportSize({width:390,height:844});assert(await web.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));await web.screenshot({path:path.join(out,'browser-mobile.png'),fullPage:true})
 await web.setViewportSize({width:1100,height:820});await web.screenshot({path:path.join(out,'browser-upload.png'),fullPage:true})
 await page.waitForFunction(()=>document.querySelector('.file-counts>div:nth-child(2) b')?.textContent==='4')
 await page.waitForFunction(()=>document.querySelectorAll('.file-url').length>0,{},{timeout:20000})
 const displayedUrls=await page.locator('.file-url code').allTextContents()
 assert(displayedUrls.every(url=>!new URL(url).hostname.startsWith('127.')&&new URL(url).hostname!=='[::1]'))
 assert.equal(await page.locator('.filter-bar small').count(),0)
 const interfaceUrl=displayedUrls[0]
 assert.equal((await fetch(interfaceUrl+'api/config')).status,200)
 await page.locator('.appearance-switch button:last-child').click()
 assert.equal(await page.locator('.file-service-results').evaluate(e=>getComputedStyle(e).fontSize),'16px')
 await page.screenshot({path:path.join(out,'desktop-readable.png'),fullPage:true})
 await page.locator('.appearance-switch button:first-of-type').click()
 await page.screenshot({path:path.join(out,'desktop-upload.png'),fullPage:true})
 const done=await stop(task.id);active='';assert.equal(done.result.data.uploads,4);assert.equal(done.result.data.bytes,payloads.reduce((sum,p)=>sum+p.buffer.length,0)+nested.buffer.length)
 assert.equal((await fs.readdir(root)).some(name=>name.endsWith('.part')),false)
 assert.deepEqual(errors,[])
 await fs.writeFile(path.join(out,'native-report.json'),JSON.stringify({reports,errors},null,2))
 console.log('PASS configurable port, multiple/empty/8-MiB/subdirectory uploads, no overwrite, browser cancellation, bilingual/mobile UI, statistics, cleanup')
}finally{if(active)await invoke('cancel_task',{id:active}).catch(()=>{});await browser.close();await desktop.close()}
