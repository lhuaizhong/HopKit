#!/usr/bin/env python3
"""Build with custom-protocol,native-test first. Real loopback TCP peer and WebView."""
import json,os,socket,subprocess,threading
from pathlib import Path
root=Path(__file__).resolve().parents[1];out=root/'test-output/m2';out.mkdir(parents=True,exist_ok=True)
report=out/'native.json'
if report.exists():report.unlink()
with socket.socket() as peer:
 peer.bind(('127.0.0.1',0));peer.listen(128);peer.settimeout(.2)
 with socket.socket() as closed:closed.bind(('127.0.0.1',0));refused=closed.getsockname()[1]
 stop=threading.Event()
 def accept():
  while not stop.is_set():
   try:conn,addr=peer.accept();conn.close()
   except socket.timeout:pass
 worker=threading.Thread(target=accept);worker.start()
 config=out/'native-config.json';config.write_text(json.dumps({'scenario':'m2','open_port':peer.getsockname()[1],'refused_port':refused}))
 process=subprocess.Popen([str(root/'target/debug/hopkit-desktop')],env={**os.environ,'HOPKIT_NATIVE_TEST_CONFIG':str(config)},stdout=subprocess.PIPE,stderr=subprocess.PIPE)
 try:
  stdout,stderr=process.communicate(timeout=55)
  assert process.returncode==0,(process.returncode,stderr.decode(errors='replace'))
  data=json.loads(report.read_text());assert data['status']=='passed',data
  print(json.dumps(data,ensure_ascii=False,indent=2))
 finally:
  if process.poll() is None:process.terminate();process.wait(timeout=10)
  stop.set();worker.join(timeout=2)
