#!/usr/bin/env python3
"""Qualify private owner close/retirement on synthetic persistent Linux files only."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import queue
import shutil
import subprocess
import tempfile
import threading
import time
import uuid

support_path = Path(__file__).with_name('verify-broker-namespace-linux.py')
spec = importlib.util.spec_from_file_location('namespace_support', support_path)
support = importlib.util.module_from_spec(spec)
spec.loader.exec_module(support)
run, IMAGE = support.run, support.IMAGE


class Hold:
    def __init__(self, args, marker):
        self.process = subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=subprocess.STDOUT, text=True, bufsize=1)
        self.queue, self.log = queue.Queue(), []
        def read():
            for line in self.process.stdout:
                self.queue.put(line)
            self.queue.put(None)
        threading.Thread(target=read, daemon=True).start()
        self.until(marker)

    def until(self, marker):
        deadline = time.monotonic() + 20
        while True:
            line = self.queue.get(timeout=max(.01, deadline - time.monotonic()))
            self.log.append(line)
            assert line is not None, (marker, self.log)
            if marker in line:
                return

    def send(self, command, marker):
        self.process.stdin.write(command + '\n')
        self.process.stdin.flush()
        self.until(marker)

    def finish(self):
        self.process.stdin.write('exit\n')
        self.process.stdin.flush()
        self.process.stdin.close()
        assert self.process.wait(timeout=20) == 0, self.log


def qualify(container, host_exe, runtime_exe):
    h = support.helpers(container, runtime_exe)
    checks = []
    def record(label):
        checks.append(label)
        print(label, flush=True)
    def command(root, extra=None, interactive=False, test='broker_probe'):
        args = ['docker', 'exec'] + (['-i'] if interactive else [])
        for key, value in {'RM_OWNER_ROOT': root, **(extra or {})}.items():
            args += ['-e', key + '=' + value]
        return args + [container, 'setpriv', '--bounding-set=-all', '--reuid=10001',
                       '--regid=10001', '--clear-groups', '--no-new-privs', 'sh', '-c',
                       'umask 077; exec "$@"', 'rm-owner', host_exe,
                       '--ignored', '--exact', 'tests::' + test, '--nocapture']
    def lock(root, name, expected):
        output = run(command(root, {'RM_OWNER_DATABASE': name}, test='contender'))
        assert 'RM_OWNER_LOCK=' + expected in output, output
    def both(root, expected):
        for name in ['.rentemester/workspace-control.sqlite', 'a/data/ledger.sqlite']:
            lock(root, name, expected)
    for mode in ['WAL', 'DELETE']:
        for leak in ['none', 'control', 'company']:
            label = mode + '-' + leak
            root = h.root_case(label)
            owner = Hold(command(root, {'RM_OWNER_MODE': mode, 'RM_OWNER_LEAK': leak}, True), 'RM_OWNER_OPEN=held')
            try:
                both(root, 'busy/busy'); record(label + '/both-held')
                owner.send('check', 'RM_OWNER_CHECK=false')
                both(root, 'busy/busy'); record(label + '/observation-keeps-locks')
                output = run(h.command(root, test='client', uid=10002))
                assert 'RM_BROKER_CLIENT_DENIED=66' in output, output
                record(label + '/66-different-UID-denials')
                if leak == 'none':
                    owner.send('commit', 'RM_OWNER_COMMIT=done')
                    owner.send('close', 'RM_OWNER_CLOSE=closed')
                    both(root, 'free/free'); record(label + '/confirmed-close-releases')
                    owner.finish()
                    # New process reopens the same named files and verifies committed rows.
                    output = run(command(root, test='reopen'))
                    assert 'RM_OWNER_REOPEN=2/2' in output, output
                    record(label + '/persistent-reopen')
                else:
                    owner.send('close', 'RM_OWNER_CLOSE=pending')
                    target = '.rentemester/workspace-control.sqlite' if leak == 'control' else 'a/data/ledger.sqlite'
                    other = 'a/data/ledger.sqlite' if leak == 'control' else '.rentemester/workspace-control.sqlite'
                    lock(root, target, 'busy/busy'); lock(root, other, 'busy/free')
                    record(label + '/BUSY-retains-exact-handle-and-entire-lease')
                    owner.send('retry', 'RM_OWNER_RETRY=pending')
                    lock(root, target, 'busy/busy'); record(label + '/retry-cannot-release')
                    owner.send('drop', 'RM_OWNER_DROP=done')
                    lock(root, target, 'busy/busy'); record(label + '/Drop-quarantines')
                    owner.finish()
                    both(root, 'free/free'); record(label + '/process-exit-releases')
            finally:
                if owner.process.poll() is None:
                    owner.finish()
        root = h.root_case(mode + '-alias')
        owner = Hold(command(root, {'RM_OWNER_MODE': mode}, True), 'RM_OWNER_OPEN=held')
        try:
            alias = root + '/a/data/ledger.sqlite' + ('-journal' if mode == 'WAL' else '-shm')
            run(['docker', 'exec', container, 'ln', root + '/a/data/ledger.sqlite', alias])
            owner.send('check', 'RM_OWNER_CHECK=true'); both(root, 'busy/busy')
            record(mode + '/namespace-invalidation-keeps-locks')
            run(['docker', 'exec', container, 'rm', alias])
            owner.send('check', 'RM_OWNER_CHECK=true'); both(root, 'busy/busy')
            record(mode + '/restoration-cannot-revive')
            owner.finish(); both(root, 'free/free')
        finally:
            if owner.process.poll() is None:
                owner.finish()
        root = h.root_case(mode + '-panic')
        owner = Hold(command(root, {'RM_OWNER_MODE': mode}, True), 'RM_OWNER_OPEN=held')
        try:
            owner.send('panic', 'RM_OWNER_PANIC=retired')
            owner.send('check', 'RM_OWNER_CHECK=true'); both(root, 'busy/busy')
            record(mode + '/dispatch-panic-retires')
            owner.finish(); both(root, 'free/free')
        finally:
            if owner.process.poll() is None:
                owner.finish()
    for fault in ['clean', 'busy', 'panic']:
        root = h.root_case('constructor-' + fault)
        owner = Hold(command(root, {'RM_OWNER_CONSTRUCTOR_FAULT': fault}, True),
                     'RM_OWNER_OPEN=' + ('panic' if fault == 'panic' else 'broker_database_refused'))
        try:
            lock(root, '.rentemester/workspace-control.sqlite', 'free/free' if fault == 'clean' else 'busy/busy')
            lock(root, 'a/data/ledger.sqlite', 'free/free' if fault == 'clean' else 'busy/free')
            record('constructor-' + fault + '/partial-open-cleanup')
            owner.finish(); both(root, 'free/free')
            record('constructor-' + fault + '/exit-releases')
        finally:
            if owner.process.poll() is None:
                owner.finish()
    return checks


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        raise SystemExit('Refusing to overwrite qualification evidence')
    repo = Path(__file__).resolve().parents[2]
    prefix = 'rm-owner-' + uuid.uuid4().hex[:12]
    build_volume, data_volume = prefix + '-build', prefix + '-data'
    with tempfile.TemporaryDirectory(prefix=prefix) as temp:
        source = Path(temp) / 'source'; source.mkdir()
        for name in ['Cargo.toml', 'Cargo.lock']:
            shutil.copyfile(repo / name, source / name)
        for name in ['crates', 'vendor']:
            shutil.copytree(repo / name, source / name)
        # Storage embeds these general product schemas at compile time. No workspace data.
        for name in ['src/core/migrations', 'src/core/workspace-migrations']:
            destination = source / name
            destination.mkdir(parents=True)
            for schema in sorted((repo / name).glob('*.json')):
                shutil.copyfile(schema, destination / schema.name)
        hashes = {str(p.relative_to(source)): hashlib.sha256(p.read_bytes()).hexdigest()
                  for p in sorted(source.rglob('*')) if p.is_file()}
        image = json.loads(run(['docker', 'image', 'inspect', IMAGE]))[0]
        try:
            run(['docker', 'run', '-d', '--name', prefix, '--security-opt', 'no-new-privileges',
                 '--mount', 'type=volume,src=' + data_volume + ',dst=/qualified',
                 '--mount', 'type=volume,src=' + build_volume + ',dst=/build',
                 '--mount', 'type=bind,src=' + str(source) + ',dst=/work,readonly',
                 '--workdir', '/work', '--env', 'CARGO_HOME=/build/cargo-home',
                 '--env', 'CARGO_TARGET_DIR=/build/target', IMAGE, 'sleep', 'infinity'])
            build = run(['docker', 'exec', prefix, 'cargo', 'test', '--locked', '-p',
                         'rentemester-runtime', '-p', 'rentemester-host', '--no-run', '--message-format=json'])
            artifacts = {}
            for line in build.splitlines():
                try:
                    row = json.loads(line)
                except ValueError:
                    continue
                if row.get('executable'):
                    artifacts[row['target']['name']] = row['executable']
            checks = qualify(prefix, artifacts['rentemester_host'], artifacts['rentemester_runtime'])
            assert all(hashlib.sha256((repo / p).read_bytes()).hexdigest() == digest
                       for p, digest in hashes.items()), 'Source changed during qualification'
            report = {'ok': True, 'checks': len(checks), 'observations': checks,
                      'image_repository_digest': IMAGE, 'image_configuration_digest': image['Id'],
                      'architecture': image['Architecture'],
                      'filesystem': run(['docker', 'exec', prefix, 'stat', '-f', '-c', '%T', '/qualified']).strip(),
                      'source_files_sha256': hashes,
                      'driver_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                      'support_sha256': hashlib.sha256(support_path.read_bytes()).hexdigest(),
                      'build_log_sha256': hashlib.sha256(build.encode()).hexdigest(),
                      'limits': ['Synthetic physical-owner qualification; no canonical bootstrap/adoption or deployment',
                                 'Fixed typed dispatch compiles; complete domain/transport composition remains unqualified',
                                 'Root and enrolled broker UID trusted; prior descriptors require offline provisioning',
                                 'macOS local namespace profile remains to be implemented']}
        finally:
            support.cleanup(prefix, build_volume, data_volume)
        report['owned_resources_removed'] = True
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps({'ok': True, 'checks': len(checks), 'cleanup_verified': True}), flush=True)


if __name__ == '__main__':
    main()
