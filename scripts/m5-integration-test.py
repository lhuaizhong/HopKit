"""Native CLI/OS ICMP acceptance. Loopback only; no simulated physical routers."""
import concurrent.futures,hashlib,json,os,platform,signal,subprocess,time
from pathlib import Path
root=Path(__file__).resolve().parents[1];out=root/'test-output/m5';out.mkdir(parents=True,exist_ok=True);exe=Path(os.environ.get('HOPKIT_TEST_EXE',str(root/'target/debug'/('hopkit-cli.exe' if os.name=='nt' else 'hopkit-cli'))))
checks=[]
def run(args,code=0):
 p=subprocess.run([str(exe),*args],capture_output=True,text=True,encoding='utf-8',timeout=15);assert p.returncode==code,(args,p.returncode,p.stdout,p.stderr);return p
def mtr(target,source):
 p=run(['mtr',target,'--source',source,'--cycles','4','--interval-ms','100','--max-hops','4','--format','json']);d=json.loads(p.stdout)['data'];assert d['stats']['sent']==d['stats']['responses']==d['stats']['cycles_completed']==d['stats']['target_reached_cycles']==4;assert d['hops'][0]['responders'][0]['address']==target;assert d['hops'][0]['rtt']['jitter_pairs']==3;return d
v4=mtr('127.0.0.1','127.0.0.1');v6=mtr('::1','::1');checks.append('Native IPv4/IPv6 loopback echo at TTL 1, source binding and independent exact counts')
with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
 results=list(pool.map(lambda i:mtr('::1','::1') if i%2 else mtr('127.0.0.1','127.0.0.1'),range(4)))
checks.append('Four concurrent mixed-family CLI processes: no target crossover; process budgets remain independent')
for args in [['--rate','101'],['--max-hops','0'],['--timeout-ms','1001'],['--interval-ms','99'],['--source','::1','-4'],['--cycles','1000001']]:
 p=run(['mtr','127.0.0.1',*args,'--format','json'],2);v=json.loads(p.stdout);assert v['error']['code']=='INVALID_ARGUMENT'
run(['mtr','127.0.0.1','--cycles','0','--format','json'],2);checks.append('Invalid configuration rejected; unbounded JSON forbidden')
v=json.loads(run(['mtr','127.0.0.1','--cycles','0','--duration','1','--interval-ms','100','--format','json']).stdout);assert v['data']['stats']['cycles_completed']>=5
lines=[json.loads(line) for line in run(['mtr','127.0.0.1','--cycles','4','--interval-ms','100','--format','jsonl']).stdout.splitlines()];assert lines[-1]['event_type']=='finished';assert len({v['task_id'] for v in lines})==1;assert all(v['schema_version']==1 for v in lines);assert sum(v['event_type']=='mtr_sample' for v in lines)==4
run(['mtr','127.0.0.1','--cycles','2','--interval-ms','100','--format','text','--lang','zh-CN']);checks.append('Duration limit, text/JSON/JSONL schema and final envelope')
flags=subprocess.CREATE_NEW_PROCESS_GROUP if os.name=='nt' else 0
p=subprocess.Popen([str(exe),'mtr','127.0.0.1','--cycles','0','--interval-ms','100','--format','jsonl'],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,encoding='utf-8',creationflags=flags)
try:
 first=json.loads(p.stdout.readline());assert first['event_type']=='mtr_sample';began=time.monotonic();p.send_signal(signal.CTRL_BREAK_EVENT if os.name=='nt' else signal.SIGINT);stdout,stderr=p.communicate(timeout=4);elapsed=(time.monotonic()-began)*1000;assert p.returncode==130,(p.returncode,stderr);v=json.loads(stdout.splitlines()[-1]);assert v['status']=='cancelled';assert v['data']['stats']['responses']>=1;assert elapsed<1500,elapsed
finally:
 if p.poll() is None:p.terminate();p.wait(timeout=5)
checks.append('Native signal cancellation returns 130 promptly and preserves prior observations')
report={'status':'passed','platform':platform.platform(),'date':time.strftime('%Y-%m-%d'),'version':run(['--version']).stdout.strip(),'binary_sha256':hashlib.sha256(exe.read_bytes()).hexdigest(),'checks':checks,'cancel_ms':elapsed,'ipv4':v4,'ipv6':v6,'unmeasured':['physical suspend','macOS native','isolated routed multi-hop topology']};(out/'integration.json').write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf-8');print(json.dumps({k:v for k,v in report.items() if k not in ('ipv4','ipv6')},ensure_ascii=False,indent=2))
