"""Verify CLI entries from a real distribution ZIP, not the source build directory."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import subprocess
import tempfile
import zipfile
from package_common import verify_cli

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--archive', required=True, type=Path)
a = p.parse_args()
with tempfile.TemporaryDirectory(prefix='HopKit 解压 中文 ') as temp:
    if platform.system() == 'Darwin':
        subprocess.run(['ditto', '-x', '-k', str(a.archive.resolve()), temp], check=True)
    else:
        with zipfile.ZipFile(a.archive) as archive:
            archive.extractall(temp)
    folders = [p for p in Path(temp).iterdir() if p.is_dir() and p.name != '__MACOSX']
    assert len(folders) == 1, folders
    manifest = folders[0] / 'SHA256SUMS.txt'
    for line in manifest.read_text(encoding='utf-8-sig').splitlines():
        digest, name = line.split('  ', 1)
        assert hashlib.sha256((folders[0] / name).read_bytes()).hexdigest() == digest, name
    result = verify_cli(folders[0])
    result['checksums'] = 'passed'
    print(json.dumps(result, ensure_ascii=False, indent=2))
