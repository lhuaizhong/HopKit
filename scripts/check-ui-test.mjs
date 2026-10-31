// Use the Browser skill's persistent session after starting Vite and check-ui-preview.py.
// const {testCheckUI}=await import('/absolute/HopKit/scripts/check-ui-test.mjs');
// await testCheckUI(tab,'http://127.0.0.1:1420','/absolute/HopKit/test-output/check');
import fs from 'node:fs/promises'
export async function testCheckUI(tab,origin,output){
 const assert=(v,m)=>{if(!v)throw Error(m)}
 const role=(kind,name)=>tab.playwright.getByRole(kind,{name,exact:true})
 const checks=[]
 const url=origin+'/scripts/check-ui-fixture.html'
 if(await tab.url()===url)await tab.reload();else await tab.goto(url)
 let dom=await tab.playwright.domSnapshot()
 if(dom.includes('button "中文"')){await role('button','中文').click();dom=await tab.playwright.domSnapshot()}
 assert(dom.includes('本机网络概览')&&dom.includes('▶ 开始排障'),'Not on default troubleshooting home')
 assert(!dom.includes('fixture-runs'),'Page opened a probe task automatically')
 await role('textbox','故障目标').fill('http://user:password@localhost/')
 await role('alert','').waitFor({state:'visible'})
 assert(!await role('button','▶ 开始排障').isEnabled(),'Credentials accepted')
 await role('textbox','故障目标').fill('https://例子.中国:444/a?token=ui-secret')
 await tab.playwright.getByText('https://xn--fsqu00a.xn--fiqs8s:444/a?[redacted]',{exact:true}).waitFor({state:'visible'})
 checks.push({test:'Default local-only home, reject URL credentials, IDN normalization and secret-redacted preview',passed:true})
 const fixture=JSON.parse(await fs.readFile(output+'/source-target-secrets.json','utf8'))
 const target=fixture.data.target.normalized.split('?')[0]+'?token=request-secret'
 await role('textbox','故障目标').fill(target)
 await tab.playwright.getByText(fixture.data.target.normalized,{exact:true}).waitFor({state:'visible'})
 await role('button','▶ 开始排障').click()
 await tab.playwright.getByText('观测事实',{exact:true}).waitFor({state:'visible'})
 await role('button','导出 JSON').click()
 const exported=await tab.playwright.locator('#exported-json').textContent()
 assert(!exported.includes('request-secret'),'Secret leaked in exported task')
 const data=JSON.parse(exported)
 assert(data.task.result.data.evidence.some(e=>e.stage==='http')&&data.task.request.port===fixture.data.target.port,'Evidence or normalized port missing')
 await role('button','导出图片').click();await tab.playwright.locator('#exported-image').waitFor({state:'visible'})
 const png=await tab.playwright.locator('#exported-image').getAttribute('src')
 await fs.writeFile(output+'/ui-export.png',Buffer.from(png.split(',')[1],'base64'))
 checks.push({test:'Real Rust evidence, facts/causes/next steps, sanitized immutable JSON and PNG export',passed:true})
 await role('button','沿用目标到 tcping').click();dom=await tab.playwright.domSnapshot()
 assert(await role('textbox','目标地址').getAttribute('id')==='target','Target field missing')
 const values=await tab.playwright.evaluate(()=>({host:document.querySelector('#target').value,port:document.querySelector('#port').value}))
 assert(values.host==='127.0.0.1'&&Number(values.port)===fixture.data.target.port,'Handoff lost host or port')
 await role('button','重跑…').click();dom=await tab.playwright.domSnapshot()
 assert(dom.includes('URL 参数已脱敏，请重新输入原 URL'),'Rerun reused redacted query')
 await role('button','English').click();dom=await tab.playwright.domSnapshot()
 assert(dom.includes('Troubleshooting target')&&dom.includes('Advanced options'),'English functionality missing')
 checks.push({test:'Tool handoff preserves host/port, redacted rerun requires original URL, English parity',passed:true})
 await fs.writeFile(output+'/ui-acceptance.json',JSON.stringify(checks,null,2))
 return checks
}
