#!/usr/bin/env python3
"""Source-bound native HTTP catalog. --check is required alongside server tests.
Identity: SHA256 of UTF-8 JSON with sorted object keys, compact separators,
ASCII escaping, sorted (path, method) operations; manifestId excluded.
"""
import argparse
import hashlib
import json
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parents[2]
SERVER = ROOT / 'crates/rentemester-server/src'

def balanced(text, start):
    depth = 1
    quoted = False
    escaped = False
    for i in range(start, len(text)):
        c = text[i]
        if quoted:
            if escaped: escaped = False
            elif c == '\\': escaped = True
            elif c == '"': quoted = False
        elif c == '"': quoted = True
        elif c == '(': depth += 1
        elif c == ')':
            depth -= 1
            if not depth: return text[start:i]
    raise ValueError('unclosed registration')

def routes(file):
    text = file.read_text()
    # Production only: unit-test router fixtures are not installed by main.
    text = text.split('#[cfg(test)]')[0]
    constants = dict(re.findall(r'const (\w+): &str = "([^"]+)";', text))
    result = []
    for hit in re.finditer(r'\.route\s*\(', text):
        args = balanced(text, hit.end())
        m = re.match(r'\s*("[^"]+"|\w+)\s*,\s*(.*)', args, re.S)
        if not m: raise ValueError(f'unparsed registration {file}: {args}')
        path = m[1][1:-1] if m[1].startswith('"') else constants.get(m[1])
        if path == '/health': path = '/api/health' # main nests only this API router
        if not path or not path.startswith('/api/') or any(c in path for c in '*?'):
            raise ValueError(f'unparsed path {file}: {args}')
        methods = re.findall(r'(?:^|\.|::)\s*(get|head|post|put|patch|delete)\s*\(', m[2])
        if not methods: raise ValueError(f'unparsed methods {file}: {args}')
        # Axum implicitly installs HEAD for GET, including explicit refusal handlers.
        if 'get' in methods and 'head' not in methods: methods.append('head')
        result.extend((x.upper(), path) for x in methods)
    return result

def manifest(entries):
    if len(entries) != len(set(entries)): raise ValueError('duplicate method/path')
    value = dict(format='rentemester-http-protocol/1', protocolMajor=1,
                 wireProfiles=['legacy-react-v1', 'rust-http-v1'],
                 operations=[dict(method=m, path=p, capability=f'{m} {p}')
                             for m,p in sorted(entries, key=lambda x:(x[1],x[0]))])
    raw = json.dumps(value, sort_keys=True, separators=(',', ':')).encode()
    value['manifestId'] = hashlib.sha256(raw).hexdigest()
    assert len(json.dumps(value).encode()) <= 1024*1024
    return value

def outputs():
    public = routes(SERVER/'main.rs') + routes(SERVER/'agent_discovery.rs') + routes(SERVER/'protocol.rs')
    configured = public[:]
    files = [SERVER/'product_http.rs', SERVER/'setup_http.rs', SERVER/'lib.rs', SERVER/'auth_http.rs']
    files += sorted((SERVER/'auth_http').glob('*.rs'))
    files = [f for f in files if f.name != 'tests.rs']
    for file in files: configured += routes(file)
    # main Backend::identity installs recovery + identity + enrollment together.
    backend = (SERVER/'backend.rs').read_text()
    assert 'enrollment: Some(EnrollmentHandlers' in backend
    assert 'rentemester_server::recovery_identity_router(' in backend
    yield ROOT/'protocol/http-v1.json', json.dumps(manifest(configured), indent=2)+'\n'
    file = SERVER/'protocol.rs'
    text = file.read_text()
    public_json = json.dumps(manifest(public), sort_keys=True, separators=(',', ':'))
    replacement = 'const PUBLIC_MANIFEST: &str = r#"'+public_json+'"#;'
    text, count = re.subn(r'const PUBLIC_MANIFEST: &str = .*?;', lambda _:replacement, text)
    assert count == 1
    yield file, text

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    for path, content in outputs():
        if args.check:
            if not path.exists() or path.read_text() != content:
                raise SystemExit(f'stale protocol artifact: {path.relative_to(ROOT)}')
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content)
