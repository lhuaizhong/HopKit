"""Debug native WebView+Rust IPC; private test storage, exits the test window."""
import hashlib,json,os,platform,subprocess,time
from pathlib import Path
root=Path(__file__).resolve().parents[1];out=root/'test-output/m5';out.mkdir(parents=True,exist_ok=True);report=out/'native.json'
if report.exists():report.unlink()
config=out/'native-config.json';config.write_text('{"scenario":"m5"}',encoding='utf-8');exe=root/'target/debug'/('hopkit-desktop.exe' if os.name=='nt' else 'hopkit-desktop')
p=subprocess.Popen([str(exe)],env={**os.environ,'HOPKIT_NATIVE_TEST_CONFIG':str(config)},stdout=subprocess.PIPE,stderr=subprocess.PIPE)
try:
 stdout,stderr=p.communicate(timeout=50);assert p.returncode==0,(p.returncode,stderr.decode(errors='replace'));data=json.loads(report.read_text(encoding='utf-8'));assert data['status']=='passed',data;data.update(platform=platform.platform(),date=time.strftime('%Y-%m-%d'),binary_sha256=hashlib.sha256(exe.read_bytes()).hexdigest());report.write_text(json.dumps(data,ensure_ascii=False,indent=2),encoding='utf-8');print(json.dumps(data,ensure_ascii=False,indent=2))
finally:
 if p.poll() is None:p.terminate();p.wait(timeout=5)
