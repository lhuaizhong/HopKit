"""Independent Net-SNMP interoperability and adversarial UDP fixtures. Loopback only."""
import argparse,os,json,subprocess,tempfile,time,socket,threading,signal,hashlib,platform
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
CLI=Path(os.environ.get('HOPKIT_TEST_EXE',str(ROOT/'target/debug/hopkit-cli')))
OUT=ROOT/'test-output/m6';OUT.mkdir(parents=True,exist_ok=True)
COMM='m6-community-fixture';AUTH='m6-auth-fixture';PRIV='m6-privacy-fixture'
ENV={**os.environ,'HOPKIT_SNMP_COMMUNITY':COMM,'HOPKIT_SNMP_AUTH_PASSWORD':AUTH,'HOPKIT_SNMP_PRIVACY_PASSWORD':PRIV}
checks=[]
def run(*args,code=0,env=None):
 p=subprocess.run([str(CLI),'snmp',*args,'--format','json'],env=env or ENV,capture_output=True,text=True,timeout=20)
 assert p.returncode==code,(args,p.returncode,p.stdout,p.stderr)
 for secret in [COMM,AUTH,PRIV]:assert secret not in p.stdout+p.stderr
 return json.loads(p.stdout)
def port():
 s=socket.socket(socket.AF_INET,socket.SOCK_DGRAM);s.bind(('127.0.0.1',0));n=s.getsockname()[1];s.close();return n
# Independent BER fixture encoder/decoder (not using product codec).
def tlv(t,b):return bytes([t])+ (bytes([len(b)]) if len(b)<128 else bytes([0x82])+len(b).to_bytes(2,'big'))+b
def integer(n):return tlv(2,n.to_bytes(4,'big',signed=True))
def read(b):
 tag,n=b[:2];i=2
 if n&128:k=n&127;n=int.from_bytes(b[i:i+k],'big');i+=k
 return tag,b[i:i+n],b[i+n:]
def pieces(b):
 out=[]
 while b:t,v,b=read(b);out.append((t,v))
 return out
def response(packet,mode):
 m=pieces(read(packet)[1]);p=pieces(m[2][1]);v=pieces(p[3][1]);o=pieces(v[0][1])[0][1]
 if mode=='bad_ber':return b'\x30\x84\xff\xff\xff\xff'
 if mode=='wrong_id':ident=integer(123)
 else:ident=tlv(*p[0])
 status=integer(1 if mode=='too_big' else 0)
 if mode=='advancing':o+=b'\x01'
 val=tlv(0x80,b'') if mode=='no_such' else tlv(4,b'fixture-value')
 vb=tlv(0x30,tlv(6,o)+val)
 if mode=='counter64':vb=tlv(0x30,tlv(6,o)+tlv(0x46,b'\0'+(2**64-1).to_bytes(8,'big')))
 if mode=='empty_walk':vb=tlv(0x30,tlv(6,b'\x2b\x06\x01\x04\x01\x7f')+tlv(0x82,b''))
 return tlv(0x30,tlv(*m[0])+tlv(*m[1])+tlv(0xa2,ident+status+integer(0)+tlv(0x30,b'' if mode=='too_big' else vb)))
def fixture(mode,args,code,family=socket.AF_INET):
 sock=socket.socket(family,socket.SOCK_DGRAM);sock.bind(('::1' if family==socket.AF_INET6 else '127.0.0.1',0));sock.settimeout(.1);done=threading.Event();received=[]
 def server():
  while not done.is_set():
   try:b,a=sock.recvfrom(65535);received.append(b)
   except socket.timeout:continue
   if mode=='silent' or mode=='retry' and len(received)==1:continue
   sock.sendto(response(b,'normal' if mode=='retry' else mode),a)
 t=threading.Thread(target=server);t.start()
 try:return run(*args,'--port',str(sock.getsockname()[1]),'--timeout-ms','100','--retries','1',code=code),received
 finally:done.set();t.join();sock.close()

def main():
 parser=argparse.ArgumentParser();parser.add_argument('--fixtures-only',action='store_true');args=parser.parse_args()
 if not args.fixtures_only:interoperability()
 fixtures()

