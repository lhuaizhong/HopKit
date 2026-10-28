import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess

root=Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser()
parser.add_argument('--target')
parser.add_argument('--build-dir',type=Path)
parser.add_argument('--output-dir',type=Path)
parser.add_argument('--minimum-system')
args=parser.parse_args()
output=args.output_dir or root
output.mkdir(parents=True,exist_ok=True)
build=args.build_dir or root/'target/release'
if args.target:
    command=['cargo','metadata','--format-version','1','--locked','--offline','--filter-platform',args.target]
    metadata=json.loads(subprocess.check_output(command,cwd=root,text=True))
else:
    metadata=json.loads((root/'test-output/cargo-metadata.json').read_text(encoding='utf-8-sig'))
packages={p['id']:p for p in metadata['packages']}
nodes={n['id']:n for n in metadata['resolve']['nodes']}
used=set()
def visit(key):
    if key in used:return
    used.add(key)
    for d in nodes.get(key,{}).get('deps',[]):visit(d['pkg'])
for p in metadata['workspace_members']:visit(p)
lines=['# Third-party dependencies / 第三方依赖','', '本清单记录构建时依赖和声明的许可证。原始许可文件保存在 `licenses/`；netbox 源码以 MIT 许可证发布；第三方依赖保留各自许可。','', '| Package | Version | License | Source |','| --- | --- | --- | --- |']
licenses=output/'licenses';licenses.mkdir(exist_ok=True)
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
(output/'THIRD_PARTY_NOTICES.md').write_text('\n'.join(lines)+'\n',encoding='utf-8')
(output/'SBOM.cdx.json').write_text(json.dumps({'bomFormat':'CycloneDX','specVersion':'1.5','version':1,'components':components},ensure_ascii=False,indent=2),encoding='utf-8')
def version(cmd):return subprocess.check_output(cmd,cwd=root,text=True).strip()
suffix='.exe' if platform.system()=='Windows' else ''
binaries = [build/('netbox-desktop'+suffix),build/('netbox-cli'+suffix)]
if platform.system()=='Darwin':
    # Tauri patches/signs the .app binary after compilation. Record shipped files.
    binaries = ([output/'netbox.app/Contents/MacOS/netbox-desktop'] if (output/'netbox.app').exists() else [build/'netbox-desktop'])+[output/'netbox-cli' if (output/'netbox-cli').exists() else build/'netbox-cli']
files={p.name:{'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'bytes':p.stat().st_size}for p in binaries}
source_files=sorted(p for p in root.rglob('*') if p.is_file() and not any(x in p.relative_to(root).parts for x in ['.git','target','node_modules','dist','release','test-output','licenses','__pycache__']) and p.name not in ['BUILD_INFO.json','THIRD_PARTY_NOTICES.md','SBOM.cdx.json','.DS_Store'])
source_digest=hashlib.sha256()
for p in source_files:
    source_digest.update(p.relative_to(root).as_posix().encode()+b'\0'+p.read_bytes())
info={'product':'netbox Network Toolkit','version':json.loads((root/'package.json').read_text())['version'],'build_host':platform.platform(),'target':args.target,'architecture':{'aarch64-apple-darwin':'arm64','x86_64-apple-darwin':'x86_64'}.get(args.target,platform.machine()),'minimum_system_target':args.minimum_system,'minimum_system_runtime_verified':False,'rustc':version(['rustc','--version']),'node':version(['node','--version']),'source_commit':version(['git','rev-parse','HEAD']),'source_dirty':bool(version(['git','status','--porcelain','--untracked-files=no'])),'source_sha256':source_digest.hexdigest(),'build_features':['netbox-desktop/custom-protocol'],'files':files,'dependency_locks':{f:hashlib.sha256((root/f).read_bytes()).hexdigest()for f in['Cargo.lock','package-lock.json']},'commercial_code_signing':False,'update_signature_verification':False,'preview':True}
if platform.system()=='Darwin':
    info.update(sdk=version(['xcrun','--show-sdk-version']),code_signing='ad-hoc',notarized=False,webview='System WKWebView')
else:
    info.update(webview2='System installed runtime required')
(output/'BUILD_INFO.json').write_text(json.dumps(info,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({'components':len(components),'target':args.target,'files':files},indent=2))
