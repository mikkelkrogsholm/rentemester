#!/usr/bin/env python3
"""Read-only PDFium pin/notice integrity check; NEVER a license policy approval.

--archives checks every source-pinned archive in a directory, without extraction.
--bundle checks a staged PDFium directory (or bundle/bin/pdfium); artifact.json
identifies the selected artifact, optionally constrained by --artifact. Staged
checks cannot attest to the original compressed archive bytes; use --archives
for that evidence. Text and review metadata are data, not executable policy.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import re
import stat
import sys
import tarfile

FORMAT = 'rentemester-pdfium-license-inventory/1'


def require(ok, message):
    if not ok:
        raise ValueError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def safe_name(name):
    require(isinstance(name, str) and name and '\\' not in name
            and not any(ord(c) < 32 for c in name)
            and all(p not in ('', '.', '..') for p in name.split('/'))
            and not name.startswith('/') and ':' not in name, 'unsafe path')
    return name


def regular(path):
    # Reject symlink ancestors too, before opening any input.
    for part in (path, *path.parents):
        require(not part.is_symlink(), 'symlink input: ' + str(part))
    info = path.lstat()
    require(stat.S_ISREG(info.st_mode) and info.st_nlink == 1,
            'nonregular or hardlinked input: ' + str(path))
    return path.read_bytes()


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'duplicate JSON key: ' + key)
        result[key] = value
    return result


def load(data):
    return json.loads(data, object_pairs_hook=unique_object)


def hash_field(value):
    require(isinstance(value, str) and re.fullmatch('[0-9a-f]{64}', value),
            'malformed SHA256')


def notice_name(name):
    return name == 'LICENSE' or name.startswith('licenses/')


def indexed(rows, key):
    require(isinstance(rows, list) and rows, 'nonempty list required: ' + key)
    result = {}
    for row in rows:
        require(isinstance(row, dict), 'object row required')
        name = safe_name(row[key])
        require(name not in result, 'duplicate inventory/pin path: ' + name)
        result[name] = row
    return result


def read_contract(pins_path, inventory_path):
    data = regular(pins_path)
    pins = load(data)
    inventory = load(regular(inventory_path))
    require(pins['format'] == 'rentemester-pdfium-artifacts/1', 'unknown pins format')
    require(inventory['format'] == FORMAT and inventory['policyApprovalClaim'] is False,
            'unknown inventory format or policy approval claim')
    hash_field(inventory['pinsSha256'])
    require(inventory['pinsSha256'] == sha(data), 'pins drift')
    ps = indexed(pins['artifacts'], 'archive')
    rows = indexed(inventory['artifacts'], 'archive')
    require(ps.keys() == rows.keys(), 'complete pinned artifact set required')
    for name, pin in ps.items():
        require('/' not in name, 'archive must be a basename')
        row = rows[name]
        safe_name(pin['library'])
        for key in ('sha256', 'librarySha256'):
            hash_field(pin[key])
        require((row['archiveSha256'], row['library'], row['librarySha256']) ==
                (pin['sha256'], pin['library'], pin['librarySha256']), 'inventory pin drift')
        notices = indexed(row['notices'], 'path')
        require('LICENSE' in notices and any(n.startswith('licenses/') for n in notices),
                'complete notices required')
        require(pin['library'] not in notices, 'library cannot be a notice')
        for n, notice in notices.items():
            require(notice_name(n), 'unexpected inventory notice path')
            hash_field(notice['sha256'])
            require(type(notice['size']) is int and notice['size'] > 0,
                    'positive notice size required')
            # Null means unresolved identification, not approval; integrity can
            # still be checked, but admission must be decided by the lead.
            require(notice['licenseExpression'] is None or
                    (isinstance(notice['licenseExpression'], str) and
                     bool(notice['licenseExpression'].strip())), 'malformed license identification')
            require(isinstance(notice['review'], str) and notice['review'].strip(),
                    'source review required')
    return ps, rows


def check_contents(contents, row):
    expected = {n['path']: n for n in row['notices']}
    actual = {n: b for n, b in contents.items() if notice_name(n)}
    require(actual.keys() == expected.keys(), 'notice path set mismatch')
    for name, body in actual.items():
        require(len(body) == expected[name]['size'] and sha(body) == expected[name]['sha256'],
                'notice content drift: ' + name)
    require(row['library'] in contents and sha(contents[row['library']]) == row['librarySha256'],
            'library drift or missing library')


def check_archive(path, row):
    data = regular(path)
    require(sha(data) == row['archiveSha256'], 'archive drift: ' + row['archive'])
    contents, seen = {}, set()
    with tarfile.open(fileobj=io.BytesIO(data), mode='r:gz') as archive:
        for member in archive:
            name = safe_name(member.name)
            require(name not in seen, 'duplicate archive member: ' + name)
            seen.add(name)
            require(member.isdir() or (member.isfile() and not member.issparse()),
                    'nonregular archive member: ' + name)
            if member.isdir():
                require(name != 'LICENSE' and name != row['library'], 'required file is directory')
                continue
            # Inspect all names/types, read only library and notices; no extraction.
            if name == row['library'] or notice_name(name):
                require(member.size <= 128 * 1024 * 1024, 'oversized member')
                contents[name] = archive.extractfile(member).read()
    check_contents(contents, row)


def check_bundle(bundle, artifact, pins, rows):
    require(not bundle.is_symlink() and bundle.is_dir(), 'bundle directory required')
    root = bundle / 'bin/pdfium' if (bundle / 'bin/pdfium').exists() else bundle
    metadata = load(regular(root / 'artifact.json'))
    name = metadata['archive']
    require(isinstance(name, str) and name in pins, 'unknown staged artifact')
    require(artifact is None or artifact == name, 'selected artifact mismatch')
    require(metadata == pins[name], 'staged artifact pin drift')
    contents = {}
    # Walk without following links; reject every nonordinary staged member,
    # including ones not used by the notice checker.
    def walk(directory):
        for path in sorted(directory.iterdir()):
            relative = safe_name(path.relative_to(root).as_posix())
            mode = path.lstat().st_mode
            if stat.S_ISDIR(mode):
                walk(path)
            else:
                body = regular(path)
                if relative == rows[name]['library'] or notice_name(relative):
                    contents[relative] = body
    walk(root)
    check_contents(contents, rows[name])
    return name


def verify(pins, inventory, archives=None, bundle=None, artifact=None):
    ps, rows = read_contract(Path(pins), Path(inventory))
    require((archives is None) != (bundle is None), 'choose archives or bundle')
    if archives is not None:
        require(artifact is None, '--archives requires all artifacts, not --artifact')
        names = sorted(ps)
        for name in names:
            check_archive(Path(archives) / name, rows[name])
        mode = 'archives'
    else:
        names = [check_bundle(Path(bundle), artifact, ps, rows)]
        mode = 'bundle'
    return {'format': 'rentemester-pdfium-license-integrity/1', 'mode': mode,
            'artifacts': names, 'pinsSha256': sha(regular(Path(pins))),
            'inventorySha256': sha(regular(Path(inventory))),
            'integrityVerified': True, 'policyApprovalClaim': False}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--pins', type=Path, required=True)
    p.add_argument('--inventory', type=Path, required=True)
    group = p.add_mutually_exclusive_group(required=True)
    group.add_argument('--archives', type=Path)
    group.add_argument('--bundle', type=Path)
    p.add_argument('--artifact')
    a = p.parse_args()
    try:
        print(json.dumps(verify(a.pins, a.inventory, a.archives, a.bundle, a.artifact), sort_keys=True))
    except (ValueError, KeyError, TypeError, OSError, tarfile.TarError, EOFError) as error:
        print('PDFium license integrity refused: ' + str(error), file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
