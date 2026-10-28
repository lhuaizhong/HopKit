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

ROOT = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--platform', required=True, choices=['windows-x64', 'macos-arm64', 'macos-x86_64'])
args = p.parse_args()
version = json.loads((ROOT / 'package.json').read_text())['version']
name = f'netbox-{version}-{args.platform}'
assets = ROOT / 'release/assets'
assets.mkdir(parents=True, exist_ok=True)
build = ROOT / 'target/release'
package = ROOT / 'release' / name
shutil.copytree(ROOT / 'release/metadata', package)
for doc in ['LICENSE', 'README.md', 'README.zh-CN.md', 'README-macOS.md']:
    shutil.copy2(ROOT / doc, package / doc)
validation = package / 'validation'
validation.mkdir()
shutil.copy2(ROOT / 'test-output/integration-release.json', validation)

def run(*cmd):
    subprocess.run(list(map(str, cmd)), check=True)

if args.platform == 'windows-x64':
    assert platform.system() == 'Windows'
    shutil.copy2(build / 'netbox-desktop.exe', package / 'netbox.exe')
    shutil.copy2(build / 'netbox-cli.exe', package / 'netbox-cli.exe')
    shutil.copy2(build / 'netbox-desktop.exe', assets / f'{name}-portable.exe')
    installers = list((build / 'bundle/nsis').glob('*-setup.exe'))
    assert len(installers) == 1, installers
    installer = assets / f'{name}-setup.exe'
    shutil.copy2(installers[0], installer)
    # Install silently into a temporary per-user directory, then launch the installed GUI.
    with tempfile.TemporaryDirectory(prefix='netbox-install-') as folder:
        run(installer, '/S', f'/D={folder}')
        installed = Path(folder) / 'netbox-desktop.exe'
        assert installed.is_file(), list(Path(folder).rglob('*'))
        assert (Path(folder) / 'netbox-cli.exe').is_file()
        run(Path(folder) / 'netbox-cli.exe', 'doctor', '--format', 'json')
        for exe in [installed, package / 'netbox.exe']:
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
    shutil.make_archive(str(assets / f'{name}-portable'), 'zip', package.parent, package.name)
else:
    assert platform.system() == 'Darwin'
    expected_arch = args.platform.removeprefix('macos-')
    app = build / 'bundle/macos/netbox.app'
    actual = subprocess.check_output(['lipo', '-archs', str(app / 'Contents/MacOS/netbox-desktop')], text=True).strip()
    assert actual == expected_arch, actual
    run('codesign', '--verify', '--deep', '--strict', app)
    shutil.copytree(app, package / 'netbox.app', symlinks=True)
    shutil.copy2(build / 'netbox-cli', package / 'netbox-cli')
    images = list((build / 'bundle/dmg').glob('*.dmg'))
    assert len(images) == 1, images
    image = assets / f'{name}.dmg'
    shutil.copy2(images[0], image)
    run('hdiutil', 'verify', image)
    with tempfile.TemporaryDirectory(prefix='netbox-dmg-') as mount:
        run('hdiutil', 'attach', '-readonly', '-nobrowse', '-mountpoint', mount, image)
        try:
            run('codesign', '--verify', '--deep', '--strict', Path(mount) / 'netbox.app')
            assert (Path(mount) / 'Applications').is_symlink()
        finally:
            run('hdiutil', 'detach', mount)
    run('ditto', '-c', '-k', '--sequesterRsrc', '--keepParent', package, assets / f'{name}.zip')
# Record the bytes actually shipped after Tauri patches/signs the GUI binary.
info_path = package / 'BUILD_INFO.json'
info = json.loads(info_path.read_text())
shipped = [package / 'netbox.exe', package / 'netbox-cli.exe'] if args.platform == 'windows-x64' else [package / 'netbox.app/Contents/MacOS/netbox-desktop', package / 'netbox-cli']
info['metadata_stage'] = 'packaged'
info['files'] = {f.name: {'sha256': hashlib.sha256(f.read_bytes()).hexdigest(), 'bytes': f.stat().st_size} for f in shipped}
info_path.write_text(json.dumps(info, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
# Regenerate the ZIP with final build information.
if args.platform == 'windows-x64':
    shutil.make_archive(str(assets / f'{name}-portable'), 'zip', package.parent, package.name)
else:
    (assets / f'{name}.zip').unlink()
    run('ditto', '-c', '-k', '--sequesterRsrc', '--keepParent', package, assets / f'{name}.zip')
for artifact in sorted(assets.iterdir()):
    if artifact.suffix == '.sha256':
        continue
    digest = hashlib.sha256(artifact.read_bytes()).hexdigest()
    artifact.with_name(artifact.name + '.sha256').write_text(f'{digest}  {artifact.name}\n', encoding='utf-8')
    print(f'{digest}  {artifact.name}')
