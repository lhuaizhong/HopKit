// Exercise the actual migration module with a controlled Storage implementation.
import assert from 'node:assert/strict'
import fs from 'node:fs'
import vm from 'node:vm'
import ts from 'typescript'

const code = ts.transpileModule(fs.readFileSync(new URL('../src/preferences.ts', import.meta.url), 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS },
}).outputText
let passed = 0
function run(initial, action, blocked = false) {
  const values = new Map(Object.entries(initial))
  const storage = {
    getItem(key) { if (blocked) throw Error('blocked'); return values.get(key) ?? null },
    setItem(key, value) { if (blocked) throw Error('blocked'); values.set(key, value) },
  }
  const context = { exports: {}, localStorage: storage }
  vm.runInNewContext(code, context)
  action(context.exports, values)
  passed++
}
for (const [name, legacy, current] of [['language', 'zh-CN', 'en-US'], ['appearance', 'readable', 'fresh']]) {
  run({ [`netbox-${name}`]: legacy }, (api, values) => {
    assert.equal(api.preference(name), legacy)
    assert.equal(values.get(`hopkit-${name}`), legacy)
    api.savePreference(name, current)
    assert.equal(api.preference(name), current)
    assert.equal(values.get(`netbox-${name}`), legacy)
  })
  run({ [`netbox-${name}`]: legacy, [`hopkit-${name}`]: current }, api => assert.equal(api.preference(name), current))
  run({ [`netbox-${name}`]: legacy, [`hopkit-${name}`]: 'invalid' }, api => assert.equal(api.preference(name), 'invalid'))
  run({ [`netbox-${name}`]: 'invalid' }, (api, values) => {
    assert.equal(api.preference(name), null)
    assert.equal(values.has(`hopkit-${name}`), false)
  })
  run({}, api => { assert.equal(api.preference(name), null); api.savePreference(name, current) }, true)
}
console.log(`PASS ${passed} preference migration cases`)

// A denied migration write must not discard an otherwise readable old preference.
const values = new Map([['netbox-language', 'zh-CN']])
const context = { exports: {}, localStorage: {
  getItem: key => values.get(key) ?? null,
  setItem() { throw Error('quota exceeded') },
} }
vm.runInNewContext(code, context)
assert.equal(context.exports.preference('language'), 'zh-CN')
assert.equal(values.get('netbox-language'), 'zh-CN')
console.log('PASS migration remains usable when new-key writes fail')
