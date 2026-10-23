#!/usr/bin/env python3
"""Verify frozen R01 inventory coverage/anchors; semantic review and runtime parity are separate."""
import copy
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
BASE = ROOT / 'docs/architecture/rust-target/implementation/baseline'
REV = '8866e33b49023a02e8f82fe9e69b5e8d2a412b7e'
ORIGINAL = '0bbe82bcf0cafd73e2326152740cb06b90222405'
OWNERS = {f'R{n:02}' for n in range(1, 56)}
blob_cache = {}


def read(name):
    return json.loads((BASE / name).read_text())


def digest(body):
    return hashlib.sha256(body).hexdigest()


def frozen_bytes(path):
    p = Path(path)
    assert not p.is_absolute() and '..' not in p.parts, path
    if path not in blob_cache:
        blob_cache[path] = subprocess.check_output(['git', 'show', f'{REV}:{path}'], cwd=ROOT)
    return blob_cache[path]


def reference(ref):
    assert {'path', 'sha256', 'location'} <= set(ref), ref
    assert str(ref['location']).strip(), ref
    assert digest(frozen_bytes(ref['path'])) == ref['sha256'], ref['path']


def nested_references(value):
    if isinstance(value, dict):
        if {'path', 'sha256', 'location'} <= set(value):
            reference(value)
        for item in value.values():
            nested_references(item)
    elif isinstance(value, list):
        for item in value:
            nested_references(item)


def validate(d, expected, frozen):
    assert d['format'] == 'rentemester-r01-contract-inventory/1'
    assert d['sourceRevision'] == REV
    rows = d['rows']
    ids = {r['id'] for r in rows}
    assert rows and len(ids) == len(rows), 'Duplicate/empty behavior inventory'
    if expected is not None:
        assert expected <= ids, ('Missing registry behavior', sorted(expected - ids))
    if d['surface'] == 'core':
        needed = {(x['path'], x['name'], x['line']) for x in frozen['coreExportedFunctions']}
        actual = {(x['path'], x['name'], x['line']) for x in d['functionIndex']}
        assert needed <= actual, 'Missing exported function classification'
        assert all(x['behaviorId'] in ids for x in d['functionIndex'])
        for source, index in [('migrations', 'migrationIndex'), ('providerJobCandidateModules', 'providerJobIndex')]:
            assert {x['path'] for x in frozen[source]} <= {x['path'] for x in d[index]}
            assert all(x['behaviorId'] in ids for x in d[index])
        assert d['additionalExtractionReview'] and len(d['releaseContracts']) >= 7
    for r in rows:
        assert r['domainOwner'] in OWNERS, r['id']
        assert r['status'] in {'source-supported', 'partial', 'unavailable', 'pending-roadmap', 'corrected-target'}
        assert r['statusReason'] and r['interfaces'] and r['sourceRefs']
        for kind in ['input', 'success', 'refusal', 'parseProtocol']:
            c = r['contracts'][kind]
            assert c['definition'] and c['sourceRefs'], (r['id'], kind)
            assert not (isinstance(c['definition'], str) and c['definition'].strip().upper() in {'UNKNOWN', 'TODO', 'ANY', 'SEE SOURCE'})
        assert all(k in r['wire'] and r['wire'][k] is not None for k in ['casing', 'nullability', 'stdout', 'stderr', 'exitCodes', 'mcpTextStructuredContent', 'interfaceOnlyBoundaries'])
        for kind in ['oracle', 'negativeCase']:
            c = r[kind]
            assert len(c['expectation']) >= 24 and c['evidenceRefs']
            assert c['evidenceLevel'] in {'source-reviewed', 'executed-retained', 'planned'}
        assert r['negativeCase']['scenario']
    assert not d.get('unresolvedRequiredContracts'), 'Incomplete inventory'
    assert d.get('limitations') is not None
    nested_references(d)
    return ids


def must_refuse(check):
    try:
        check()
    except AssertionError:
        return
    raise AssertionError('Negative control did not refuse')


