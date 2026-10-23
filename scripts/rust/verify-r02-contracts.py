#!/usr/bin/env python3
"""Check frozen R02 contract coverage and identity; semantic/runtime review is separate."""
import copy
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
BASE = ROOT / 'docs/architecture/rust-target/implementation/contracts'
REV = '115326a1ef298e0c6f7b1f40623cb9a3d732dac2'
CACHE = {}
RISKS = {'authorization', 'boundaries', 'concurrency', 'idempotency', 'atomicity'}


def digest(body):
    return hashlib.sha256(body).hexdigest()


def frozen(path):
    p = Path(path)
    assert not p.is_absolute() and '..' not in p.parts, path
    if path not in CACHE:
        CACHE[path] = subprocess.check_output(['git', 'show', f'{REV}:{path}'], cwd=ROOT)
    return CACHE[path]


def refs(value):
    if isinstance(value, dict):
        if {'path', 'sha256', 'location'} <= value.keys():
            assert value['location'] and digest(frozen(value['path'])) == value['sha256'], value
        for child in value.values():
            refs(child)
    elif isinstance(value, list):
        for child in value:
            refs(child)


def contract(row):
    c = row['contract']
    assert all(c.get(k) for k in ['input', 'context', 'output', 'errors', 'preconditions', 'postconditions'])
    for name in ['input', 'context', 'output']:
        t = c[name]
        assert isinstance(t, dict) and t.get('definition') and t.get('sourceRefs')
        assert str(t['definition']).strip().lower() not in {'any', 'object', 'unknown', 'todo', 'see source'}
    assert isinstance(c['externalEffects'], list) and c['effectsComplete'] is True
    assert set(c['risks']) == RISKS
    for risk in c['risks'].values():
        assert type(risk['relevant']) is bool and risk['rationale'] and risk['scenarios']
        for scenario in risk['scenarios']:
            assert all(scenario.get(k) for k in ['id', 'given', 'when', 'then'])
    assert row['readiness']['modelValidityIsNotAuthorization'] is True
    assert row['readiness']['readinessIsNotExecution'] is True
    assert row['readiness']['acceptancePlan'] and row['runtimeQualificationClaim'] is False
    assert row['baselineBindings'] and row['sourceRefs'] and row['reason']
    refs(row)


def validate(index, amendments, common):
    assert index['sourceRevision'] == common['sourceRevision'] == REV
    paths = subprocess.check_output(['git', 'ls-tree', '-r', '--name-only', REV,
                                    'docs/architecture/rust-target'], cwd=ROOT, text=True).splitlines()
    expected = {p for p in paths if p.endswith('/gdd.json')}
    rows = {r['path']: r for r in index['rows']}
    assert len(rows) == len(index['rows']) and set(rows) == expected and len(expected) == 67
    required = set()
    for path, row in rows.items():
        body = frozen(path)
        assert digest(body) == row['sha256'] == digest((ROOT / path).read_bytes()), path
        actions = {k for k, v in json.loads(body)['nodes'].items() if v['kind'] == 'action'}
        assert actions == set(row['actionIds'])
        if row['classification'] == 'current-contract':
            assert row['amendmentArtifact'] and row['effectiveStatus'] in {'effective-prospective', 'preserved-only'}
            required.update((path, a) for a in actions)
        else:
            assert row['effectiveStatus'] in {'preserved-only', 'unready-sketch'}
    assert len(required) == 58
    actual = {}
    for doc in amendments:
        assert doc['sourceRevision'] == REV and not doc['unresolvedRequiredContracts']
        assert doc['format'] == 'rentemester-r02-prospective-contracts/1'
        for row in doc['rows']:
            original = row['original']; key = (original['path'], original['nodeId'])
            assert key not in actual
            assert original['sha256'] == rows[key[0]]['sha256']
            assert row['status'] == rows[key[0]]['effectiveStatus']
            contract(row); actual[key] = row
    assert set(actual) == required
    base = 'docs/architecture/rust-target/implementation/control-fence/gdd.json'
    qualified = base.replace('/gdd.json', '/qualified-spec/gdd.json')
    assert actual[(base, 'action.control_fence')]['status'] == 'effective-prospective'
    assert actual[(qualified, 'action.control_fence')]['status'] == 'preserved-only'
    superseder = next(d['frozen182Superseder'] for d in amendments if 'frozen182Superseder' in d)
    assert superseder['preservedOriginalModelSHA'] == 'd62037b60f9d1c59fdde70298303ebe09604e4e3ab53dec8776be71bcfc7c42c'
    assert superseder['preservedOriginalModelRevision'] == 'sha256:5b6b0dbc5ead773c9f1b9bfee8381a092cab51778ecb4afcbd8acd6609043b0a'
    assert superseder['stableActionRef'] == '/packages/batch182-integrated-repair/nodes/action.backend'
    contract(superseder)
    refs(common)
    assert {d['decision'] for d in common['decisionSeparation']} == {
        'model-validity', 'authorization', 'scope-readiness', 'executed-acceptance'}
    assert len(common['components']) == 8 and common['readinessForNextSelectedSlice']
    assert not common['runtimeQualificationClaim'] and not index['allOriginalIssuesReady']
    return len(expected), len(required)


def main():
    index = json.loads((BASE / 'r02-effective-index.json').read_text())
    common = json.loads((BASE / 'r02-boundary-readiness.json').read_text())
    docs = [json.loads((BASE / f'r02-{part}-amendments.json').read_text()) for part in ['foundation', 'domain']]
    counts = validate(index, docs, common)
    for case in ['dropped-scope', 'missing-atomicity', 'false-precedence', 'source-tamper']:
        i, a, c = copy.deepcopy((index, docs, common))
        if case == 'dropped-scope':
            a[0]['rows'].pop()
        elif case == 'missing-atomicity':
            a[0]['rows'][0]['contract']['risks'].pop('atomicity')
        elif case == 'false-precedence':
            next(r for r in i['rows'] if '/qualified-spec/gdd.json' in r['path'])['effectiveStatus'] = 'effective-prospective'
        else:
            a[0]['rows'][0]['sourceRefs'][0]['sha256'] = '0' * 64
        try:
            validate(i, a, c)
        except AssertionError:
            continue
        raise AssertionError(f'Negative control did not refuse: {case}')
    print(json.dumps({'status': 'PASS', 'sourceRevision': REV, 'preservedModels': counts[0],
                      'typedActionDeclarations': counts[1], 'frozen182Superseder': True,
                      'negativeControls': ['dropped-scope', 'missing-atomicity', 'false-precedence', 'source-tamper'],
                      'limits': 'Static scope/hash/shape integrity only. Lead semantic review and actual boundary negative receipts are independent; no runtime or whole epic acceptance.'}))


if __name__ == '__main__':
    main()
