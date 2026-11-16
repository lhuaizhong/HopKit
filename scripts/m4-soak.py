"""Bounded loopback listener workload. Default two hours; shorter runs are smoke only.
Optional psutil records native RSS/CPU/handles; not a runtime dependency of HopKit.
"""
import argparse,hashlib,json,os,platform,socket,subprocess,threading,time
from pathlib import Path
parser=argparse.ArgumentParser();parser.add_argument('--duration',type=int,default=7200);args=parser.parse_args()
assert 1<=args.duration<=86400,'Duration must be 1..86400 seconds'
root=Path(__file__).resolve().parents[1];out=root/'test-output/m4';out.mkdir(parents=True,exist_ok=True);exe=Path(os.environ.get('HOPKIT_TEST_EXE',str(root/'target/debug'/('hopkit-cli.exe' if os.name=='nt' else 'hopkit-cli'))))
started_at=time.strftime('%Y-%m-%dT%H:%M:%S%z');binary_hash=hashlib.sha256(exe.read_bytes()).hexdigest()
with socket.socket(socket.AF_INET,socket.SOCK_DGRAM) as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
process=subprocess.Popen([str(exe),'socket','udp','listen','--listen','127.0.0.1','--port',str(port),'--duration',str(args.duration),'--echo','--allow','127.0.0.1/32','--rate','100','--preview-bytes','4096','--format','jsonl'],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,encoding='utf-8')
samples=[];final=[];reader_error=[];last_stats={};ready=threading.Event();last_progress=-60
def read():
 try:
  for line in process.stdout:
   v=json.loads(line)
   if v.get('event_type')=='socket_stats':last_stats.clear();last_stats.update(v['snapshot']['stats']);ready.set()
   if v.get('event_type')=='finished':final.append(v)
 except Exception as e:reader_error.append(str(e))
worker=threading.Thread(target=read);worker.start();began=time.monotonic();sent=received=0;resource=None
try:
 import psutil
 resource=psutil.Process(process.pid)
except ImportError:pass
try:
 assert ready.wait(5),'Listener did not become ready'
 with socket.socket(socket.AF_INET,socket.SOCK_DGRAM) as peer:
  peer.settimeout(.2)
  while process.poll() is None and time.monotonic()-began<args.duration-.1:
   payload=bytes(range(256))*2;peer.sendto(payload,('127.0.0.1',port));sent+=1
   try:
    b,_=peer.recvfrom(65535);assert b==payload;received+=1
   except socket.timeout:pass
   except ConnectionResetError:
    if time.monotonic()-began>=args.duration-1:break
    raise
   if not samples or time.monotonic()-began-samples[-1]['elapsed_secs']>=2:
    sample={'elapsed_secs':round(time.monotonic()-began,2),'stats':dict(last_stats)}
    if resource:
     sample.update(rss=resource.memory_info().rss,cpu_seconds=sum(resource.cpu_times()[:2]),handles=resource.num_handles() if os.name=='nt' else resource.num_fds())
    samples.append(sample)
    if time.monotonic()-began-last_progress>=60:
     last_progress=time.monotonic()-began
     (out/'soak-progress.json').write_text(json.dumps({'status':'running','started_at':started_at,'pid':process.pid,'duration_secs':args.duration,'sent_datagrams':sent,'echoed_datagrams':received,**sample}),encoding='utf-8')
   time.sleep(.01)
 process.wait(timeout=5);worker.join(timeout=3);assert process.returncode==0;assert not reader_error;assert final and final[0]['data']['stats']['received_bytes']>4096;assert final[0]['data']['stats']['preview_dropped']>0
 stats=final[0]['data']['stats'];assert sent==received==stats['datagrams_received']==stats['datagrams_sent'];assert stats['send_errors']==0 and stats['last_error'] is None;assert stats['received_bytes']==stats['sent_bytes']==sent*512
 report={'status':'passed','scope':'two_hour' if args.duration>=7200 else 'short_smoke','platform':platform.platform(),'started_at':started_at,'duration_secs':args.duration,'version':subprocess.check_output([str(exe),'--version'],text=True).strip(),'binary_sha256':binary_hash,'sent_datagrams':sent,'echoed_datagrams':received,'samples':samples,'resource_measurement':bool(resource),'final':final[0]};(out/'soak.json').write_text(json.dumps(report,indent=2),encoding='utf-8');(out/'soak-progress.json').write_text(json.dumps({'status':'passed','duration_secs':args.duration,'sent_datagrams':sent,'echoed_datagrams':received}),encoding='utf-8');print(json.dumps({k:v for k,v in report.items() if k not in ('samples','final')},indent=2))
finally:
 if process.poll() is None:process.terminate();process.wait(timeout=5)
