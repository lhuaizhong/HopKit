"""Offline CLI presentation regression; Python standard library, local peers only."""
import argparse
import contextlib
import json
import os
from pathlib import Path
import queue
import signal
import socket
import struct
import subprocess
import tempfile
import threading
import time
import urllib.request

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--exe', default='target/debug/hopkit-cli.exe' if os.name == 'nt' else 'target/debug/hopkit-cli')
a = p.parse_args()
exe = str(Path(a.exe).resolve())
checks = []


def plain(output):
    for field in ('"task_id"', '"schema_version"', '"event_type"', '"request"', 'null', '\x1b'):
        assert field not in output, (field, output)


def run(*args, code=0):
    result = subprocess.run([exe, *map(str, args)], capture_output=True, text=True, encoding='utf-8', timeout=25)
    assert result.returncode == code, (args, result.returncode, result.stdout, result.stderr)
    plain(result.stdout)
    checks.append(' '.join(map(str, args)))
    return result.stdout


def port(udp=False):
    with socket.socket(type=socket.SOCK_DGRAM if udp else socket.SOCK_STREAM) as s:
        s.bind(('127.0.0.1', 0))
        return s.getsockname()[1]


@contextlib.contextmanager
def service(*args):
    proc = subprocess.Popen([exe, *map(str, args)], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding='utf-8',
                            creationflags=subprocess.CREATE_NEW_PROCESS_GROUP if os.name == 'nt' else 0)
    lines = []
    ready = queue.Queue()

    def read():
        for line in proc.stdout:
            lines.append(line)
            if line.startswith('Listening:'):
                ready.put(line)

    reader = threading.Thread(target=read, daemon=True)
    reader.start()
    try:
        assert '127.0.0.1:' in ready.get(timeout=10)
        yield proc, lines
    finally:
        if proc.poll() is None:
            proc.send_signal(signal.CTRL_BREAK_EVENT if os.name == 'nt' else signal.SIGINT)
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
            raise AssertionError('Service did not stop after Ctrl+C')
        reader.join(timeout=2)
        proc.stdout.close()
        proc.stderr.close()
        plain(''.join(lines))
        assert proc.returncode == 130 and 'Cancelled.' in ''.join(lines), lines


