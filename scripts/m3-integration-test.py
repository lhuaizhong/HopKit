#!/usr/bin/env python3
"""Independent loopback TCP/DNS peers; no public scanning. Requires local network sandbox access."""
import argparse,json,os,signal,socket,struct,subprocess,threading,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1];CLI=ROOT/'target/debug/hopkit-cli';OUT=ROOT/'test-output/m3';OUT.mkdir(parents=True,exist_ok=True)
parser=argparse.ArgumentParser();parser.add_argument("--lan-target");parser.add_argument("--lan-network");parser.add_argument("--lan-interface",default="");options=parser.parse_args()
checks=[]
def run(args,code=0,fmt='json'):
 p=subprocess.run([str(CLI),'--format',fmt,*args],capture_output=True,text=True,timeout=45 if options.lan_network else 20)
 assert p.returncode==code,(args,p.returncode,p.stdout,p.stderr)
 if fmt=='json':return json.loads(p.stdout)
 return p.stdout
stop=threading.Event()
with socket.socket() as peer,socket.socket(socket.AF_INET,socket.SOCK_DGRAM) as dns,socket.socket() as closed:
 peer.bind(('127.0.0.1',0));peer.listen(128);peer.settimeout(.1);port=peer.getsockname()[1]
 closed.bind(('127.0.0.1',0));refused=closed.getsockname()[1];closed.close()
 dns.bind(('127.0.0.1',0));dns.settimeout(.1);queries=[]
 def accept():
  while not stop.is_set():
   try:conn,addr=peer.accept();conn.close()
   except socket.timeout:pass
 def answer():
  while not stop.is_set():
   try:packet,addr=dns.recvfrom(4096)
   except socket.timeout:continue
   queries.append(packet)
   header=packet[:2]+b'\x81\x80\x00\x01\x00\x01\x00\x00\x00\x00'
   name=b'\x07fixture\x07example\x00'
   dns.sendto(header+packet[12:]+b'\xc0\x0c'+struct.pack('!HHIH',12,1,60,len(name))+name,addr)
 workers=[threading.Thread(target=accept),threading.Thread(target=answer)]
 for w in workers:w.start()
 try:
  data=run(['discover','127.0.0.1','127.0.0.1/32','--methods','tcp','--ports',f'{port},{refused}','--source','127.0.0.1'])['data']
  assert data['stats']['candidates']==1 and data['stats']['responders']==1 and data['stats']['completed']==2,data
  row=data['rows'][0];assert {o['status'] for o in row['observations']}=={'open','refused'},row;assert all(o['responded'] for o in row['observations']);assert row['first_response_at'] and row['last_response_at'];assert next(o for o in row['observations'] if o['status']=='open')['local_endpoint'].startswith('127.0.0.1:')
  checks.append('IPv4 real TCP open/refused evidence, candidate deduplication, source binding and timestamps')
  data=run(['discover','127.0.0.1','--methods','icmp','--source','127.0.0.1'])['data'];assert data['rows'][0]['observations'][0]['status']=='reply',data
  checks.append('Native source-bound loopback ICMP echo')
  with socket.socket(socket.AF_INET6) as v6:
   v6.bind(('::1',0));v6.listen();d=run(['discover','::1','0:0:0:0:0:0:0:1','--methods','tcp','--source','::1','--ports',str(v6.getsockname()[1])])['data'];assert d['stats']['candidates']==1 and d['stats']['responders']==1
  checks.append('IPv6 explicit candidates, canonical deduplication and TCP source binding')
  d=run(['discover','127.0.0.1','--methods','tcp','--ports',str(port),'--name-lookup','--dns-server',f'127.0.0.1:{dns.getsockname()[1]}'])['data']
  assert d['rows'][0]['name']['names']==['fixture.example'],d;assert queries and b'1\x010\x010\x03127\x07in-addr\x04arpa' in queries[0]
  checks.append('Independent DNS PTR response with question validation and source attribution')
  with socket.socket(socket.AF_INET,socket.SOCK_DGRAM) as silent:
   silent.bind(('127.0.0.1',0));before=time.monotonic();d=run(['discover','127.0.0.1','--methods','tcp','--ports',str(port),'--name-lookup','--dns-server',f'127.0.0.1:{silent.getsockname()[1]}','--timeout-ms','100'])['data'];assert d['rows'][0]['responded'] and d['rows'][0]['name']['error']['code']=='TIMEOUT' and d['partial'];assert time.monotonic()-before<2
  checks.append('PTR timeout preserves active evidence and bounded completion')
  with socket.socket(socket.AF_INET,socket.SOCK_DGRAM) as silent:
   silent.bind(('127.0.0.1',0));p=subprocess.Popen([str(CLI),'--format','jsonl','discover','127.0.0.1','--methods','tcp','--ports',str(port),'--name-lookup','--dns-server',f'127.0.0.1:{silent.getsockname()[1]}','--timeout-ms','1000'],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
   while True:
    line=p.stdout.readline();assert line
    event=json.loads(line)
    if event.get('event_type')=='discovery_device' and event['device']['responded']:break
   time.sleep(.05);p.send_signal(signal.SIGINT);stdout,stderr=p.communicate(timeout=3);assert p.returncode==130,(stdout,stderr);d=json.loads(stdout.splitlines()[-1])['data'];assert d['stats']['remaining']==0 and d['partial'] and d['rows'][0]['responded'],d
  checks.append('Cancellation during optional PTR preserves completed active evidence and marks the result partial')
  for target in ['192.0.2.0/0','::/64','192.0.2.2-192.0.2.1','fe80::1','localhost']:
   assert run(['discover',target],2)['error']['code']=='INVALID_ARGUMENT'
  assert run(['discover','127.0.0.1','--methods','tcp','--ports',str(port),'--source','192.0.2.222'],3)['error']['code']=='LOCAL_PROBE_FAILED'
  checks.append('Invalid range/scope/name and unavailable source return explicit nonzero exits')
  skipped=run(['discover','127.0.0.1','198.51.100.1','2001:db8::1','--methods','arp'])['data']
  assert skipped['stats']['responders']==0 and all(o['status']=='skipped_not_on_link' and not o['responded'] and 'neighbor' not in o for r in skipped['rows'] for o in r['observations']),skipped
  text=run(['discover','198.51.100.1','--methods','arp'],fmt='text');assert '198.51.100.1' not in text,text
  checks.append('ARP skips routed IPv4, IPv6 and loopback; text hides unknown IPs while JSON retains evidence')
  cache=run(['discover'])['data'];assert cache['stats']['planned']==0 and cache['stats']['responders']==0;assert all(not r['responded'] and not r['observations'] for r in cache['rows']);assert cache['neighbor_sources']
  (OUT/'neighbors.json').write_text(json.dumps(cache,ensure_ascii=False,indent=2))
  checks.append('Real macOS read-only ARP/ND cache: states/interfaces/timestamps, no liveness inference')
  text=run(['--lang','zh-CN','discover','127.0.0.1','--methods','tcp','--ports',str(port)],fmt='text');assert '127.0.0.1' in text and '未知' in text
  lines=run(['discover','127.0.0.1','--methods','tcp','--ports',str(port)],fmt='jsonl').splitlines();assert any(json.loads(l).get('event_type')=='discovery_device' for l in lines)
  checks.append('Chinese text and structured JSON/JSONL output')
  p=subprocess.Popen([str(CLI),'--format','json','discover','127.0.0.0/31','--methods','tcp','--ports',str(port),'--rate','1'],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True);time.sleep(.25);before=time.monotonic();p.send_signal(signal.SIGINT);stdout,stderr=p.communicate(timeout=3);assert p.returncode==130,(p.returncode,stdout,stderr);d=json.loads(stdout)['data'];assert d['partial'] and d['stats']['remaining']>0;assert time.monotonic()-before<1.2
  checks.append('Ctrl+C exits 130 within 1.2 seconds and retains partial rows/unknown candidates')
 finally:
  stop.set()
  for w in workers:w.join(timeout=2)
if options.lan_network:
 assert options.lan_target,'--lan-network requires --lan-target for gateway response assertion'
 data=run(['discover',options.lan_network,'--interface',options.lan_interface])['data']
 row=next(r for r in data['rows'] if r['address']==options.lan_target)
 assert row['responded'] and any(o['responded'] for o in row['observations']),row
 (OUT/'gateway-subnet.json').write_text(json.dumps(data,ensure_ascii=False,indent=2))
 checks.append('User-specified LAN subnet includes a genuinely responding gateway; full result in gateway-subnet.json')
if options.lan_target:
 data=run(['discover',options.lan_target,'--interface',options.lan_interface,'--ports','80,443'])['data'];assert data['stats']['candidates']==1 and data['stats']['completed'] in [4,5],data
 assert all(o['source'] for o in data['rows'][0]['observations']),data
 (OUT/'lan-device.json').write_text(json.dumps(data,ensure_ascii=False,indent=2))
 checks.append('Single LAN target: selected interface/source and honest response/unknown evidence; see lan-device.json for physical acceptance limits')
report={'status':'passed','version':subprocess.check_output([str(CLI),'--version'],text=True).strip(),'checks':checks};(OUT/'integration.json').write_text(json.dumps(report,ensure_ascii=False,indent=2));print(json.dumps(report,ensure_ascii=False,indent=2))