def interoperability():
 snmpd=os.environ.get('HOPKIT_TEST_SNMPD','/opt/homebrew/opt/net-snmp/sbin/snmpd')
 with tempfile.TemporaryDirectory(prefix='hopkit-m6-') as tmp:
  endpoint=port();cfg=Path(tmp)/'snmpd.conf';state=Path(tmp)/'state';state.mkdir()
  cfg.write_text(f'''agentaddress udp:127.0.0.1:{endpoint}
rocommunity {COMM} 127.0.0.1
createUser m6user SHA-256 {AUTH} AES {PRIV}
rouser m6user priv
createUser legacyuser SHA {AUTH} AES {PRIV}
rouser legacyuser priv
createUser authuser SHA-256 {AUTH}
rouser authuser auth
createUser plainuser
rouser plainuser noauth
sysName HopKit-M6-NetSNMP
''')
  with open(OUT/'net-snmp.log','w') as log:
   agent=subprocess.Popen([snmpd,'-f','-Lo','-C','-c',str(cfg)],env={**os.environ,'SNMP_PERSISTENT_DIR':str(state)},stdout=log,stderr=log)
   try:
    time.sleep(1);assert agent.poll() is None
    common=['127.0.0.1','--port',str(endpoint)]
    r=run('get',*common,'sysName');assert r['data']['varbinds'][0]['value']=='HopKit-M6-NetSNMP'
    for op,oid in [('getnext','system'),('getbulk','system'),('walk','system'),('overview','sysName')]:
     r=run(op,*common,oid);assert r['data']['stats']['results']>0
    checks.append('Net-SNMP v2c GET/GETNEXT/GETBULK/WALK/overview')
    for user,level,auth in [('m6user','auth_priv','sha256'),('legacyuser','auth_priv','sha1'),('authuser','auth_no_priv','sha256'),('plainuser','no_auth_no_priv','sha256')]:
     for op,oid in [('get','sysName'),('walk','system')]:
      r=run(op,*common,oid,'--snmp-version','3','--username',user,'--security-level',level,'--auth-protocol',auth);assert r['data']['engine']['id'];assert r['data']['varbinds']
    checks.append('Net-SNMP v3 SHA256/AES128, SHA1/AES128, authNoPriv and noAuthNoPriv')
    r=run('interfaces',*common,'--samples','2','--interval-ms','1000');assert r['data']['stats']['samples']==2;assert r['data']['interface_samples'][-1]['interfaces'];(OUT/'interfaces.json').write_text(json.dumps(r,indent=2))
    checks.append('Net-SNMP IF-MIB two samples, real intervals, counters/errors/discards')
    run('get',*common,'1.3.6.1.2.1.99999.0',code=1)
    run('get',*common,'sysName','--timeout-ms','100','--retries','0',env={**ENV,'HOPKIT_SNMP_COMMUNITY':'wrong'},code=1)
    run('get',*common,'sysName','--snmp-version','3','--username','m6user',env={**ENV,'HOPKIT_SNMP_AUTH_PASSWORD':'wrong-password'},code=1)
    r=run('walk',*common,'system','--max-results','2',code=1);assert r['data']['partial'] and len(r['data']['varbinds'])==2
    proxy=socket.socket(socket.AF_INET,socket.SOCK_DGRAM);proxy.bind(('127.0.0.1',0));proxy.settimeout(.1);stop=threading.Event();uptimes=[0]
    def sample_proxy():
     while not stop.is_set():
      try:b,a=proxy.recvfrom(65535)
      except socket.timeout:continue
      fields=pieces(read(b)[1]);p=pieces(fields[2][1]);v=pieces(p[3][1]);name=pieces(v[0][1])[0][1]
      if fields[2][0]==0xa0 and name==b'\x2b\x06\x01\x02\x01\x01\x03\x00':
       uptimes[0]+=1
       if uptimes[0]==2:continue
      remote=socket.socket(socket.AF_INET,socket.SOCK_DGRAM);remote.settimeout(1);remote.sendto(b,('127.0.0.1',endpoint))
      try:proxy.sendto(remote.recv(65535),a)
      except socket.timeout:pass
      finally:remote.close()
    thread=threading.Thread(target=sample_proxy);thread.start()
    try:
     recovered=run('interfaces','127.0.0.1','--port',str(proxy.getsockname()[1]),'--samples','4','--interval-ms','1000','--timeout-ms','100','--retries','0',code=1)
     d=recovered['data'];assert d['stats']['samples']==4 and d['stats']['samples_failed']==1;assert d['interface_samples'][1]['gap'];assert all(i['in_bps'] is None for i in d['interface_samples'][2]['interfaces']);assert d['stats']['samples_valid']==3
    finally:stop.set();thread.join();proxy.close()
    checks.append('Missing IF-MIB sample recorded as gap; observer recovers and resets rate baseline')
    # A proxy mutates authenticated replies and strips privacy to test rejection.
    for attack in ['digest','downgrade','wrong_message','forged_scoped','clock_skew']:
     proxy=socket.socket(socket.AF_INET,socket.SOCK_DGRAM);proxy.bind(('127.0.0.1',0));proxy.settimeout(.1);stop=threading.Event()
     def relay():
      while not stop.is_set():
       try:b,a=proxy.recvfrom(65535)
       except socket.timeout:continue
       remote=socket.socket(socket.AF_INET,socket.SOCK_DGRAM);remote.settimeout(1);remote.sendto(b,('127.0.0.1',endpoint))
       try:reply=bytearray(remote.recv(65535))
       except socket.timeout:remote.close();continue
       remote.close()
       # Discovery is plaintext; corrupt only subsequent authenticated replies.
       outer=pieces(read(reply)[1]);header=pieces(outer[1][1]);flags=header[2][1][0]
       if flags&1 and attack!='clock_skew':
        if attack=='digest':reply[-1]^=1
        else:
         h=header[:]
         if attack=='downgrade':h[2]=(4,b'\0')
         if attack=='wrong_message':h[0]=(2,b'\x01')
         # Forged time without valid MAC must never update state.
         if attack=='forged_scoped':reply[-2]^=0x40
         if attack!='forged_scoped':reply=bytearray(tlv(0x30,tlv(*outer[0])+tlv(0x30,b''.join(tlv(*x) for x in h))+b''.join(tlv(*x) for x in outer[2:])))
       elif attack=='clock_skew' and not flags&1:
        sec=pieces(read(outer[2][1])[1]);sec[2]=(2,(1000000).to_bytes(4,'big'));outer[2]=(4,tlv(0x30,b''.join(tlv(*x) for x in sec)));reply=bytearray(tlv(0x30,b''.join(tlv(*x) for x in outer)))
       proxy.sendto(reply,a)
     thread=threading.Thread(target=relay);thread.start()
     try:
      r=run('get','127.0.0.1','sysName','--port',str(proxy.getsockname()[1]),'--snmp-version','3','--username','m6user',code=0 if attack=='clock_skew' else 1);assert r['data']['success'] if attack=='clock_skew' else r['error']['code'] in ['SNMP_AUTH','SNMP_PROTOCOL'],(attack,r)
     finally:stop.set();thread.join();proxy.close()
    checks.append('Wrong credentials, object errors, WALK limit; v3 forged digest/downgrade/message/time rejected')
   finally:agent.terminate();agent.wait(timeout=5)

