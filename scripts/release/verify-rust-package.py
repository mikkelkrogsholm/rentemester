#!/usr/bin/env python3
"""Verify actual Rust image bytes and live metadata without extracting its archive."""
import argparse
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import subprocess
import tarfile


def sha(data):
    return hashlib.sha256(data).hexdigest()


def package_inventory(archive):
    payloads, rows = {}, []
    with tarfile.open(fileobj=io.BytesIO(archive), mode='r:') as stream:
        for member in stream:
            path = PurePosixPath(member.name)
            if path.is_absolute() or '..' in path.parts:
                raise ValueError('unsafe archive path')
            if member.isdir():
                continue
            name = path.as_posix()
            if not member.isfile() or name in payloads:
                raise ValueError('nonregular or duplicate archive member')
            if member.mode & 0o7777 not in (0o444, 0o555):
                raise ValueError('mutable or unexpected runtime mode')
            body = stream.extractfile(member).read()
            payloads[name] = body
            if name != 'build-identity.json':
                rows.append(dict(path=name, size=len(body), sha256=sha(body), executable=bool(member.mode & 0o111)))
    identity_raw = payloads['build-identity.json']
    identity = json.loads(identity_raw)
    if identity.get('format') != 'rentemester-rust-build/1' or sorted(rows, key=lambda row: row['path']) != identity['files']:
        raise ValueError('packaged inventory differs from identity')
    return identity_raw, identity, payloads


def command(args):
    return subprocess.run(args, check=True, capture_output=True, timeout=120).stdout


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--image', required=True)
    parser.add_argument('--identity', type=Path, required=True)
    parser.add_argument('--evidence', type=Path, required=True)
    parser.add_argument('--commit', required=True)
    parser.add_argument('--version', required=True)
    parser.add_argument('--require-clean', action='store_true')
    args = parser.parse_args()
    if args.identity.exists() or args.evidence.exists() or args.identity == args.evidence:
        parser.error('identity and evidence must be distinct new files')
    image = json.loads(command(['docker', 'image', 'inspect', args.image]))[0]
    image_id = image['Id']
    labels = image['Config']['Labels']
    base = ['docker', 'run', '--rm', '--network', 'none', '--read-only', '--cap-drop', 'ALL',
            '--security-opt', 'no-new-privileges:true', '--pids-limit', '64', '--memory', '512m']
    def run(entrypoint, *arguments):
        return command(base + ['--entrypoint', entrypoint, image_id, *arguments])
    raw, identity, payloads = package_inventory(run('tar', '-C', '/opt/rentemester', '-cf', '-', '.'))
    product = identity['product']
    if product['version'] != args.version or product['gitCommit'] != args.commit:
        raise ValueError('package does not match requested source/version')
    if (labels.get('org.rentemester.runtime') != 'rust' or image['Config']['User'] != '10001:10001'
            or labels.get('org.opencontainers.image.version') != product['version']
            or labels.get('org.opencontainers.image.revision') != product['gitCommit']
            or type(product['sourceDirty']) is not bool
            or labels.get('org.rentemester.source.dirty') != str(product['sourceDirty']).lower()):
        raise ValueError('image labels/profile differ from package identity')
    if args.require_clean and product['sourceDirty']:
        raise ValueError('dirty source cannot qualify a release')
    targets = {'arm64': ('linux-aarch64', 'aarch64-unknown-linux-gnu'),
               'amd64': ('linux-x86_64', 'x86_64-unknown-linux-gnu')}
    platform, target = targets[image['Architecture']]
    if identity['toolchain']['target'] != target:
        raise ValueError('image architecture differs from compiler target')
    for binary, flag, filename in [('rentemester-server', '--build-info', 'native-server.json'),
                                   ('rentemester-mcp', '--catalogue', 'native-mcp-catalogue.json')]:
        live = json.loads(run('/opt/rentemester/bin/' + binary, flag))
        if live != json.loads(payloads[filename]) or live['version'] != product['version']:
            raise ValueError('live binary metadata differs from packaged identity')
    server = json.loads(payloads['native-server.json'])
    if (server['format'] != 'rentemester-native-server/1' or server['platform'] != platform
            or {key: server[key] for key in ['apiEpoch', 'controlSchema', 'companySchema']} != identity['schemas']):
        raise ValueError('compiled policy differs from package identity')
    mcp = json.loads(payloads['native-mcp-catalogue.json'])
    if (mcp['format'] != 'rentemester-native-mcp-catalogue/1' or not mcp['tools']
            or len({tool['name'] for tool in mcp['tools']}) != len(mcp['tools'])):
        raise ValueError('invalid live MCP registry')
    entrypoint = run('cat', '/usr/local/bin/rentemester-entrypoint')
    if sha(entrypoint) != identity['inputs']['entrypointSha256']:
        raise ValueError('entrypoint differs from package identity')
    report = dict(status='passed', imageId=image_id, architecture=image['Architecture'],
                  identityRawSha256=sha(raw), version=product['version'], gitCommit=product['gitCommit'],
                  sourceDirty=product['sourceDirty'], fileCount=len(identity['files']), mcpToolCount=len(mcp['tools']),
                  checks=dict(exactInventoryAndModes=True, liveBinaryMetadata=True, entrypointHash=True),
                  limits=['Interface identity is not full CLI/MCP/HTTP parity',
                          'No SBOM, registry attestation, promotion or desktop GUI acceptance'])
    # Exclusive creation prevents replacing previously collected evidence.
    with args.identity.open('xb') as output:
        output.write(raw)
    with args.evidence.open('x') as output:
        output.write(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
