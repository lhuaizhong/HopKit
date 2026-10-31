#!/usr/bin/env python3
"""Stage 3 independent HTTP/TLS peers. No public HTTP requests required."""
import datetime, hashlib, http.server, json, os, platform, signal, socket, socketserver, ssl, subprocess, tempfile, threading, time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
CLI=ROOT/'target/debug/hopkit-cli'
OUT=ROOT/'test-output/check'
OUT.mkdir(parents=True,exist_ok=True)
checks=[]
requests=[]
slow=threading.Event()
class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self,*args): pass
    def do_HEAD(self):
        requests.append({'path':self.path,'source':self.client_address[0],'headers':dict(self.headers)})
        if self.path.startswith('/slow'): slow.set();time.sleep(3)
        code=403 if self.path.startswith('/forbidden') else 500 if self.path=='/error' else 302 if self.path.startswith(('/loop','/redirect','/credentials','/cross')) else 200
        try:
            self.send_response(code)
            if self.path.startswith('/loop'): self.send_header('Location','/loop')
            if self.path.startswith('/redirect'): self.send_header('Location','/ok?token=redirect-secret')
            if self.path.startswith('/credentials'): self.send_header('Location','http://user:credential-secret@127.0.0.1/')
            if self.path.startswith('/cross'): self.send_header('Location',f'http://localhost:{self.server.server_port}/ok')
            if self.path=='/large': self.send_header('X-Large','x'*70000)
            self.send_header('Content-Length','999999999') # HEAD must never read this body.
            self.end_headers()
        except (BrokenPipeError,ConnectionResetError): pass
class Server(http.server.ThreadingHTTPServer):
    daemon_threads=True

def serve(context=None):
    s=Server(('127.0.0.1',0),Handler)
    if context:s.socket=context.wrap_socket(s.socket,server_side=True)
    threading.Thread(target=s.serve_forever,daemon=True).start()
    return s

def run(target,*args,env=None):
    p=subprocess.run([str(CLI),'check',target,'--timeout-ms','700','--format','json',*args],capture_output=True,text=True,timeout=25,env=env)
    assert p.returncode==0,(p.returncode,p.stdout,p.stderr)
    v=json.loads(p.stdout)
    assert v['status']=='succeeded',v
    assert 'request-secret' not in p.stdout and 'redirect-secret' not in p.stdout and 'credential-secret' not in p.stdout
    return v

def byid(v,id): return next(e for e in v['data']['evidence'] if e['id']==id)
def record(name,v=None):
    checks.append({'test':name,'passed':True})
    if v:(OUT/(name+'.json')).write_text(json.dumps(v,ensure_ascii=False,indent=2))

