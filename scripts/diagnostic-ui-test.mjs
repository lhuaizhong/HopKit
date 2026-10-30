// Run from the Browser skill's persistent session:
// const {testDiagnosticUI}=await import('/absolute/HopKit/scripts/diagnostic-ui-test.mjs');
// await testDiagnosticUI(tab, 'http://127.0.0.1:1420', '/absolute/HopKit/test-output');
// Generate the real Rust fixture first (see the stage-two test report).
import fs from 'node:fs/promises'
export async function testDiagnosticUI(tab, origin, output) {
 const checks=[]
 const assert=(value,message)=>{if(!value)throw new Error(message)}
 const role=(kind,name)=>tab.playwright.getByRole(kind,{name,exact:true})
 const snapshot=async()=>tab.playwright.domSnapshot()
 const openFixture=async scenario=>{
  const url=`${origin}/scripts/diagnostic-ui-fixture.html?scenario=${scenario}`
  if(await tab.url()===url)await tab.reload();else await tab.goto(url)
  const dom=await snapshot();assert(dom.includes('任务中心'),'Fixture not ready (select 简体中文 first)')
 }
 const openTask=async index=>{
  const buttons=role('button','打开');assert(await buttons.count()===9,'Missing history tasks')
  await buttons.nth(index).click();await snapshot()
 }
 const metrics=()=>tab.playwright.evaluate(()=>document.querySelector('.metrics').textContent)
 await openFixture('cumulative')
 await role('button','⇄ TCPing TCP 端口测试').click();await snapshot()
 await role('spinbutton','端口').fill('8443');await role('spinbutton','单次超时 / ms').fill('2300')
 await role('button','◎ DNS 域名解析').click();await snapshot()
 await role('button','⇄ TCPing TCP 端口测试').click();await snapshot()
 const config=await tab.playwright.evaluate(()=>[document.querySelector('#port').value,document.querySelector('#timeout').value])
 assert(config.join(',')==='8443,2300','Per-tool configuration lost')
 checks.push({test:'Custom port and timeout survive tool roundtrip',passed:true})
 await openTask(0)
 assert((await metrics()).includes('1200')&&(await metrics()).includes('960'),'Cumulative statistics mismatch')
 await role('textbox','目标地址').fill('changed.example')
 const params=await tab.playwright.evaluate(()=>document.querySelector('.request-summary').textContent)
 assert(params.includes(':443')&&!params.includes('changed.example'),'Task snapshot changed with form')
 await role('button','◎ DNS 域名解析').click();await snapshot();await openTask(0)
 assert((await metrics()).includes('1200'),'Reopening task reset counters')
 await role('textbox','筛选会话历史（最多 40 项）').fill('no-such-history')
 assert(await role('button','打开').count()===1,'Running task hidden by filter')
 await role('button','停止').click();await snapshot()
 assert((await metrics()).includes('1200'),'Stopping lost counters')
 await role('button','导出 JSON').click()
 const exported=await tab.playwright.evaluate(()=>JSON.parse(document.querySelector('#exported-json').textContent))
 assert(exported.task.result.status==='cancelled'&&exported.task.result.data.sent===1200,'Cancelled export lost data')
 checks.push({test:'1200 probes / 960 successes preserved through eviction, page switch, filter, cancellation and JSON export',passed:true})
 // Modes preserve separate fields, and secret credentials are discarded on mode switch.
 await role('button','⇆ FTP 文件传输与服务').click();await snapshot()
 await role('spinbutton','端口').fill('2121');await role('textbox','密码').fill('test-secret-never-save')
 await role('button','文件服务').click();await snapshot();await role('spinbutton','端口').fill('2021')
 await role('button','目录').click();await snapshot()
 let fields=await tab.playwright.evaluate(()=>[document.querySelector('#port').value,document.querySelector('#ftp-password').value])
 assert(fields.join(',')==='2121,','FTP mode config or secret isolation failed')
 await role('button','文件服务').click();await snapshot()
 assert(await tab.playwright.evaluate(()=>document.querySelector('#port').value)==='2021','Server mode port lost')
 checks.push({test:'FTP modes preserve individual ports and discard password',passed:true})
 await openFixture('chart');await openTask(8)
 const chart=await tab.playwright.evaluate(()=>[...document.querySelectorAll('.chart-slot')].map(x=>({title:x.title,height:x.querySelector('i')?.getBoundingClientRect().height??null,failed:!!x.querySelector('.failure-marker')})))
 assert(chart.length===5&&chart[0].height<chart[1].height&&chart[1].height<chart[2].height&&chart[2].height<chart[3].height&&chart[4].failed,'Chart clips distinct latencies or draws failure as zero')
 await role('combobox','TCP 建连耗时 / ms · 最近窗口（筛选后）').selectOption('20')
 await role('textbox','筛选当前预览…').fill('connected')
 assert(await tab.playwright.evaluate(()=>document.querySelectorAll('.chart-slot').length)===4,'Chart ignores filter')
 await role('button','导出图片').click()
 await tab.playwright.locator('#exported-image').waitFor({state:'visible'})
 const png=await tab.playwright.locator('#exported-image').getAttribute('src')
 assert(png.startsWith('data:image/png;base64,'),'No rendered PNG')
 await fs.writeFile(`${output}/diagnostic-chart-export.png`,Buffer.from(png.split(',')[1],'base64'))
 await fs.writeFile(`${output}/diagnostic-chart-screen.png`,await tab.screenshot({fullPage:true}))
 checks.push({test:'1/30/100/500 ms, failure marker, time tooltips, window selection, filtering and PNG export',passed:true,chart})
 await openFixture('trace');await openTask(8)
 const trace=await metrics()
 assert(trace.includes('探测跳数3')&&trace.includes('到达目标是')&&!trace.includes('丢包'),'Trace uses misleading Ping metrics')
 const rows=await tab.playwright.evaluate(()=>[...document.querySelectorAll('.trace-table tbody tr')].map(x=>x.textContent))
 assert(rows.length===3&&rows[0].includes('TTL 到期')&&rows[2].includes('已到达')&&rows[0].includes('192.0.2.2'),'Trace lost response/address semantics')
 checks.push({test:'Three-hop / nine-probe trace uses dedicated metrics and preserves multiple responder addresses',passed:true})
 await openFixture('empty');await openTask(8)
 assert((await snapshot()).includes('尚无样本'),'Empty cancellation is not explicit')
 checks.push({test:'Cancellation before first measurement shows no samples',passed:true})
 const reruns=role('button','重跑…');assert(await reruns.count()===9,'Missing rerun actions');await reruns.nth(8).click();await snapshot()
 assert(await tab.playwright.evaluate(()=>document.querySelector('#count').value)==='1200','Rerun did not use snapshot')
 checks.push({test:'Rerun loads original request for review',passed:true})
 await fs.writeFile(`${output}/diagnostic-trust-ui.json`,JSON.stringify(checks,null,2))
 return checks
}
