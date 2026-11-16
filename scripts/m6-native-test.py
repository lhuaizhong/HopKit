"""Private native WebView + Rust IPC against an independent Net-SNMP agent."""
import hashlib,json,os,platform,subprocess,tempfile,time
from pathlib import Path
from importlib.machinery import SourceFileLoader
m=SourceFileLoader('m6test',str(Path(__file__).with_name('m6-integration-test.py'))).load_module()
root=m.ROOT;out=m.OUT;report=out/'native.json'
if report.exists():report.unlink()
exe=root/'target/debug/hopkit-desktop'
with tempfile.TemporaryDirectory(prefix='hopkit-m6-native-') as tmp:
 port=m.port();agentconfig=Path(tmp)/'snmpd.conf';state=Path(tmp)/'state';state.mkdir()
 agentconfig.write_text(f'agentaddress udp:127.0.0.1:{port}\nrocommunity {m.COMM} 127.0.0.1\ncreateUser m6user SHA-256 {m.AUTH} AES {m.PRIV}\nrouser m6user priv\nsysName HopKit-M6-NetSNMP\n')
 config=out/'native-config.json';config.write_text(json.dumps({'scenario':'m6','port':port}))
 with open(out/'native-agent.log','w') as log:
  agent=subprocess.Popen([os.environ.get('HOPKIT_TEST_SNMPD','/opt/homebrew/opt/net-snmp/sbin/snmpd'),'-f','-Lo','-C','-c',str(agentconfig)],env={**os.environ,'SNMP_PERSISTENT_DIR':str(state)},stdout=log,stderr=log)
  p=None
  try:
   time.sleep(.5);assert agent.poll() is None
   p=subprocess.Popen([str(exe)],env={**os.environ,'HOPKIT_NATIVE_TEST_CONFIG':str(config)},stdout=subprocess.PIPE,stderr=subprocess.PIPE)
   stdout,stderr=p.communicate(timeout=60);assert p.returncode==0,(p.returncode,stderr.decode(errors='replace'));data=json.loads(report.read_text());assert data['status']=='passed',data;data.update(platform=platform.platform(),date=time.strftime('%Y-%m-%d'),binary_sha256=hashlib.sha256(exe.read_bytes()).hexdigest());report.write_text(json.dumps(data,ensure_ascii=False,indent=2));print(json.dumps(data,ensure_ascii=False,indent=2))
  finally:
   if p and p.poll() is None:p.terminate();p.wait(timeout=5)
   agent.terminate();agent.wait(timeout=5)
