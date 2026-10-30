"""Shared CLI packaging and acceptance for every release channel (stdlib only)."""
import hashlib
import json
import os
from pathlib import Path
import queue
import shutil
import signal
import socket
import subprocess
import tempfile
import threading


def add_shortcut(cli, folder):
    """Copy the exact binary; no PATH modification or shell argument forwarding."""
    shortcut = Path(folder) / 'shortcuts' / ('hk.exe' if cli.suffix == '.exe' else 'hk')
    shortcut.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(cli, shortcut)
    return shortcut


def verify_cli(folder):
    """Run shipped bytes after copying to a path with spaces and Chinese characters."""
    folder = Path(folder)
    suffix = '.exe' if (folder / 'hopkit-cli.exe').exists() else ''
    source = folder / ('hopkit-cli' + suffix)
    source_short = folder / 'shortcuts' / ('hk' + suffix)
    assert source.read_bytes() == source_short.read_bytes(), 'Packaged shortcut is missing or differs'
    with tempfile.TemporaryDirectory(prefix='HopKit 发布验证 中文 ') as temp:
        copied = Path(temp) / '发布 包'
        copied.mkdir()
        full = copied / ('hopkit-cli' + suffix)
        shutil.copy2(source, full)
        short = copied / 'shortcuts' / source_short.name
        short.parent.mkdir()
        shutil.copy2(source_short, short)
        assert full.read_bytes() == short.read_bytes(), 'CLI shortcut differs from full binary'
        cases = [(['--help'], 0), (['--version'], 0), (['ftp', 'list', '--help'], 0),
                 (['doctor', '--format', 'json'], 0),
                 (['doctor', '--format', 'jsonl'], 0),
                 (['ping', '127.0.0.1', '--size', '65501', '--format', 'json'], 2),
                 (['tftp', 'put', '127.0.0.1', str(copied / 'missing 文件.cfg'), '--format', 'json'], 4)]
        for argv, code in cases:
            outputs = [subprocess.run([str(exe), *argv], cwd=temp, capture_output=True, timeout=20) for exe in (full, short)]
            for result in outputs:
                assert result.returncode == code, (argv, result.returncode, result.stderr)
            if '--format' in argv:
                values = [json.loads(result.stdout) for result in outputs]
                # Timing/IDs are generated per invocation; compare the stable contract.
                for value in values:
                    for key in ('started_at', 'finished_at', 'duration_ms', 'task_id'):
                        value.pop(key, None)
                assert values[0] == values[1], (argv, values)
            else:
                assert outputs[0].stdout == outputs[1].stdout, argv
        message = '参数 空格 中文 "quoted" $literal'
        with socket.socket(type=socket.SOCK_DGRAM) as peer:
            peer.bind(('127.0.0.1', 0))
            peer.settimeout(5)
            for exe in (full, short):
                result = subprocess.run([str(exe), 'syslog', 'send', '127.0.0.1', message,
                                         '--port', str(peer.getsockname()[1]), '--format', 'json'],
                                        cwd=temp, capture_output=True, timeout=20)
                assert result.returncode == 0, result.stderr
                assert peer.recv(65535).decode().endswith('hopkit hopkit: ' + message)
        for exe in (full, short):
            with socket.socket() as reserved:
                reserved.bind(('127.0.0.1', 0))
                result = subprocess.run([str(exe), 'tcping', '127.0.0.1', str(reserved.getsockname()[1]),
                                         '-c', '1', '--format', 'json'], cwd=temp, capture_output=True, timeout=20)
                assert result.returncode == 1, result.stdout
            logs = copied / ('日志 ' + exe.name)
            legacy = logs / 'netbox-syslog'
            legacy.mkdir(parents=True)
            (legacy / 'old.jsonl').write_bytes(b'legacy log\n')
            with socket.socket(type=socket.SOCK_DGRAM) as reserved:
                reserved.bind(('127.0.0.1', 0))
                argv = [str(exe), 'syslog', 'listen', '--listen', '127.0.0.1', '--port',
                        str(reserved.getsockname()[1]), '--dir', str(logs), '--format', 'jsonl']
                result = subprocess.run(argv, cwd=temp, capture_output=True, timeout=20)
                assert result.returncode == 3, result.stdout
            proc = subprocess.Popen(argv, cwd=temp, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                    creationflags=subprocess.CREATE_NEW_PROCESS_GROUP if os.name == 'nt' else 0)
            lines = queue.Queue()
            def read_ready():
                for line in proc.stdout:
                    value = json.loads(line)
                    if value.get('event_type') == 'ready':
                        lines.put(value)
                        return
            reader = threading.Thread(target=read_ready, daemon=True)
            reader.start()
            try:
                ready = lines.get(timeout=15)
                reader.join(timeout=1)
                assert Path(ready['directory']).name == 'hopkit-syslog'
                proc.send_signal(signal.CTRL_BREAK_EVENT if os.name == 'nt' else signal.SIGINT)
                stdout, stderr = proc.communicate(timeout=15)
                assert proc.returncode == 130, (proc.returncode, stdout, stderr)
                assert json.loads(stdout.splitlines()[-1])['status'] == 'cancelled'
            finally:
                if proc.poll() is None:
                    proc.kill()
                    proc.wait()
                reader.join(timeout=1)
            assert (legacy / 'old.jsonl').read_bytes() == b'legacy log\n'
        return {'status': 'passed', 'cases': len(cases) + 5, 'exit_codes': [0, 1, 2, 3, 4, 130], 'path_spaces_chinese': True,
                'changed_cwd': True, 'cli_sha256': hashlib.sha256(full.read_bytes()).hexdigest()}


def checksums(folder):
    folder = Path(folder)
    rows = [f'{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.relative_to(folder).as_posix()}'
            for p in sorted(folder.rglob('*')) if p.is_file() and p.name != 'SHA256SUMS.txt']
    (folder / 'SHA256SUMS.txt').write_text('\n'.join(rows) + '\n', encoding='utf-8')
