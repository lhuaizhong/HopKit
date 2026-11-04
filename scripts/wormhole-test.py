"""Local-only protocol acceptance. Requires test venv with the three official packages.
python scripts/wormhole-test.py --python-bin /tmp/hopkit-wormhole-test-env/bin
"""
import argparse, hashlib, json, os, queue, re, signal, socket, subprocess, tempfile, threading, time, platform
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--python-bin',type=Path,required=True);p.add_argument('--large',action='store_true');p.add_argument('--exe',type=Path);a=p.parse_args()
root=Path(__file__).resolve().parents[1];out=root/'test-output/wormhole';out.mkdir(parents=True,exist_ok=True)
cli=(a.exe or root/('target/debug/hopkit-cli'+('.exe' if os.name=='nt' else ''))).resolve()
def peer(name):return a.python_bin/(name+('.exe' if os.name=='nt' else ''))
def interrupt(proc):proc.send_signal(signal.CTRL_BREAK_EVENT if os.name=='nt' else signal.SIGINT)
reports=[];processes=[]
def port():
 with socket.socket() as s:s.bind(('127.0.0.1',0));return s.getsockname()[1]
def start(args,**kw):
 proc=subprocess.Popen([str(x) for x in args],creationflags=subprocess.CREATE_NEW_PROCESS_GROUP if os.name=='nt' else 0,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,**kw);processes.append(proc);return proc
def finish(proc,timeout=45):
 stdout,stderr=proc.communicate(timeout=timeout);return proc.returncode,stdout,stderr
def digest(path):
 h=hashlib.sha256()
 with path.open('rb') as f:
  for b in iter(lambda:f.read(1024*1024),b''):h.update(b)
 return h.hexdigest()
def code(proc):
 deadline=time.monotonic()+20;q=queue.Queue();seen=[]
 def read():
  for line in proc.stdout:q.put(line)
 threading.Thread(target=read,daemon=True).start()
 while time.monotonic()<deadline:
  try:line=q.get(timeout=.2)
  except queue.Empty:
   if proc.poll() is not None:raise AssertionError((proc.returncode,seen,proc.stderr.read()))
   continue
  seen.append(line);value=json.loads(line)
  if value.get('event_type')=='wormhole_code':return value['code']
 raise AssertionError('code not generated')
def check(name,body):
 started=time.monotonic();body();reports.append({'case':name,'status':'passed','seconds':round(time.monotonic()-started,3)});print(name,'passed',flush=True)