with tempfile.TemporaryDirectory(prefix='hopkit-check-') as temp:
    temp=Path(temp)
    def openssl(*args):subprocess.run(['openssl',*map(str,args)],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    ca=temp/'ca.pem';key=temp/'ca.key'
    openssl('req','-x509','-newkey','rsa:2048','-nodes','-keyout',key,'-out',ca,'-days','2','-subj','/CN=HopKit Test CA')
    config=temp/'ca.cnf';(temp/'index').touch();(temp/'serial').write_text('01\n');(temp/'new').mkdir()
    config.write_text(f'[ca]\ndefault_ca=test\n[test]\ndatabase={temp}/index\nserial={temp}/serial\nnew_certs_dir={temp}/new\ncertificate={ca}\nprivate_key={key}\ndefault_md=sha256\npolicy=policy\nunique_subject=no\nx509_extensions=ext\n[policy]\ncommonName=supplied\n[ext]\nsubjectAltName=DNS:localhost,IP:127.0.0.1\nbasicConstraints=CA:FALSE\nkeyUsage=digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n')
    contexts={}
    for kind in ('valid','expired','wrongname'):
        k=temp/(kind+'.key');csr=temp/(kind+'.csr');cert=temp/(kind+'.pem')
        openssl('req','-new','-newkey','rsa:2048','-nodes','-keyout',k,'-out',csr,'-subj','/CN=localhost')
        begin='20200101000000Z' if kind=='expired' else (datetime.datetime.now(datetime.timezone.utc)-datetime.timedelta(days=1)).strftime('%Y%m%d%H%M%SZ')
        end='20200102000000Z' if kind=='expired' else (datetime.datetime.now(datetime.timezone.utc)+datetime.timedelta(days=1)).strftime('%Y%m%d%H%M%SZ')
        active_config=temp/(kind+'.cnf');active_config.write_text(config.read_text().replace('subjectAltName=DNS:localhost,IP:127.0.0.1','subjectAltName=DNS:wrong.invalid') if kind=='wrongname' else config.read_text())
        openssl('ca','-batch','-config',active_config,'-in',csr,'-out',cert,'-startdate',begin,'-enddate',end)
        ctx=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER);ctx.load_cert_chain(cert,k);contexts[kind]=ctx
    servers=[serve(),serve(contexts['valid']),serve(contexts['expired']),serve(contexts['wrongname'])]
    plain,valid,expired,wrongname=servers
    base=f'http://127.0.0.1:{plain.server_port}'
    ca_args=['--ca-file',str(ca)]
    try:
        isolated={**os.environ,'PATH':str(temp/'missing-tools'),'HTTP_PROXY':'http://127.0.0.1:1','HTTPS_PROXY':'http://127.0.0.1:1','ALL_PROXY':'http://127.0.0.1:1','SSL_CERT_FILE':str(temp/'missing.pem'),'CURL_CA_BUNDLE':str(temp/'missing.pem')}
        v=run(f'https://127.0.0.1:{valid.server_port}/ok',*ca_args,env=isolated);assert byid(v,'http_0')['data']['status_code']==200;record('independent-runtime-no-path-tools',v)
        v=run(base+'/ok?token=request-secret','--source','127.0.0.1');assert byid(v,'http_0')['data']['status_code']==200
        assert requests[-1]['path']=='/ok?token=request-secret' and requests[-1]['source']=='127.0.0.1'
        assert byid(v,'tcp')['data']['local_address'].startswith('127.0.0.1:') and byid(v,'icmp')['status']=='skipped'
        record('source-target-secrets',v)
        for path,status,rule in [('/forbidden',403,'http_forbidden'),('/error',500,'http_error')]:
            v=run(base+path);assert byid(v,'http_0')['data']['status_code']==status;assert any(f['rule']==rule for f in v['data']['findings']);record('http-'+str(status),v)
        v=run(base+'/redirect');assert byid(v,'http_1')['data']['status_code']==200;record('redirect',v)
        v=run(base+'/loop');assert byid(v,'redirect')['error']['code']=='REDIRECT_LIMIT';record('redirect-loop',v)
        v=run(base+'/credentials');assert byid(v,'redirect')['error']['code']=='INVALID_REDIRECT';record('redirect-credentials',v)
        v=run(base+'/cross');assert byid(v,'http_1')['data']['status_code']==200;assert not any(k.lower() in ('authorization','cookie','referer') for k in requests[-1]['headers']);record('cross-origin-no-credentials',v)
        v=run(base+'/large');assert byid(v,'http_0')['error']['code']=='RESOURCE_EXHAUSTED';record('bounded-headers',v)
        v=run(base+'/slow');assert byid(v,'http_0')['stage']=='http' and byid(v,'http_0')['error']['code']=='TIMEOUT';record('http-response-timeout',v)
        v=run(f'https://127.0.0.1:{valid.server_port}/ok',*ca_args);assert byid(v,'tls_0')['status']=='observed' and 'passed' in byid(v,'tls_0')['data']['certificate_validation'];record('tls-valid',v)
        v=run(f'https://127.0.0.1:{valid.server_port}/ok');assert byid(v,'tls_0')['error']['code']=='TLS_CERTIFICATE_ERROR';record('tls-untrusted',v)
        v=run(f'https://127.0.0.1:{expired.server_port}/ok',*ca_args);assert byid(v,'tls_0')['error']['code']=='TLS_CERTIFICATE_ERROR';record('tls-expired',v)
        v=run(f'https://127.0.0.1:{wrongname.server_port}/ok',*ca_args);assert byid(v,'tls_0')['error']['code']=='TLS_CERTIFICATE_ERROR';record('tls-hostname-mismatch',v)
        v=run(f'https://127.0.0.1:{valid.server_port}/ok','--ca-file',str(temp/'missing.pem'));assert byid(v,'tls_0')['error']['code']=='TLS_CA_ERROR';record('tls-ca-file-failure',v)
        v=run(f'https://127.0.0.1:{valid.server_port}/ok','--insecure-tls');assert byid(v,'tls_0')['data']['certificate_validation'].startswith('SKIPPED');record('tls-insecure-marked',v)
        v=run(f'https://127.0.0.1:{plain.server_port}/ok');assert byid(v,'tls_0')['stage']=='tls' and byid(v,'tls_0')['status']=='failed';record('tls-handshake-failure',v)
        class Stall(socketserver.BaseRequestHandler):
            def handle(self): time.sleep(2)
        class StallServer(socketserver.ThreadingTCPServer): daemon_threads=True
        stall=StallServer(('127.0.0.1',0),Stall);threading.Thread(target=stall.serve_forever,daemon=True).start()
        try:
            v=run(f'https://127.0.0.1:{stall.server_address[1]}/');assert byid(v,'tls_0')['error']['code']=='TIMEOUT';record('tls-handshake-timeout',v)
        finally:stall.shutdown();stall.server_close()
        # Fill an unaccepted loopback listen backlog; subsequent SYNs time out.
        backlog=socket.socket();backlog.bind(('127.0.0.1',0));backlog.listen(1);clients=[]
        try:
            for _ in range(32):
                peer=socket.socket();peer.settimeout(.05);clients.append(peer)
                try:peer.connect(backlog.getsockname())
                except TimeoutError:break
            v=run(f'127.0.0.1:{backlog.getsockname()[1]}');assert byid(v,'tcp')['error']['code']=='TIMEOUT';record('tcp-connect-timeout',v)
        finally:
            for peer in clients:peer.close()
            backlog.close()
        class DNS(socketserver.BaseRequestHandler):
            def handle(self):
                packet,sock=self.request
                # Echo question, one A answer; deliberately differs from localhost.
                response=packet[:2]+b'\x81\x80\x00\x01\x00\x01\x00\x00\x00\x00'+packet[12:]+b'\xc0\x0c\x00\x01\x00\x01\x00\x00\x00\x3c\x00\x04\xc0\x00\x02\x01'
                sock.sendto(response,self.client_address)
        dns=socketserver.UDPServer(('127.0.0.1',0),DNS);threading.Thread(target=dns.serve_forever,daemon=True).start()
        try:
            v=run('localhost','--dns-server',f'127.0.0.1:{dns.server_address[1]}');assert any(f['rule']=='dns_difference' for f in v['data']['findings']);record('direct-dns-comparison',v)
        finally:dns.shutdown();dns.server_close()
        class V6Server(Server):address_family=socket.AF_INET6
        v6=V6Server(('::1',0),Handler);threading.Thread(target=v6.serve_forever,daemon=True).start()
        try:
            v=run(f'http://[::1]:{v6.server_port}/ok','--source','::1');assert byid(v,'http_0')['data']['local_address']=='::1';record('ipv6-http-source',v)
        finally:v6.shutdown();v6.server_close()
        closed=socket.socket();closed.bind(('127.0.0.1',0));port=closed.getsockname()[1];closed.close()
        v=run(f'127.0.0.1:{port}');assert byid(v,'tcp')['error']['code']=='CONNECTION_REFUSED';record('tcp-refused',v)
        v=run('hopkit-does-not-exist.invalid');assert byid(v,'dns')['status']=='failed' and byid(v,'tcp')['status']=='skipped';record('system-nxdomain',v)
        v=run('localhost');assert byid(v,'http')['status']=='skipped' and byid(v,'tcp')['status']=='skipped';record('hostname-not-assumed-http',v)
        slow.clear()
        p=subprocess.Popen([str(CLI),'check',base+'/slow','--timeout-ms','10000','--format','jsonl'],stdout=subprocess.PIPE,text=True)
        lines=[]
        def collect():
            for line in p.stdout: lines.append(json.loads(line))
        worker=threading.Thread(target=collect);worker.start();assert slow.wait(15)
        began=time.monotonic();p.send_signal(signal.SIGINT);assert p.wait(timeout=2)==130;worker.join(timeout=2)
        assert time.monotonic()-began<2 and lines[-1]['status']=='cancelled' and lines[-1]['data']['evidence']
        record('cancel-http-retains-evidence',lines[-1])
    finally:
        for s in servers:s.shutdown();s.server_close()
report={'platform':platform.platform(),'cli_sha256':hashlib.sha256(CLI.read_bytes()).hexdigest(),'checks':checks}
(OUT/'acceptance.json').write_text(json.dumps(report,ensure_ascii=False,indent=2));print(json.dumps(report,ensure_ascii=False,indent=2))
