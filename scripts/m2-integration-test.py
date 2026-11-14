#!/usr/bin/env python3
"""Independent stdlib subnet oracle and real local TCP CLI peers; no Internet access."""
import ipaddress,json,random,socket,subprocess,signal,time
from pathlib import Path
root=Path(__file__).resolve().parents[1]
cli=root/'target/debug/hopkit-cli'
def run(*args,code=0):
 p=subprocess.run([str(cli),*args,'--format','json'],capture_output=True,text=True,timeout=20)
 assert p.returncode==code,(args,p.returncode,p.stdout,p.stderr)
 return json.loads(p.stdout)
rng=random.Random(2042)
count=0
for version,bits in [(4,32),(6,128)]:
 for prefix in ([0,1,7,24,31,32] if version==4 else [0,1,33,64,73,127,128]):
  for _ in range(3):
   address=ipaddress.ip_address(rng.getrandbits(bits)) if version==6 else ipaddress.IPv4Address(rng.getrandbits(bits))
   oracle=ipaddress.ip_network(f'{address}/{prefix}',strict=False)
   data=run('subnet','inspect',f'{address}/{prefix}')['data']['subnet']
   assert data['network']==str(oracle),(oracle,data)
   assert data['address_count']==str(oracle.num_addresses)
   assert data['first']==str(oracle.network_address) and data['last']==str(oracle.broadcast_address)
   assert data['broadcast']==(str(oracle.broadcast_address) if version==4 and prefix<31 else None)
   delta=min(3,bits-prefix)
   children=run('subnet','split',str(oracle),'--prefix',str(prefix+delta))['data']['networks']
   assert children==[str(n) for n in oracle.subnets(new_prefix=prefix+delta)]
   agg=run('subnet','aggregate',*children)['data']['networks'];assert agg==[str(oracle)]
   assert run('subnet','contains',str(oracle),str(address))['data']['contains']
   overlap=run('subnet','overlap',str(oracle),children[-1])['data'];assert overlap['overlaps'] and overlap['contains_other']
   reverse=run('subnet','overlap',children[-1],str(oracle))['data'];assert reverse['overlaps']==overlap['overlaps'] and reverse['contained_by_other']
   count+=1
assert not run('subnet','contains','192.0.2.0/24','192.0.3.1',code=1)['data']['contains']
for invalid in ['192.0.2.1/255.0.255.0','::/129','10.0.0.0/33']:run('subnet','inspect',invalid,code=2)
run('subnet','split','::/0','--prefix','128',code=2)
for ports in ['0','65536','4-2','80,']:run('scan','tcp','127.0.0.1','--ports',ports,code=2)
run('scan','tcp','0.0.0.0/0','--ports','1',code=2)
run('scan','tcp','127.0.0.0/20','--ports','1-17',code=2)
for family,host in [(socket.AF_INET,'127.0.0.1'),(socket.AF_INET6,'::1')]:
 with socket.socket(family) as peer:
  peer.bind((host,0));peer.listen(20);port=peer.getsockname()[1]
  with socket.socket(family) as closed:closed.bind((host,0));refused=closed.getsockname()[1]
  d=run('scan','tcp',host,host,'--ports',f'{port},{port},{refused}','--source',host)['data']
  assert d['stats']['total']==2 and d['stats']['counts']['open']==1 and d['stats']['counts']['refused']==1
  closed_only=run('scan','tcp',host,'--ports',str(refused))
  assert closed_only['data']['stats']['counts'].get('open',0)==0 and closed_only['data']['stats']['counts']['refused']==1
  conn,addr=peer.accept();conn.close();assert addr[0]==host
  lines=subprocess.run([str(cli),'scan','tcp',host,'--ports',str(port),'--format','jsonl'],capture_output=True,text=True,timeout=10)
  events=[json.loads(x) for x in lines.stdout.splitlines()];assert lines.returncode==0 and events[-1]['event_type']=='finished' and any(e['event_type']=='scan_row' for e in events)
 p=subprocess.Popen([str(cli),'scan','tcp',host,'--ports','1-100','--rate','1','--format','jsonl'],stdout=subprocess.PIPE,text=True)
 first=p.stdout.readline();assert json.loads(first)['event_type']=='scan_target';time.sleep(.1);start=time.monotonic();p.send_signal(signal.SIGINT);out=p.communicate(timeout=3)[0]
 assert p.returncode==130 and time.monotonic()-start<1.5
 final=json.loads(out.splitlines()[-1]);assert final['data']['partial'] and final['data']['stats']['remaining']>0
report={'status':'passed','stdlib_subnet_cases':count,'cli_exit_codes':[0,1,2,130],'scan':'IPv4/IPv6 real open/refused, dedup, source binding, JSONL and Ctrl+C partial results'}
out=root/'test-output/m2';out.mkdir(parents=True,exist_ok=True);(out/'integration.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
