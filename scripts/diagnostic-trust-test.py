#!/usr/bin/env python3
"""Real loopback + CLI acceptance. Run after cargo build -p hopkit-cli."""
import json
import hashlib
import platform
import signal
import socket
import subprocess
import threading
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CLI = ROOT / 'target/debug/hopkit-cli'
OUT = ROOT / 'test-output'
OUT.mkdir(exist_ok=True)
checks = []

listener = socket.socket()
listener.bind(('127.0.0.1', 0))
port = listener.getsockname()[1]
listener.listen()
listener.settimeout(.02)
stop = threading.Event()
accepted = [0]
def accept():
    while not stop.is_set():
        try:
            peer, _ = listener.accept()
            peer.close()
            accepted[0] += 1
        except socket.timeout:
            pass
        except OSError:
            return
thread = threading.Thread(target=accept, daemon=True)
thread.start()
try:
    with (OUT / 'trust-cli-1005.jsonl').open('w') as raw:
        proc = subprocess.Popen([str(CLI), 'tcping', '127.0.0.1', str(port), '-c', '1005', '--interval-ms', '50', '--format', 'jsonl'], stdout=subprocess.PIPE, text=True)
        samples = []
        final = None
        for line in proc.stdout:
            raw.write(line)
            event = json.loads(line)
            if event.get('event_type') == 'sample':
                samples.append(event)
                if event['seq'] == 502:
                    stop.set()
                    thread.join(timeout=1)
                    listener.close()
            if event.get('event_type') == 'finished':
                final = event
        assert proc.wait(timeout=5) == 0
    data = final['data']
    assert len(samples) == data['sent'] == data['completed'] == 1005
    assert data['received'] == accepted[0] == 502, (data['received'], accepted)
    assert data['lost'] == data['failure_counts']['refused'] == 503
    assert len(data['samples']) == 1005
    checks.append({'test': '1005 actual TCP probes, close controlled peer after 502 connections', 'sent': data['sent'], 'received': data['received'], 'refused': data['lost'], 'accepted_by_peer': accepted[0]})
finally:
    stop.set()
    listener.close()
    thread.join(timeout=1)

# Closed port gives an immediate failure; cancellation during the interval must
# still return the measured failure and exit 130 instead of diagnostic exit 1.
proc = subprocess.Popen([str(CLI), 'tcping', '127.0.0.1', str(port), '-c', '10', '--interval-ms', '10000', '--format', 'jsonl'], stdout=subprocess.PIPE, text=True)
lines = []
for line in proc.stdout:
    lines.append(json.loads(line))
    if lines[-1].get('event_type') == 'sample':
        proc.send_signal(signal.SIGINT)
assert proc.wait(timeout=3) == 130
final = lines[-1]
assert final['status'] == 'cancelled' and final['error']['code'] == 'CANCELLED'
assert final['data']['sent'] == final['data']['completed'] == final['data']['lost'] == 1
assert len(final['data']['samples']) == 1
(OUT / 'trust-cli-cancelled.json').write_text(json.dumps(final, ensure_ascii=False, indent=2))
checks.append({'test': 'SIGINT during 10 second wait', 'exit': 130, 'sent': 1, 'partial_result': True})

if platform.system() == 'Darwin':
    for tool in ('ping', 'trace'):
        args = ['-c', '3', '--interval-ms', '50'] if tool == 'ping' else ['--max-hops', '3']
        proc = subprocess.run([str(CLI), tool, '127.0.0.1', *args, '--format', 'json'], capture_output=True, text=True, timeout=10)
        final = json.loads(proc.stdout)
        assert proc.returncode == 0 and final['status'] == 'succeeded'
        if tool == 'trace':
            assert final['schema_version'] == 2 and final['data']['reached']
            assert 'loss_percent' not in final['data']
            assert len(final['data']['samples']) == 3
        else:
            assert final['data']['received'] == 3
        checks.append({'test': f'macOS actual {tool} loopback', 'passed': True})
report = {'platform': platform.platform(), 'cli_sha256': hashlib.sha256(CLI.read_bytes()).hexdigest(), 'checks': checks}
(OUT / 'diagnostic-trust-cli.json').write_text(json.dumps(report, ensure_ascii=False, indent=2))
print(json.dumps(report, ensure_ascii=False, indent=2))
