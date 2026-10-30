"""Offline protocol/CLI validation against independent Python peers (no third-party packages)."""
import argparse
import contextlib
import hashlib
import json
import os
from pathlib import Path
import socket
import struct
import subprocess
import tempfile
import threading
import time
import traceback

ROOT = Path(__file__).resolve().parents[1]
args = argparse.ArgumentParser()
args.add_argument('--exe', default=str(ROOT/'target/debug'/('netbox-cli.exe' if os.name == 'nt' else 'netbox-cli')))
args.add_argument('--output', default=str(ROOT/'test-output/integration.json'))
opts = args.parse_args()
EXE = str(Path(opts.exe).resolve())
results = []
(ROOT/'test-output').mkdir(exist_ok=True)
work = Path(tempfile.mkdtemp(prefix='netbox-tests-', dir=ROOT/'test-output'))

def check(name, fn):
    started = time.monotonic()
    try:
        fn()
        result = {'name': name, 'status': 'passed'}
    except Exception as error:
        result = {'name': name, 'status': 'failed', 'error': str(error), 'traceback': traceback.format_exc()}
    result['duration_ms'] = round((time.monotonic()-started)*1000)
    results.append(result)
    print(json.dumps(result, ensure_ascii=False), flush=True)

def cli(*argv, code=0):
    p = subprocess.run([EXE, *map(str,argv), '--format', 'json'], capture_output=True, timeout=25)
    assert p.returncode == code, (argv, p.returncode, p.stdout.decode('utf-8','replace'),p.stderr.decode('utf-8','replace'))
    value = json.loads(p.stdout)
    assert value['schema_version'] == 1
    return value

def port(udp=False):
    with socket.socket(type=socket.SOCK_DGRAM if udp else socket.SOCK_STREAM) as s:
        s.bind(('127.0.0.1',0))
        return s.getsockname()[1]

@contextlib.contextmanager
def service(*argv, duration=3):
    log = work/f'service-{time.time_ns()}.jsonl'
    with log.open('wb') as output:
        proc = subprocess.Popen([EXE,*map(str,argv),'--duration',str(duration),'--format','jsonl'],stdout=output,stderr=subprocess.PIPE)
        try:
            deadline=time.monotonic()+5
            while time.monotonic()<deadline:
                if '"event_type":"ready"' in log.read_text(encoding='utf-8'):
                    break
                assert proc.poll() is None, log.read_text(encoding='utf-8')
                time.sleep(.05)
            else:
                raise AssertionError('Service never became ready')
            yield proc,log
            proc.wait(timeout=duration+8)
            assert proc.returncode == 0, log.read_text(encoding='utf-8')
        finally:
            if proc.poll() is None: proc.terminate(); proc.wait(timeout=5)

def thread(fn):
    failures=[]
    def run():
        try: fn()
        except BaseException as e: failures.append(e)
    t=threading.Thread(target=run,daemon=True);t.start()
    return t, failures

def simple():
    assert cli('doctor')['data']['gui_required'] is False
    assert cli('ping','127.0.0.1','-c',2,'--interval-ms',50)['data']['received']==2
    assert cli('ping','::1','-6','-c',1)['data']['received']==1
    assert cli('trace','127.0.0.1','--max-hops',2)['data']['reached']
    assert cli('dns','localhost')['data']['source']=='system'

def tcp():
    with socket.socket() as server:
        server.bind(('127.0.0.1',0));server.listen()
        p=server.getsockname()[1]
        def accept():
            for _ in range(2):
                conn,_=server.accept();conn.close()
        t,errors=thread(accept)
        assert cli('tcping','127.0.0.1',p,'-c',2,'--interval-ms',50)['data']['received']==2
        t.join(2);assert not errors
    value=cli('tcping','127.0.0.1',p,'-c',1,'--timeout-ms',5000,code=1)
    assert value['data']['samples'][0]['status']=='refused'

