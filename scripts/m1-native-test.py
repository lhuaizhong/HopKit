#!/usr/bin/env python3
"""Run after npm run build and cargo build -p hopkit-desktop --features custom-protocol,native-test.
Uses the debug-only driver and four unprivileged loopback NTP services.
"""
import json
import os
from pathlib import Path
import socket
import subprocess

root = Path(__file__).resolve().parents[1]
output = root / 'test-output/m1'
output.mkdir(parents=True, exist_ok=True)
report = output / 'native.json'
if report.exists():
    report.unlink()
sockets = []
try:
    for _ in range(4):
        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        sock.bind(('127.0.0.1', 0))
        sockets.append(sock)
    ports = [sock.getsockname()[1] for sock in sockets]
finally:
    for sock in sockets:
        sock.close()
config = output / 'native-config.json'
config.write_text(json.dumps({'scenario': 'm1', 'ports': ports}))
binary = root / 'target/debug' / ('hopkit-desktop.exe' if os.name == 'nt' else 'hopkit-desktop')
process = subprocess.Popen([str(binary)], env={**os.environ, 'HOPKIT_NATIVE_TEST_CONFIG': str(config)}, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
try:
    stdout, stderr = process.communicate(timeout=55)
except subprocess.TimeoutExpired:
    process.terminate()
    stdout, stderr = process.communicate(timeout=10)
    raise RuntimeError('Native M1 check timed out: ' + stderr.decode(errors='replace'))
if process.returncode or not report.exists():
    raise RuntimeError(f'Native check exited {process.returncode}: {stderr.decode(errors="replace")}')
result = json.loads(report.read_text())
assert result['status'] == 'passed', result
# Every listener must release its port after cancellation and application exit.
for port in ports:
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
        sock.bind(('127.0.0.1', port))
result['ports_released'] = True
report.write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
print(json.dumps(result, ensure_ascii=False, indent=2))
