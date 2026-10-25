#!/usr/bin/env python3
"""Qualify an already-built Rust candidate with disposable Linux data and headless web UI."""
import argparse
import base64
import contextlib
import hashlib
import json
import os
from pathlib import Path
import secrets
import socket
import subprocess
import tempfile
import time
import urllib.request
import uuid

REPO = Path(__file__).resolve().parents[2]


def command(args, **kwargs):
    return subprocess.run(args, check=True, capture_output=True, **kwargs).stdout


def synthetic_pdf():
    stream = b'BT /F1 18 Tf 72 720 Td (Synthetic packaged PDF) Tj ET\n'
    objects = [b'<< /Type /Catalog /Pages 2 0 R >>',
               b'<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
               b'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>',
               b'<< /Length ' + str(len(stream)).encode() + b' >>\nstream\n' + stream + b'endstream',
               b'<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>']
    data = b'%PDF-1.4\n'; offsets = []
    for index, body in enumerate(objects, 1):
        offsets.append(len(data))
        data += str(index).encode() + b' 0 obj\n' + body + b'\nendobj\n'
    xref = len(data)
    data += b'xref\n0 6\n0000000000 65535 f \n'
    data += b''.join(f'{offset:010} 00000 n \n'.encode() for offset in offsets)
    return data + b'trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n' + str(xref).encode() + b'\n%%EOF\n'


