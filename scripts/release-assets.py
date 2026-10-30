"""Collect and verify freshly built native release assets (run on CI)."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile
import time
import zipfile
from package_common import add_shortcut, verify_cli, checksums

ROOT = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--platform', required=True, choices=['windows-x64', 'macos-arm64', 'macos-x86_64'])
args = p.parse_args()
version = json.loads((ROOT / 'package.json').read_text())['version']
name = f'hopkit-{version}-{args.platform}'
assets = ROOT / 'release/assets'
assets.mkdir(parents=True, exist_ok=True)
build = ROOT / 'target/release'
package = ROOT / 'release' / name
shutil.copytree(ROOT / 'release/metadata', package)
for doc in ['LICENSE', 'README.md', 'README.zh-CN.md', 'README-macOS.md', 'MIGRATION.md', '测试报告-HopKit命名迁移.md']:
    shutil.copy2(ROOT / doc, package / doc)
validation = package / 'validation'
validation.mkdir()
evidence = ROOT / 'test-output/integration-release.json'
cli_suffix = '.exe' if args.platform == 'windows-x64' else ''
assert json.loads(evidence.read_text())['sha256'] == hashlib.sha256((build / ('hopkit-cli'+cli_suffix)).read_bytes()).hexdigest(), 'Stale protocol report'
shutil.copy2(evidence, validation)

def run(*cmd):
    subprocess.run(list(map(str, cmd)), check=True)

if args.platform == 'windows-x64':
    assert platform.system() == 'Windows'
    shutil.copy2(build / 'HopKit.exe', package / 'HopKit.exe')
    shutil.copy2(build / 'hopkit-cli.exe', package / 'hopkit-cli.exe')
    add_shortcut(package / 'hopkit-cli.exe', package)
    shutil.copy2(build / 'HopKit.exe', assets / f'{name}-portable.exe')
    installers = list((build / 'bundle/nsis').glob('*-setup.exe'))
    assert len(installers) == 1, installers
    installer = assets / f'{name}-setup.exe'
    shutil.copy2(installers[0], installer)
    # Install silently into a temporary per-user directory, then launch the installed GUI.
    with tempfile.TemporaryDirectory(prefix='hopkit-install-') as folder:
        run(installer, '/S', f'/D={folder}')
        installed = Path(folder) / 'HopKit.exe'
        assert installed.is_file(), list(Path(folder).rglob('*'))
        assert (Path(folder) / 'hopkit-cli.exe').is_file()
        installed_result = verify_cli(folder)
        (validation / 'installed-cli-entries.json').write_text(json.dumps(installed_result, indent=2)+'\n')
        for exe in [installed, package / 'HopKit.exe']:
            proc = subprocess.Popen([str(exe)])
            try:
                time.sleep(8)
                assert proc.poll() is None, f'GUI exited early: {exe}'
            finally:
                if proc.poll() is None:
                    proc.terminate()
                proc.wait(timeout=15)
        uninstallers = list(Path(folder).glob('*ninstall*.exe'))
        assert len(uninstallers) == 1, uninstallers
        run(uninstallers[0], '/S', '_?=' + folder)
else:
    assert platform.system() == 'Darwin'
    expected_arch = args.platform.removeprefix('macos-')
    app = build / 'bundle/macos/HopKit.app'
    actual = subprocess.check_output(['lipo', '-archs', str(app / 'Contents/MacOS/HopKit')], text=True).strip()
    assert actual == expected_arch, actual
    run('codesign', '--verify', '--deep', '--strict', app)
    shutil.copytree(app, package / 'HopKit.app', symlinks=True)
    shutil.copy2(build / 'hopkit-cli', package / 'hopkit-cli')
    add_shortcut(package / 'hopkit-cli', package)
    verify_cli(package / 'HopKit.app/Contents/Resources')
    images = list((build / 'bundle/dmg').glob('*.dmg'))
    assert len(images) == 1, images
    image = assets / f'{name}.dmg'
    shutil.copy2(images[0], image)
    run('hdiutil', 'verify', image)
    with tempfile.TemporaryDirectory(prefix='hopkit-dmg-') as mount:
        run('hdiutil', 'attach', '-readonly', '-nobrowse', '-mountpoint', mount, image)
        try:
            run('codesign', '--verify', '--deep', '--strict', Path(mount) / 'HopKit.app')
            assert (Path(mount) / 'Applications').is_symlink()
            verify_cli(Path(mount) / 'HopKit.app/Contents/Resources')
        finally:
            run('hdiutil', 'detach', mount)
# Record the bytes actually shipped after Tauri patches/signs the GUI binary.
info_path = package / 'BUILD_INFO.json'
info = json.loads(info_path.read_text())
shipped = [package / 'HopKit.exe', package / 'hopkit-cli.exe'] if args.platform == 'windows-x64' else [package / 'HopKit.app/Contents/MacOS/HopKit', package / 'hopkit-cli']
info['metadata_stage'] = 'packaged'
shipped.append(package / 'shortcuts' / ('hk.exe' if args.platform == 'windows-x64' else 'hk'))
info['files'] = {f.relative_to(package).as_posix(): {'sha256': hashlib.sha256(f.read_bytes()).hexdigest(), 'bytes': f.stat().st_size} for f in shipped}
info_path.write_text(json.dumps(info, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
checksums(package)
# Archive final build information and verify the extracted distribution.
if args.platform == 'windows-x64':
    with zipfile.ZipFile(assets / f'{name}-portable.zip', 'w', zipfile.ZIP_DEFLATED, strict_timestamps=False) as archive:
        for path in sorted(package.rglob('*')):
            if path.is_file():
                archive.write(path, path.relative_to(package.parent))
else:
    run('ditto', '-c', '-k', '--sequesterRsrc', '--keepParent', package, assets / f'{name}.zip')

archive = assets / (f'{name}-portable.zip' if args.platform == 'windows-x64' else f'{name}.zip')
with tempfile.TemporaryDirectory(prefix='HopKit 发布解压 中文 ') as temp:
    if args.platform == 'windows-x64':
        with zipfile.ZipFile(archive) as zipped:
            zipped.extractall(temp)
    else:
        run('ditto', '-x', '-k', archive, temp)
    result = verify_cli(Path(temp) / name)
(assets / f'{name}-cli-entries.json').write_text(json.dumps(result, indent=2)+'\n', encoding='utf-8')

if args.platform != 'windows-x64':
    cli_package = package.with_name(name + '-cli')
    shutil.copytree(package, cli_package, ignore=shutil.ignore_patterns('HopKit.app'))
    cli_info_path = cli_package / 'BUILD_INFO.json'
    cli_info = json.loads(cli_info_path.read_text())
    cli_info['files'] = {k: v for k, v in cli_info['files'].items() if not k.startswith('HopKit.app/')}
    cli_info_path.write_text(json.dumps(cli_info, ensure_ascii=False, indent=2)+'\n', encoding='utf-8')
    checksums(cli_package)
    cli_archive = assets / f'{name}-cli.zip'
    run('ditto', '-c', '-k', '--sequesterRsrc', '--keepParent', cli_package, cli_archive)
    with tempfile.TemporaryDirectory(prefix='HopKit CLI 解压 ') as temp:
        run('ditto', '-x', '-k', cli_archive, temp)
        verify_cli(Path(temp) / cli_package.name)

asset_hashes = []
for artifact in sorted(assets.iterdir()):
    if artifact.suffix == '.sha256' or artifact.name == 'SHA256SUMS.txt':
        continue
    digest = hashlib.sha256(artifact.read_bytes()).hexdigest()
    artifact.with_name(artifact.name + '.sha256').write_text(f'{digest}  {artifact.name}\n', encoding='utf-8')
    asset_hashes.append(f'{digest}  {artifact.name}')
    print(f'{digest}  {artifact.name}')
(assets / 'SHA256SUMS.txt').write_text('\n'.join(asset_hashes)+'\n', encoding='utf-8')