def validation():
    assert cli('ping','127.0.0.1','--timeout-ms',0,code=2)['error']['code']=='INVALID_ARGUMENT'
    assert cli('dns','localhost','AAAA',code=2)['error']['code']=='INVALID_ARGUMENT'
    assert cli('ping','127.0.0.1','--nonexistent',code=2)['status']=='failed'
    assert cli('syslog','listen','--listen','0.0.0.0','--allow','invalid-cidr','--dir',work/'denied',code=2)['error']['code']=='INVALID_ARGUMENT'
    assert cli('ping','127.0.0.1','-t',code=2)['error']['code']=='INVALID_ARGUMENT'

def dns(tcp_fallback=False,nxdomain=False):
    with socket.socket(type=socket.SOCK_DGRAM) as udp:
        udp.bind(('127.0.0.1',0));p=udp.getsockname()[1];udp.settimeout(5)
        tcp_server=socket.socket();tcp_server.bind(('127.0.0.1',p));tcp_server.listen();tcp_server.settimeout(5)
        def response(q):
            flag=0x8183 if nxdomain else 0x8180
            header=q[:2]+struct.pack('!HHHHH',flag,1,0 if nxdomain else 1,0,0)
            return header+q[12:]+(b'' if nxdomain else b'\xc0\x0c'+struct.pack('!HHIH',1,1,60,4)+bytes([192,0,2,10]))
        def responder():
            q,peer=udp.recvfrom(4096)
            if tcp_fallback:
                udp.sendto(q[:2]+struct.pack('!HHHHH',0x8380,1,0,0,0)+q[12:],peer)
                conn,_=tcp_server.accept()
                with conn:
                    n=struct.unpack('!H',conn.recv(2))[0];q=b''
                    while len(q)<n:q+=conn.recv(n-len(q))
                    body=response(q);conn.sendall(struct.pack('!H',len(body))+body)
            else: udp.sendto(response(q),peer)
        t,errors=thread(responder)
        try:
            value=cli('dns','example.test','A','-s',f'127.0.0.1:{p}',code=1 if nxdomain else 0)['data']
            assert value['rcode']==(3 if nxdomain else 0)
            if not nxdomain: assert value['records'][0]['value']=='192.0.2.10'
            assert value['transport']==('tcp' if tcp_fallback else 'udp')
            t.join(2);assert not errors,errors
        finally: tcp_server.close()

def ntp(kod=False):
    with socket.socket(type=socket.SOCK_DGRAM) as s:
        s.bind(('127.0.0.1',0));p=s.getsockname()[1];s.settimeout(5)
        def responder():
            q,peer=s.recvfrom(512);b=bytearray(48);b[0]=0x24;b[1]=0 if kod else 2;b[24:32]=q[40:48]
            if kod:b[12:16]=b'RATE'
            stamp=struct.unpack('!Q',q[40:48])[0]+round(.05*2**32)
            b[32:40]=struct.pack('!Q',stamp);b[40:48]=struct.pack('!Q',stamp);s.sendto(b,peer)
        t,errors=thread(responder)
        value=cli('ntp','127.0.0.1','--port',p,'-c',1,code=1 if kod else 0)
        if kod:assert value['error']['code']=='NTP_KOD'
        else:assert value['data']['valid_samples']==1 and 30<value['data']['best_sample']['offset_ms']<70
        t.join(2);assert not errors

