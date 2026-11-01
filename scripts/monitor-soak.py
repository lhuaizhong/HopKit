"""Real-time monitor endurance test with local TCP fault injection (default 2h).
Run: python scripts/monitor-soak.py --duration 7200
Progress and resource samples are flushed to test-output/monitor-soak/.
"""
import argparse
import json
import os
from pathlib import Path
import socket
import subprocess
import threading
import time
import hashlib
import ctypes
import shutil

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--duration', type=int, default=7200)
p.add_argument('--exe', type=Path, default=Path('target/debug/hopkit-cli.exe' if os.name == 'nt' else 'target/debug/hopkit-cli'))
p.add_argument('--output', type=Path, default=Path('test-output/monitor-soak'))
a = p.parse_args()
a.output.mkdir(parents=True, exist_ok=True)
original_exe = a.exe.resolve()
a.exe = a.output / original_exe.name
shutil.copy2(original_exe, a.exe)

class Peer:
    def __init__(self):
        self.sock = socket.socket()
        if os.name != 'nt':
            self.sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.sock.bind(('127.0.0.1', 0))
        self.port = self.sock.getsockname()[1]
        self.sock.listen(64)
        self.sock.settimeout(.2)
        self.enabled = True
        self.stopped = False
        self.accepted = 0
        self.thread = threading.Thread(target=self.run, daemon=True)
        self.thread.start()
    def run(self):
        while not self.stopped:
            if not self.enabled:
                if self.sock:
                    self.sock.close()
                    self.sock = None
                time.sleep(.1)
                continue
            if self.sock is None:
                self.sock = socket.socket()
                if os.name != 'nt':
                    self.sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
                self.sock.bind(('127.0.0.1', self.port))
                self.sock.listen(64)
                self.sock.settimeout(.2)
            try:
                conn, _ = self.sock.accept()
                self.accepted += 1
                conn.close()
            except socket.timeout:
                pass

def memory(pid):
    if os.name != 'nt':
        try:
            import resource
            return {'child_peak_rss_kib': resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss}
        except ImportError:
            return {}
    from ctypes import wintypes
    class Counters(ctypes.Structure):
        _fields_ = [('cb', wintypes.DWORD), ('PageFaultCount', wintypes.DWORD)] + [(n, ctypes.c_size_t) for n in ['PeakWorkingSetSize','WorkingSetSize','QuotaPeakPagedPoolUsage','QuotaPagedPoolUsage','QuotaPeakNonPagedPoolUsage','QuotaNonPagedPoolUsage','PagefileUsage','PeakPagefileUsage']]
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.OpenProcess.restype = wintypes.HANDLE
    kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
    handle = kernel.OpenProcess(0x410, False, pid)
    counters = Counters(); counters.cb = ctypes.sizeof(counters)
    query = ctypes.WinDLL('psapi').GetProcessMemoryInfo
    query.argtypes = [wintypes.HANDLE, ctypes.POINTER(Counters), wintypes.DWORD]
    low, high = wintypes.FILETIME(), wintypes.FILETIME()
    create, exit_time = wintypes.FILETIME(), wintypes.FILETIME()
    kernel.GetProcessTimes.argtypes = [wintypes.HANDLE] + [ctypes.POINTER(wintypes.FILETIME)] * 4
    result = {}
    try:
        if query(handle, ctypes.byref(counters), counters.cb):
            result = {'rss_bytes': counters.WorkingSetSize, 'peak_rss_bytes': counters.PeakWorkingSetSize}
        if kernel.GetProcessTimes(handle,ctypes.byref(create),ctypes.byref(exit_time),ctypes.byref(low),ctypes.byref(high)):
            result['cpu_seconds'] = (((low.dwHighDateTime << 32) + low.dwLowDateTime) + ((high.dwHighDateTime << 32) + high.dwLowDateTime)) / 10_000_000
    finally:
        if handle: kernel.CloseHandle(handle)
    return result

peers = [Peer(), Peer()]
command = [str(a.exe.resolve()), 'monitor', '--target', 'ping,127.0.0.1', '--target', f'tcping,127.0.0.1,{peers[0].port}', '--target', f'tcping,127.0.0.1,{peers[1].port}', '--duration', str(a.duration), '--timeout-ms', '200', '--interval-ms', '1000', '--log-directory', str((a.output/'journal').resolve()), '--format', 'jsonl']
start = time.time()
process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=open(a.output/'stderr.log','w'), text=True, encoding='utf-8')
state = {'status':'running','pid':process.pid,'started_at':start,'duration_secs':a.duration,'command':command,'binary_sha256':hashlib.sha256(a.exe.read_bytes()).hexdigest()}
(a.output/'progress.json').write_text(json.dumps(state, indent=2),encoding='utf-8')
final = []
def read():
    with open(a.output/'stream.jsonl','w',encoding='utf-8') as stream:
        for line in process.stdout:
            stream.write(line); stream.flush()
            event=json.loads(line)
            if event.get('event_type') == 'finished': final.append(event)
reader=threading.Thread(target=read,daemon=True);reader.start()
resources=[]
with open(a.output/'resources.jsonl','w',encoding='utf-8') as output:
    while process.poll() is None:
        elapsed=time.time()-start
        # First outage at 10s, then every five minutes, each lasting 10s.
        peers[1].enabled = not (10 <= elapsed % 300 < 20)
        sample={'elapsed_secs':elapsed,'business_enabled':peers[1].enabled,**memory(process.pid)}
        resources.append(sample);output.write(json.dumps(sample)+'\n');output.flush()
        state.update(elapsed_secs=elapsed,latest_resource=sample)
        (a.output/'progress.json').write_text(json.dumps(state,indent=2),encoding='utf-8')
        time.sleep(1 if elapsed<30 else 5)
reader.join(timeout=10)
for peer in peers: peer.stopped=True
try:
    assert process.returncode == 0, process.returncode
    assert final, 'Missing final envelope'
    result=final[0]['data']
    assert result['sample_count'] == sum(t['completed'] for t in result['totals'])
    assert result['totals'][0]['failed'] == 0, 'Loopback ICMP failures'
    assert result['totals'][1]['failed'] == 0, 'Stable TCP failures'
    assert result['totals'][2]['failed'] > 0, 'Fault injection not observed'
    assert result['totals'][1]['succeeded'] == peers[0].accepted, 'Stable TCP independent counter mismatch'
    assert result['totals'][2]['succeeded'] == peers[1].accepted, 'Business TCP independent counter mismatch'
    assert any(e.get('end_reason') == 'observed_recovery' for e in result['records'])
    assert len(result['samples']) <= 900 and len(result['buckets']) <= 1024 and len(result['records']) <= 300
    assert time.time()-start >= a.duration, 'Not a real-time endurance run'
    state.update(status='passed',sample_count=result['sample_count'],totals=result['totals'],accepted=[p.accepted for p in peers],max_rss_bytes=max((r.get('rss_bytes',0) for r in resources),default=0))
    (a.output/'final.json').write_text(json.dumps(final[0],ensure_ascii=False,indent=2),encoding='utf-8')
except Exception as error:
    state.update(status='failed',error=str(error))
state.update(exit_code=process.returncode,ended_at=time.time())
(a.output/'progress.json').write_text(json.dumps(state,indent=2),encoding='utf-8')
print(json.dumps(state,indent=2))
raise SystemExit(0 if state['status']=='passed' else 1)
