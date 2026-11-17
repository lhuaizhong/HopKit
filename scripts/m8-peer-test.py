"""Opt-in independent standard-port peer on a selected real interface.
Replies only to the short-lived test client's MAC/xid. Never serves other clients.
This is a UDP interoperability fixture, not a physical DHCP server acceptance test.
"""
import argparse,hashlib,json,os,socket,struct,subprocess,time
from pathlib import Path
root=Path(__file__).resolve().parents[1];p=argparse.ArgumentParser();p.add_argument('--interface',required=True);a=p.parse_args()
exe=root/'target/debug/hopkit-cli.exe';out=root/'test-output/m8';out.mkdir(parents=True,exist_ok=True)
snapshot=json.loads(subprocess.check_output([str(exe),'dhcp','inspect','--interface',a.interface,'--format','json']))['data']['interfaces'][0]
assert snapshot['dhcp_enabled'] is False,'Fixture requires a static IPv4 interface without an active system DHCP client'
mac=bytes.fromhex(snapshot['mac'].replace(':',''));observed=[]
with socket.socket(socket.AF_INET,socket.SOCK_DGRAM) as peer:
 peer.setsockopt(socket.SOL_SOCKET,socket.SO_EXCLUSIVEADDRUSE,1);peer.setsockopt(socket.SOL_SOCKET,socket.SO_BROADCAST,1)
 peer.setsockopt(socket.IPPROTO_IP,31,struct.pack('!I',snapshot['index']));peer.bind(('0.0.0.0',67));peer.settimeout(.1)
 process=subprocess.Popen([str(exe),'dhcp','discover','--interface',a.interface,'--duration','3','--attempts','1','--expected-server','192.0.2.1','--format','json'],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
 try:
  deadline=time.monotonic()+5;reply_count=0
  while time.monotonic()<deadline and process.poll() is None:
   try:data,source=peer.recvfrom(4096)
   except socket.timeout:continue
   if len(data)!=300 or data[:3]!=bytes([1,1,6]) or data[28:34]!=mac:continue
   expected=bytes([53,1,1,61,7,1])+mac+bytes([55,7,1,3,6,15,51,54,119,255]);expected=expected+bytes(60-len(expected))
   if data[240:]!=expected:continue
   assert source[1]==68 and data[240:243]==bytes([53,1,1]);assert data[10:12]==b'\x80\0';assert data[12:28]==bytes(16)
   observed.append(dict(source=list(source),payload_hex=data.hex(),message_type='DHCPDISCOVER'))
   # Standalone BOOTP encoder, intentionally independent of HopKit wire implementation.
   for server,yiaddr in [('192.0.2.1','192.0.2.20'),('192.0.2.2','192.0.2.21')]:
    response=bytearray(240);response[:3]=bytes([2,1,6]);response[4:8]=data[4:8];response[28:34]=mac;response[16:20]=socket.inet_aton(yiaddr);response[24:28]=socket.inet_aton('198.51.100.1');response[236:240]=bytes([99,130,83,99]);response.extend(bytes([53,1,2,54,4])+socket.inet_aton(server)+bytes([51,4])+struct.pack('!I',3600)+bytes([1,4,255,255,255,0,6,4,192,0,2,53,255]));peer.sendto(response,('255.255.255.255',68));reply_count+=1
  stdout,stderr=process.communicate(timeout=5);v=json.loads(stdout);assert process.returncode==0,v;d=v['data'];assert len(observed)==1;assert len(d['offers'])==2,d;assert {x['outside_expected'] for x in d['offers']}=={True,False};assert all(x['relay']=='198.51.100.1' for x in d['offers'])
  report=dict(status='passed',interface=snapshot['index'],observed=observed,replies=reply_count,result=v,scope='Independent UDP broadcast fixture on current interface; not physical DHCP-device acceptance',binary_sha256=hashlib.sha256(exe.read_bytes()).hexdigest());(out/'peer.json').write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf-8');print(json.dumps({k:v for k,v in report.items() if k not in ['result','observed']},ensure_ascii=False))
 finally:
  if process.poll() is None:process.terminate();process.wait(timeout=5)
