"""Bounded SNMPv3 IF-MIB observer against a standalone Net-SNMP agent. Default 2h."""
import argparse,os,subprocess,tempfile,time,signal,json,hashlib,platform
from pathlib import Path
from importlib.machinery import SourceFileLoader
m=SourceFileLoader('m6test',str(Path(__file__).with_name('m6-integration-test.py'))).load_module()
p=argparse.ArgumentParser();p.add_argument('--duration',type=int,default=7200);a=p.parse_args();assert a.duration>=10
out=m.OUT;cli=m.CLI;measure=[]
with tempfile.TemporaryDirectory(prefix='hopkit-m6-soak-') as tmp:
 endpoint=m.port();cfg=Path(tmp)/'agent.conf';state=Path(tmp)/'state';state.mkdir();cfg.write_text(f'agentaddress udp:127.0.0.1:{endpoint}\ncreateUser observer SHA-256 {m.AUTH} AES {m.PRIV}\nrouser observer priv\nsysName HopKit-M6-soak\n')
 with open(out/'soak-agent.log','w') as agentlog,open(out/'soak.jsonl','w') as log:
  agent=subprocess.Popen([os.environ.get('HOPKIT_TEST_SNMPD','/opt/homebrew/opt/net-snmp/sbin/snmpd'),'-f','-Lo','-C','-c',str(cfg)],env={**os.environ,'SNMP_PERSISTENT_DIR':str(state)},stdout=agentlog,stderr=agentlog)
  child=None
  try:
   time.sleep(.5);assert agent.poll() is None
   child=subprocess.Popen([str(cli),'snmp','interfaces','127.0.0.1','--port',str(endpoint),'--snmp-version','3','--username','observer','--samples','0','--interval-ms','1000','--max-bytes','65536','--format','jsonl'],env=m.ENV,stdout=log,stderr=subprocess.PIPE,text=True)
   start=time.monotonic()
   while time.monotonic()-start<a.duration:
    assert child.poll() is None
    rss,cpu=subprocess.check_output(['ps','-o','rss=,pcpu=','-p',str(child.pid)],text=True).split();measure.append({'elapsed':round(time.monotonic()-start,2),'rss_kib':int(rss),'cpu_percent':float(cpu)});time.sleep(min(5,max(.1,a.duration-(time.monotonic()-start))))
   stopped=time.monotonic();child.send_signal(signal.SIGINT);child.wait(timeout=2);stop=time.monotonic()-stopped;assert child.returncode==130
  finally:
   if child and child.poll() is None:child.terminate();child.wait(timeout=5)
   agent.terminate();agent.wait(timeout=5)
lines=[json.loads(s) for s in (out/'soak.jsonl').read_text().splitlines()];data=lines[-1]['data'];assert data['stats']['samples_valid']>=2 and data['stats']['samples_failed']==0;assert len(data['interface_samples'])<=128;assert stop<1
if a.duration>=140:assert data['stats']['samples_dropped']>0,(data['stats'],len(data['interface_samples']))
report={'status':'passed','duration_seconds':a.duration,'platform':platform.platform(),'version':subprocess.check_output([str(cli),'--version'],text=True).strip(),'sha256':hashlib.sha256(cli.read_bytes()).hexdigest(),'stats':data['stats'],'retained_samples':len(data['interface_samples']),'stop_seconds':stop,'measurements':measure,'max_rss_kib':max(s['rss_kib'] for s in measure)}
(out/'soak.json').write_text(json.dumps(report,indent=2));print(json.dumps({k:v for k,v in report.items() if k!='measurements'},indent=2))
