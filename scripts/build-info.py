import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess

root=Path(__file__).resolve().parents[1]
metadata=json.loads((root/'test-output/cargo-metadata.json').read_text(encoding='utf-8-sig'))
packages={p['id']:p for p in metadata['packages']}
nodes={n['id']:n for n in metadata['resolve']['nodes']}
used=set()
def visit(key):
    if key in used:return
    used.add(key)
    for d in nodes.get(key,{}).get('deps',[]):visit(d['pkg'])
for p in metadata['workspace_members']:visit(p)
lines=['# Third-party dependencies / 第三方依赖','', '本清单记录构建时依赖和声明的许可证。原始许可文件保存在 `licenses/`；应用自身许可/公开发布模式尚待发布者确定。本次为本地评估构建。','', '| Package | Version | License | Source |','| --- | --- | --- | --- |']
licenses=root/'licenses';licenses.mkdir(exist_ok=True)
components=[]
for p in sorted((packages[k] for k in used if packages[k].get('source')),key=lambda p:p['name']):
    source=p.get('repository')or p.get('homepage')or'https://crates.io/crates/'+p['name']
    lines.append(f"| {p['name']} | {p['version']} | {(p.get('license') or 'see package').replace('|',' / ')} | {source} |")
    components.append({'type':'library','name':p['name'],'version':p['version'],'purl':f"pkg:cargo/{p['name']}@{p['version']}",'licenses':[{'license':{'name':p.get('license')or'Unspecified'}}]})
    source_dir=Path(p['manifest_path']).parent
    candidates=[f for f in source_dir.iterdir()if f.is_file() and any(f.name.lower().startswith(s)for s in['license','licence','copying','notice','copyright'])]
    if p.get('license_file'):
        f=source_dir/p['license_file']
        if f.is_file()and f not in candidates:candidates.append(f)
    if candidates:
        folder=licenses/(p['name']+'-'+p['version']);folder.mkdir(exist_ok=True)
        for f in candidates:shutil.copy2(f,folder/f.name)
lines+=['','## Frontend / 前端','', '| Package | Version | License |','| --- | --- | --- |']
for name in ['vue','@vue/runtime-core','@vue/runtime-dom','@vue/reactivity','@vue/shared','@vue/compiler-dom','@vue/compiler-core','@vue/compiler-sfc','@vue/server-renderer','@vue/compiler-ssr','@tauri-apps/api']:
    folder=root/'node_modules'/name;manifest=folder/'package.json'
    if not manifest.exists():continue
    p=json.loads(manifest.read_text(encoding='utf-8'))
    lines.append(f"| {name} | {p['version']} | {p.get('license','see package')} |")
    target=licenses/(name.replace('/','-').replace('@','')+'-'+p['version']);target.mkdir(exist_ok=True)
    for f in folder.iterdir():
        if f.is_file()and f.name.lower().startswith(('license','notice')):shutil.copy2(f,target/f.name)
    components.append({'type':'library','name':name,'version':p['version'],'purl':f"pkg:npm/{name}@{p['version']}"})
(root/'THIRD_PARTY_NOTICES.md').write_text('\n'.join(lines)+'\n',encoding='utf-8')
(root/'SBOM.cdx.json').write_text(json.dumps({'bomFormat':'CycloneDX','specVersion':'1.5','version':1,'components':components},ensure_ascii=False,indent=2),encoding='utf-8')
def version(cmd):return subprocess.check_output(cmd,cwd=root,text=True).strip()
files={p.name:{'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'bytes':p.stat().st_size}for p in[root/'target/release/netbox-desktop.exe',root/'target/release/netbox-cli.exe']}
info={'product':'netbox Network Toolkit','version':'0.1.0-alpha.1','platform':platform.platform(),'architecture':platform.machine(),'rustc':version(['rustc','--version']),'node':version(['node','--version']),'build_features':['netbox-desktop/custom-protocol','static-crt'],'files':files,'dependency_locks':{f:hashlib.sha256((root/f).read_bytes()).hexdigest()for f in['Cargo.lock','package-lock.json']},'commercial_code_signing':False,'webview2':'System installed runtime required'}
(root/'BUILD_INFO.json').write_text(json.dumps(info,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({'rust_dependencies':len(used)-len(metadata['workspace_members']),'components':len(components),'files':files},indent=2))
