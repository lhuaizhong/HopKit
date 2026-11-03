"""Archive the actual corresponding source, including uncommitted integration work."""
import argparse,tarfile
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);a=p.parse_args()
root=Path(__file__).resolve().parents[1];skip={'.git','node_modules','target','dist','release','test-output','licenses','__pycache__'}
a.output.parent.mkdir(parents=True,exist_ok=True)
with tarfile.open(a.output,'w:gz') as tar:
 for f in sorted(root.rglob('*')):
  if any(part in skip for part in f.relative_to(root).parts) or f.name in {'.DS_Store','BUILD_INFO.json','SBOM.cdx.json'} or not f.is_file() or f.resolve()==a.output.resolve():continue
  tar.add(f,arcname='HopKit/'+f.relative_to(root).as_posix(),recursive=False)
print(a.output)
