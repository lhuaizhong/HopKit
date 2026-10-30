// Bundled-frontend checks with an IPC fixture; protocol checks live in gui-test.mjs.
// Uses the workspace's development-only puppeteer-core and installed Edge/Chrome.
import assert from 'node:assert/strict'
import fs from 'node:fs'
import http from 'node:http'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import puppeteer from 'puppeteer-core'

const root = fileURLToPath(new URL('..', import.meta.url))
const dist = path.join(root, 'dist')
const out = path.join(root, 'test-output', 'appearance')
fs.mkdirSync(out, { recursive: true })
const server = http.createServer((req, res) => {
  const pathname = new URL(req.url, 'http://localhost').pathname
  const file = path.resolve(dist, '.' + (pathname === '/' ? '/index.html' : pathname))
  if (!file.startsWith(dist + path.sep)) { res.writeHead(403).end(); return }
  const target = file.endsWith(path.sep) ? path.join(file, 'index.html') : file
  if (!fs.existsSync(target)) { res.writeHead(404).end(); return }
  res.setHeader('Content-Type', ({ '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css' })[path.extname(target)] || 'application/octet-stream')
  fs.createReadStream(target).pipe(res)
})
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve))
const url = `http://127.0.0.1:${server.address().port}/`
let browser
const report = []
const errors = []
const check = async (name, fn) => { await fn(); report.push({ name, status: 'passed' }); console.log('PASS ' + name) }
try {
  browser = await puppeteer.launch({
    executablePath: process.env.HOPKIT_BROWSER || 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
    headless: true,
    args: ['--no-first-run', '--disable-extensions'],
  })
  const page = await browser.newPage()
  await page.setViewport({ width: 1220, height: 820 })
  page.on('pageerror', e => errors.push(e.message))
  await page.evaluateOnNewDocument(() => {
    let task
    window.__detailPolls = 0
    window.__imageExports = []
    window.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
      if (command === 'app_info') return { version: 'test', platform: 'Windows x64', os: 'Windows' }
      if (command === 'export_result_image') { window.__imageExports.push(args); return !window.__cancelImage }
      if (command === 'list_tasks') return task ? [{ ...task, result: null, events: [] }] : []
      if (command === 'task_poll') {
        window.__detailPolls++
        const snapshot = { ...task, events: task.events.filter(x => x.cursor > args.after) }
        await new Promise(resolve => setTimeout(resolve, 80))
        return snapshot
      }
      if (command === 'start_task') {
        const r = args.request
        const events = [
          { cursor: 1, seq: 1, event_type: 'sample', ok: true, status: 'reply', hop: 1, address: '127.0.0.1', rtt_ms: 0.25 },
          { cursor: 2, seq: 2, event_type: 'sample', ok: false, status: 'timeout', hop: 1, address: '::1', rtt_ms: null },
        ]
        task = { id: 'appearance-test', port:r.port, request:r, tool: r.tool, action: r.action || '', target: r.target || '127.0.0.1', status: 'succeeded', started_at: new Date().toISOString(), seq: 2, events,
          result: { data: { sent: 2, received: 1, loss_percent: 50, rtt_avg_ms: 0.25, samples: events, diagnostics: Array.from({length:100}, (_,i)=>`Diagnostic line ${i}`),
            ...(r.tool === 'dns' ? { addresses: ['127.0.0.1', '::1'] } : {}),
            ...(r.tool === 'ntp' ? { best_sample: { offset_ms: 0.42 } } : {}),
          } } }
        return task.id
      }
      return null
    } }
  })
  const pick = name => page.evaluate(name => [...document.querySelectorAll('nav button')].find(x => x.textContent.includes(name)).click(), name)
  const assertTheme = value => page.waitForFunction(value => document.documentElement.dataset.theme === value, {}, value)
  const noOverflow = async () => assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'Horizontal page overflow')
  // Inspect rendered text against its composited ancestor backgrounds, not just CSS tokens.
  const contrast = async () => {
    const failures = await page.evaluate(() => {
      const rgba = s => (s.match(/[\d.]+/g) || []).map(Number)
      const blend = (fg, bg) => fg.slice(0, 3).map((c, i) => c * (fg[3] ?? 1) + bg[i] * (1 - (fg[3] ?? 1)))
      const luminance = rgb => rgb.slice(0, 3).map(x => x / 255).map(x => x <= 0.04045 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4).reduce((s, x, i) => s + x * [0.2126, 0.7152, 0.0722][i], 0)
      const background = e => {
        const chain = []; for (let p = e; p; p = p.parentElement) chain.unshift(p)
        return chain.reduce((bg, p) => blend(rgba(getComputedStyle(p).backgroundColor), bg), [255, 255, 255])
      }
      const issues = []
      const walk = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT)
      while (walk.nextNode()) {
        const node = walk.currentNode, e = node.parentElement, text = node.textContent.trim()
        if (!text || !e.checkVisibility() || e.closest('button:disabled,.brand-icon,.nav-glyph,.radar,.task-icon')) continue
        const s = getComputedStyle(e), bg = background(e), fg = blend(rgba(s.color), bg)
        const a = luminance(fg), b = luminance(bg), ratio = (Math.max(a, b) + .05) / (Math.min(a, b) + .05)
        if (ratio < 4.5) issues.push({ text: text.slice(0, 60), ratio, color: s.color })
      }
      return issues
    })
    assert.deepEqual(failures, [], 'Low-contrast text: ' + JSON.stringify(failures))
  }
  await page.goto(url)
  await page.waitForSelector('.appearance-switch')
  await check('Fresh appearance and English are the initial defaults', async () => { await assertTheme('fresh'); assert.equal(await page.$eval('#target', e => getComputedStyle(e).fontSize), '12px'); assert.equal(await page.$eval('form .primary', e => e.textContent.trim()), '▶Run test') })
  await page.screenshot({ path: path.join(out, 'fresh.png'), fullPage: true })
  await check('Keyboard activates readable style and exposes selected state', async () => {
    await page.focus('.appearance-switch button:last-child'); await page.keyboard.press('Space'); await assertTheme('readable')
    assert.equal(await page.$eval('.appearance-switch button:last-child', e => e.getAttribute('aria-pressed')), 'true')
    assert.equal(await page.$eval('#target', e => getComputedStyle(e).fontSize), '16px')
    assert.equal(await page.$eval('.field-help', e => getComputedStyle(e).fontSize), '14px')
    assert.equal(await page.$eval('.appearance-switch button:last-child', e => getComputedStyle(e).outlineStyle), 'solid')
    await contrast(); await noOverflow()
  })
  await page.screenshot({ path: path.join(out, 'readable.png'), fullPage: true })
  await check('Appearance persists after reload and a new window', async () => {
    await page.reload(); await assertTheme('readable')
    const second = await browser.newPage(); await second.goto(url)
    await second.waitForFunction(() => document.documentElement.dataset.theme === 'readable'); await second.close()
  })
  await check('Saved Chinese language takes precedence over the English default', async () => {
    await page.click('.header-actions > .quiet'); await page.reload(); await assertTheme('readable')
    assert.equal(await page.$eval('.header-actions > .quiet', e => e.textContent), 'English')
    await page.click('.header-actions > .quiet'); await page.reload(); await assertTheme('readable')
    assert.equal(await page.$eval('.header-actions > .quiet', e => e.textContent), '中文')
  })
  await check('Switching style preserves parameters and a completed task', async () => {
    await page.$eval('#target', e => { e.value = 'localhost'; e.dispatchEvent(new Event('input', { bubbles: true })) })
    await page.click('form .primary'); await page.waitForSelector('tbody .event-label')
    await page.click('.appearance-switch button:first-of-type'); await assertTheme('fresh')
    assert.equal(await page.$eval('#target', e => e.value), 'localhost')
    assert.equal(await page.$$eval('tbody tr', rows => rows.length), 2)
    await page.click('.appearance-switch button:last-child'); await assertTheme('readable'); await contrast()
  })
  await page.screenshot({ path: path.join(out, 'readable-results.png'), fullPage: true })
  await check('PNG captures styled result cards without JSON; cancel stays quiet', async () => {
    await page.$eval('.filter-bar input', e => { e.value='reply'; e.dispatchEvent(new Event('input',{bubbles:true})) })
    await page.evaluate(()=>{
      const body=document.querySelector('.table-scroll tbody')
      for(let i=0;i<35;i++){const row=body.firstElementChild.cloneNode(true);row.dataset.screenshotTest='true';body.append(row)}
    })
    assert.equal(await page.$('.raw-result pre'),null)
    await page.evaluate(()=>{
      const descriptor=Object.getOwnPropertyDescriptor(HTMLImageElement.prototype,'src')
      window.__imageSrcDescriptor=descriptor
      Object.defineProperty(HTMLImageElement.prototype,'src',{...descriptor,set(value){if(value.startsWith('data:image/svg+xml'))window.__screenshotSvg=decodeURIComponent(value.split(',').slice(1).join(','));descriptor.set.call(this,value)}})
      ;[...document.querySelectorAll('.result-tools button')].find(e=>e.textContent.includes('Export image')).click()
    })
    await page.waitForFunction(()=>window.__imageExports.length===1)
    const exported=await page.evaluate(()=>({base64:btoa(window.__imageExports[0].png.reduce((s,b)=>s+String.fromCharCode(b),''))}))
    const bytes=Buffer.from(exported.base64,'base64')
    assert.deepEqual([...bytes.subarray(0,8)],[137,80,78,71,13,10,26,10])
    assert(bytes.readUInt32BE(16)>600)
    assert(bytes.readUInt32BE(20)>2000,'Scrollable rows were clipped')
    const svg=await page.evaluate(()=>window.__screenshotSvg)
    assert(svg.includes('class="metrics"'),'Metric cards missing')
    assert(!svg.includes('raw-result'),'JSON disclosure included')
    assert(!svg.includes('result-tools'),'Export toolbar included')
    assert(svg.includes('reply'),'Current filter was not preserved')
    await page.evaluate(()=>{Object.defineProperty(HTMLImageElement.prototype,'src',window.__imageSrcDescriptor)})
    fs.writeFileSync(path.join(out,'export-result.png'),bytes)
    assert.equal(await page.$('.raw-result pre'),null,'Export changed expansion state')
    await page.waitForSelector('.result-tools [role="status"]')
    await page.evaluate(()=>window.__cancelImage=true)
    await page.evaluate(()=>[...document.querySelectorAll('.result-tools button')].find(e=>e.textContent.includes('Export image')).click())
    await page.waitForFunction(()=>window.__imageExports.length===2)
    assert.equal(await page.$('.result-tools [role="status"]'),null,'Cancel reported success')
    await page.evaluate(()=>document.querySelectorAll('[data-screenshot-test]').forEach(row=>row.remove()))
    await page.$eval('.filter-bar input',e=>{e.value='';e.dispatchEvent(new Event('input',{bubbles:true}))})
    await page.evaluate(()=>{
      Object.defineProperty(navigator,'clipboard',{configurable:true,value:{write:async items=>{
        const blob=await items[0].getType('image/png');window.__clipboardImageBytes=blob.size
      }}})
    })
    await page.evaluate(()=>[...document.querySelectorAll('.result-tools button')].find(e=>e.textContent.includes('Copy image')).click())
    await page.waitForFunction(()=>window.__clipboardImageBytes>0)
    await page.waitForFunction(()=>document.querySelector('.result-tools').textContent.includes('Image copied'))
  })
  await check('Completed results stay mounted and retain JSON scroll while summaries refresh', async () => {
    for (const style of ['fresh', 'readable']) {
      await page.click(`.appearance-switch button:${style === 'fresh' ? 'first-of-type' : 'last-child'}`)
      await assertTheme(style)
      await page.click('.raw-result > button'); await page.waitForSelector('.raw-result pre')
      const before = await page.evaluate(() => {
        const pre = document.querySelector('.raw-result pre')
        pre.scrollTop = 200
        pre.scrollIntoView({block:'center'})
        window.__resultPre = pre
        window.__resultMutations = 0
        window.__resultObserver = new MutationObserver(mutations => { window.__resultMutations += mutations.length })
        window.__resultObserver.observe(document.querySelector('.result-panel'), {childList:true,subtree:true,characterData:true})
        return {top:pre.scrollTop, page:scrollY, polls:window.__detailPolls}
      })
      assert(before.top > 0, 'JSON must be long enough to scroll')
      await new Promise(resolve => setTimeout(resolve, 1800))
      const after = await page.evaluate(() => {
        window.__resultObserver.disconnect()
        const pre = document.querySelector('.raw-result pre')
        return {same:pre===window.__resultPre,top:pre?.scrollTop,page:scrollY,polls:window.__detailPolls,mutations:window.__resultMutations}
      })
      assert(after.same, 'The JSON node was replaced')
      assert.equal(after.top,before.top,'JSON scroll reset')
      assert.equal(after.page,before.page,'Page scroll jumped')
      assert.equal(after.polls,before.polls,'Completed task details were polled again')
      assert.equal(after.mutations,0,'Completed result DOM kept refreshing')
      await page.click('.raw-result > button')
    }
    await page.click('.task-row'); await page.waitForSelector('.raw-result')
    assert.equal(await page.$$eval('tbody tr',rows=>rows.length),2,'Reopening history must fetch preview again')
  })
  await check('Both languages and all eight tools fit at 960, 1220 and 1440px', async () => {
    for (const width of [960, 1220, 1440]) {
      await page.setViewport({ width, height: 820 })
      for (const english of [false, true]) {
        await page.evaluate(english => { localStorage.setItem('hopkit-language', english ? 'en-US' : 'zh-CN') }, english)
        await page.reload(); await assertTheme('readable')
        for (const name of ['Ping', 'Traceroute', 'TCPing', 'DNS', 'NTP', 'TFTP', 'FTP', 'Syslog']) {
          await pick(name); await noOverflow(); await contrast()
          if (['Ping', 'Traceroute', 'TCPing', 'DNS', 'NTP'].includes(name)) {
            await page.click('form .primary'); await page.waitForSelector('.raw-result'); await contrast(); await noOverflow()
            if(name==='TCPing'){
              const actual=await page.$eval('#port',e=>e.value)
              assert((await page.$eval('.tcp-port',e=>e.textContent)).endsWith(actual))
              await page.$eval('#port',e=>{e.value='1';e.dispatchEvent(new Event('input',{bubbles:true}))})
              assert((await page.$eval('.tcp-port',e=>e.textContent)).endsWith(actual),'Result port followed edited input')
            }
          }
          if (name === 'FTP' || name === 'TFTP' || name === 'Syslog' || name === 'DNS') {
            const count = await page.$$eval('.segmented button', x => x.length)
            for (let i = 0; i < count; i++) { await page.click(`.segmented button:nth-child(${i + 1})`); await noOverflow(); await contrast() }
          }
        }
      }
    }
  })
  await check('Readable layout reflows at 200 percent equivalent viewport', async () => {
    await page.setViewport({ width: 610, height: 410 }); await pick('Ping'); await noOverflow(); await contrast()
    await page.screenshot({ path: path.join(out, 'readable-narrow.png'), fullPage: true })
  })
  await check('Invalid stored appearance falls back to Fresh', async () => {
    await page.evaluate(() => localStorage.setItem('hopkit-appearance', 'invalid')); await page.reload(); await assertTheme('fresh')
  })
  await check('Switch remains usable when preference storage is unavailable', async () => {
    await page.evaluateOnNewDocument(() => {
      const get = Storage.prototype.getItem, set = Storage.prototype.setItem
      Storage.prototype.getItem = function (key) { if (key === 'hopkit-appearance') throw new Error('Storage blocked'); return get.call(this, key) }
      Storage.prototype.setItem = function (key, value) { if (key === 'hopkit-appearance') throw new Error('Storage blocked'); return set.call(this, key, value) }
    })
    await page.reload(); await assertTheme('fresh')
    await page.click('.appearance-switch button:last-child'); await assertTheme('readable')
  })
  await check('No uncaught frontend errors', async () => assert.deepEqual(errors, []))
} finally {
  fs.writeFileSync(path.join(out, 'report.json'), JSON.stringify({ report, errors }, null, 2))
  await browser?.close()
  await new Promise(resolve => server.close(resolve))
}
