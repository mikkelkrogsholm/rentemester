#!/usr/bin/env python3
"""Build-time only: verify pinned native artifact and retain every license. Never called by worker."""
import argparse
import hashlib
import io
import json
import pathlib
import tarfile
import urllib.request

p = argparse.ArgumentParser()
p.add_argument('--artifact', required=True)
p.add_argument('--destination', required=True, type=pathlib.Path)
a = p.parse_args()
manifest = json.loads(pathlib.Path(__file__).with_name('artifacts.json').read_text())
pin = next(row for row in manifest['artifacts'] if row['archive'] == a.artifact)
if a.destination.exists():
    raise SystemExit('destination must be new')
with urllib.request.urlopen(pin['url'], timeout=30) as response:
    data = response.read(64 * 1024 * 1024 + 1)
if len(data) > 64 * 1024 * 1024 or hashlib.sha256(data).hexdigest() != pin['sha256']:
    raise SystemExit('artifact hash/size mismatch')
with tarfile.open(fileobj=io.BytesIO(data), mode='r:gz') as archive:
    selected = []
    for member in archive.getmembers():
        name = pathlib.PurePosixPath(member.name)
        if name.is_absolute() or '..' in name.parts:
            raise SystemExit('unsafe archive path')
        if member.name == pin['library'] or member.name in ('LICENSE', 'VERSION', 'args.gn') or member.name.startswith('licenses/'):
            if member.isdir():
                continue
            if not member.isfile() or member.size > 128 * 1024 * 1024:
                raise SystemExit('unsupported artifact member')
            selected.append((member.name, archive.extractfile(member).read()))
    actual = next(body for name, body in selected if name == pin['library'])
    if hashlib.sha256(actual).hexdigest() != pin['librarySha256']:
        raise SystemExit('library hash mismatch')
    a.destination.mkdir(mode=0o700, parents=False)
    for name, body in selected:
        target = a.destination / name
        target.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        target.write_bytes(body)
    (a.destination / 'artifact.json').write_text(json.dumps(pin, indent=2) + '\n')
print(json.dumps({'artifact': pin['archive'], 'sha256': pin['sha256'], 'librarySha256': pin['librarySha256']}))
