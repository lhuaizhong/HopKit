"""Independent Python peers against the native CLI. Loopback only."""
import json,os,socket,subprocess,threading,time,platform,hashlib
from pathlib import Path
root=Path(__file__).resolve().parents[1];exe=Path(os.environ.get('HOPKIT_TEST_EXE',root/'target/debug'/('hopkit-cli.exe' if os.name=='nt' else 'hopkit-cli')));out=root/'test-output/m4';out.mkdir(parents=True,exist_ok=True)
checks=[]
def run(args,input=None,code=0):
 p=subprocess.run([str(exe),*args],input=input,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=15)
 assert p.returncode==code,(args,p.returncode,p.stdout.decode(errors='replace'),p.stderr.decode(errors='replace'))
 return p.stdout
for family,host in [(socket.AF_INET,'127.0.0.1'),(socket.AF_INET6,'::1')]:
 with socket.socket(family) as peer:
  peer.bind((host,0));peer.listen();received=[]
  def serve():
   s,addr=peer.accept()
   with s:
    s.settimeout(5);b=b''
    while len(b)<4:b+=s.recv(4-len(b))
    received.append(b);s.sendall(b'\x00\xff\xf0');s.sendall(b'\x9f\x98\x80');s.shutdown(socket.SHUT_WR)
  t=threading.Thread(target=serve);t.start()
  lines=run(['socket','tcp','connect',host,str(peer.getsockname()[1]),'--source',host,'--duration','1','--input','raw','--format','jsonl'],b'\x00\xff\x01\x02').splitlines();t.join()
  values=[json.loads(x) for x in lines];assert received==[b'\x00\xff\x01\x02'];assert values[-1]['data']['stats']['received_bytes']==6;assert 'records' not in values[-1]['data'];assert any(v.get('record',{}).get('kind')=='read_half_closed' for v in values);assert all(v.get('event_type') for v in values)
  assert all(v['schema_version']==1 and v['task_id']==values[-1]['task_id'] for v in values);assert values[-1]['request']['socket']['target']==host;assert 'record_path' not in values[-1]['request']['socket']
  checks.append(f'TCP {host}: raw stdin, binary zeros, stream chunks, source binding, half-close, JSONL framing and metadata-only final result')
 with socket.socket(family,socket.SOCK_DGRAM) as peer:
  peer.bind((host,0));peer.settimeout(4);received=[]
  def receive():
   b,addr=peer.recvfrom(65535);received.append(b);peer.sendto(b,addr)
  t=threading.Thread(target=receive);t.start();r=json.loads(run(['socket','udp','send',host,str(peer.getsockname()[1]),'--hex','','--source',host,'--duration','1','--format','json']));t.join();assert received==[b''];assert r['data']['stats']['datagrams_sent']==1;assert r['data']['stats']['datagrams_received']==1
  checks.append(f'UDP {host}: zero-length datagram and independent echo')
with socket.socket(socket.AF_INET,socket.SOCK_DGRAM) as receiver:
 receiver.bind(('127.0.0.1',0));receiver.settimeout(3);data=[]
 def receive():
  for _ in range(3):data.append(receiver.recvfrom(200))
 t=threading.Thread(target=receive);t.start();r=json.loads(run(['wol','aabb.ccdd.ee02','--broadcast','127.0.0.1','--port',str(receiver.getsockname()[1]),'--source','127.0.0.1','--format','json']));t.join();assert all(b==b'\xff'*6+bytes.fromhex('aabbccddee02')*16 for b,addr in data);assert r['data']['sent']==3;assert 'wake_success' not in r['data'];checks.append('WOL: exact 102 bytes, three repetitions, actual loopback source and packet-send-only outcome')
# Endpoints belong to this independent Python process, not the HopKit query process.
for family,host in [(socket.AF_INET,'127.0.0.1'),(socket.AF_INET6,'::1')]:
 for typ,proto in [(socket.SOCK_STREAM,'tcp'),(socket.SOCK_DGRAM,'udp')]:
  with socket.socket(family,typ) as peer:
   peer.bind((host,0))
   if proto=='tcp':peer.listen()
   r=json.loads(run(['connections','--port',str(peer.getsockname()[1]),'--protocol',proto,'--listening','--format','json']))
   rows=r['data']['rows'];assert rows
   if os.name=='nt':assert any(row['pid']==os.getpid() for row in rows),rows
   if proto=='udp':assert all(row['state'] is None for row in rows)
   checks.append(f'Connections {host}/{proto}: independent process endpoint, port, family, PID and UDP state semantics')
for args in [['wol','ff:ff:ff:ff:ff:ff','--format','json'],['socket','tcp','connect','127.0.0.1','9','--hex','0','--format','json'],['socket','udp','listen','--echo','--duration','1','--format','json'],['connections','--port','0','--format','json']]:
 r=json.loads(run(args,code=2));assert r['error']['code']=='INVALID_ARGUMENT'
checks.append('CLI invalid MAC, HEX, unrestricted echo and endpoint filters reject before network I/O')
version=run(['--version']).decode().strip();report={'status':'passed','date':time.strftime('%Y-%m-%d'),'platform':platform.platform(),'version':version,'binary_sha256':hashlib.sha256(exe.read_bytes()).hexdigest(),'checks':checks,'not_tested':['macOS native permissions','physical WOL wake-up','two-hour socket soak']};(out/'integration.json').write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf-8');print(json.dumps(report,ensure_ascii=False,indent=2))
