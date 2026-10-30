"""FTP interoperability cases used by integration_test.py; Python standard library only."""
import contextlib
import ftplib
import io
import os
import socket
import time


def run_checks(check, cli, service, port, work, thread):
    def connect(p, host='127.0.0.1', user='anonymous', password='guest'):
        ftp = ftplib.FTP(timeout=4)
        ftp.connect(host, p)
        ftp.login(user, password)
        return ftp

    def rejected(fn):
        try:
            fn()
        except ftplib.error_perm:
            return
        raise AssertionError('FTP command unexpectedly permitted')

    def server_files():
        root = work/'ftp-files'; root.mkdir(); (root/'子目录').mkdir()
        payload = bytes(range(256))*1025
        (root/'子目录'/'binary file.bin').write_bytes(payload)
        (root/'empty.bin').write_bytes(b'')
        p = port()
        with service('ftp', 'serve', '--root', root, '--listen', '127.0.0.1', '--port', p, '--write', duration=4) as (_, log):
            with connect(p) as ftp:
                assert set(ftp.nlst()) == {'子目录', 'empty.bin'}
                rejected(lambda: ftp.sendcmd('RETR empty.bin'))  # TYPE A listings do not enable ASCII file transfers.
                assert dict(ftp.mlsd())['子目录']['type'] == 'dir'
                ftp.cwd('子目录'); assert ftp.pwd() == '/子目录'
                output = io.BytesIO(); ftp.retrbinary('RETR binary file.bin', output.write)
                assert output.getvalue() == payload
                assert ftp.size('binary file.bin') == len(payload)
                assert ftp.sendcmd('MDTM binary file.bin').startswith('213 ')
                ftp.storbinary('STOR 上传.bin', io.BytesIO(payload))
                assert (root/'子目录'/'上传.bin').read_bytes() == payload
                rejected(lambda: ftp.storbinary('STOR 上传.bin', io.BytesIO(b'overwrite')))
                ftp.cwd('/'); output = io.BytesIO(); ftp.retrbinary('RETR empty.bin', output.write)
                assert output.getvalue() == b''
                ftp.storbinary('STOR zero.bin', io.BytesIO())
                assert (root/'zero.bin').read_bytes() == b''
                rejected(lambda: ftp.cwd('../../'))
                rejected(lambda: ftp.retrbinary('RETR ../outside.bin', lambda _: None))
                if os.name != 'nt':
                    outside = work/'outside.bin'; outside.write_bytes(b'secret')
                    (root/'link.bin').symlink_to(outside)
                    rejected(lambda: ftp.retrbinary('RETR link.bin', lambda _: None))
                    assert 'link.bin' not in ftp.nlst()
                assert cli('ftp', 'list', '127.0.0.1', '--port', p)['data']['entries']
                dest = work/'ftp-client.bin'
                assert cli('ftp', 'get', '127.0.0.1', '/子目录/binary file.bin', '-o', dest, '--port', p)['data']['bytes'] == len(payload)
                assert dest.read_bytes() == payload
                assert cli('ftp', 'get', '127.0.0.1', 'empty.bin', '-o', dest, '--port', p, code=4)['error']['code'] == 'ALREADY_EXISTS'
                cli('ftp', 'get', '127.0.0.1', 'empty.bin', '-o', dest, '--port', p, '--overwrite')
                assert dest.read_bytes() == b''
                cli('ftp', 'put', '127.0.0.1', dest, '--remote-name', 'client-upload.bin', '--port', p)
                assert (root/'client-upload.bin').read_bytes() == b''
        assert '"event_type":"transfer"' in log.read_text(encoding='utf-8')
        assert not list(root.rglob('*.part'))

    def authentication():
        root = work/'ftp-auth'; root.mkdir(); p = port()
        previous = os.environ.get('NETBOX_FTP_PASSWORD')
        os.environ['NETBOX_FTP_PASSWORD'] = 'test-ftp-secret'
        try:
            with service('ftp', 'serve', '--root', root, '--listen', '127.0.0.1', '--port', p, '--user', 'tester', duration=2) as (_, log):
                ftp = ftplib.FTP(timeout=3); ftp.connect('127.0.0.1', p)
                rejected(lambda: ftp.login('tester', 'wrong')); ftp.close()
                with connect(p, user='tester', password='test-ftp-secret') as ftp:
                    rejected(lambda: ftp.storbinary('STOR denied.bin', io.BytesIO(b'data')))
                    rejected(lambda: ftp.sendcmd('DELE anything'))
                assert cli('ftp', 'list', '127.0.0.1', '--port', p, '--user', 'tester')['status'] == 'succeeded'
                os.environ['NETBOX_FTP_PASSWORD'] = 'incorrect'
                value = cli('ftp', 'list', '127.0.0.1', '--port', p, '--user', 'tester', code=1)
                assert value['error']['code'] == 'FTP_AUTH'
            assert 'test-ftp-secret' not in log.read_text(encoding='utf-8')
            assert not (root/'denied.bin').exists()
        finally:
            if previous is None: os.environ.pop('NETBOX_FTP_PASSWORD', None)
            else: os.environ['NETBOX_FTP_PASSWORD'] = previous

    def deny_and_limit():
        root = work/'ftp-limits'; root.mkdir(); p = port()
        with service('ftp', 'serve', '--root', root, '--listen', '127.0.0.1', '--port', p, '--allow', '192.0.2.0/24', duration=1):
            with socket.create_connection(('127.0.0.1', p), timeout=2) as s:
                assert s.recv(512).startswith(b'421')
        with service('ftp', 'serve', '--root', root, '--listen', '127.0.0.1', '--port', p, duration=2):
            with contextlib.ExitStack() as stack:
                for _ in range(4): stack.enter_context(connect(p))
                with socket.create_connection(('127.0.0.1', p), timeout=2) as s:
                    assert s.recv(512).startswith(b'421')

    def ipv6():
        root = work/'ftp-ipv6'; root.mkdir(); (root/'v6.txt').write_bytes(b'IPv6')
        with socket.socket(socket.AF_INET6) as reservation:
            reservation.bind(('::1', 0)); p = reservation.getsockname()[1]
        with service('ftp', 'serve', '--root', root, '--listen', '::1', '--port', p, '--allow', '::1/128', duration=2):
            with connect(p, '::1') as ftp:
                output = io.BytesIO(); ftp.retrbinary('RETR v6.txt', output.write)
                assert output.getvalue() == b'IPv6'
            value = cli('ftp', 'list', '::1', '-6', '--port', p)
            assert any('v6.txt' in line for line in value['data']['entries'])

    def independent_client_peer(pasv=False, fail_final=False, upload=False):
        payload = bytes(range(256))*257
        received = bytearray()
        with socket.socket() as server:
            server.bind(('127.0.0.1', 0)); server.listen(); server.settimeout(5)
            p = server.getsockname()[1]
            def responder():
                with server.accept()[0] as control:
                    control.settimeout(5)
                    with control.makefile('rb') as lines, contextlib.ExitStack() as stack:
                        control.sendall(b'220-Welcome\r\n independent FTP peer\r\n220 Ready\r\n')
                        passive = None
                        while True:
                            line = lines.readline().decode().rstrip('\r\n')
                            if not line: return
                            verb, _, arg = line.partition(' ')
                            if verb == 'USER': reply = '331 Password required'
                            elif verb == 'PASS': reply = '230 OK'
                            elif verb == 'TYPE': assert arg == 'I'; reply = '200 Binary'
                            elif verb == 'EPSV' and pasv: reply = '502 Use PASV'
                            elif verb in ('EPSV', 'PASV'):
                                passive = stack.enter_context(socket.socket()); passive.bind(('127.0.0.1', 0)); passive.listen(); passive.settimeout(5)
                                dp = passive.getsockname()[1]
                                # An unusable advertised host verifies the control peer is used.
                                reply = f'227 Passive (192,0,2,99,{dp//256},{dp%256})' if pasv else f'229 Extended (|||{dp}|)'
                            elif verb in ('RETR', 'STOR'):
                                assert arg == 'remote.bin'
                                control.sendall(b'150 Transfer starting\r\n')
                                with passive.accept()[0] as data:
                                    data.settimeout(5)
                                    if upload:
                                        while True:
                                            chunk = data.recv(65536)
                                            if not chunk: break
                                            received.extend(chunk)
                                    else: data.sendall(payload)
                                reply = '451 Final transfer failed' if fail_final else '226 Complete'
                            elif verb == 'QUIT': control.sendall(b'221 Bye\r\n'); return
                            else: raise AssertionError(line)
                            control.sendall((reply+'\r\n').encode())
            t, errors = thread(responder)
            dest = work/f'ftp-independent-{pasv}-{fail_final}-{upload}.bin'
            if upload:
                dest.write_bytes(payload)
                result = cli('ftp', 'put', '127.0.0.1', dest, '--remote-name', 'remote.bin', '--port', p)
            else:
                if fail_final: dest.write_bytes(b'keep original')
                result = cli('ftp', 'get', '127.0.0.1', 'remote.bin', '-o', dest, '--port', p, '--overwrite', code=1 if fail_final else 0)
            t.join(5); assert not t.is_alive(); assert not errors, errors
            if fail_final:
                assert result['error']['code'] == 'FTP_REMOTE'
                assert dest.read_bytes() == b'keep original'
                assert not list(work.glob('*.part'))
            else:
                assert result['data']['bytes'] == len(payload)
                assert (bytes(received) if upload else dest.read_bytes()) == payload

    check('FTP server interoperability: ftplib listings, binary/empty transfers, paths and no-clobber', server_files)
    check('FTP named login, readonly server and password redaction', authentication)
    check('FTP source filtering and four-session limit', deny_and_limit)
    check('FTP IPv6 EPSV with independent client', ipv6)
    check('FTP client: independent multiline/EPSV binary download', independent_client_peer)
    check('FTP client: PASV fallback ignores advertised third-party host', lambda: independent_client_peer(pasv=True))
    check('FTP client: independent binary upload', lambda: independent_client_peer(upload=True))
    check('FTP download final failure preserves destination and removes partial file', lambda: independent_client_peer(fail_final=True))
