"""Independent standard-library DNS peer. Verifies PTR is optional and separate."""
import json,os,socket,struct,subprocess,threading
from pathlib import Path
root=Path(__file__).resolve().parents[1];out=root/'test-output/m5';out.mkdir(parents=True,exist_ok=True);exe=Path(os.environ.get('HOPKIT_TEST_EXE',str(root/'target/debug'/('hopkit-cli.exe' if os.name=='nt' else 'hopkit-cli'))))
server=socket.socket(socket.AF_INET,socket.SOCK_DGRAM);server.bind(('127.0.0.1',0));server.settimeout(.1);stop=threading.Event();questions=[]
def serve():
 while not stop.is_set():
  try:
   q,peer=server.recvfrom(4096);pos=12;labels=[]
   while q[pos]:n=q[pos];labels.append(q[pos+1:pos+1+n].decode('ascii'));pos+=1+n
   pos+=1;assert q[pos:pos+4]==b'\0\x0c\0\x01';questions.append('.'.join(labels));name=b'\x08loopback\x04test\0';reply=q[:2]+b'\x81\x80'+struct.pack('!HHHH',1,1,0,0)+q[12:pos+4]+b'\xc0\x0c'+struct.pack('!HHIH',12,1,60,len(name))+name;server.sendto(reply,peer)
  except socket.timeout:pass
thread=threading.Thread(target=serve);thread.start()
try:
 cmd=[str(exe),'mtr','127.0.0.1','--cycles','8','--interval-ms','100','--format','json'];p=subprocess.run(cmd,capture_output=True,text=True,encoding='utf-8',timeout=10);assert p.returncode==0;assert json.loads(p.stdout)['data']['names']==[];assert not questions
 p=subprocess.run([*cmd,'--ptr-server',f'127.0.0.1:{server.getsockname()[1]}'],capture_output=True,text=True,encoding='utf-8',timeout=10);assert p.returncode==0,(p.returncode,p.stdout,p.stderr);d=json.loads(p.stdout)['data'];assert questions==['1.0.0.127.in-addr.arpa'];assert d['names']==[{'address':'127.0.0.1','names':['loopback.test'],'status':'complete'}];assert d['stats']['responses']==8
 report={'status':'passed','question':questions[0],'names':d['names'],'icmp_responses':d['stats']['responses'],'checks':['No implicit DNS','Explicit numeric resolver','Independent PTR peer validates question and answer','One lookup per responder; no ICMP count contamination']};(out/'ptr.json').write_text(json.dumps(report,indent=2),encoding='utf-8');print(json.dumps(report,indent=2))
finally:stop.set();thread.join(timeout=2);server.close()
