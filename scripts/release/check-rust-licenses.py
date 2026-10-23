#!/usr/bin/env python3
"""Offline, fail-closed Cargo license-policy verification (not runtime qualification)."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import sys
import tomllib
from urllib.parse import unquote


class Refusal(ValueError):
    """An input cannot establish the complete policy claim."""


def require(condition, message):
    if not condition:
        raise Refusal(message)


def text(value):
    return isinstance(value, str) and bool(value.strip())


class SPDX:
    """Strict expression grammar; policy, not a bundled SPDX catalogue, authorizes atoms."""
    def __init__(self, expression):
        require(text(expression), 'missing/null license expression')
        self.tokens = re.findall(r'[A-Za-z0-9][A-Za-z0-9.+-]*|[()]|\S', expression)
        self.index = 0
        self.tree = self.disjunction()
        require(self.index == len(self.tokens), 'malformed SPDX: trailing token')

    def take(self, token):
        if self.index < len(self.tokens) and self.tokens[self.index] == token:
            self.index += 1
            return True
        return False

    def atom(self):
        require(self.index < len(self.tokens), 'malformed SPDX: missing operand')
        token = self.tokens[self.index]
        require(re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9.+-]*', token)
                and token not in ('AND', 'OR', 'WITH'), 'malformed SPDX identifier')
        self.index += 1
        return token

    def primary(self):
        if self.take('('):
            node = self.disjunction()
            require(self.take(')'), 'malformed SPDX: missing closing parenthesis')
            return node
        term = self.atom()
        if self.take('WITH'):
            term += ' WITH ' + self.atom()
        return ('term', term)

    def conjunction(self):
        node = self.primary()
        while self.take('AND'):
            node = ('AND', node, self.primary())
        return node

    def disjunction(self):
        node = self.conjunction()
        while self.take('OR'):
            node = ('OR', node, self.conjunction())
        return node


def selected(node, allowed):
    if node[0] == 'term':
        return [node[1]] if node[1] in allowed else None
    left, right = selected(node[1], allowed), selected(node[2], allowed)
    if node[0] == 'OR':
        return left if left is not None else right
    return None if left is None or right is None else sorted(set(left + right))


def load_json(raw):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, 'duplicate JSON key: ' + key)
            result[key] = value
        return result
    return json.loads(raw, object_pairs_hook=unique)


def policy_check(policy):
    require(isinstance(policy, dict) and set(policy) == {
        'format', 'allowedLicenses', 'packageExceptions', 'normalizations', 'ownWorkspace'},
        'invalid policy fields')
    require(policy['format'] == 'rentemester-rust-license-policy/1'
            and policy['ownWorkspace'] is True, 'unsupported policy format/ownership')
    allowed = policy['allowedLicenses']
    require(isinstance(allowed, list) and allowed and all(text(x) for x in allowed)
            and len(set(allowed)) == len(allowed), 'invalid allowedLicenses')
    for term in allowed:
        tree = SPDX(term).tree
        require(tree == ('term', term) and '*' not in term, 'allowedLicenses must be exact SPDX terms')
    for field, fields, key_fields in [
        ('packageExceptions', {'name', 'version', 'expression', 'reason', 'sourceUrl'},
         ('name', 'version', 'expression')),
        ('normalizations', {'name', 'version', 'from', 'to', 'reason'}, ('name', 'version', 'from')),
    ]:
        rows = policy[field]
        require(isinstance(rows, list), 'invalid policy ' + field)
        keys = set()
        for row in rows:
            require(isinstance(row, dict) and set(row) == fields
                    and all(text(v) and '*' not in v for v in row.values()), 'invalid exact policy entry')
            key = tuple(row[k] for k in key_fields)
            require(key not in keys, 'duplicate policy entry')
            keys.add(key)
            if field == 'normalizations':
                SPDX(row['to'])
            else:
                SPDX(row['expression'])  # Exceptions cannot authorize malformed expressions.
                require(re.match(r'https?://[^/\s]+', row['sourceUrl']), 'exception sourceUrl required')
    return set(allowed)


def license_check(name, version, original, policy, allowed):
    require(text(original), f'{name}@{version}: missing/null license')
    normalized = original
    normalization = None
    for row in policy['normalizations']:
        if (name, version, original) == (row['name'], row['version'], row['from']):
            normalized, normalization = row['to'], row
    tree = SPDX(normalized).tree  # Parse the entire expression before considering an OR.
    terms = selected(tree, allowed)
    exception = next((row for row in policy['packageExceptions']
                      if (name, version, original) == (row['name'], row['version'], row['expression'])), None)
    if terms is None and exception:
        # Exact full-expression approval, not a wildcard or an approval for a base of WITH.
        def all_terms(node):
            return [node[1]] if node[0] == 'term' else all_terms(node[1]) + all_terms(node[2])
        terms = sorted(set(all_terms(tree)))
    require(terms is not None, f'{name}@{version}: unapproved license: {original}')
    return {'original': original, 'normalized': normalized, 'selectedTerms': terms,
            'normalization': normalization, 'packageException': exception if exception and selected(tree, allowed) is None else None}


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def verify(source, metadata, policy):
    """Verify paths offline; raise Refusal/ValueError on any incomplete candidate.

    Metadata paths may be relocated only by their recorded workspace_root. Registry
    bytes are not downloaded or claimed to have been rehashed: Cargo metadata has
    no checksum field; registry checksums are bound to the hashed Cargo.lock.
    """
    source = Path(source).resolve()
    lock_raw = (source / 'Cargo.lock').read_bytes()
    metadata_raw, policy_raw = Path(metadata).read_bytes(), Path(policy).read_bytes()
    lock, meta, rules = tomllib.loads(lock_raw.decode()), load_json(metadata_raw), load_json(policy_raw)
    allowed = policy_check(rules)
    require(meta.get('version') == 1 and isinstance(meta.get('packages'), list)
            and meta['packages'], 'complete cargo metadata v1 required')
    require(isinstance(meta.get('resolve'), dict) and isinstance(meta['resolve'].get('nodes'), list),
            'complete resolve graph required (no --no-deps)')
    require(text(meta.get('workspace_root')) and Path(meta['workspace_root']).is_absolute(),
            'absolute metadata workspace_root required')
    recorded_root = Path(meta['workspace_root'])
    root_raw = (source / 'Cargo.toml').read_bytes()
    root = tomllib.loads(root_raw.decode())
    workspace = root['workspace']
    manifests = {'Cargo.toml': sha(root_raw)}

    def local_manifest(recorded):
        require(text(recorded) and Path(recorded).is_absolute(), 'absolute manifest path required')
        try:
            relative = Path(recorded).relative_to(recorded_root)
        except ValueError:
            raise Refusal('foreign path manifest outside metadata workspace root') from None
        require('..' not in relative.parts and relative.name == 'Cargo.toml', 'invalid local manifest path')
        path = (source / relative).resolve()
        require(path.is_relative_to(source), 'local manifest escapes source root')
        raw = path.read_bytes()
        manifests[relative.as_posix()] = sha(raw)
        return relative, tomllib.loads(raw.decode())['package']

    members = set()
    excluded = set()
    for pattern in workspace.get('exclude', []):
        excluded.update(p.resolve() for p in source.glob(pattern))
    for pattern in workspace.get('members', []):
        require(text(pattern) and not Path(pattern).is_absolute() and '..' not in Path(pattern).parts,
                'unsafe workspace member')
        matches = list(source.glob(pattern))
        require(matches, 'workspace member missing: ' + pattern)
        for member in matches:
            require(member.resolve().is_relative_to(source), 'workspace member escapes root')
            if member.resolve() not in excluded:
                members.add((member.resolve() / 'Cargo.toml').relative_to(source).as_posix())
    if 'package' in root:
        members.add('Cargo.toml')
    require(members, 'explicit workspace members required')

    locked = {}
    for package in lock.get('package', []):
        name, version, origin = package.get('name'), package.get('version'), package.get('source')
        require(text(name) and text(version) and (origin is None or text(origin)), 'invalid lock identity')
        key = (name, version, origin)
        require(key not in locked, 'duplicate lock identity')
        checksum = package.get('checksum')
        if origin and origin.startswith('registry+'):
            require(isinstance(checksum, str) and re.fullmatch('[0-9a-f]{64}', checksum),
                    f'{name}: invalid/missing registry lock checksum')
        else:
            require(checksum is None, 'unexpected checksum for non-registry source')
        locked[key] = package
    require(locked, 'empty Cargo.lock')
    packages, ids, identities = meta['packages'], {}, {}
    for package in packages:
        require(isinstance(package, dict), 'invalid metadata package')
        key = (package.get('name'), package.get('version'), package.get('source'))
        require(all(text(x) for x in key[:2]) and (key[2] is None or text(key[2])), 'invalid metadata identity')
        identity = package.get('id')
        require(text(identity) and identity not in ids and key not in identities, 'duplicate/invalid metadata identity')
        ids[identity], identities[key] = package, identity
    missing, extra = set(locked) - set(identities), set(identities) - set(locked)
    require(not missing and not extra,
            f'Cargo.lock/metadata coverage drift: locked={len(locked)} metadata={len(packages)} '
            f'missing={len(missing)} extra={len(extra)}')
    nodes = meta['resolve']['nodes']
    require(all(isinstance(n, dict) and text(n.get('id')) for n in nodes), 'invalid resolve node')
    require(len(nodes) == len(ids) and {n['id'] for n in nodes} == set(ids), 'incomplete/duplicate resolve graph')
    for node in nodes:
        require(isinstance(node.get('dependencies'), list) and all(d in ids for d in node['dependencies']),
                'resolve dependency missing from inventory')
    workspace_ids = meta.get('workspace_members')
    require(isinstance(workspace_ids, list) and len(set(workspace_ids)) == len(workspace_ids)
            and all(i in ids for i in workspace_ids), 'invalid workspace identities')
    actual_members, rows = set(), []
    for key in sorted(locked, key=lambda k: (k[0], k[1], k[2] or '')):
        name, version, origin = key
        identity = identities[key]
        package = ids[identity]
        checksum = locked[key].get('checksum')
        require('checksum' not in package or package['checksum'] == checksum, 'metadata/lock checksum drift')
        own = False
        if origin is None:
            relative, manifest = local_manifest(package.get('manifest_path'))
            manifest_version = manifest.get('version')
            if manifest_version == {'workspace': True}:
                manifest_version = workspace.get('package', {}).get('version')
            require((manifest.get('name'), manifest_version) == (name, version), 'local manifest identity drift')
            url_path = unquote(identity.split('#', 1)[0].removeprefix('path+file://'))
            require(identity.startswith('path+file://') and Path(url_path) == recorded_root / relative.parent
                    and identity.split('#', 1)[-1] in (version, name + '@' + version), 'path package ID drift')
            own = relative.as_posix() in members and identity in workspace_ids
            if identity in workspace_ids:
                require(own, 'workspace member not owned by root manifest')
                actual_members.add(relative.as_posix())
            declared = manifest.get('license')
            if declared == {'workspace': True}:
                declared = workspace.get('package', {}).get('license')
            require(declared == package.get('license'), 'local manifest license drift')
        else:
            require(identity == origin + '#' + name + '@' + version, 'source/name/version package ID drift')
            require(identity not in workspace_ids, 'registry/git package cannot be own workspace')
        decision = {'original': package.get('license'), 'selectedTerms': [], 'classification': 'own-workspace'} if own else {
            **license_check(name, version, package.get('license'), rules, allowed), 'classification': 'third-party'}
        rows.append({'id': identity, 'name': name, 'version': version, 'source': origin,
                     'checksum': checksum, 'checksumEvidence': 'Cargo.lock' if checksum else None,
                     'manifestPath': package.get('manifest_path'), 'licenseFile': package.get('license_file'),
                     'repository': package.get('repository'), 'metadata': package.get('metadata'), 'license': decision})
    require(actual_members == members, 'workspace manifest/metadata member coverage drift')
    return {'format': 'rentemester-rust-license-report/1', 'status': 'PASS',
            'inputs': {'cargoLockSha256': sha(lock_raw), 'policySha256': sha(policy_raw),
                       'metadataSha256': sha(metadata_raw), 'manifests': dict(sorted(manifests.items()))},
            'packageCount': len(rows), 'packages': rows,
            'limitations': ['Offline source/lock/metadata policy binding only; no compiler or network invoked.',
                            'Cargo metadata omits registry checksums; checksums bind the lock, not rehashed dependency archives.',
                            'Metadata license declarations are not independent license-text provenance verification.',
                            'No Rust build, PDFium archive, runtime, distribution, R53, cutover or installed-desktop qualification.']}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for option in ('source', 'metadata', 'policy'):
        parser.add_argument('--' + option, type=Path, required=True)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    try:
        if args.output:
            output = args.output.resolve()
            require(output not in {args.policy.resolve(), args.metadata.resolve(),
                                   (args.source / 'Cargo.lock').resolve()}
                    and not output.is_relative_to(args.source.resolve()), 'report must not overwrite source/input files')
        result = verify(args.source, args.metadata, args.policy)
        rendered = json.dumps(result, sort_keys=True, indent=2) + '\n'
        if args.output:
            # Exclusive creation prevents stale-report replacement and input destruction.
            with args.output.open('x') as stream:
                stream.write(rendered)
        else:
            print(rendered, end='')
        return 0
    except (ValueError, OSError, KeyError, TypeError, RecursionError) as error:
        print('Rust license verification REFUSED: ' + str(error), file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main())
