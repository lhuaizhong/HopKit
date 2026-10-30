"""Stage the same CLI binary for Tauri's bundled shortcuts/hk resource."""
import argparse
from pathlib import Path
import shutil

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--build-dir', type=Path, default=Path(__file__).resolve().parents[1] / 'target/release')
a = p.parse_args()
suffix = '.exe' if (a.build_dir / 'hopkit-cli.exe').is_file() else ''
shutil.copy2(a.build_dir / ('hopkit-cli' + suffix), a.build_dir / ('hk' + suffix))