def ntp_server():
    p=port(True)
    with service('ntp','serve','--listen','127.0.0.1','--port',p,'--stratum',5,duration=2) as (_,log):
        with socket.socket(type=socket.SOCK_DGRAM) as s:
            s.settimeout(.3)
            for version in [3,4]:
                q=bytearray(48);q[0]=(version<<3)|3;q[2]=6
                q[40:48]=struct.pack('!Q',round((time.time()+2208988800)*2**32))
                before=time.time();s.sendto(q,('127.0.0.1',p));b,peer=s.recvfrom(512);after=time.time()
                assert len(b)==48 and peer[1]==p and b[0]==(version<<3)|4 and b[1]==5
                assert b[24:32]==q[40:48]
                receive=struct.unpack('!Q',b[32:40])[0]/2**32-2208988800
                transmit=struct.unpack('!Q',b[40:48])[0]/2**32-2208988800
                assert before-.001<=receive<=transmit<=after+.001
            s.sendto(bytes([0x26])+bytes(47),('127.0.0.1',p))
            try:s.recvfrom(512);raise AssertionError('Answered a control packet')
            except socket.timeout:pass
        assert cli('ntp','127.0.0.1','--port',p,'-c',1)['data']['valid_samples']==1
    events=[json.loads(line) for line in log.read_text(encoding='utf-8').splitlines()]
    assert events[-1]['event_type']=='finished'
    data=events[-1]['data']
    assert data['received']==4 and data['replied']==3 and data['invalid']==1
    assert data['stratum']==5 and data['clock_source']=='system' and data['clock_changed'] is False
    with socket.socket(type=socket.SOCK_DGRAM) as s:s.bind(('127.0.0.1',p))
    data=cli('ntp','serve','--listen','127.0.0.1','--port',p,'--duration',1)['data']
    assert data['success'] and data['replied']==0
    assert cli('ntp','serve','--format','json',code=2)['error']['code']=='INVALID_ARGUMENT'
    assert cli('ntp','serve','--stratum',16,code=2)['error']['code']=='INVALID_ARGUMENT'

def ntp_server_denied():
    p=port(True)
    with service('ntp','serve','--listen','127.0.0.1','--port',p,'--allow','192.0.2.0/24',duration=1) as (_,log):
        with socket.socket(type=socket.SOCK_DGRAM) as s:
            s.settimeout(.3);s.sendto(bytes([0x23])+bytes(47),('127.0.0.1',p))
            try:s.recvfrom(512);raise AssertionError('Answered a denied source')
            except socket.timeout:pass
    data=json.loads(log.read_text(encoding='utf-8').splitlines()[-1])['data']
    assert data['denied']==1 and data['replied']==0

def tftp_client(content,put=False):
    with socket.socket(type=socket.SOCK_DGRAM) as listener:
        listener.bind(('127.0.0.1',0));listener.settimeout(5);p=listener.getsockname()[1];received=[]
        def responder():
            q,peer=listener.recvfrom(2048);assert q[:2]==(b'\0\2' if put else b'\0\1')
            with socket.socket(type=socket.SOCK_DGRAM) as transfer:
                transfer.bind(('127.0.0.1',0));transfer.settimeout(5)
                if put:
                    transfer.sendto(b'\0\4\0\0',peer);expected=1
                    while True:
                        b,_=transfer.recvfrom(2048);assert b[:2]==b'\0\3' and struct.unpack('!H',b[2:4])[0]==expected
                        received.append(b[4:]);transfer.sendto(b'\0\4'+b[2:4],peer)
                        if len(b)<516:break
                        expected+=1
                else:
                    for offset in range(0,len(content)+1,512):
                        block=offset//512+1;transfer.sendto(b'\0\3'+struct.pack('!H',block)+content[offset:offset+512],peer)
                        ack,_=transfer.recvfrom(2048);assert ack==b'\0\4'+struct.pack('!H',block)
        t,errors=thread(responder)
        local=work/f'传输 {time.time_ns()}.bin'
        if put:
            local.write_bytes(content);value=cli('tftp','put','127.0.0.1',local,'--port',p,'--remote-name','test.bin')
        else:value=cli('tftp','get','127.0.0.1','test.bin','-o',local,'--port',p)
        t.join(5);assert not errors,errors;assert not t.is_alive()
        assert value['data']['bytes']==len(content)
        assert (b''.join(received) if put else local.read_bytes())==content

def tftp_server():
    root=work/'tftp-root';root.mkdir();content=b'config\r\n'*150;(root/'source.cfg').write_bytes(content);p=port(True)
    with service('tftp','serve','--root',root,'--port',p,'--write',duration=6):
        with socket.socket(type=socket.SOCK_DGRAM) as s:
            s.settimeout(4);s.sendto(b'\0\1source.cfg\0octet\0',('127.0.0.1',p));received=b''
            while True:
                b,peer=s.recvfrom(2048);assert b[:2]==b'\0\3';received+=b[4:];s.sendto(b'\0\4'+b[2:4],peer)
                if len(b)<516:break
            assert received==content
            s.sendto(b'\0\2new.cfg\0octet\0',('127.0.0.1',p));b,peer=s.recvfrom(2048);assert b==b'\0\4\0\0'
            s.sendto(b'\0\3\0\1new config',peer);b,_=s.recvfrom(2048);assert b==b'\0\4\0\1'
            assert(root/'new.cfg').read_bytes()==b'new config'
    assert(root/'source.cfg').read_bytes()==content

