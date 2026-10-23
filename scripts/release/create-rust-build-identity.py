#!/usr/bin/env python3
"""Build-time inventory of the actual staged Rust runtime; no workspace or credentials."""
import argparse
import datetime
import hashlib
import json
from pathlib import Path
import re
import stat
import tomllib


def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(chunk)
    return h.hexdigest()


def inventory(root):
    rows = []
    for path in sorted(root.rglob('*')):
        info = path.lstat()
        if stat.S_ISDIR(info.st_mode):
            continue
        if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
            raise ValueError('inventory requires ordinary independent files')
        rows.append({'path': path.relative_to(root).as_posix(), 'size': info.st_size,
                     'sha256': digest(path), 'executable': bool(info.st_mode & 0o111)})
    return rows


def verify_pdf_licenses(source, pdfium, artifact):
    inventory_path = source / 'scripts/rust/pdf-worker/license-inventory.json'
    approved = json.loads((source / 'scripts/release/pdfium-license-approval.json').read_text())
    if (approved.get('format') != 'rentemester-pdfium-license-approval/1'
            or approved.get('dependencyApproved') is not True
            or approved.get('inventorySha256') != digest(inventory_path)):
        raise ValueError('PDF license inventory lacks exact reviewed policy approval')
    notices = json.loads(inventory_path.read_text())
    if (notices.get('format') != 'rentemester-pdfium-license-inventory/1'
            or notices.get('policyApprovalClaim') is not False
            or notices.get('pinsSha256') != digest(source / 'scripts/rust/pdf-worker/artifacts.json')
            or approved.get('pinsSha256') != notices['pinsSha256']):
        raise ValueError('PDF license inventory/source pins mismatch')
    rows = [r for r in notices['artifacts'] if r['archive'] == artifact['archive']]
    if len(rows) != 1 or any(rows[0].get(k) != artifact[v] for k, v in [
            ('archiveSha256', 'sha256'), ('library', 'library'), ('librarySha256', 'librarySha256')]):
        raise ValueError('PDF license inventory artifact mismatch')
    expected = {}
    for row in rows[0]['notices']:
        path = Path(row['path'])
        if (path.is_absolute() or '..' in path.parts or path.as_posix() != row['path']
                or not (row['path'] == 'LICENSE' or row['path'].startswith('licenses/'))
                or row['path'] in expected or not re.fullmatch('[0-9a-f]{64}', row['sha256'])
                or type(row['size']) is not int or row['size'] <= 0):
            raise ValueError('PDF license inventory contains invalid notice')
        expected[row['path']] = row
    actual = {'LICENSE'} | {p.relative_to(pdfium).as_posix()
                            for p in (pdfium / 'licenses').rglob('*') if not p.is_dir()}
    if 'LICENSE' not in expected or len(expected) < 2 or actual != set(expected):
        raise ValueError('PDF licenses must be retained: exact complete notice set required')
    for name, row in expected.items():
        path = pdfium / name
        if (not path.is_file() or path.is_symlink() or path.stat().st_size != row['size']
                or digest(path) != row['sha256']):
            raise ValueError('PDF license content differs from reviewed pinned notice')


