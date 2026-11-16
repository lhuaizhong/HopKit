"""Native ICMP loopback soak; --duration 7200 is the two-hour gate.
Reads every JSONL event but retains only resource samples and final state.
"""
import argparse,datetime,hashlib,json,os,platform,subprocess,threading,time
from pathlib import Path
import psutil
p=argparse.ArgumentParser();p.add_argument('--duration',type=int,default=7200);a=p.parse_args();assert 1<=a.duration<=86400
root=Path(__file__).resolve().parents[1];out=root/'test-output/m5';out.mkdir(parents=True,exist_ok=True)
exe=root/'target/debug'/('hopkit-cli.exe' if os.name=='nt' else 'hopkit-cli');binary_hash=hashlib.sha256(exe.read_bytes()).hexdigest()
cmd=[str(exe),'mtr','127.0.0.1','--cycles','0','--duration',str(a.duration),'--interval-ms','100','--rate','20','--max-hops','4','--format','jsonl']
started_at=datetime.datetime.now(datetime.timezone.utc).isoformat();process=subprocess.Popen(cmd,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,encoding='utf-8');resources=psutil.Process(process.pid);start=time.monotonic();latest={};final=[];errors=[];events=0;output_bytes=0
def read():
 global latest,events,output_bytes
 try:
  for line in process.stdout:
   events+=1;output_bytes+=len(line.encode('utf-8'));v=json.loads(line)
   if v.get('event_type')=='mtr_stats':latest=v['stats']
   if v.get('event_type')=='finished':final.append(v)
 except Exception as e:errors.append(str(e))
reader=threading.Thread(target=read);reader.start();samples=[]
try:
 while process.poll() is None:
  elapsed=time.monotonic()-start
  try:sample={'elapsed_secs':round(elapsed,3),'rss':resources.memory_info().rss,'cpu_seconds':sum(resources.cpu_times()[:2]),'handles':resources.num_handles() if os.name=='nt' else resources.num_fds(),'stats':dict(latest),'output_bytes':output_bytes}
  except psutil.NoSuchProcess:break
  samples.append(sample);(out/'soak-progress.json').write_text(json.dumps({'status':'running','duration_secs':a.duration,'pid':process.pid,**sample}),encoding='utf-8')
  if elapsed>a.duration+10:raise AssertionError('duration overrun')
  time.sleep(min(2,a.duration))
 reader.join(timeout=3);stderr=process.stderr.read();assert process.returncode==0,(process.returncode,stderr);assert not errors,errors;assert len(final)==1
 d=final[0]['data'];assert d['stats']['cycles_completed']>0;assert d['stats']['target_reached_cycles']==d['stats']['cycles_completed'];assert d['stats']['timeouts']==d['stats']['interrupted']==0
 assert d['stats']['sent']==d['stats']['responses']==d['stats']['cycles_completed'];assert len(d['samples'])<=256;assert d['samples_dropped']==max(0,d['stats']['sent']-256)
 stable=samples[5:] or samples;rss_growth=stable[-1]['rss']-stable[0]['rss'];handle_growth=stable[-1]['handles']-stable[0]['handles'];assert rss_growth<32*1024*1024,rss_growth;assert handle_growth<=8,handle_growth
 report={'status':'passed','scope':'two_hour' if a.duration>=7200 else 'short_smoke','platform':platform.platform(),'started_at':started_at,'ended_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'source_state':'working tree M5 changes','duration_secs':a.duration,'actual_secs':round(time.monotonic()-start,3),'version':subprocess.check_output([str(exe),'--version'],text=True).strip(),'binary_sha256':binary_hash,'command':cmd,'events':events,'output_bytes':output_bytes,'persisted_output':'resource samples and final only; event stream drained','rss_growth_after_warmup':rss_growth,'handle_growth_after_warmup':handle_growth,'samples':samples,'final':final[0]}
 (out/'soak.json').write_text(json.dumps(report,indent=2),encoding='utf-8');(out/'soak-progress.json').write_text(json.dumps({k:v for k,v in report.items() if k not in ('samples','final')}),encoding='utf-8');print(json.dumps({k:v for k,v in report.items() if k not in ('samples','final')}))
finally:
 if process.poll() is None:process.terminate();process.wait(timeout=5)
