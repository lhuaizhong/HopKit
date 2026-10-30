#!/usr/bin/env python3
"""Native/CLI acceptance; optional official iperf3 peers are test-only dependencies.

python3 scripts/iperf3-test.py --reference /path/to/iperf3 [--reference /another/version]
"""
import argparse
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--reference', type=Path, action='append', default=[])
args = parser.parse_args()
CLI = ROOT / 'target/debug' / ('hopkit-cli.exe' if os.name == 'nt' else 'hopkit-cli')
OUT = ROOT / 'test-output/iperf3'
OUT.mkdir(parents=True, exist_ok=True)
checks = []
# Both native roles must work with no iperf3, shell, or other program on PATH.
NATIVE_ENV = {**os.environ, 'PATH': str(OUT / 'empty-path')}


def free_port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        return sock.getsockname()[1]


def execute(argv, native=False):
    result = subprocess.run([str(x) for x in argv], capture_output=True, text=True,
                            timeout=15, env=NATIVE_ENV if native else None)
    value = json.loads(result.stdout)
    assert result.returncode == 0, (argv, value, result.stderr)
    assert not value.get('error'), value
    return value


def native_client(port, mode):
    flags = {'tcp': [], 'reverse': ['-R'], 'parallel': ['-P', '3'],
             'udp': ['-u', '--bitrate-mbps', '5'],
             'udp-reverse': ['-u', '-R', '--bitrate-mbps', '5']}[mode]
    value = execute([CLI, 'iperf3', 'client', '127.0.0.1', '--port', port,
                     '--duration', '1', '--format', 'json', *flags], native=True)
    data = value['data']
    assert data['implementation'] == 'native-rust' and data['success'], value
    assert data['sent_bytes'] > 0 and data['received_bytes'] > 0, value
    assert data['sent_mbps'] > .1 and data['received_mbps'] > .1, value
    if mode.startswith('udp'):
        assert data['jitter_ms'] is not None and 0 <= data['loss_percent'] <= 100, value
    return value


def stop(proc):
    if proc.poll() is None:
        proc.send_signal(signal.SIGINT)
    try:
        proc.wait(timeout=3)
    finally:
        if proc.poll() is None:
            proc.kill()
            proc.wait()


def ready(proc, log, port, native):
    end = time.monotonic() + 8
    while time.monotonic() < end:
        assert proc.poll() is None, 'Server exited during startup'
        if native and hasattr(os, 'pread'):
            if b'"event_type":"ready"' in os.pread(log.fileno(), 4096, 0):
                return
        else:
            try:
                with socket.create_connection(('127.0.0.1', port), timeout=.1):
                    pass
                time.sleep(.1)  # Let the reference discard the empty readiness connection.
                return
            except OSError:
                pass
        time.sleep(.02)
    raise AssertionError('Server did not become ready')


for reference in [None, *args.reference]:
    native_server = reference is None
    label = 'native' if native_server else subprocess.check_output(
        [str(reference), '--version'], text=True).splitlines()[0]
    port = free_port()
    with tempfile.TemporaryFile(mode='w+') as log:
        cmd = [CLI, 'iperf3', 'serve', '--listen', '127.0.0.1', '--port', str(port), '--format', 'jsonl'] if native_server else [reference, '-s', '-B', '127.0.0.1', '-p', str(port)]
        proc = subprocess.Popen([str(x) for x in cmd], stdout=log, stderr=log,
                                env=NATIVE_ENV if native_server else None)
        try:
            ready(proc, log, port, native_server)
            for mode in ['tcp', 'reverse', 'parallel', 'udp', 'udp-reverse']:
                value = native_client(port, mode)
                checks.append({'test': f'native client → {label} server: {mode}', 'passed': True,
                               'sent_mbps': value['data']['sent_mbps'], 'received_mbps': value['data']['received_mbps']})
                if native_server and mode == 'udp':
                    (OUT / 'ui-result.json').write_text(json.dumps(value, indent=2) + '\n')
                print(checks[-1]['test'], 'PASS', flush=True)
        finally:
            stop(proc)
            log.seek(0)
            (OUT / f'{label.replace(" ", "-")}-server.log').write_text(log.read())
    # Reference client exercises our server's parameter parsing/results independently.
    if reference is not None:
        port = free_port()
        with tempfile.TemporaryFile(mode='w+') as log:
            proc = subprocess.Popen([str(CLI), 'iperf3', 'serve', '--listen', '127.0.0.1', '--port', str(port), '--format', 'jsonl'], stdout=log, stderr=log, env=NATIVE_ENV)
            try:
                ready(proc, log, port, True)
                for mode, flags in [('tcp', []), ('reverse', ['-R']), ('parallel', ['-P', '3']),
                                    ('udp', ['-u', '-b', '5M']), ('udp-reverse', ['-u', '-R', '-b', '5M']),
                                    ('udp-64bit', ['-u', '-b', '5M', '--udp-counters-64bit'])]:
                    value = execute([reference, '-c', '127.0.0.1', '-p', port, '-t', '1', '-J', *flags])
                    assert value['end'], value
                    checks.append({'test': f'{label} client → native server: {mode}', 'passed': True})
                    print(checks[-1]['test'], 'PASS', flush=True)
            finally:
                stop(proc)
                log.seek(0)
                events = log.read()
                (OUT / f'{label.replace(" ", "-")}-native-server.jsonl').write_text(events)
                assert '"completed_tests":6' in events, events[-3000:]

(OUT / 'interop-report.json').write_text(json.dumps({'checks': checks, 'native_PATH': NATIVE_ENV['PATH']}, indent=2) + '\n')
print(f'{len(checks)} checks passed. Report: {OUT / "interop-report.json"}')
