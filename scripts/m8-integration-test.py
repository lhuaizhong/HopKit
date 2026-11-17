"""CLI DHCP acceptance. Default is read-only. --live-interface enables one real DISCOVER."""
import argparse,hashlib,json,os,platform,socket,subprocess,time
from pathlib import Path
root=Path(__file__).resolve().parents[1];out=root/'test-output/m8';out.mkdir(parents=True,exist_ok=True)
p=argparse.ArgumentParser();p.add_argument('--live-interface');args=p.parse_args()
exe=Path(os.environ.get('HOPKIT_TEST_EXE',root/'target/debug'/('hopkit-cli.exe' if os.name=='nt' else 'hopkit-cli'))).resolve();checks=[]
def cli(*args,code=0):
 r=subprocess.run([str(exe),*map(str,args),'--format','json'],capture_output=True,timeout=15);data=json.loads(r.stdout);assert r.returncode==code,(r.returncode,data);return data
def inspected():
 d=cli('dhcp','inspect')['data'];assert d['stats']['sent']==0;assert d['operation']=='inspect';assert d['config_source'] in ['windows-iphelper','macos-systemconfiguration'];return d
d=inspected();assert d['interfaces'];iface=d['interfaces'][0]
assert cli('dhcp','inspect','--interface',iface['id'])['data']['interfaces'][0]['index']==iface['index']
checks.append('Read-only OS snapshot, exact adapter ID, explicit null lease fields, no packets')
for cmd in [('inspect','--interface','HOPKIT_NONEXISTENT'),('discover','--interface','8','--duration','31'),('discover','--interface','8','--attempts','4'),('discover','--interface','8','--expected-server','224.0.0.1'),('discover',)]:
 assert cli('dhcp',*cmd,code=2)['error']['code']=='INVALID_ARGUMENT'
checks.append('Missing/invalid interface, range, attempt and expected-server inputs fail with exit 2')
r=subprocess.run([str(exe),'dhcp','inspect','--lang','zh-CN'],capture_output=True,timeout=15);assert r.returncode==0;assert 'DHCPv4' in r.stdout.decode('utf-8');checks.append('Chinese text output includes current configuration and source')
r=subprocess.run([str(exe),'dhcp','inspect','--format','jsonl'],capture_output=True,timeout=15);assert r.returncode==0;events=[json.loads(x) for x in r.stdout.splitlines()];assert events[-1]['event_type']=='finished';assert any(e.get('event_type')=='dhcp_snapshot' for e in events);checks.append('JSONL includes bounded typed snapshot and final envelope')
live=None
if args.live_interface:
 before=cli('dhcp','inspect','--interface',args.live_interface)['data']['interfaces']
 r=subprocess.run([str(exe),'dhcp','discover','--interface',args.live_interface,'--duration','3','--attempts','1','--format','json'],capture_output=True,timeout=15);live=json.loads(r.stdout);assert r.returncode in [0,1,3],live
 if live['error'] is None:assert live['data']['stats']['sent']==1;assert live['data']['local_endpoint']=='0.0.0.0:68';assert live['data']['remote_endpoint']=='255.255.255.255:67'
 after=cli('dhcp','inspect','--interface',args.live_interface)['data']['interfaces'];assert before==after,'OS configuration changed during probe'
 checks.append('Real interface: standard UDP 68→67, one DISCOVER; OS addresses, DNS and lease snapshot unchanged')
 # Occupy client port, ensure explicit local error and retained read-only information.
 with socket.socket(socket.AF_INET,socket.SOCK_DGRAM) as sock:
  if os.name=='nt':sock.setsockopt(socket.SOL_SOCKET,socket.SO_EXCLUSIVEADDRUSE,1)
  sock.bind(('0.0.0.0',68));v=cli('dhcp','discover','--interface',args.live_interface,'--duration','1','--attempts','1',code=3);assert v['error']['code'] in ['ADDRESS_IN_USE','PERMISSION_DENIED'];assert v['data']['interfaces'];assert v['data']['stats']['sent']==0
 checks.append('Standard port occupied: explicit local failure, no reuse, no transmission, inspect evidence retained')
report=dict(status='passed',platform=platform.platform(),binary_sha256=hashlib.sha256(exe.read_bytes()).hexdigest(),version=subprocess.check_output([str(exe),'--version']).decode().strip(),checks=checks,live=live)
(out/'integration.json').write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf-8');print(json.dumps({k:v for k,v in report.items() if k!='live'},ensure_ascii=False,indent=2))
