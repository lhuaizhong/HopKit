#!/usr/bin/env python3
"""Build architecture-specific ad-hoc signed macOS preview packages."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
from package_common import add_shortcut, verify_cli, checksums
import tempfile

ROOT = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--arch',choices=['arm64','x86_64'],default=platform.machine())
p.add_argument('--skip-build',action='store_true',help='Package existing target release binaries')
p.add_argument('--build-dir',type=Path,help='Existing build directory (requires --skip-build)')
a = p.parse_args()
if a.build_dir and not a.skip_build:p.error('--build-dir requires --skip-build')
if platform.system() != 'Darwin':p.error('Run this script on macOS with Xcode Command Line Tools')
target = {'arm64':'aarch64-apple-darwin','x86_64':'x86_64-apple-darwin'}[a.arch]
minimum = '11.0'
version = json.loads((ROOT/'package.json').read_text())['version']
name = f'hopkit-{version}-macos-{a.arch}'
release = ROOT/'release'
package = release/name
cli_package = release/(name+'-cli')
for output in (package,cli_package,release/(name+'.zip'),release/(name+'-cli.zip')):
    if output.exists():raise SystemExit(f'Output already exists: {output}. Archive it before packaging again.')
env = dict(os.environ,MACOSX_DEPLOYMENT_TARGET=minimum)
# Avoid embedding the developer's home/registry paths or native CPU flags.
env['CARGO_ENCODED_RUSTFLAGS']='\x1f'.join([
    '--remap-path-prefix='+str(Path.home())+'=/build-user',
    '--remap-path-prefix='+str(ROOT)+'=/hopkit',
    '-C','target-cpu=generic',
])
def run(args):return subprocess.run(list(map(str,args)),cwd=ROOT,env=env,check=True)
if not a.skip_build:
    if not (ROOT/'src-tauri/icons/icon.icns').exists():run(['python3',ROOT/'scripts/create-macos-icon.py'])
    run(['node_modules/.bin/tauri','build','--target',target,'--bundles','app','--config',json.dumps({'bundle':{'macOS':{'minimumSystemVersion':minimum}}}),'--','--locked'])
    run(['cargo','build','--release','--locked','--target',target,'-p','hopkit-cli'])
build = a.build_dir.resolve() if a.build_dir else ROOT/'target'/target/'release'
app = build/'bundle/macos/HopKit.app'
cli = build/'hopkit-cli'
if not app.is_dir() or not cli.is_file():raise SystemExit('Missing application bundle or CLI; build before packaging')
# Refuse a mislabeled package, including accidental host binaries.
for binary in (app/'Contents/MacOS/HopKit',cli):
    actual = subprocess.check_output(['lipo','-archs',str(binary)],text=True).strip()
    if actual != a.arch:raise SystemExit(f'Expected {a.arch}, found {actual}: {binary}')
release.mkdir(exist_ok=True)
package.mkdir();cli_package.mkdir()
run(['ditto',app,package/'HopKit.app'])
for folder in (package,cli_package):
    shutil.copy2(cli,folder/'hopkit-cli')
    run(['codesign','--force','--sign','-',folder/'hopkit-cli'])
    run(['codesign','--verify','--strict',folder/'hopkit-cli'])
    add_shortcut(folder/'hopkit-cli', folder)
    shutil.copy2(ROOT/'README-macOS.md',folder/'使用说明.md')
    for doc in ('LICENSE','README.md','README.zh-CN.md','README-macOS.md','MIGRATION.md','测试报告-HopKit命名迁移.md'):
        if (ROOT/doc).is_file():shutil.copy2(ROOT/doc,folder/doc)
    (folder/'validation').mkdir()
    evidence_names=[f'integration-release-macos-{a.arch}.json',f'platform-macos-{a.arch}.json',f'gui-macos-{a.arch}.json']
    for evidence in (ROOT/'test-output'/name for name in evidence_names):
        if not evidence.exists():continue
        # A report for the other architecture must never be mistaken for evidence.
        data = json.loads(evidence.read_text())
        if data.get('architecture',data.get('machine')) not in (None,a.arch):continue
        # Only include CLI reports that match this exact signed executable.
        if data.get('gui_sha256'):
            if not (folder/'HopKit.app').exists():continue
            if data['gui_sha256'] != hashlib.sha256((folder/'HopKit.app/Contents/MacOS/HopKit').read_bytes()).hexdigest():continue
        if data.get('sha256') and data['sha256'] != hashlib.sha256((folder/'hopkit-cli').read_bytes()).hexdigest():continue
        serialized=json.dumps(data,ensure_ascii=False,indent=2)
        serialized=serialized.replace(str(ROOT),'<workspace>').replace(str(Path.home()),'<user-home>')
        (folder/'validation'/evidence.name).write_text(serialized+'\n')
    run(['python3',ROOT/'scripts/build-info.py','--target',target,'--build-dir',build,'--output-dir',folder,'--minimum-system',minimum])
run(['codesign','--verify','--deep','--strict',package/'HopKit.app'])
for folder in (package,cli_package):
    checksums(folder)
outputs=[]
for folder in (package,cli_package):
    archive=folder.with_name(folder.name+'.zip')
    run(['ditto','-c','-k','--sequesterRsrc','--keepParent',folder,archive])
    with tempfile.TemporaryDirectory(prefix='HopKit 解压验证 ') as temp:
        run(['ditto','-x','-k',archive,temp])
        result=verify_cli(Path(temp)/folder.name)
    (folder/'validation/cli-entries.json').write_text(json.dumps(result,indent=2)+'\n')
    checksums(folder)
    archive.unlink()
    run(['ditto','-c','-k','--sequesterRsrc','--keepParent',folder,archive])
    outputs.append(archive)
for output in outputs:
    digest=hashlib.sha256(output.read_bytes()).hexdigest()
    output.with_name(output.name+'.sha256').write_text(f'{digest}  {output.name}\n')
    print(f'{digest}  {output}')
