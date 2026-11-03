"""Real native WebView test. Build first:
npm run build && cargo build -p hopkit-desktop --features custom-protocol,native-test
Test feature is forbidden in release builds. No production debug API is enabled.
"""
import argparse,subprocess,tempfile,socket,time,json,hashlib
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--python-bin',type=Path,required=True);a=p.parse_args()
root=Path(__file__).resolve().parents[1];out=root/'test-output/wormhole';out.mkdir(parents=True,exist_ok=True)
def port():
 with socket.socket() as s:s.bind(('127.0.0.1',0));return s.getsockname()[1]
with tempfile.TemporaryDirectory(prefix='hopkit-native-') as temp:
 t=Path(temp);m,r=port(),port();source=t/'原生 中文 file.txt';dest=t/'received.txt';source.write_bytes(b'Native WKWebView transfer\n'*4096)
 report_path=out/'native.json'
 if report_path.exists():report_path.unlink()
 config=t/'config.json';config.write_text(json.dumps(dict(source=str(source),dest=str(dest),mailbox=f'ws://127.0.0.1:{m}/v1',relay=f'tcp://127.0.0.1:{r}')))
 processes=[]
 try:
  for command,args in [('wormhole-mailbox',['--channel-db',str(t/'db')]),('transitrelay',[])]:
   processes.append(subprocess.Popen([str(a.python_bin/'twistd'),'--nodaemon','--pidfile',str(t/(command+'.pid')),command,'--port',f'tcp:{m if command=="wormhole-mailbox" else r}:interface=127.0.0.1',*args],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL))
  time.sleep(2)
  import os
  processes.append(subprocess.Popen([str(root/'target/debug/hopkit-desktop')],env={**os.environ,'HOPKIT_NATIVE_TEST_CONFIG':str(config)},stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL))
  processes[-1].wait(timeout=90)
  report=json.loads((out/'native.json').read_text());assert report['status']=='passed',report
  assert source.read_bytes()==dest.read_bytes();assert hashlib.sha256(dest.read_bytes()).hexdigest()==report['receiver']['data']['sha256']
  print(json.dumps(report,ensure_ascii=False,indent=2))
 finally:
  for proc in processes:
   if proc.poll() is None:proc.terminate()
  for proc in processes:
   try:proc.wait(timeout=3)
   except subprocess.TimeoutExpired:proc.kill()