def main():
    frozen = read('current-source.json')
    expected = {
        'cli': {'cli:' + x['key'] for x in frozen['cli']},
        'mcp': {'mcp:' + x['name'] for x in frozen['mcp']},
        'http-ui': {'http:' + x['method'] + ':' + x['pattern'] for x in frozen['http']}
        | {'screen:' + x['id'] for x in frozen['browserCompany']}
        | {'screen-static:' + x['path'] for x in frozen['browserStatic']},
        'core': None,
    }
    documents, all_ids, counts = {}, set(), {}
    for surface, needed in expected.items():
        d = read(f'contracts-{surface}.json')
        assert d['surface'] == surface
        ids = validate(d, needed, frozen)
        documents[surface], counts[surface] = d, len(ids)
        all_ids |= ids
        broken = copy.deepcopy(d)
        if needed is None:
            broken['functionIndex'] = []
        else:
            broken['rows'].remove(next(r for r in broken['rows'] if r['id'] in needed))
        must_refuse(lambda: validate(broken, needed, frozen))
        broken = copy.deepcopy(d)
        broken['rows'][0]['sourceRefs'][0]['sha256'] = '0' * 64
        must_refuse(lambda: validate(broken, needed, frozen))
    # Lead's corrected stream attribution must remain present.
    for r in documents['cli']['rows']:
        assert all(not b['expression'].lstrip().startswith(('console.error(', 'process.exit(')) for b in r['wire']['stdout']['branches'])
    def cross_refs(value):
        if isinstance(value, dict):
            for key, item in value.items():
                if key == 'existingBehaviorBoundaries':
                    assert set(item) <= all_ids, set(item) - all_ids
                else:
                    cross_refs(item)
        elif isinstance(value, list):
            for item in value:
                cross_refs(item)
    cross_refs(documents['core'])
    history = read('historical-dispositions.json')
    assert {r['issue'] for r in history['rows']} == {363, 664, 641, 643, 642, 625, 624, 612, 611, 566, 543, 525, 524, 512, 511, 330, 329, 328}
    assert history['original55ScopeUnchanged'] and history['noNewEnhancementScopeImported']
    assert all(r['classification'] and r['decision'] and r['deliveryOwners'] for r in history['rows'])
    nested_references(history)
    backlog = read('existing-backlog-reconciliation.json')
    planning = json.loads((ROOT / 'docs/architecture/rust-target/epic/existing-issues.snapshot.json').read_text())
    assert len(backlog['rows']) == len({r['number'] for r in backlog['rows']}) == 520
    assert {r['number'] for r in backlog['rows']} == {r['number'] for r in planning}
    assert backlog['open'] + backlog['closed'] == 520
    assert backlog['namedOriginalDispositionsSha256'] == digest((BASE / 'historical-dispositions.json').read_bytes())
    delta = read('source-delta-review.json')
    actual = subprocess.check_output(['git', 'diff', '--name-only', ORIGINAL, REV, '--', 'src', 'app/src', 'scripts/release'], cwd=ROOT, text=True).splitlines()
    assert {r['path'] for r in delta['rows']} == set(actual)
    assert len(actual) == delta['actualChangedSourceFiles'] == 295
    for r in delta['rows']:
        assert digest(frozen_bytes(r['path'])) == r['currentSha256']
        raw_diff = subprocess.check_output(['git', 'diff', ORIGINAL, REV, '--', r['path']], cwd=ROOT)
        assert digest(raw_diff) == r['diffSha256'], r['path']
        assert r['review']['deliveryOwners'] and set(r['review']['deliveryOwners']) <= OWNERS
        assert r['review']['meaning']
        if not r['review']['contractIds']:
            exclusion = r['review']['intentionalExclusion']
            assert exclusion['deliveryOwner'] in OWNERS and exclusion['issue'] == 718
            assert exclusion['reason'] and exclusion['contractAnchor']
            assert r['path'] == 'app/src/lib/types.ts', 'Only reviewed type-only barrel has this exclusion'
        assert set(r['review']['contractIds']) <= all_ids, r['path']
    ui = read('ui-source-delta-review.json')
    assert len(ui['rows']) == ui['verification']['rows'] == 277
    assert not ui['verification']['unmappedExecutableOrTestPaths']
    assert not ui['verification']['remainingRuntimeClosureOmissions']
    artifact = ui['contractArtifact']
    assert digest((ROOT / artifact['path']).read_bytes()) == artifact['sha256']
    print(json.dumps({'status': 'passed', 'sourceRevision': REV, 'sourceAnchorFiles': len(blob_cache), 'behaviorRows': counts, 'functionDeclarations': len(frozen['coreExportedFunctions']), 'migrations': len(frozen['migrations']), 'providerJobModules': len(frozen['providerJobCandidateModules']), 'namedDispositions': 18, 'historicalIssues': 520, 'sourceDeltas': 295, 'droppedAndTamperedNegativeControls': True, 'limits': 'Static source-contract coverage and source identity only. Lead semantic review remains necessary. Planned oracles are not executed tests; this does not establish runtime parity, provider availability, release approval, cutover or final installed desktop acceptance.'}))


if __name__ == '__main__':
    main()