with tempfile.TemporaryDirectory(prefix='HopKit text ') as temp:
    root = Path(temp)
    (root/'report.txt').write_text('HopKit', encoding='utf-8')
    for lang, expected in [('en-US', 'Statistics:'), ('zh-CN', '统计:')]:
        text = run('ping', '127.0.0.1', '-c', 2, '--interval-ms', 100, '--lang', lang)
        assert text.count(' ms') == 3 and expected in text, text
    assert 'Destination reached.' in run('trace', '127.0.0.1', '--max-hops', 2)
    with socket.socket() as tcp:
        tcp.bind(('127.0.0.1', 0))
        tcp.listen()
        tcp_port = tcp.getsockname()[1]
        text = run('tcping', '127.0.0.1', tcp_port, '-c', 1)
        assert ' OK ' in text and 'Lost 0' in text
        text = run('monitor', '--target', f'tcping,127.0.0.1,{tcp_port}', '--duration', 2)
        assert 'Monitor summary:' in text and 'failed=0' in text
    failed = run('tcping', '127.0.0.1', port(), '-c', 1, '--timeout-ms', 100, code=1)
    assert ('Timed out' in failed or 'Connection refused' in failed) and 'null ms' not in failed
    assert '127.0.0.1' in run('dns', 'localhost')
    assert 'hidden' not in run('target', 'https://example.test/path?secret=hidden')
    assert 'HopKit' in run('doctor')
    assert 'Error' in run('tftp', 'put', '127.0.0.1', root/'missing.txt', code=4)
    run('wormhole', 'send', root/'missing.txt', code=4)

    # Explicit machine formats retain envelopes/events and parse as before.
    for fmt in ('json', 'jsonl'):
        result = subprocess.run([exe, 'ping', '127.0.0.1', '-c', '1', '--format', fmt], capture_output=True, text=True, encoding='utf-8', timeout=10)
        assert result.returncode == 0
        values = [json.loads(line) for line in result.stdout.splitlines()]
        assert values[-1]['data']['received'] == 1 and values[-1]['schema_version'] == 1
        if fmt == 'jsonl':
            assert values[-1]['event_type'] == 'finished' and any(v.get('event_type') == 'sample' for v in values)
        else:
            assert len(values) == 1 and 'request' in values[0]
        checks.append(f'--format {fmt}')

    # Independent DNS UDP responder: answer records and NXDOMAIN stay readable.
    for rcode in (0, 3):
        with socket.socket(type=socket.SOCK_DGRAM) as dns:
            dns.bind(('127.0.0.1', 0))
            dns.settimeout(5)

            def answer_dns():
                packet, peer = dns.recvfrom(4096)
                header = packet[:2] + struct.pack('!HHHHH', 0x8180 | rcode, 1, 0 if rcode else 1, 0, 0)
                answer = b'' if rcode else b'\xc0\x0c' + struct.pack('!HHIH', 1, 1, 60, 4) + socket.inet_aton('192.0.2.7')
                dns.sendto(header + packet[12:] + answer, peer)

            thread = threading.Thread(target=answer_dns)
            thread.start()
            text = run('dns', 'example.test', 'A', '-s', f'127.0.0.1:{dns.getsockname()[1]}', code=1 if rcode else 0)
            thread.join(timeout=5)
            assert ('NXDOMAIN' if rcode else 'example.test  A  192.0.2.7') in text, text

    ntp_port = port(True)
    with service('ntp', 'serve', '--listen', '127.0.0.1', '--port', ntp_port):
        text = run('ntp', '127.0.0.1', '--port', ntp_port, '-c', 1)
        assert 'offset=' in text and 'stratum=10' in text, text

    http_port = port()
    with service('http', 'serve', '--root', root, '--listen', '127.0.0.1', '--port', http_port) as (_, lines):
        url = f'http://127.0.0.1:{http_port}/files/report.txt'
        with urllib.request.build_opener(urllib.request.ProxyHandler({})).open(url) as response:
            assert response.read() == b'HopKit'
        text = run('check', url)
        assert 'HTTP' in text and 'HTTP 200' in text and 'Check complete' in text, text
        text = run('check', f'http://127.0.0.1:{http_port}/files/missing.txt')
        assert 'FAIL  HTTP 404' in text and 'issue(s)' in text, text

    ftp_port = port()
    with service('ftp', 'serve', '--root', root, '--listen', '127.0.0.1', '--port', ftp_port):
        assert 'report.txt' in run('ftp', 'list', '127.0.0.1', '--port', ftp_port)
        text = run('ftp', 'get', '127.0.0.1', 'report.txt', '-o', root/'download.txt', '--port', ftp_port)
        assert 'Transfer complete: 6 B' in text and (root/'download.txt').read_bytes() == b'HopKit'

    tftp_port = port(True)
    with service('tftp', 'serve', '--root', root, '--listen', '127.0.0.1', '--port', tftp_port):
        assert 'Transfer complete: 6 B' in run('tftp', 'get', '127.0.0.1', 'report.txt', '-o', root/'tftp.txt', '--port', tftp_port)

    syslog_port = port(True)
    with service('syslog', 'listen', '--listen', '127.0.0.1', '--port', syslog_port, '--dir', root/'logs') as (_, lines):
        assert 'UDP submitted' in run('syslog', 'send', '127.0.0.1', 'text-smoke', '--port', syslog_port)
        until = time.monotonic() + 3
        while time.monotonic() < until and not any('text-smoke' in line for line in lines):
            time.sleep(.05)
        assert any('text-smoke' in line for line in lines), lines

    iperf_port = port()
    with service('iperf3', 'serve', '--listen', '127.0.0.1', '--port', iperf_port):
        text = run('iperf3', 'client', '127.0.0.1', '--port', iperf_port, '--duration', 1)
        assert 'Mbps' in text and 'Summary:' in text, text

    text = run('ntp', 'serve', '--listen', '127.0.0.1', '--port', port(True), '--duration', 1)
    assert 'Stopped:' in text and 'received=0' in text

print(f'PASS: {len(checks)} local CLI checks; concise text, both languages, and JSON/JSONL compatibility')
