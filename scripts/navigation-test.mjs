import assert from 'node:assert/strict'
import fs from 'node:fs'
import ts from 'typescript'
import vm from 'node:vm'
const cache=new Map()
const storage=new Map()
const localStorage={getItem:k=>storage.get(k)??null,setItem:(k,v)=>storage.set(k,v)}
function module(name){if(cache.has(name))return cache.get(name);const file=new URL('../src/tools/'+name+'.ts',import.meta.url);const exports={};cache.set(name,exports);const code=ts.transpileModule(fs.readFileSync(file,'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2020}}).outputText;vm.runInNewContext(code,{exports,require:path=>module(path.replace('./','')),localStorage,URL,TextEncoder});return exports}
const registry=module('registry'),library=module('library'),nav=module('navigation')
assert.equal(registry.tools.length,13)
assert.equal(new Set(registry.tools.map(t=>t.id)).size,13)
assert.equal(registry.tools.findIndex(t=>t.id==='http'),registry.tools.findIndex(t=>t.id==='ftp')+1)
assert.equal(registry.searchTools('HTTP')[0].id,'http')
assert.equal(registry.searchTools('FTP')[0].id,'ftp')
assert.equal(registry.displayName(registry.tools.find(t=>t.id==='http'),true),'HTTP File Sharing')
assert.equal(registry.searchTools('文件共享')[0].id,'http')
assert.equal(registry.searchTools('SNMP').length,0)
storage.set('hopkit-navigation',JSON.stringify({schema_version:1,tool:'removed-tool',category:'expert',favorites:['dns','dns','removed-tool'],scroll:{common:20,expert:-1}}))
let state=nav.readNavigation();assert.equal(state.tool,'check');assert.equal(state.category,'common');assert.equal(state.favorites.join(','),'dns');assert.equal(state.scroll.expert,0)
const snapshot=library.sanitizeSnapshot({schema_version:1,tool:'check',mode:'',values:{target:'https://example.com/a?token=private#secret',ftp_password:'PASSWORD',message:'BODY',timeout_ms:1500,future_secret:'FUTURE'}})
assert.equal(snapshot.values.target,'https://example.com/a')
for(const secret of ['PASSWORD','BODY','FUTURE','private','secret'])assert.ok(!JSON.stringify(snapshot).includes(secret))
const wormhole=library.sanitizeSnapshot({schema_version:1,tool:'wormhole',mode:'receive',values:{},wormhole:{code:'PAIRING',mailbox_url:'wss://a.test/?token=SECRET'}})
assert.ok(!JSON.stringify(wormhole).includes('PAIRING'));assert.ok(!JSON.stringify(wormhole).includes('SECRET'))
for(const settings of [{code_length:2.5},{peer_wait_secs:1},{verify:'yes'},{connection_mode:'unknown'},{max_receive_bytes:'18446744073709551616'},{mailbox_url:'https://example.com/'}])assert.throws(()=>library.sanitizeSnapshot({schema_version:1,tool:'wormhole',mode:'send',values:{},wormhole:settings}))
for(const values of [{timeout_ms:0},{port:65536},{port:1.5},{target:'https://user:pass@example.com/'},{ipv4:'yes'},{ipv4:true,ipv6:true},{iperf_protocol:'bogus'},{record_type:'BOGUS'}])assert.throws(()=>library.sanitizeSnapshot({schema_version:1,tool:'ping',mode:'',values}))
const data=library.emptyLibrary();data.configurations=[snapshot];data.profiles=[{id:'a',name:'site',snapshot}];assert.equal(library.parseLibrary(JSON.stringify(data)).profiles[0].snapshot.values.timeout_ms,1500)
const large={...library.emptyLibrary(),bookmarks:Array.from({length:100},(_,i)=>({id:String(i),name:'site',target:'192.0.2.1'}))};assert.throws(()=>library.mergeLibraries(large,{...library.emptyLibrary(),bookmarks:[{id:'extra',name:'site',target:'192.0.2.2'}]}));assert.equal(large.bookmarks.length,100)
assert.ok(library.saveLibrary(data));assert.equal(library.loadLibrary().library.profiles.length,1)
for(const bad of ['{}','{"schema_version":2}',JSON.stringify({...data,profiles:[...data.profiles,...data.profiles]})])assert.throws(()=>library.parseLibrary(bad))
storage.set('hopkit-library','broken');assert.ok(library.loadLibrary().error)
localStorage.setItem=()=>{throw Error('quota')};assert.equal(library.saveLibrary(data),false);assert.equal(nav.writeNavigation(state),false)
console.log('PASS registry/search, invalid/removed navigation, limits, secret exclusion, library round trip, duplicate/version rejection and blocked storage')