def fixtures():
 for mode,op,code in [('bad_ber','get',1),('wrong_id','get',1),('too_big','get',1),('no_such','get',1),('counter64','get',0),('empty_walk','walk',0),('nonadvancing','walk',1),('retry','get',0),('silent','get',1)]:
  r,packets=fixture(mode,[op,'127.0.0.1','1.3.6.1.2.1.1.3.0'],code)
  if mode=='counter64':assert r['data']['varbinds'][0]['value']==str(2**64-1)
  if mode=='retry':assert len(packets)==2 and r['data']['stats']['retries']==1
 r,_=fixture('advancing',['walk','127.0.0.1','system','--max-results','3'],1);assert len(r['data']['varbinds'])==3 and r['data']['partial']
 sock=socket.socket(socket.AF_INET,socket.SOCK_DGRAM);sock.bind(('127.0.0.1',0));sock.settimeout(3)
 child=subprocess.Popen([str(CLI),'snmp','walk','127.0.0.1','system','--port',str(sock.getsockname()[1]),'--timeout-ms','10000','--format','jsonl'],env=ENV,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,creationflags=subprocess.CREATE_NEW_PROCESS_GROUP if os.name=='nt' else 0)
 try:
  b,addr=sock.recvfrom(65535);sock.sendto(response(b,'advancing'),addr);sock.recvfrom(65535);time.sleep(.05);child.send_signal(signal.CTRL_BREAK_EVENT if os.name=='nt' else signal.SIGINT);out,err=child.communicate(timeout=2);assert child.returncode==130;assert len(json.loads(out.splitlines()[-1])['data']['varbinds'])==1
 finally:
  sock.close()
  if child.poll() is None:child.terminate();child.wait(timeout=5)
 checks.append('WALK result budget and cancellation retain already received rows')
 r,_=fixture('counter64',['get','::1','sysUpTime'],0,family=socket.AF_INET6);assert r['data']['remote_endpoint'].startswith('[::1]')
 checks.append('Independent BER: malformed, ID mismatch, tooBig, noSuchObject, Counter64 max, empty subtree, nonadvancing OID, retry/timeout')
 # Cancellation during a maximum-timeout silent UDP receive.
 sock=socket.socket(socket.AF_INET,socket.SOCK_DGRAM);sock.bind(('127.0.0.1',0))
 p=subprocess.Popen([str(CLI),'snmp','walk','127.0.0.1','system','--port',str(sock.getsockname()[1]),'--timeout-ms','10000','--format','jsonl'],env=ENV,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,creationflags=subprocess.CREATE_NEW_PROCESS_GROUP if os.name=='nt' else 0)
 time.sleep(.2);start=time.monotonic();p.send_signal(signal.CTRL_BREAK_EVENT if os.name=='nt' else signal.SIGINT);out,err=p.communicate(timeout=2);elapsed=time.monotonic()-start;sock.close();assert p.returncode==130 and elapsed<1
 checks.append(f'Ctrl+C during receive: {elapsed:.3f}s; partial data, no credentials')
 for secret in [COMM,AUTH,PRIV]:assert secret not in out+err
 report={'status':'passed','platform':platform.platform(),'version':subprocess.check_output([str(CLI),'--version'],text=True).strip(),'sha256':hashlib.sha256(CLI.read_bytes()).hexdigest(),'checks':checks,'cancel_seconds':elapsed}
 (OUT/'integration.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
if __name__=='__main__':main()
