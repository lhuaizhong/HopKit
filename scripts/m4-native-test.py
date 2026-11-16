"""Native GUI + Rust IPC. Requires debug custom-protocol,native-test build."""
import hashlib,json,os,platform,socket,subprocess,threading,time
from pathlib import Path
root=Path(__file__).resolve().parents[1];out=root/'test-output/m4';out.mkdir(parents=True,exist_ok=True);report=out/'native.json'
if report.exists():report.unlink()
tcp=socket.socket();tcp.bind(('127.0.0.1',0));tcp.listen();tcp.settimeout(.2);udp=socket.socket(socket.AF_INET,socket.SOCK_DGRAM);udp.bind(('127.0.0.1',0));udp.settimeout(.2);stop=threading.Event();packets=[];received=[]
def serve():
 while not stop.is_set():
  try:
   s,_=tcp.accept()
   with s:
    s.settimeout(.2)
    while not stop.is_set():
     try:
      b=s.recv(65536)
      if not b:break
      received.append(b);s.sendall(b)
     except socket.timeout:pass
  except socket.timeout:pass
def recv():
 while not stop.is_set():
  try:packets.append(udp.recvfrom(200)[0])
  except socket.timeout:pass
threads=[threading.Thread(target=serve),threading.Thread(target=recv)]
for t in threads:t.start()
ports=[]
for i in range(4):
 with socket.socket(socket.AF_INET,socket.SOCK_DGRAM if i==3 else socket.SOCK_STREAM) as s:s.bind(('127.0.0.1',0));ports.append(s.getsockname()[1])
def flood():
 with socket.socket(socket.AF_INET,socket.SOCK_DGRAM) as peer:
  while not stop.is_set():
   for _ in range(16):peer.sendto(bytes(range(256))*2,('127.0.0.1',ports[3]))
   stop.wait(.001)
load=threading.Thread(target=flood);threads.append(load);load.start()
config=out/'native-config.json';config.write_text(json.dumps({'scenario':'m4','tcp_port':tcp.getsockname()[1],'udp_port':udp.getsockname()[1],'peer_pid':os.getpid(),'listen_ports':ports}),encoding='utf-8')
exe=root/'target/debug'/('hopkit-desktop.exe' if os.name=='nt' else 'hopkit-desktop')
process=subprocess.Popen([str(exe)],env={**os.environ,'HOPKIT_NATIVE_TEST_CONFIG':str(config)},stdout=subprocess.PIPE,stderr=subprocess.PIPE)
try:
 stdout,stderr=process.communicate(timeout=65);assert process.returncode==0,(process.returncode,stderr.decode(errors='replace'))
 data=json.loads(report.read_text(encoding='utf-8'));assert data['status']=='passed',data
 assert b''.join(received)==bytes.fromhex('00ff0102'),received
 assert packets==[b'\xff'*6+bytes.fromhex('aabbccddee02')*16]*3,packets
 data.update(date=time.strftime('%Y-%m-%d'),platform=platform.platform(),binary_sha256=hashlib.sha256(exe.read_bytes()).hexdigest(),parameters=json.loads(config.read_text(encoding='utf-8')))
 report.write_text(json.dumps(data,ensure_ascii=False,indent=2),encoding='utf-8')
 print(json.dumps(data,ensure_ascii=False,indent=2))
finally:
 if process.poll() is None:process.terminate();process.wait(timeout=10)
 stop.set()
 for t in threads:t.join(timeout=2)
 tcp.close();udp.close()
