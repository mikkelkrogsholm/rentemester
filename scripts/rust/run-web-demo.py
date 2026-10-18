#!/usr/bin/env python3
"""Run the actual Rust server with the existing cockpit and disposable synthetic data."""
import argparse
import base64
import json
import os
from pathlib import Path
import queue
import secrets
import shutil
import subprocess
import tempfile
import threading
import uuid

IMAGE = 'rust@sha256:cf9dd0ec73e75f827fe59123fff9dc65af1a1c8363c3c31ee8d7f8ad0b6a5fb2'
REPO = Path(__file__).resolve().parents[2]

def run(args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='Run the complete Chromium journey and clean up')
    parser.add_argument('--port', type=int, default=44321)
    parser.add_argument('--skip-web-build', action='store_true')
    args = parser.parse_args()
    if not 1 <= args.port <= 65535:
        parser.error('--port must be 1–65535')
    if not args.skip_web_build:
        run(['bun', 'run', 'build'], cwd=REPO/'app')
    prefix = 'rm-native-web-' + uuid.uuid4().hex[:10]
    volume = prefix + '-data'
    origin = f'http://127.0.0.1:{args.port}'
    server = None
    with tempfile.TemporaryDirectory(prefix=prefix) as temp:
        stage = Path(temp)/'source'; stage.mkdir()
        for name in ['Cargo.toml','Cargo.lock']:
            shutil.copy2(REPO/name,stage/name)
        for name in ['crates','vendor','app/dist']:
            shutil.copytree(REPO/name,stage/name)
        for name in ['src/core/migrations','src/core/workspace-migrations']:
            target = stage/name; target.mkdir(parents=True)
            for schema in (REPO/name).glob('*.json'):
                shutil.copy2(schema,target/schema.name)
        key = lambda: base64.urlsafe_b64encode(secrets.token_bytes(32)).decode().rstrip('=')
        config = {'root':'/qualified/parent/workspace','brokerUid':10001,'workspace':'synthetic-native-web',
                  'companySlug':'synthetic','companyName':'Synthetic Rust Web','cookieKey':key(),'mfaKey':key(),
                  'operatorActors':['system:synthetic-installer'],
                  'setupActor':'system:synthetic-installer',
                  'accounts':[{'accountNo':'1800','name':'Synthetic bank','type':'asset','normalBalance':'debit'},
                              {'accountNo':'8000','name':'Synthetic opening equity','type':'equity','normalBalance':'credit'},
                              {'accountNo':'3000','name':'Synthetic office supplies','type':'expense','normalBalance':'debit'}]}
        (stage/'synthetic.json').write_text(json.dumps(config))
        try:
            run(['docker','run','-d','--name',prefix,'--security-opt','no-new-privileges',
                 '-p',f'127.0.0.1:{args.port}:4319','--mount',f'type=bind,src={stage},dst=/work,readonly',
                 '--mount','type=volume,src=rm-rust-web-smoke-build,dst=/build',
                 '--mount',f'type=volume,src={volume},dst=/qualified','--workdir','/work',
                 '-e','CARGO_HOME=/build/cargo-home','-e','CARGO_TARGET_DIR=/build/target',IMAGE,'sleep','infinity'],stdout=subprocess.DEVNULL)
            run(['docker','exec',prefix,'cargo','build','--locked','-p','rentemester-server'])
            run(['docker','exec',prefix,'sh','-c','mkdir -p /qualified/parent && chown 10001:10001 /qualified/parent && chmod 700 /qualified/parent && chmod 755 /build /build/target /build/target/debug'])
            command = ['docker','exec',prefix,'setpriv','--bounding-set=-all','--reuid=10001',
                '--regid=10001','--clear-groups','--no-new-privs','sh','-c','umask 077; exec "$@"','rm-native-web',
                '/build/target/debug/rentemester-server','--config','/work/synthetic.json','--create',
                '--listen','0.0.0.0:4319','--origin',origin]
            server = subprocess.Popen(command,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
            output = queue.Queue()
            def read(process, messages):
                for line in process.stdout:
                    messages.put(line)
                messages.put(None)
            threading.Thread(target=read,args=(server,output),daemon=True).start()
            setup_link = None
            while True:
                line = output.get(timeout=90)
                if line is None:
                    raise RuntimeError('Rust server exited before readiness')
                if line.startswith('Workspace setup: '):
                    setup_link = line.strip().removeprefix('Workspace setup: ')
                    if args.check:
                        print('Private browser installer prepared',flush=True)
                        continue
                print(line.rstrip(),flush=True)
                if 'Native workspace connected: true' in line:
                    break
            if not setup_link: raise RuntimeError('Missing private installer link')
            print(f'Rust cockpit ready: {origin}',flush=True)
            if args.check:
                environment = {**os.environ,'RM_NATIVE_WEB_URL':origin,'RM_NATIVE_WEB_SETUP':setup_link,'RM_NATIVE_WEB_CONTAINER':prefix,'RM_NATIVE_WEB_STATE':str(Path(temp)/'browser-state.json')}
                run(['bun','run','scripts/rust/check-native-web.ts'],cwd=REPO,env=environment)
                for phase in ['0', '1', '2', '3']:
                    run(['docker','restart','-t','2',prefix],stdout=subprocess.DEVNULL)
                    server.wait(timeout=15)
                    if '--create' in command: command.remove('--create')
                    server = subprocess.Popen(command,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
                    output = queue.Queue()
                    threading.Thread(target=read,args=(server,output),daemon=True).start()
                    while True:
                        line = output.get(timeout=90)
                        if line is None: raise RuntimeError('Rust server did not reopen')
                        print(line.rstrip(),flush=True)
                        if 'Native workspace connected: true' in line: break
                    run(['bun','run','scripts/rust/check-native-web.ts'],cwd=REPO,env={**environment,'RM_NATIVE_WEB_REOPEN':'1','RM_NATIVE_WEB_EXPENSE_REOPEN':phase})
            else:
                print('Open the private setup link above to create your synthetic owner.',flush=True)
                print('After owner creation, read the local verification link in another terminal:',flush=True)
                print("docker exec "+prefix+" sh -c 'cat /qualified/parent/workspace/.rentemester/verification-*.txt'",flush=True)
                print('Press Ctrl-C to stop and remove this disposable workspace.',flush=True)
                server.wait()
        finally:
            subprocess.run(['docker','rm','-f',prefix],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
            if server:
                server.wait(timeout=15)
            subprocess.run(['docker','volume','rm',volume],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)

if __name__ == '__main__':
    try:
        main()
    except KeyboardInterrupt:
        pass