with tempfile.TemporaryDirectory(prefix='hopkit-wormhole-') as temp:
 t=Path(temp);m,r=port(),port();mailbox=f'ws://127.0.0.1:{m}/v1';relay=f'tcp://127.0.0.1:{r}'
 service_logs=[]
 def service_start(args):
  log=(t/f'service-{len(service_logs)}.log').open('w+',encoding='utf-8');service_logs.append(log)
  proc=subprocess.Popen([str(x) for x in args],cwd=t,stdout=log,stderr=subprocess.STDOUT);processes.append(proc);return proc
 def service_command(name):
  return [peer('twistd'),'--nodaemon',*(['--pidfile',t/f'{name}.pid'] if os.name!='nt' else [])]
 services=[service_start([*service_command('mail'),'wormhole-mailbox','--port',f'tcp:{m}:interface=127.0.0.1','--channel-db',t/'channel.sqlite']),service_start([*service_command('relay'),'transitrelay','--port',f'tcp:{r}:interface=127.0.0.1'])]
 options=['--mailbox-url',mailbox,'--transit-relay',relay,'--connection','relay']
 python=[peer('wormhole'),'--relay-url',mailbox,'--transit-helper',f'tcp:127.0.0.1:{r}']
 try:
  # Python/Twisted cold startup can exceed two seconds on Windows runners.
  for service,service_port in zip(services,[m,r]):
   deadline=time.monotonic()+30
   while True:
    assert service.poll() is None, 'Local service exited; see service logs'
    try:
     with socket.create_connection(('127.0.0.1',service_port),timeout=.2):pass
     break
    except OSError:
     if time.monotonic()>=deadline:
      for log in service_logs:
       log.flush();log.seek(0);print(log.read(),flush=True)
      raise AssertionError(f'Local service not ready on {service_port}')
     time.sleep(.1)
  def hop_pair(size,name):
   source=t/name
   with source.open('wb') as f:
    if size>2**32:f.truncate(size)
    else:f.write(os.urandom(size))
   dest=t/('recv-'+str(size));dest.mkdir(exist_ok=True)
   sender=start([cli,'wormhole','send',source,*options,'--format','jsonl','--emit-code']);pair=code(sender)
   receiver=start([cli,'wormhole','receive',pair,'--dir',dest,'--yes',*options,'--format','json'])
   peak={}
   def memory():
    while receiver.poll() is None:
     if os.name=='posix':
      result=subprocess.run(['ps','-o','pid=,rss=','-p',str(sender.pid)+','+str(receiver.pid)],capture_output=True,text=True)
      for line in result.stdout.splitlines():
       pid,rss=map(int,line.split());peak[pid]=max(peak.get(pid,0),rss)
     time.sleep(.1)
   monitor=threading.Thread(target=memory,daemon=True) if size>2**32 else None
   if monitor:monitor.start()
   rc,stdout,stderr=finish(receiver,600 if a.large else 45);assert rc==0,(rc,stdout,stderr)
   if monitor:
    monitor.join(timeout=2)
    (out/'large-memory.json').write_text(json.dumps({'bytes':size,'sampled_peak_rss_kib':{'sender':peak.get(sender.pid),'receiver':peak.get(receiver.pid)},'method':'ps RSS sampled every 100ms; includes process resident memory'},indent=2))
   value=json.loads(stdout);assert value['data']['local_saved'];assert value['data']['connection']=='relay';assert pair not in stdout
   sent=sender.wait(timeout=45);assert sent==0,sender.stderr.read()
   assert digest(source)==digest(dest/name)==value['data']['sha256'];assert not list(dest.glob('*.part'))
   assert value['data']['bytes']==str(size)
  for size,name in [(0,'空 文件.txt'),(1024,'中文 file.bin'),(8*1024*1024,'stream.bin')]:check(f'hopkit pair {size}',lambda size=size,name=name:hop_pair(size,name))
  if a.large:check('actual >4 GiB transfer',lambda:hop_pair(2**32+1024,'large.bin'))
  def hop_python():
   source=t/'to-python.txt';source.write_bytes(b'hopkit to python\n');dest=t/'python-received.txt'
   sender=start([cli,'wormhole','send',source,*options,'--format','jsonl','--emit-code']);pair=code(sender)
   receiver=start([*python,'receive','--accept-file','--no-listen','--output-file',dest,pair]);rc,stdout,stderr=finish(receiver);assert rc==0,(stdout,stderr)
   assert sender.wait(timeout=45)==0;assert dest.read_bytes()==source.read_bytes()
  check('HopKit to Python 0.23.0',hop_python)
  def python_hop(words,directory=False):
   source=t/f'python-{words}-{directory}';source.mkdir(exist_ok=True) if directory else source.write_bytes(b'python to hopkit')
   if directory:(source/'inside.txt').write_text('archive')
   # A deterministic test code is supplied only to the independent Python peer.
   pair='42-'+('alpha-bravo' if words==2 else 'alpha-bravo-charlie-delta')
   sender=start([*python,'send','--code',pair,'--no-listen','--hide-progress',source]);time.sleep(.5)
   dest=t/f'python-recv-{words}-{directory}';dest.mkdir(exist_ok=True)
   receiver=start([cli,'wormhole','receive',pair,'--dir',dest,'--yes',*options,'--format','json']);rc,stdout,stderr=finish(receiver);assert rc==0,(stdout,stderr)
   assert sender.wait(timeout=30)==0,sender.stderr.read()
   saved=Path(json.loads(stdout)['data']['save_path']);assert saved.exists()
   if directory:
    import zipfile
    assert zipfile.is_zipfile(saved)
   else:assert saved.read_bytes()==source.read_bytes()
  for words in [2,4]:check(f'Python {words}-word to HopKit',lambda words=words:python_hop(words))
  check('Python directory saved as ZIP',lambda:python_hop(2,True))
  def cancellation():
   sender=start([cli,'wormhole','send',t/'to-python.txt',*options,'--format','jsonl','--emit-code']);code(sender);started=time.monotonic();interrupt(sender);assert sender.wait(timeout=3)==130;assert time.monotonic()-started<2
  check('waiting-peer cancellation <2s',cancellation)
  def conflict():
   dest=t/'conflict';dest.mkdir();(dest/'to-python.txt').write_bytes(b'original')
   sender=start([cli,'wormhole','send',t/'to-python.txt',*options,'--format','jsonl','--emit-code']);pair=code(sender)
   receiver=start([cli,'wormhole','receive',pair,'--dir',dest,'--yes',*options,'--format','json']);rc,stdout,stderr=finish(receiver);assert rc==4,(rc,stdout);assert (dest/'to-python.txt').read_bytes()==b'original';interrupt(sender);sender.wait(timeout=3)
  check('existing target preserved',conflict)
  def midstream(cancelled=False):
   source=t/'fault.bin';source.write_bytes(b'x'*(32*1024*1024));dest=t/('midcancel' if cancelled else 'commit-race');dest.mkdir();target=dest/source.name
   sender=start([cli,'wormhole','send',source,*options,'--format','jsonl','--emit-code']);pair=code(sender)
   receiver=start([cli,'wormhole','receive',pair,'--dir',dest,'--yes',*options,'--format','json'])
   until=time.monotonic()+15
   while not list(dest.glob('*.part')):
    assert time.monotonic()<until,'No part file';time.sleep(.002)
   if cancelled:
    interrupt(receiver)
   else:target.write_bytes(b'race-original')
   rc,stdout,stderr=finish(receiver);assert rc!=0,(stdout,stderr)
   assert sender.wait(timeout=10)!=0,'Sender incorrectly succeeded after failed commit/cancel'
   assert not list(dest.glob('*.part')),'Partial file was left behind'
   if cancelled:assert not target.exists()
   else:assert target.read_bytes()==b'race-original'
  check('commit race prevents sender success',midstream)
  check('midstream cancellation removes temporary file',lambda:midstream(True))
 finally:
  for proc in processes:
   if proc.poll() is None:proc.terminate()
  for proc in processes:
   try:proc.wait(timeout=3)
   except subprocess.TimeoutExpired:proc.kill()
  for i,log in enumerate(service_logs):
   log.flush();log.seek(0);content=log.read();print(content,flush=True);(out/f'service-{i}.log').write_text(content,encoding='utf-8');log.close()
  (out/('large-protocol.json' if a.large else 'local-protocol.json')).write_text(json.dumps({'mailbox':'magic-wormhole-mailbox-server 0.8.0','relay':'magic-wormhole-transit-relay 0.5.0','python':'magic-wormhole 0.23.0','platform':platform.platform(),'machine':platform.machine(),'binary_sha256':digest(cli),'results':reports},indent=2))