def tftp_rejects():
    root=work/'readonly';root.mkdir();(root/'keep.cfg').write_bytes(b'keep');p=port(True)
    with service('tftp','serve','--root',root,'--port',p,duration=2):
        with socket.socket(type=socket.SOCK_DGRAM) as s:
            s.settimeout(2)
            for request in [b'\0\2keep.cfg\0octet\0',b'\0\1../keep.cfg\0octet\0',b'\0\1CON\0octet\0']:
                s.sendto(request,('127.0.0.1',p));b,_=s.recvfrom(2048);assert b[:2]==b'\0\5'
    assert(root/'keep.cfg').read_bytes()==b'keep'
    dest=work/'exists.bin';dest.write_bytes(b'keep')
    assert cli('tftp','get','127.0.0.1','test','-o',dest,code=4)['error']['code']=='ALREADY_EXISTS'

def syslog():
    p=port(True);directory=work/'logs'
    with service('syslog','listen','--dir',directory,'--port',p,duration=3):
        with socket.socket(type=socket.SOCK_DGRAM) as s:
            s.sendto(b'<13>hello\xff\x00world',('127.0.0.1',p))
            s.sendto('<191>中文日志'.encode(),('127.0.0.1',p))
        cli('syslog','send','127.0.0.1','independent sender test','--port',p)
        time.sleep(.2)
        assert cli('syslog','listen','--dir',directory,'--port',port(True),'--duration',1,code=3)['error']['code']=='DIRECTORY_IN_USE'
    records=[json.loads(line) for f in(directory/'netbox-syslog').glob('*.jsonl')for line in f.read_text(encoding='utf-8').splitlines()]
    assert len(records)==3,len(records)
    assert bytes.fromhex(records[0]['raw_hex'])==b'<13>hello\xff\x00world'
    assert records[1]['facility']==23 and records[1]['severity']==7

check('Local doctor, IPv4/IPv6 ICMP, trace and system DNS',simple)
check('TCP connection and refused result/exit code',tcp)
check('Input validation and machine-readable CLI failures',validation)
check('DNS UDP record parsing with independent server',lambda:dns())
check('DNS TC flag -> TCP fallback',lambda:dns(tcp_fallback=True))
check('DNS NXDOMAIN and exit code',lambda:dns(nxdomain=True))
check('NTP offset with independent responder',lambda:ntp())
check('NTP RATE KoD handling',lambda:ntp(kod=True))
check('NTP server v3/v4 with independent client, statistics and finite JSON',ntp_server)
check('NTP server source CIDR filtering',ntp_server_denied)
for n in [0,512,513,4097]:
    check(f'TFTP download {n} bytes / exact-block boundary',lambda n=n:tftp_client(bytes(i%251 for i in range(n))))
check('TFTP upload to independent Python receiver',lambda:tftp_client(b'upload\0\xff'*150,put=True))
check('TFTP server with independent RRQ/WRQ client',tftp_server)
check('TFTP readonly, traversal and local no-clobber',tftp_rejects)
check('Syslog receive/send/raw bytes/exclusive directory',syslog)
report={'executable':EXE,'sha256':hashlib.sha256(Path(EXE).read_bytes()).hexdigest(),'timestamp':time.strftime('%Y-%m-%dT%H:%M:%S%z'),'results':results,'passed':sum(x['status']=='passed' for x in results),'total':len(results),'artifacts':str(work)}
Path(opts.output).parent.mkdir(parents=True,exist_ok=True)
Path(opts.output).write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf-8')
raise SystemExit(0 if report['passed']==report['total'] else 1)
