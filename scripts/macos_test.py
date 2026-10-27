"""macOS-specific acceptance checks for the packaged independent CLI (stdlib)."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import platform
import os
import signal
import socket
import subprocess
import tempfile
import time
import traceback

root = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser()
p.add_argument('--exe', required=True)
p.add_argument('--output', default=str(root/'test-output/macos.json'))
a = p.parse_args()
exe = str(Path(a.exe).resolve())
results = []
def check(name, fn):
    start = time.monotonic()
    try:
        detail = fn()
        r = dict(name=name, status='passed', detail=detail)
    except Exception as e:
        r = dict(name=name,status='failed',error=str(e),traceback=traceback.format_exc())
    r['duration_ms'] = round((time.monotonic()-start)*1000)
    results.append(r)
    print(json.dumps(r,ensure_ascii=False),flush=True)
def cli(*args):
    p = subprocess.run([exe,*map(str,args),'--format','json'],capture_output=True,timeout=15)
    assert p.returncode == 0, (p.returncode,p.stdout.decode(errors='replace'),p.stderr.decode(errors='replace'))
    return json.loads(p.stdout)['data']
def concurrent_icmp():
    def probe(i):
        v6 = i%2 == 1
        data = cli('ping','::1' if v6 else '127.0.0.1','-c',8,'--interval-ms',50,*(['-6'] if v6 else []))
        assert data['received']==8 and data['lost']==0, data
        return data['received']
    with ThreadPoolExecutor(max_workers=8) as pool:
        received = sum(pool.map(probe,range(8)))
    assert cli('trace','::1','-6','--max-hops',2)['reached']
    return dict(tasks=8,replies=received,ipv6_trace=True)
def ping_sizes():
    observations=[]
    for family,host,protocol in [(socket.AF_INET,'127.0.0.1',socket.IPPROTO_ICMP),(socket.AF_INET6,'::1',socket.IPPROTO_ICMPV6)]:
        # A separate native ICMP socket observes the actual echo replies, rather
        # than trusting only the CLI's reported request size.
        with socket.socket(family,socket.SOCK_DGRAM,protocol) as observer:
            observer.setsockopt(socket.SOL_SOCKET,socket.SO_RCVBUF,262144)
            observer.settimeout(1)
            for size in [0,1,32,1472,4096,65500]:
                data=cli('ping',host,'-c',1,'--size',size,*(['-6'] if family==socket.AF_INET6 else []))
                assert data['received']==1 and data['payload_size_bytes']==size,data
                assert data['samples'][0]['payload_size_bytes']==size,data
                while True:
                    packet,_=observer.recvfrom(65535)
                    body=packet[(packet[0]&15)*4:] if family==socket.AF_INET else packet
                    if body[0]==(0 if family==socket.AF_INET else 129):break
                assert len(body)==8+size,(host,size,len(body))
                observations.append(dict(host=host,payload_bytes=size,observed_icmp_bytes=len(body)))
    assert cli('ping','127.0.0.1','-c',1)['payload_size_bytes']==32
    for size in ['65501','-1','1.5']:
        proc=subprocess.run([exe,'ping','must-not-resolve.invalid','-c','1','--size',size,'--format','json'],capture_output=True,timeout=3)
        assert proc.returncode==2,(size,proc.returncode,proc.stdout,proc.stderr)
        if size=='65501':assert json.loads(proc.stdout)['error']['code']=='INVALID_ARGUMENT'
    return dict(observations=observations,default_bytes=32,invalid_sizes_rejected=True)
def cancel_probe():
    process = subprocess.Popen([exe,'ping','192.0.2.1','-t','--timeout-ms','10000','--format','jsonl'],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
    try:
        time.sleep(.25)
        start = time.monotonic(); process.send_signal(signal.SIGINT)
        out,err = process.communicate(timeout=2)
        elapsed = time.monotonic()-start
        assert process.returncode==130, (process.returncode,out,err)
        assert json.loads(out.splitlines()[-1])['status']=='cancelled'
        assert elapsed<1, elapsed
        return dict(stop_ms=round(elapsed*1000))
    finally:
        if process.poll() is None: process.kill(); process.wait()
def broken_pipe():
    process = subprocess.Popen([exe,'ping','127.0.0.1','-t','--format','jsonl'],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
    try:
        process.stdout.close();process.wait(timeout=3)
        assert process.returncode==4, process.returncode
        return dict(exit_code=4)
    finally:
        if process.poll() is None:process.kill();process.wait()
def services():
    with tempfile.TemporaryDirectory(prefix='netbox macOS 中文 ') as tmp:
        rootpath = Path(tmp); (rootpath/'源文件.cfg').write_bytes(b'netbox\0config\xff')
        def serve(args):
            proc = subprocess.Popen([exe,*map(str,args),'--duration','10','--format','jsonl'],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            line = proc.stdout.readline()
            assert json.loads(line)['event_type']=='ready', line
            return proc
        ports=[]
        for family,host in [(socket.AF_INET,'127.0.0.1'),(socket.AF_INET6,'::1')]:
            with socket.socket(family,socket.SOCK_DGRAM) as s:
                s.bind((host,0)); tftp_port=s.getsockname()[1]
            with socket.socket(family,socket.SOCK_DGRAM) as s:
                s.bind((host,0)); syslog_port=s.getsockname()[1]
            ports.append(dict(host=host,tftp=tftp_port,syslog=syslog_port))
            tftp = serve(['tftp','serve','--root',rootpath,'--listen',host,'--port',tftp_port])
            try:
                output=rootpath/f'download-{family}.cfg'
                data=cli('tftp','get',host,'源文件.cfg','-o',output,'--port',tftp_port)
                assert output.read_bytes()==(rootpath/'源文件.cfg').read_bytes()
                assert data['bytes']==len((rootpath/'源文件.cfg').read_bytes())
            finally:
                tftp.send_signal(signal.SIGINT);tftp.communicate(timeout=3)
            logs=rootpath/f'logs-{family}'
            syslog=serve(['syslog','listen','--dir',logs,'--listen',host,'--port',syslog_port])
            try:
                cli('syslog','send',host,'中文 IPv6 日志','--port',syslog_port)
                time.sleep(.15)
            finally:
                syslog.send_signal(signal.SIGINT);syslog.communicate(timeout=3)
            records=[json.loads(line) for f in (logs/'netbox-syslog').glob('*.jsonl') for line in f.read_text().splitlines()]
            assert len(records)==1 and '中文 IPv6 日志' in records[0]['text'],records
            with socket.socket(family,socket.SOCK_DGRAM) as s:s.bind((host,tftp_port))
            with socket.socket(family,socket.SOCK_DGRAM) as s:s.bind((host,syslog_port))
        return dict(ports=ports,ipv4=True,ipv6=True,unicode_paths=True)
def outward_services():
    with socket.socket(socket.AF_INET,socket.SOCK_DGRAM) as route:
        route.connect(('192.0.2.1',9))
        lan=route.getsockname()[0]
    assert not lan.startswith('127.'),lan
    observations=[]
    with tempfile.TemporaryDirectory(prefix='netbox-default-services-') as tmp:
        rootpath=Path(tmp);files=rootpath/'files';files.mkdir()
        content=b'netbox wildcard listener test\n'
        (files/'probe.cfg').write_bytes(content)
        for family,host,listen in [(socket.AF_INET,lan,None),(socket.AF_INET6,'::1','::')]:
            wildcard='0.0.0.0' if family==socket.AF_INET else '::'
            def free_port():
                with socket.socket(family,socket.SOCK_DGRAM) as s:
                    s.bind((wildcard,0));return s.getsockname()[1]
            tftp_port,syslog_port=free_port(),free_port()
            logs=rootpath/f'logs-{family}'
            processes=[]
            def start(args):
                proc=subprocess.Popen([exe,*map(str,args),*(['--listen',listen] if listen else []),'--duration','15','--format','jsonl'],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
                processes.append(proc)
                ready=json.loads(proc.stdout.readline())
                assert ready['event_type']=='ready',ready
                assert ready['listen']==(f'0.0.0.0:{args[-1]}' if family==socket.AF_INET else f'[::]:{args[-1]}'),ready
                return proc
            try:
                start(['tftp','serve','--root',files,'--port',tftp_port])
                start(['syslog','listen','--dir',logs,'--port',syslog_port])
                # Independent clients address the machine's LAN IP rather than
                # loopback for IPv4, without passing --listen or --allow.
                with socket.socket(family,socket.SOCK_DGRAM) as peer:
                    peer.bind((host,0));peer.settimeout(3)
                    peer.sendto(b'\0\1probe.cfg\0octet\0',(host,tftp_port))
                    packet,transfer=peer.recvfrom(1024)
                    assert packet[:4]==b'\0\3\0\1' and packet[4:]==content,packet
                    peer.sendto(b'\0\4\0\1',transfer)
                with socket.socket(family,socket.SOCK_DGRAM) as peer:
                    peer.bind((host,0));peer.sendto(b'<13>default outward listener',(host,syslog_port))
                time.sleep(.15)
            finally:
                for proc in processes:
                    if proc.poll() is None:proc.send_signal(signal.SIGINT)
                    proc.communicate(timeout=3)
            records=[json.loads(line) for f in (logs/'netbox-syslog').glob('*.jsonl') for line in f.read_text().splitlines()]
            assert len(records)==1 and 'default outward listener' in records[0]['text'],records
            for port in [tftp_port,syslog_port]:
                with socket.socket(family,socket.SOCK_DGRAM) as s:s.bind((wildcard,port))
            observations.append(dict(listen=wildcard,client_address=host,tftp_bytes=len(content),syslog_records=1,ports_released=True))
    return dict(observations=observations,ipv4_default_without_flags=True,ipv6_explicit_wildcard=True,scope='Same-host non-loopback IPv4 access; no separate physical device used')
def standard_ports():
    observations=[]
    with tempfile.TemporaryDirectory(prefix='netbox-standard-ports-') as tmp:
        for command,port in [('tftp',69),('syslog',514)]:
            operation=['tftp','serve','--root',tmp] if command=='tftp' else ['syslog','listen','--dir',tmp]
            proc=subprocess.run([exe,*operation,'--port',str(port),'--duration','1','--format','json'],capture_output=True,timeout=4)
            value=json.loads(proc.stdout)
            if proc.returncode==0:
                observations.append(dict(port=port,result='available',uid=os.geteuid()))
            else:
                assert proc.returncode==3 and value['error']['code'] in ['PERMISSION_DENIED','ADDRESS_IN_USE'],value
                observations.append(dict(port=port,result=value['error']['code'],uid=os.geteuid()))
    return dict(observations=observations,meaning='Capability observation; denied/occupied ports are not supported under this identity')

def metadata():
    data=cli('doctor')
    assert data['platform']=='macos' and data['icmp_adapter']=='darwin-datagram',data
    p=subprocess.run(['otool','-L',exe],capture_output=True,text=True,check=True)
    dylibs='\n'.join(p.stdout.splitlines()[1:])
    assert '/opt/homebrew/' not in dylibs and '/Users/' not in dylibs,dylibs
    subprocess.run(['codesign','--verify','--strict',exe],check=True,capture_output=True)
    return dict(doctor=data,dylibs=p.stdout.splitlines()[1:])
check('macOS metadata, independent executable and ad-hoc signature',metadata)
check('8 concurrent IPv4/IPv6 ICMP tasks and IPv6 trace',concurrent_icmp)
check('Ping payload sizes 0..65500 with independent IPv4/IPv6 wire observation',ping_sizes)
check('Ctrl+C interrupts a 10 second ICMP receive budget',cancel_probe)
check('Closed output pipe stops a continuous task',broken_pipe)
check('High-port TFTP/Syslog IPv4/IPv6 and Chinese paths',services)
check('Default outward TFTP/Syslog via LAN IP and explicit IPv6 wildcard',outward_services)
check('Observe actual standard UDP 69/514 access without elevation',standard_ports)
report=dict(executable=exe,sha256=hashlib.sha256(Path(exe).read_bytes()).hexdigest(),system=platform.platform(),machine=platform.machine(),timestamp=time.strftime('%Y-%m-%dT%H:%M:%S%z'),results=results,passed=sum(r['status']=='passed' for r in results),total=len(results))
Path(a.output).parent.mkdir(parents=True,exist_ok=True)
Path(a.output).write_text(json.dumps(report,ensure_ascii=False,indent=2))
raise SystemExit(0 if report['passed']==report['total'] else 1)