def main():
    os.umask(0o077)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--image', required=True)
    parser.add_argument('--evidence', type=Path, required=True)
    parser.add_argument('--backup-recovery', action='store_true',
                        help='qualify signed backup into a fresh separate Linux target; preserve failed fixtures')
    args = parser.parse_args()
    if args.evidence.exists():
        parser.error('evidence must be a new file')
    assets = Path(str(args.evidence) + '.assets')
    assets.mkdir(mode=0o700)
    image = json.loads(command(['docker', 'image', 'inspect', args.image]))[0]
    image_id = image['Id']  # Every subsequent operation uses these exact bytes.
    assert image['Config']['Labels']['org.rentemester.runtime'] == 'rust'
    assert image['Config']['User'] == '10001:10001'
    prefix = 'rm-native-web-' + uuid.uuid4().hex[:12]
    volumes = [prefix + '-state', prefix + '-config']
    report = {'status': 'running', 'imageId': image_id, 'architecture': image['Architecture'],
              'imageLabels': image['Config']['Labels'], 'checks': {},
              'limits': ['Loopback HTTP candidate; HTTPS proxy/provider/release qualification remains open',
                         'Packaged CLI/MCP help only; their authenticated business parity is not claimed here',
                         'No installed desktop GUI or production deployment',
                         'No independent no-cache reproducibility or registry attestation qualification']}
    checks = report['checks']
    owned_containers = [prefix]
    server_created = False
    failure = None
    try:
        private_context = (contextlib.nullcontext(tempfile.mkdtemp(prefix=prefix)) if args.backup_recovery
                           else tempfile.TemporaryDirectory(prefix=prefix))
        with private_context as private:
            private = Path(private)
            with socket.socket() as listener:
                listener.bind(('127.0.0.1', 0)); port = listener.getsockname()[1]
            origin = f'http://127.0.0.1:{port}'
            key = lambda: base64.urlsafe_b64encode(secrets.token_bytes(32)).decode().rstrip('=')
            config = {'root': '/state/workspace', 'brokerUid': 10001,
                      'runtimeProfile': 'dedicated-linux', 'workspace': 'synthetic-container',
                      'companySlug': 'synthetic', 'companyName': 'Synthetic Rust Web',
                      'cookieKey': key(), 'mfaKey': key(),
                      'operatorActors': ['system:synthetic-installer'],
                      'setupActor': 'system:synthetic-installer',
                      'documentPdfWorker': {'bundleDir': '/opt/rentemester/bin'},
                      'accounts': [{'accountNo': '1800', 'name': 'Synthetic bank', 'type': 'asset', 'normalBalance': 'debit'},
                                   {'accountNo': '8000', 'name': 'Synthetic opening equity', 'type': 'equity', 'normalBalance': 'credit'},
                                   {'accountNo': '3000', 'name': 'Synthetic supplies', 'type': 'expense', 'normalBalance': 'debit'}]}
            recovery_env = {**os.environ, 'RM_CONTAINER_RECOVERY_DIR': str(private)}
            helper = ['bun', 'run', 'scripts/rust/check-native-container-restore.ts']
            if args.backup_recovery:
                checks['backupRecovery'] = {'status': 'running', 'fixturePrefix': prefix,
                                            'privateFixture': str(private)}
                subprocess.run(helper + ['keys'], cwd=REPO, env=recovery_env, check=True, timeout=30)
                config['backupSigningKeyFile'] = '/config/signing.der'
            for volume in volumes:
                command(['docker', 'volume', 'create', volume])
            # Explicit fixture operator initialization, separate from product startup.
            command(['docker', 'run', '--rm', '-i', '--network', 'none', '--user', '0:0',
                     '--mount', f'type=volume,src={volumes[0]},dst=/state',
                     '--mount', f'type=volume,src={volumes[1]},dst=/config',
                     '--entrypoint', 'sh', image_id, '-c',
                     'umask 077; cat > /config/server.json; chown 10001:10001 /state /config /config/server.json; chmod 0700 /state /config; chmod 0600 /config/server.json'],
                    input=json.dumps(config).encode())
            if args.backup_recovery:
                command(['docker', 'run', '--rm', '-i', '--network', 'none', '--user', '0:0',
                         '--mount', f'type=volume,src={volumes[1]},dst=/config',
                         '--entrypoint', 'sh', image_id, '-c',
                         'umask 077; cat > /config/signing.der; chown 10001:10001 /config/signing.der; chmod 0600 /config/signing.der'],
                        input=(private / 'signing.der').read_bytes())
            base = ['docker', 'run', '--detach', '--name', prefix, '--read-only',
                    '--memory', '1g', '--pids-limit', '128', '--cap-drop', 'ALL',
                    '--security-opt', 'no-new-privileges:true',
                    '--tmpfs', '/tmp:rw,noexec,nosuid,size=64m',
                    '--publish', f'127.0.0.1:{port}:4319',
                    '--env', 'RENTEMESTER_ORIGIN=' + origin,
                    '--mount', f'type=volume,src={volumes[0]},dst=/state',
                    '--mount', f'type=volume,src={volumes[1]},dst=/config,readonly']

            def start(create, restore_flags=None):
                nonlocal server_created
                command(base + [image_id] + (restore_flags or (['--create'] if create else [])))
                server_created = True
                opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
                for _ in range(150):
                    state = json.loads(command(['docker', 'inspect', prefix]))[0]['State']
                    assert state['Running'], 'candidate exited; inspect private synthetic container logs'
                    try:
                        with opener.open(origin + '/api/health', timeout=1) as response:
                            health = json.load(response)
                        if health.get('workspaceReady') is True:
                            return health
                    except (OSError, ValueError):
                        pass
                    time.sleep(0.1)
                raise RuntimeError('candidate did not connect workspace')

            def stop():
                nonlocal server_created
                command(['docker', 'stop', '--time', '60', prefix], timeout=75)
                state = json.loads(command(['docker', 'inspect', prefix]))[0]['State']
                assert state['ExitCode'] == 0 and not state['OOMKilled'], state
                command(['docker', 'rm', prefix]); server_created = False
                return state['ExitCode']

            checks['freshHealth'] = start(True)
            logs = command(['docker', 'logs', prefix], text=True)
            setup = next(line.removeprefix('Workspace setup: ') for line in logs.splitlines() if line.startswith('Workspace setup: '))
            profile = command(['docker', 'exec', prefix, 'sh', '-c',
                               'id -u; cat /proc/1/status; test ! -w /opt/rentemester/web/index.html; for tool in bun node python3 cargo rustc; do ! command -v "$tool" || exit 1; done'], text=True)
            assert profile.startswith('10001\n')
            assert 'NoNewPrivs:\t1' in profile
            assert 'Umask:\t0077' in profile
            for field in ['CapInh', 'CapPrm', 'CapEff', 'CapBnd', 'CapAmb']:
                assert f'{field}:\t0000000000000000' in profile
            checks['dedicatedNonrootReadOnlyNoCapabilitiesNoJsRuntime'] = True
            for name in ['rentemester-rust', 'rentemester-mcp']:
                command(['docker', 'exec', prefix, '/opt/rentemester/bin/' + name, '--help'])
            checks['packagedClientHelp'] = True
            pdf = synthetic_pdf()
            worker = command(['docker', 'run', '--rm', '-i', '--network', 'none', '--read-only',
                              '--memory', '512m', '--pids-limit', '64', '--cap-drop', 'ALL',
                              '--security-opt', 'no-new-privileges:true',
                              '--entrypoint', '/opt/rentemester/bin/rentemester-pdf-worker', image_id],
                             input=pdf, timeout=25)
            parsed = json.loads(worker)
            assert parsed['status'] == 'ok' and parsed['pages'][0]['text'] == 'Synthetic packaged PDF', parsed
            assert parsed['inputSha256'] == hashlib.sha256(pdf).hexdigest()
            checks['packagedPdfParse'] = {'status': parsed['status'], 'inputSha256': parsed['inputSha256'], 'provenance': parsed['provenance']}
            env = {k: v for k, v in os.environ.items() if not k.startswith('RM_NATIVE_')}
            env.update(RM_NATIVE_WEB_URL=origin, RM_NATIVE_WEB_SETUP=setup,
                       RM_NATIVE_WEB_CONTAINER=prefix, RM_NATIVE_WEB_STATE=str(private / 'state.json'),
                       RM_NATIVE_WEB_WORKSPACE_ROOT='/state/workspace', RM_NATIVE_WEB_PACKAGING='1',
                       RM_NATIVE_REPORT_DIR=str(assets / 'reports'),
                       RM_NATIVE_WEB_SCREENSHOT=str(assets / 'first.png'),
                       RM_NATIVE_DOCUMENT_SCREENSHOT=str(assets / 'first-original.png'))
            subprocess.run(['bun', 'run', 'scripts/rust/check-native-web.ts'], cwd=REPO, env=env, check=True, timeout=300)
            checks['normalOwnerMfaPost125_50BalancedReportsExactReceipt'] = True
            checks['firstStopExit'] = stop()
            checks['reopenHealth'] = start(False)
            subprocess.run(['bun', 'run', 'scripts/rust/check-native-web.ts'], cwd=REPO,
                           env={**env, 'RM_NATIVE_WEB_REOPEN': '1',
                                'RM_NATIVE_WEB_SCREENSHOT': str(assets / 'reopen.png'),
                                'RM_NATIVE_DOCUMENT_SCREENSHOT': str(assets / 'reopen-original.png')}, check=True, timeout=300)
            checks['normalMfaReopenPersistedJournalOriginalIdempotentReplay'] = True
            if args.backup_recovery:
                recovery_env.update(RM_NATIVE_WEB_URL=origin, RM_NATIVE_WEB_STATE=str(private / 'state.json'))
                subprocess.run(helper + ['source'], cwd=REPO, env=recovery_env, check=True, timeout=180)
                proof = checks['backupRecovery']
                for name, file in [('archiveSha256', 'signed.native-backup.json'), ('trustedKeySha256', 'trusted.pub'),
                                   ('sourceCheckpointSha256', 'checkpoint.json')]:
                    proof[name] = hashlib.sha256((private / file).read_bytes()).hexdigest()
                proof['helperSha256'] = hashlib.sha256((REPO / 'scripts/rust/check-native-container-restore.ts').read_bytes()).hexdigest()
                proof['imageId'] = image_id
                build_identity = command(['docker', 'exec', prefix, 'cat', '/opt/rentemester/build-identity.json'])
                proof['packagedBuildIdentitySha256'] = hashlib.sha256(build_identity).hexdigest()
                proof['sourceRevision'] = image['Config']['Labels'].get('org.opencontainers.image.revision')
                # Copy exact frozen archive and independently established public key to an owned volume.
                recovery_volume = prefix + '-archive'
                volumes.append(recovery_volume)
                command(['docker', 'volume', 'create', recovery_volume])
                for filename in ['signed.native-backup.json', 'trusted.pub']:
                    command(['docker', 'run', '--rm', '-i', '--network', 'none', '--user', '0:0',
                             '--mount', f'type=volume,src={recovery_volume},dst=/recovery',
                             '--entrypoint', 'sh', image_id, '-c',
                             'umask 077; cat > /recovery/"$1"; chown 10001:10001 /recovery /recovery/"$1"; chmod 0700 /recovery; chmod 0600 /recovery/"$1"',
                             'copy', filename], input=(private / filename).read_bytes())
                verified = command(['docker', 'run', '--rm', '--network', 'none', '--read-only',
                                    '--memory', '1g', '--pids-limit', '128', '--cap-drop', 'ALL',
                                    '--security-opt', 'no-new-privileges:true',
                                    '--mount', f'type=volume,src={recovery_volume},dst=/recovery,readonly',
                                    '--entrypoint', 'sh', image_id, '-c',
                                    'umask 077; exec /opt/rentemester/bin/rentemester-server "$@"', 'verify',
                                    '--verify-backup', '/recovery/signed.native-backup.json',
                                    '--trusted-backup-key', '/recovery/trusted.pub'], timeout=120)
                proof['independentPackagedVerification'] = json.loads(verified)
                assert proof['independentPackagedVerification']['ok'] is True
                # No live source writer exists before declaring source-unavailable.
                proof['sourceStopExit'] = stop()
                source_prefix, source_state, source_config = prefix, volumes[0], volumes[1]
                prefix += '-restore'
                owned_containers.append(prefix)
                target_state, target_config = prefix + '-state', prefix + '-config'
                volumes.extend([target_state, target_config])
                for volume in [target_state, target_config]:
                    command(['docker', 'volume', 'create', volume])
                target = {**config, 'root': '/state/restored-workspace', 'cookieKey': key(), 'mfaKey': key(),
                          'restoreActor': 'system:synthetic-installer',
                          'firstOwner': {'actor': 'system:synthetic-installer', 'email': 'restored-owner@example.invalid',
                                         'name': 'Synthetic Restored Owner', 'password': secrets.token_urlsafe(24)}}
                target.pop('setupActor', None)
                target.pop('backupSigningKeyFile', None)
                (private / 'target-owner.json').write_text(json.dumps(target['firstOwner']))
                command(['docker', 'run', '--rm', '-i', '--network', 'none', '--user', '0:0',
                         '--mount', f'type=volume,src={target_state},dst=/state',
                         '--mount', f'type=volume,src={target_config},dst=/config',
                         '--entrypoint', 'sh', image_id, '-c',
                         'umask 077; test ! -e /state/restored-workspace; cat > /config/server.json; chown 10001:10001 /state /config /config/server.json; chmod 0700 /state /config; chmod 0600 /config/server.json'],
                        input=json.dumps(target).encode())
                base = [part.replace(source_prefix, prefix).replace(source_state, target_state).replace(source_config, target_config)
                        for part in base]
                base += ['--mount', f'type=volume,src={recovery_volume},dst=/recovery,readonly']
                proof['freshTargetHealth'] = start(False, ['--restore-backup', '/recovery/signed.native-backup.json',
                    '--trusted-backup-key', '/recovery/trusted.pub', '--confirm-target', target['root'], '--source-unavailable'])
                verification = command(['docker', 'exec', prefix, 'sh', '-c',
                    'for f in /state/restored-workspace/.rentemester/verification-*.txt; do cat "$f"; break; done'], text=True)
                subprocess.run(helper + ['restored'], cwd=REPO,
                               env={**recovery_env, 'RM_CONTAINER_RECOVERY_VERIFY': verification}, check=True, timeout=180)
                proof.update(json.loads((private / 'restored-proof.json').read_text()))
                proof['targetStopExit'] = stop()
                proof['status'] = 'passed'
            else:
                checks['secondStopExit'] = stop()
            report['status'] = 'passed'
    except BaseException as error:
        failure = error
        report['status'] = 'failed'
        report['failureType'] = type(error).__name__
    finally:
        preserve = args.backup_recovery and failure is not None
        if preserve:
            checks.setdefault('backupRecovery', {})['status'] = 'failed'
            report['preservedOwnedFixtures'] = {'containers': owned_containers, 'volumes': volumes}
            report['ownedContainerAndVolumesRemoved'] = False
        else:
            for container in owned_containers:
                subprocess.run(['docker', 'rm', '--force', container], capture_output=True)
            for volume in volumes:
                subprocess.run(['docker', 'volume', 'rm', volume], capture_output=True)
            for container in owned_containers:
                assert subprocess.run(['docker', 'inspect', container], capture_output=True).returncode != 0
            for volume in volumes:
                assert subprocess.run(['docker', 'volume', 'inspect', volume], capture_output=True).returncode != 0
            report['ownedContainerAndVolumesRemoved'] = True
            if args.backup_recovery:
                # Only this invocation's tagged resources; private recovery material is retained as evidence.
                checks['backupRecovery']['terminalCleanup'] = True
        report['runnerSha256'] = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
        report['screenshotsSha256'] = {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in assets.glob('*.png')}
        args.evidence.write_text(json.dumps(report, indent=2) + '\n')
    if failure:
        raise failure
    print(json.dumps({'status': report['status'], 'evidence': str(args.evidence), 'imageId': image_id}))


if __name__ == '__main__':
    main()
