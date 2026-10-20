#!/usr/bin/env python3
"""Private independent HTTPS fixture for an existing synthetic desktop profile.

No build, identity seeds, workspace copies, OS trust changes or certificate bypass.
Close the managed desktop first. State and private logs remain for inspection.
"""
import argparse
import hashlib
import http.client
import http.server
import json
import os
from pathlib import Path
import re
import secrets
import signal
import socket
import ssl
import stat
import subprocess
import tempfile
import threading
import time


class Refused(Exception):
    pass


def private(path, directory=False):
    before = path.lstat()
    if path != path.resolve(strict=True) or before.st_uid != os.getuid() or before.st_mode & 0o077:
        raise Refused('Private owned canonical path required')
    if not (stat.S_ISDIR(before.st_mode) if directory else stat.S_ISREG(before.st_mode)):
        raise Refused('Private regular file/directory required')
    if not directory and before.st_nlink != 1:
        raise Refused('Private file must have exactly one link')
    acl = subprocess.run(['/bin/ls', '-lde', str(path)], capture_output=True, timeout=10)
    if acl.returncode or len(acl.stdout.splitlines()) != 1:
        raise Refused('Extended ACL refused')
    return before


def fingerprint(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_new(path, value):
    with path.open('x') as output:
        json.dump(value, output)
        output.write('\n')


class GateRefused(Refused):
    pass


class ProxyGates:
    """Private one-shot synthetic gates; never stores request headers or bodies."""
    def __init__(self, directory, stop):
        self.directory, self.stop = directory, stop
        identity = private(directory, True)
        if stat.S_IMODE(identity.st_mode) != 0o700:
            raise GateRefused('Gate directory must have mode 0700')
        self.fd = os.open(directory, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        opened = os.fstat(self.fd)
        self.identity = (opened.st_dev, opened.st_ino)
        if self.identity != (identity.st_dev, identity.st_ino):
            os.close(self.fd)
            raise GateRefused('Gate directory changed')
        self.lock = threading.Lock()
        self.consumed = set()

    def check(self):
        current = self.directory.lstat()
        opened = os.fstat(self.fd)
        if (self.directory != self.directory.resolve(strict=True)
                or (current.st_dev, current.st_ino) != self.identity
                or (opened.st_dev, opened.st_ino) != self.identity
                or current.st_uid != os.getuid() or stat.S_IMODE(current.st_mode) != 0o700):
            raise GateRefused('Private gate directory changed')

    def read(self, name, limit):
        self.check()
        try:
            fd = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=self.fd)
        except FileNotFoundError:
            return None
        try:
            metadata = os.fstat(fd)
            if (not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != os.getuid()
                    or metadata.st_mode & 0o077 or metadata.st_nlink != 1 or metadata.st_size > limit):
                raise GateRefused('Private bounded gate file required')
            # Retain the existing fixture ACL policy, then compare the pinned descriptor.
            checked = private(self.directory / name)
            if (checked.st_dev, checked.st_ino) != (metadata.st_dev, metadata.st_ino):
                raise GateRefused('Gate file changed')
            with os.fdopen(fd, 'rb', closefd=False) as value:
                data = value.read(limit + 1)
            if len(data) > limit:
                raise GateRefused('Gate file bound exceeded')
            self.check()
            return data
        finally:
            os.close(fd)

    def write(self, name, value):
        self.check()
        temporary = '.gate-' + secrets.token_hex(16)
        fd = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW,
                     0o600, dir_fd=self.fd)
        try:
            with os.fdopen(fd, 'w') as output:
                json.dump(value, output)
                output.write('\n')
                output.flush()
                os.fsync(output.fileno())
            # Publish complete marker bytes atomically without replacing earlier evidence.
            os.link(temporary, name, src_dir_fd=self.fd, dst_dir_fd=self.fd,
                    follow_symlinks=False)
        finally:
            os.unlink(temporary, dir_fd=self.fd)
        self.check()

    def claim(self, method, path):
        with self.lock:
            raw = self.read('gate.json', 4096)
            if raw is None:
                return None
            def unique(pairs):
                value = {}
                for key, item in pairs:
                    if key in value:
                        raise GateRefused('Duplicate gate field')
                    value[key] = item
                return value
            try:
                gate = json.loads(raw, object_pairs_hook=unique)
            except (ValueError, UnicodeError) as error:
                raise GateRefused('Invalid gate configuration') from error
            if (not isinstance(gate, dict) or set(gate) != {'id', 'method', 'path', 'phase'}
                    or not all(isinstance(value, str) for value in gate.values())
                    or not re.fullmatch('[A-Za-z0-9_-]{1,64}', gate['id'])
                    or gate['method'] not in ('GET', 'POST')
                    or gate['phase'] not in ('before_forward', 'after_response')
                    or not gate['path'].startswith('/api/') or gate['path'].startswith('//')
                    or len(gate['path']) > 2048 or '#' in gate['path']
                    or any(ord(char) < 32 or ord(char) == 127 for char in gate['path'])):
                raise GateRefused('Exact bounded synthetic API gate required')
            if gate['id'] in self.consumed or gate['method'] != method or gate['path'] != path:
                return None
            if self.read('release-' + gate['id'], 0) is not None:
                raise GateRefused('Gate release must follow arrival')
            self.consumed.add(gate['id'])
            return gate

    def wait(self, gate, backend_status=None):
        metadata = {key: gate[key] for key in ('id', 'method', 'path', 'phase')}
        if gate['phase'] == 'after_response':
            metadata['backendStatus'] = backend_status
        self.write('arrived-' + gate['id'] + '.json', metadata)
        deadline = time.monotonic() + 45
        while not self.stop.is_set() and time.monotonic() < deadline:
            if self.read('release-' + gate['id'], 0) is not None:
                if self.stop.is_set():
                    raise GateRefused('Gate stopped')
                return
            self.stop.wait(.1)
        raise GateRefused('Gate stopped or release deadline expired')

    def completed(self, gate, backend_status):
        metadata = {key: gate[key] for key in ('id', 'method', 'path', 'phase')}
        metadata['backendStatus'] = backend_status
        self.write('completed-' + gate['id'] + '.json', metadata)

    def close(self):
        os.close(self.fd)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('server-binary', 'web-dir', 'profile', 'ca-cert', 'ca-key'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--ready-file', type=Path)
    parser.add_argument('--gate-dir', type=Path, help='Existing private canonical 0700 one-shot gate directory; release deadline 45 seconds')
    args = parser.parse_args()
    if os.uname().sysname != 'Darwin' or os.getuid() == 0 or os.getuid() != os.geteuid():
        raise Refused('Ordinary macOS fixture owner required')
    os.umask(0o077)
    profile = args.profile.absolute()
    private(profile, True)
    source = profile / 'server.json'
    original_identity = private(source)
    if original_identity.st_size > 65536:
        raise Refused('Bounded profile configuration required')
    original_hash = fingerprint(source)
    config = json.loads(source.read_text())
    if (config.get('runtimeProfile') != 'local-user-macos'
            or config.get('brokerUid') != os.getuid()
            or config.get('root') != str(profile / 'workspace')
            or not config.get('workspace')):
        raise Refused('Existing owned same-user profile required')
    private(profile / 'workspace', True)
    server = args.server_binary.resolve(strict=True)
    web = args.web_dir.resolve(strict=True)
    if not server.is_file() or not os.access(server, os.X_OK) or not (web / 'index.html').is_file():
        raise Refused('Existing executable and built web assets required')
    ca = args.ca_cert.resolve(strict=True)
    ca_key = args.ca_key.absolute()
    private(ca_key)
    if not ca.is_file() or ca.stat().st_size > 65536:
        raise Refused('Bounded public CA certificate required')
    state = Path(tempfile.mkdtemp(prefix='Rentemester-Synthetic-DesktopHTTPS-', dir=profile.parent))
    private(state, True)
    ready = args.ready_file.absolute() if args.ready_file else state / 'ready.json'
    if ready.exists() or ready.is_symlink():
        raise Refused('Readiness destination must be new')
    private(ready.parent, True)
    mail = state / 'mail'
    mail.mkdir(mode=0o700)
    stop = threading.Event()
    gates = ProxyGates(args.gate_dir.absolute(), stop) if args.gate_dir else None
    for signum in (signal.SIGTERM, signal.SIGINT):
        signal.signal(signum, lambda *_: stop.set())
    child = None
    servers = []
    started = []
    report = {'status': 'starting', 'stateDir': str(state), 'sourceConfigSha256': original_hash,
              'sourceProfile': str(profile), 'mailDir': str(mail), 'trustVerified': False}
    log = (state / 'fixture.log').open('xb')
    server_log = None
    try:
        def command(argv):
            result = subprocess.run(argv, stdin=subprocess.DEVNULL, stdout=log, stderr=log, timeout=30)
            if result.returncode:
                raise Refused('Certificate generation or strict trust verification refused; inspect private fixture log')
        leaf, key = state / 'leaf.pem', state / 'leaf.key'
        csr, ext = state / 'leaf.csr', state / 'leaf.ext'
        ext.write_text('basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=DNS:localhost,IP:127.0.0.1\n')
        command(['openssl', 'req', '-new', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(key), '-out', str(csr), '-subj', '/CN=localhost'])
        # Never creates a serial file next to, copies, or prints the supplied CA key.
        command(['openssl', 'x509', '-req', '-in', str(csr), '-CA', str(ca), '-CAkey', str(ca_key), '-set_serial', '0x' + secrets.token_hex(16), '-out', str(leaf), '-days', '1', '-extfile', str(ext)])
        command(['openssl', 'verify', '-CAfile', str(ca), '-purpose', 'sslserver', '-verify_hostname', 'localhost', str(leaf)])
        command(['openssl', 'verify', '-CAfile', str(ca), '-purpose', 'sslserver', '-verify_ip', '127.0.0.1', str(leaf)])
        # No supplied root override: this proves the existing macOS trust store.
        command(['security', 'verify-cert', '-c', str(leaf), '-p', 'ssl', '-n', 'localhost', '-L'])
        report['trustVerified'] = True
        tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        tls.load_cert_chain(leaf, key)
        bearer = secrets.token_urlsafe(32)
        rows = {}
        mail_lock = threading.Lock()
        with socket.socket() as reservation:
            reservation.bind(('127.0.0.1', 0))
            backend_port = reservation.getsockname()[1]
        origin = authority = None

        class Server(http.server.ThreadingHTTPServer):
            daemon_threads = True

        class Handler(http.server.BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def length(self, limit):
                values = self.headers.get_all('content-length', [])
                if self.headers.get('transfer-encoding') or len(values) > 1:
                    raise Refused('Unambiguous bounded content length required')
                value = int(values[0]) if values else 0
                if not 0 <= value <= limit:
                    raise Refused('Request bound exceeded')
                return value

        class Gateway(Handler):
            def do_POST(self):
                try:
                    if (self.path != '/mail' or self.headers.get('authorization') != 'Bearer ' + bearer):
                        raise Refused('Gateway authentication refused')
                    size = self.length(32768)
                    if size == 0:
                        raise Refused('Empty gateway body')
                    body = json.loads(self.rfile.read(size))
                    event = self.headers.get('x-rentemester-auth-event')
                    idem = self.headers.get('idempotency-key', '')
                    if (not re.fullmatch('[0-9a-f]{64}', idem)
                            or body.get('version') != 'rentemester-auth-email-v1'
                            or body.get('kind') != event
                            or event not in ('verification', 'workspace-invitation', 'password-reset')
                            or not body.get('to', '').endswith('@example.invalid')
                            or body.get('from') != 'auth@example.invalid'
                            or not body.get('actionUrl', '').startswith(origin + '/')):
                        raise Refused('Synthetic mail envelope required')
                    with mail_lock:
                        if idem in rows and rows[idem] != body:
                            raise Refused('Idempotency payload changed')
                        if idem not in rows:
                            write_new(mail / (str(len(rows) + 1) + '.json'), body)
                            rows[idem] = body
                    self.send_response(202)
                    self.send_header('content-length', '0')
                    self.end_headers()
                except Exception:
                    self.send_error(403)

        class Proxy(Handler):
            def forward(self):
                connection = None
                try:
                    if (self.headers.get_all('host') != [authority]
                            or not self.path.startswith('/') or self.path.startswith('//')):
                        raise Refused('Exact external authority required')
                    size = self.length(16 * 1024 * 1024)
                    headers = {k: v for k, v in self.headers.items()
                               if k.lower() not in ('host', 'connection', 'transfer-encoding', 'content-length')}
                    headers['host'] = authority
                    self.connection.settimeout(45)
                    body = self.rfile.read(size) if size else None
                    if size and len(body) != size:
                        raise Refused('Incomplete bounded request body')
                    gate = gates.claim(self.command, self.path) if gates else None
                    if gate and gate['phase'] == 'before_forward':
                        gates.wait(gate)
                    connection = http.client.HTTPConnection('127.0.0.1', backend_port, timeout=45)
                    connection.request(self.command, self.path, body, headers)
                    response = connection.getresponse()
                    payload = response.read(96 * 1024 * 1024 + 1)
                    if len(payload) > 96 * 1024 * 1024:
                        raise Refused('Proxy response bound exceeded')
                    if gate:
                        if gate['phase'] == 'after_response':
                            gates.wait(gate, response.status)
                        else:
                            gates.completed(gate, response.status)
                    self.send_response(response.status)
                    for name, value in response.getheaders():
                        if name.lower() not in ('connection', 'transfer-encoding', 'content-length'):
                            self.send_header(name, value)
                    self.send_header('content-length', str(len(payload)))
                    self.end_headers()
                    self.wfile.write(payload)
                except (BrokenPipeError, ConnectionResetError):
                    pass
                except GateRefused:
                    self.send_error(503)
                except Exception:
                    # Unsafe/malformed gate files never silently forward a gated request.
                    self.send_error(503 if gates else 502)
                finally:
                    if connection:
                        connection.close()
            do_GET = do_POST = do_HEAD = do_PUT = do_PATCH = do_DELETE = forward

        proxy = Server(('127.0.0.1', 0), Proxy)
        servers.append(proxy)
        authority = 'localhost:' + str(proxy.server_port)
        origin = 'https://' + authority
        gateway = Server(('127.0.0.1', 0), Gateway)
        servers.append(gateway)
        for listener in servers:
            listener.socket = tls.wrap_socket(listener.socket, server_side=True)
            thread = threading.Thread(target=listener.serve_forever, daemon=True)
            thread.start()
            started.append(listener)
        config['authMail'] = {'endpoint': f'https://127.0.0.1:{gateway.server_port}/mail',
                              'bearerToken': bearer, 'from': 'auth@example.invalid',
                              'idempotencySecret': secrets.token_urlsafe(32), 'trustedCaFile': str(ca)}
        runtime_config = state / 'server.json'
        write_new(runtime_config, config)
        server_log = (state / 'server.log').open('xb')
        # Reopen only. Native namespace admission owns the lock and refuses busy ledgers.
        child = subprocess.Popen([str(server), '--config', str(runtime_config), '--web-dir', str(web),
                                  '--listen', f'127.0.0.1:{backend_port}', '--origin', origin],
                                 stdin=subprocess.DEVNULL, stdout=server_log, stderr=server_log, umask=0o077)
        deadline = time.monotonic() + 30
        while True:
            if stop.is_set() or child.poll() is not None:
                raise Refused('Fixture stopped or native profile startup refused; no retry')
            try:
                connection = http.client.HTTPConnection('127.0.0.1', backend_port, timeout=.3)
                try:
                    connection.request('GET', '/api/health', headers={'host': authority})
                    response = connection.getresponse()
                    health = json.loads(response.read(65537))
                    if response.status == 200 and health.get('workspaceReady') is True and health.get('backend') == 'rust':
                        break
                finally:
                    connection.close()
            except (OSError, ValueError, http.client.HTTPException):
                pass
            if time.monotonic() >= deadline:
                raise Refused('Native readiness deadline; no restart')
            stop.wait(.1)
        # Exercise the proxy with explicit CA + hostname verification. Native OS
        # trust was independently checked above; Python may use OpenSSL roots.
        verified = ssl.create_default_context(cafile=str(ca))
        connection = http.client.HTTPSConnection('localhost', proxy.server_port, context=verified, timeout=5)
        try:
            connection.request('GET', '/api/health')
            response = connection.getresponse()
            health = json.loads(response.read(65537))
            if response.status != 200 or health.get('workspaceReady') is not True:
                raise Refused('Verified HTTPS proxy health refused')
        finally:
            connection.close()
        report.update(status='ready', origin=origin, serverPid=child.pid,
                      fixtureConfigPath=str(runtime_config), fixtureCertificateSha256=fingerprint(leaf))
        write_new(ready, report)
        print(json.dumps({'readyFile': str(ready), 'origin': origin}), flush=True)
        while not stop.wait(.25):
            if child.poll() is not None:
                raise Refused('Owned fixture server exited; no restart')
    finally:
        stop.set()
        if child is not None and child.poll() is None:
            child.terminate()
            try:
                child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait(timeout=5)
        for listener in started:
            listener.shutdown()
        for listener in servers:
            listener.server_close()
        if gates:
            gates.close()
        if server_log:
            server_log.close()
        log.close()
        preserved = fingerprint(source) == original_hash
        current_identity = private(source)
        preserved = preserved and (current_identity.st_dev, current_identity.st_ino) == (original_identity.st_dev, original_identity.st_ino)
        report.update(status='stopped', sourceConfigPreserved=preserved,
                      stoppedServerPid=child.pid if child else None)
        write_new(state / 'stopped.json', report)
        if not preserved:
            raise Refused('Original profile configuration changed; all fixture state preserved')


if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        # Never echo command output, configuration, mail tokens or exception data.
        print(json.dumps({'ok': False, 'error': str(error) if isinstance(error, Refused) else 'Private HTTPS fixture refused; inspect retained state'}), flush=True)
        raise SystemExit(1)