def create(source, bundle, commit, epoch, dirty, images, version):
    if not re.fullmatch('[0-9a-f]{40}', commit):
        raise ValueError('full source commit required')
    if not isinstance(epoch, int) or isinstance(epoch, bool) or epoch <= 0:
        raise ValueError('positive source epoch required')
    for name in ['rustBuilder', 'webBuilder', 'runtime']:
        if not re.fullmatch(r'[^\s@]+@sha256:[0-9a-f]{64}', images[name]):
            raise ValueError('digest-pinned base images required')
    workspace = tomllib.loads((source / 'Cargo.toml').read_text())['workspace']['package']
    product = json.loads((source / 'package.json').read_text())['version']
    web = json.loads((source / 'app/package.json').read_text())['version']
    if workspace['version'] != product or web != product or version != product:
        raise ValueError('Rust/product/web version drift')
    for name in ['bin/rentemester-server', 'bin/rentemester-rust', 'bin/rentemester-mcp',
                 'bin/rentemester-pdf-worker', 'web/index.html', 'debian-packages.txt']:
        path = bundle / name
        if path.is_symlink() or not path.is_file() or (name.startswith('bin/') and not path.stat().st_mode & 0o111):
            raise ValueError('required runtime file missing or not executable')
    server = json.loads((bundle / 'native-server.json').read_text())
    mcp = json.loads((bundle / 'native-mcp-catalogue.json').read_text())
    if server['format'] != 'rentemester-native-server/1' or server['version'] != product:
        raise ValueError('compiled server version mismatch')
    if mcp['format'] != 'rentemester-native-mcp-catalogue/1' or mcp['version'] != product:
        raise ValueError('compiled MCP version mismatch')
    if not isinstance(mcp['tools'], list) or not mcp['tools']:
        raise ValueError('actual MCP registry required')
    targets = {'linux-aarch64': 'aarch64-unknown-linux-gnu', 'linux-x86_64': 'x86_64-unknown-linux-gnu'}
    target = targets[server['platform']]
    rustc = (bundle / 'bin/rustc.txt').read_text().strip()
    cargo = (bundle / 'bin/cargo.txt').read_text().strip()
    if not rustc.startswith('rustc ' + workspace['rust-version'] + ' ') or f'host: {target}' not in rustc.splitlines():
        raise ValueError('actual compiler identity mismatch')
    if not re.fullmatch(r'cargo [0-9]+\.[0-9]+\.[0-9]+ \([^\n]+\)', cargo):
        raise ValueError('actual Cargo identity required')
    schemas = {key: server[key] for key in ['apiEpoch', 'controlSchema', 'companySchema']}
    if any(not isinstance(v, int) or isinstance(v, bool) or v < 1 for v in schemas.values()):
        raise ValueError('compiled schema policy required')
    pins = json.loads((source / 'scripts/rust/pdf-worker/artifacts.json').read_text())
    artifact = json.loads((bundle / 'bin/pdfium/artifact.json').read_text())
    name = 'pdfium-linux-arm64.tgz' if server['platform'] == 'linux-aarch64' else 'pdfium-linux-x64.tgz'
    expected = next(p for p in pins['artifacts'] if p['archive'] == name)
    if artifact != expected or digest(bundle / 'bin/pdfium' / artifact['library']) != expected['librarySha256']:
        raise ValueError('PDF artifact/library does not match source pins')
    license_file = bundle / 'bin/pdfium/LICENSE'
    if not license_file.is_file() or license_file.stat().st_size == 0 or not any(
            p.is_file() and p.stat().st_size > 0 for p in (bundle / 'bin/pdfium/licenses').rglob('*')):
        raise ValueError('PDF licenses must be retained')
    verify_pdf_licenses(source, bundle / 'bin/pdfium', artifact)
    credits = bundle / 'THIRD-PARTY-NOTICES.txt'
    if (not credits.is_file() or credits.is_symlink()
            or digest(credits) != digest(source / 'scripts/release/rust-third-party-notices.txt')):
        raise ValueError('required third-party credits differ from source')
    output = bundle / 'build-identity.json'
    if output.exists():
        raise ValueError('identity must not overwrite an existing build')
    # Normalize only the caller-owned staging tree. Inventory describes the exact
    # modes copied to the final image; there is no runtime permission repair.
    for path in bundle.rglob('*'):
        info = path.lstat()
        if stat.S_ISDIR(info.st_mode):
            path.chmod(0o555)
        elif stat.S_ISREG(info.st_mode) and info.st_nlink == 1:
            path.chmod(0o555 if info.st_mode & 0o111 else 0o444)
        else:
            raise ValueError('unsafe staged member')
    result = {'format': 'rentemester-rust-build/1',
              'product': {'version': product, 'gitCommit': commit,
                          'builtAt': datetime.datetime.fromtimestamp(epoch, datetime.timezone.utc).isoformat().replace('+00:00', 'Z'),
                          'sourceDateEpoch': epoch, 'sourceDirty': dirty},
              'toolchain': {'rustc': rustc, 'cargo': cargo, 'target': target},
              'baseImages': images, 'schemas': schemas,
              'inputs': {'cargoLockSha256': digest(source / 'Cargo.lock'),
                         'webLockSha256': digest(source / 'bun.lock'),
                         'pdfPinsSha256': digest(source / 'scripts/rust/pdf-worker/artifacts.json'),
                         'vendor': inventory(source / 'vendor'),
                         'entrypointSha256': digest(source / 'scripts/release/rust-entrypoint.sh')},
              'pdf': {'artifact': expected, 'binding': pins['binding'], 'release': pins['release']},
              'files': inventory(bundle)}
    output.write_text(json.dumps(result, sort_keys=True, indent=2) + '\n')
    output.chmod(0o444)
    bundle.chmod(0o555)
    return result


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--source', type=Path, required=True)
    p.add_argument('--bundle', type=Path, required=True)
    p.add_argument('--commit', required=True)
    p.add_argument('--version', required=True)
    p.add_argument('--epoch', type=int, required=True)
    p.add_argument('--dirty', choices=['true', 'false'], required=True)
    p.add_argument('--rust-image', required=True)
    p.add_argument('--web-image', required=True)
    p.add_argument('--runtime-image', required=True)
    a = p.parse_args()
    create(a.source, a.bundle, a.commit, a.epoch, a.dirty == 'true',
           {'rustBuilder': a.rust_image, 'webBuilder': a.web_image, 'runtime': a.runtime_image}, a.version)
