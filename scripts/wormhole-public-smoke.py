"""Manual, opt-in small-file public-service test. Same-host result is NOT a different-network acceptance."""
import argparse,hashlib,json,queue,subprocess,tempfile,threading,time,platform
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--run-public-smoke',action='store_true',required=True);a=p.parse_args()
root=Path(__file__).resolve().parents[1];cli=root/'target/release/hopkit-cli';out=root/'test-output/wormhole';out.mkdir(parents=True,exist_ok=True);reports=[]
with tempfile.TemporaryDirectory(prefix='hopkit-public-') as tmp:
 t=Path(tmp);source=t/'smoke.txt';source.write_bytes(b'HopKit Wormhole public small-file smoke\n')
 for mode in ['relay','auto']:
  sender=receiver=None;started=time.monotonic()
  try:
   sender=subprocess.Popen([str(cli),'wormhole','send',str(source),'--connection',mode,'--format','jsonl','--emit-code'],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
   q=queue.Queue()
   def read():
    for line in sender.stdout:q.put(json.loads(line))
   threading.Thread(target=read,daemon=True).start();pair=None
   until=time.monotonic()+20
   while time.monotonic()<until:
    try:value=q.get(timeout=.1)
    except queue.Empty:
     if sender.poll() is not None:break
     continue
    if value.get('event_type')=='wormhole_code':pair=value['code'];break
   if not pair:raise RuntimeError('Public mailbox did not generate code within budget')
   dest=t/mode;dest.mkdir();receiver=subprocess.Popen([str(cli),'wormhole','receive',pair,'--dir',str(dest),'--yes','--connection',mode,'--format','json'],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
   stdout,stderr=receiver.communicate(timeout=90);assert receiver.returncode==0,'Public receive failed'
   assert sender.wait(timeout=35)==0,'Public sender receipt failed'
   value=json.loads(stdout);assert source.read_bytes()==(dest/source.name).read_bytes()
   reports.append({'mode':mode,'status':'passed','actual_connection':value['data']['connection'],'sha256':value['data']['sha256'],'seconds':round(time.monotonic()-started,3)})
  except Exception as e:reports.append({'mode':mode,'status':'environment_or_network_failure','error':str(e),'seconds':round(time.monotonic()-started,3)})
  finally:
   for proc in [sender,receiver]:
    if proc and proc.poll() is None:proc.terminate()
   for proc in [sender,receiver]:
    if proc:
     try:proc.wait(timeout=3)
     except subprocess.TimeoutExpired:proc.kill()
report={'platform':platform.platform(),'scope':'same computer, same network; NOT different-network acceptance','mailbox':'wss://relay.magic-wormhole.io/v1','relay':'tcp://transit.magic-wormhole.io:4001','binary_sha256':hashlib.sha256(cli.read_bytes()).hexdigest(),'results':reports}
(out/'public-smoke.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
