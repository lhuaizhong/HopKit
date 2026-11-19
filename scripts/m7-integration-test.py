"""Loopback native ICMP and strict CLI contract. No public network targets."""
import hashlib,json,os,platform,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1];OUT=ROOT/'test-output/m7';OUT.mkdir(parents=True,exist_ok=True)
EXE=Path(os.environ.get('HOPKIT_TEST_EXE',str(ROOT/('target/debug/hopkit-cli.exe' if os.name=='nt' else 'target/debug/hopkit-cli'))))
checks=[]
def run(*args,code=0):
 p=subprocess.run([str(EXE),*map(str,args),'--format','json'],capture_output=True,text=True,encoding='utf-8',timeout=20)
 assert p.returncode==code,(args,p.returncode,p.stdout,p.stderr)
 d=json.loads(p.stdout);checks.append({'args':args,'exit_code':p.returncode,'result':d});return d
for args in [('mtu','127.0.0.1','--min','67'),('mtu','127.0.0.1','--max','65536'),('mtu','127.0.0.1','--retries','4'),('mtu','https://localhost/'),('mtu','127.0.0.1:80'),('mtu','127.0.0.1','--source','::1')]:run(*args,code=2)
d=run('mtu','127.0.0.1','-4','--min','576','--max','1500')['data'];assert d['confirmed_lower']==1500 and d['confirmed_upper'] is None and d['diagnostic_outcome']=='at_least'
assert all(p['ip_bytes']==p['payload_bytes']+28 for p in d['probes'])
assert d['local_endpoint']=='127.0.0.1'
d=run('mtu','127.0.0.1','--source','127.0.0.1')['data'];assert d['local_endpoint']=='127.0.0.1'
if os.name=='nt':
 d=run('mtu','::1','-6','--min','1280',code=3);assert d['error']['code']=='UNSUPPORTED_CAPABILITY' and not d['data']['probes']
elif platform.system()=='Darwin':
 p=subprocess.run([str(EXE),'mtu','::1','-6','--min','1280','--format','json'],capture_output=True,text=True,encoding='utf-8',timeout=20)
 d=json.loads(p.stdout);checks.append({'args':['mtu','::1','-6','--min','1280'],'exit_code':p.returncode,'result':d})
 if p.returncode==3:
  assert d['error']['code']=='UNSUPPORTED_CAPABILITY' and not d['data']['probes'] and d['data']['fragmentation_control']=='unavailable'
 else:
  assert p.returncode==0,(p.returncode,p.stdout,p.stderr)
  data=d['data'];assert data['confirmed_lower']==1500 and data['confirmed_upper'] is None
  assert all(probe['ip_bytes']==probe['payload_bytes']+48 for probe in data['probes'])
else:
 d=run('mtu','::1','-6','--min','1280')['data'];assert d['confirmed_lower']==1500 and d['confirmed_upper'] is None
 assert all(p['ip_bytes']==p['payload_bytes']+48 for p in d['probes'])
p=subprocess.run([str(EXE),'mtu','127.0.0.1','--format','jsonl'],capture_output=True,text=True,encoding='utf-8',timeout=20);assert p.returncode==0,p.stderr
events=[json.loads(line) for line in p.stdout.splitlines()];assert events[-1]['event_type']=='finished';seq=[e['event_seq'] for e in events if 'event_seq' in e];assert seq==sorted(set(seq));assert any(e['event_type']=='pmtu_probe' for e in events)
text=subprocess.run([str(EXE),'mtu','127.0.0.1','--lang','zh-CN'],capture_output=True,text=True,encoding='utf-8',timeout=20);assert text.returncode==0 and 'ICMP' in text.stdout and '未知' in text.stdout
report={'status':'passed','platform':platform.platform(),'version':subprocess.check_output([str(EXE),'--version'],text=True).strip(),'binary_sha256':hashlib.sha256(EXE.read_bytes()).hexdigest(),'checks':checks,'jsonl_events':len(events)}
(OUT/'integration.json').write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf-8');print('M7 CLI loopback/validation/IPv6 capability/JSONL/text: passed')
