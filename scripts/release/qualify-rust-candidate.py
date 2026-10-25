#!/usr/bin/env python3
"""Qualify immutable Rust candidate witnesses; incomplete original discovery fails closed."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

SOURCE = Path(__file__).resolve().parents[2]
IMAGE = re.compile(r'ghcr\.io/mikkelkrogsholm/rentemester@sha256:[0-9a-f]{64}')
HEX = re.compile(r'[0-9a-f]{64}')


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def render(value):
    return (json.dumps(value, sort_keys=True, indent=2, ensure_ascii=False) + '\n').encode()


def run(args, *, env=None, timeout=120, cwd=SOURCE):
    return subprocess.run(args, cwd=cwd, env=env, check=True, capture_output=True, timeout=timeout).stdout


def load(path):
    raw = path.read_bytes()
    require(len(raw) <= 64 * 1024 * 1024, 'evidence exceeds 64 MiB')
    return raw, json.loads(raw)


def docker_base(image):
    return ['docker', 'run', '--rm', '--network', 'none', '--read-only', '--cap-drop', 'ALL',
            '--security-opt', 'no-new-privileges:true', '--pids-limit', '64', '--memory', '512m',
            '--entrypoint', '/bin/sh', image]


def validate_sbom(supplied, actual, platform):
    # Docker's .SBOM is either one platform's {SPDX: document} or a platform map.
    require(isinstance(actual, dict), 'BuildKit SBOM lookup is absent or malformed')
    require(not any('/' in key for key in actual) or platform in actual, 'BuildKit SPDX platform absent')
    selected = actual.get(platform) if platform in actual else actual
    require(isinstance(selected, dict) and isinstance(selected.get('SPDX'), dict),
            'BuildKit SPDX is absent for requested platform')
    document = selected['SPDX']
    require(document == supplied, 'supplied SPDX differs from immutable image BuildKit SPDX')
    require(document.get('spdxVersion') in ('SPDX-2.2', 'SPDX-2.3')
            and document.get('SPDXID') == 'SPDXRef-DOCUMENT'
            and document.get('dataLicense') == 'CC0-1.0'
            and isinstance(document.get('packages'), list) and document['packages']
            and isinstance(document.get('creationInfo'), dict)
            and document['creationInfo'].get('creators'), 'invalid/empty SPDX document')
    return len(document['packages'])


def normalized_license(report):
    require(report.get('format') == 'rentemester-rust-license-report/1'
            and report.get('status') == 'PASS', 'complete locked Rust license report required')
    require(isinstance(report.get('inputs'), dict) and isinstance(report.get('packages'), list),
            'complete license inputs/packages required')
    inputs = dict(report['inputs'])
    require(HEX.fullmatch(inputs.pop('metadataSha256')), 'license metadata hash invalid')
    rows = []
    for original in report['packages']:
        row = dict(original)
        # Cargo IDs and cache/checkout paths vary across independent CI runners.
        # All identities, lock checksums, policy decisions and local manifest hashes remain exact.
        row.pop('id')
        row.pop('manifestPath')
        row.pop('licenseFile')
        rows.append(row)
    require(type(report['packageCount']) is int and report['packageCount'] == len(rows) > 0,
            'license package inventory incomplete')
    require(len({(r['name'], r['version'], r['source']) for r in rows}) == len(rows),
            'duplicate license package identity')
    return {'inputs': inputs, 'packages': sorted(rows, key=lambda r: (r['name'], r['version'], r['source'] or ''))}


def original_inventory(source=SOURCE):
    # R01's original 214-row advertised contracts are the preserved compatibility baseline.
    raw, baseline = load(source / 'docs/architecture/rust-target/implementation/baseline/contracts-mcp.json')
    require(baseline.get('format') == 'rentemester-r01-contract-inventory/1' and baseline.get('surface') == 'mcp',
            'original MCP contract baseline unavailable')
    tools = []
    for row in baseline['rows']:
        require(row['id'].startswith('mcp:'), 'invalid original MCP identity')
        tools.append({'name': row['id'][4:], 'inputSchema': row['contracts']['input']['definition']['schema'],
                      'annotations': row['wire']['interfaceOnlyBoundaries']['annotations']})
    program = '''import {AGENT_WORKFLOWS} from "./src/agent-discovery-catalog";
console.log(JSON.stringify({workflows:AGENT_WORKFLOWS}));'''
    result = json.loads(run(['bun', '-e', program], cwd=source))
    require(tools and result['workflows'], 'original full discovery inventory unavailable')
    return {'tools': tools, 'workflows': result['workflows'], 'originalBaselineSha256': sha(raw)}


def native_discovery(image, workflow_ids):
    require(all(re.fullmatch('[a-z0-9-]+', name) for name in workflow_ids), 'unsafe workflow identifier')
    # No config, workspace, credentials, host ports, volumes or external network.
    script = '''/opt/rentemester/bin/rentemester-server --listen 127.0.0.1:4319 --web-dir /opt/rentemester/web >/dev/null 2>&1 &
pid=$!; trap 'kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true' EXIT
ready=false
for attempt in $(seq 1 30); do
 if curl --noproxy '*' -fsS --max-time 1 http://127.0.0.1:4319/api/health >/dev/null; then ready=true; break; fi
 kill -0 "$pid" || exit 1; sleep 1
done
[ "$ready" = true ] || exit 1
for id in __about__ ''' + ' '.join(workflow_ids) + '''; do
 printf '%s\\t' "$id"
 if [ "$id" = __about__ ]; then endpoint=agent-about; else endpoint=agent-workflows/$id; fi
 curl --noproxy '*' -sS --max-time 5 -w '\\t%{http_code}\\n' "http://127.0.0.1:4319/api/$endpoint"
done'''
    records = {}
    for line in run(docker_base(image) + ['-ec', script]).decode().splitlines():
        name, body, status = line.split('\t')
        require(name not in records and status in ('200', '404'), 'unknown native discovery response')
        records[name] = {'status': int(status), 'body': json.loads(body)}
    require(set(records) == {'__about__', *workflow_ids}, 'incomplete native discovery lookup')
    return records


def schema_signature(schema):
    """Public field/type/required ABI only; prose and domain bounds are separate contracts."""
    require(isinstance(schema, dict), 'input schema must be an object')
    result = {}
    kind = schema.get('type')
    if kind is None and ('enum' in schema or 'const' in schema):
        values = schema.get('enum', [schema.get('const')])
        def primitive(value):
            if value is None: return 'null'
            if isinstance(value, bool): return 'boolean'
            if isinstance(value, str): return 'string'
            if isinstance(value, (int, float)): return 'number'
            if isinstance(value, list): return 'array'
            if isinstance(value, dict): return 'object'
            raise ValueError('unknown schema literal type')
        kind = sorted({primitive(value) for value in values})
    if kind is not None:
        result['types'] = sorted(kind if isinstance(kind, list) else [kind])
    if 'properties' in schema or kind == 'object':
        properties = schema.get('properties', {})
        require(isinstance(properties, dict), 'input properties must be an object')
        result['properties'] = {name: schema_signature(value) for name, value in sorted(properties.items())}
        result['required'] = sorted(schema.get('required', []))
    if 'items' in schema:
        result['items'] = schema_signature(schema['items'])
    for key in ('anyOf', 'oneOf', 'allOf'):
        if key in schema:
            result[key] = sorted((schema_signature(part) for part in schema[key]), key=render)
    # Unresolved references cannot silently become an unconstrained field.
    for key in ('$ref', '$defs'):
        if key in schema:
            result[key] = schema[key]
    return result


def tool_signature(tool):
    schema = tool.get('inputSchema')
    require(isinstance(schema, dict) and schema.get('type') == 'object', 'advertised object input schema required')
    annotations = tool.get('annotations', {})
    require(isinstance(annotations, dict), 'tool annotations must be an object')
    defaults = {'readOnlyHint': False, 'destructiveHint': True, 'idempotentHint': False, 'openWorldHint': True}
    hints = {key: annotations.get(key, default) for key, default in defaults.items()}
    require(all(type(value) is bool for value in hints.values()), 'tool safety hints must be boolean')
    return {'input': schema_signature(schema), 'annotations': hints}


def discovery_report(expected, packaged, records, binding):
    tools = packaged.get('tools')
    require(packaged.get('format') == 'rentemester-native-mcp-catalogue/1'
            and isinstance(tools, list) and tools, 'packaged MCP catalogue invalid')
    by_name = {tool['name']: tool for tool in tools}
    require(len(by_name) == len(tools), 'duplicate packaged MCP tool')
    expected_names = sorted(tool['name'] for tool in expected['tools'])
    require(len(set(expected_names)) == len(expected_names), 'duplicate original tool')
    missing = sorted(set(expected_names) - set(by_name))
    refusal = sorted(name for name in expected_names if name in by_name and re.search(
        r'refusal[- ]only|not (?:yet )?(?:implemented|ported|supported)|unsupported legacy',
        by_name[name].get('description', ''), re.I))
    incompatible = {}
    for tool in expected['tools']:
        name = tool['name']
        if name in by_name:
            original, native = tool_signature(tool), tool_signature(by_name[name])
            if original != native:
                incompatible[name] = {'expected': original, 'packaged': native}
    missing_workflows, invalid_workflows = [], []
    required = ['title', 'intendedOutcome', 'prerequisites', 'nonGoals', 'steps', 'blockers', 'recovery', 'stopConditions']
    for workflow in expected['workflows']:
        name = workflow['id']; record = records[name]
        if record['status'] == 404:
            missing_workflows.append(name); continue
        actual = record['body'].get('workflow', {})
        if actual.get('id') != name or any(not actual.get(key) for key in required):
            invalid_workflows.append(name); continue
        for original_step in workflow['steps']:
            matching = [step for step in actual['steps'] if original_step['operation'] in
                        [step.get('operation', {})] + step.get('aliases', [])]
            safety = ['expectedSafety', 'expectedIdempotent', 'requiresActor', 'requiresConfirmation', 'retryClass']
            if not any(all(step.get(key) == original_step.get(key) for key in safety) for step in matching):
                invalid_workflows.append(name); break
        for step in actual['steps']:
            references = [step.get('operation', {})] + step.get('aliases', [])
            if any(not isinstance(ref, dict) or (ref.get('surface') == 'mcp' and ref.get('name') not in by_name) for ref in references):
                invalid_workflows.append(name); break
    about = records['__about__']
    require(about['status'] == 200 and isinstance(about['body'].get('catalogue'), dict), 'native about unavailable')
    full = about['body'].get('fullCoverage') is True and about['body']['catalogue'].get('fullCoverage') is True
    ok = not (missing or refusal or incompatible or missing_workflows or invalid_workflows) and full
    return {**binding, 'format': 'rentemester-rust-agent-discovery-coverage/1', 'ok': ok,
            'status': 'passed' if ok else 'failed', 'originalToolNames': expected_names,
            'packagedToolNames': sorted(by_name), 'missingOriginalToolNames': missing,
            'refusalOnlyOriginalToolNames': refusal,
            'incompatibleOriginalToolSignatures': sorted(incompatible),
            'originalToolSignatureDifferences': incompatible,
            'originalWorkflowIds': sorted(w['id'] for w in expected['workflows']),
            'missingOriginalWorkflowIds': sorted(missing_workflows),
            'invalidOriginalWorkflowIds': sorted(set(invalid_workflows)), 'nativeFullCoverage': full,
            'expectedInventorySha256': sha(render(expected)), 'packagedCatalogueSha256': sha(render(packaged)),
            'nativeDiscovery': records,
            'limitations': ['Discovery describes contracts; it is not whole semantic parity or installed desktop acceptance.']}


def write_report(path, report):
    raw = render(report)
    with path.open('xb') as stream:
        stream.write(raw)
    with Path(str(path) + '.sha256').open('x') as stream:
        stream.write(sha(raw) + '  ' + path.name + '\n')


def qualify(args):
    require(IMAGE.fullmatch(args.image), 'immutable canonical image@sha256 required')
    require(re.fullmatch('[0-9a-f]{40}', args.commit), 'full source commit required')
    require(args.platform in ('linux/amd64', 'linux/arm64'), 'unsupported platform')
    require(re.fullmatch(r'(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?', args.version), 'release version invalid')
    source = getattr(args, 'source', SOURCE).resolve()
    require(source.is_dir(), 'approved source checkout missing')
    paths = [args.identity, args.package_evidence, args.license_evidence, args.sbom]
    outputs = [args.output, args.discovery_output]
    all_paths = [p.resolve() for p in paths + outputs + [Path(str(p) + '.sha256') for p in outputs]]
    require(len(set(all_paths)) == len(all_paths), 'evidence/output paths must be distinct')
    require(not any(p.exists() or p.is_symlink() for p in outputs + [Path(str(p) + '.sha256') for p in outputs]), 'outputs must be new files')
    raw_identity, identity = load(args.identity)
    package_raw, package = load(args.package_evidence)
    license_raw, licenses = load(args.license_evidence)
    sbom_raw, sbom = load(args.sbom)
    binding = {'image': args.image, 'commit': args.commit, 'version': args.version, 'platform': args.platform}
    errors = []
    discovery = {**binding, 'format': 'rentemester-rust-agent-discovery-coverage/1', 'ok': False,
                 'status': 'failed', 'errors': ['Discovery was not collected because a prerequisite failed.']}
    try:
        require(run(['git', 'rev-parse', 'HEAD'], cwd=source).decode().strip() == args.commit, 'checkout source differs from candidate')
        require(not run(['git', 'status', '--porcelain', '--untracked-files=no'], cwd=source).strip(), 'source checkout is dirty')
        inspected = json.loads(run(['docker', 'image', 'inspect', args.image]))[0]
        require(inspected['Os'] + '/' + inspected['Architecture'] == args.platform, 'image platform mismatch')
        require(args.image in inspected.get('RepoDigests', []), 'immutable image digest not bound by Docker')
        with tempfile.TemporaryDirectory(prefix='rentemester-rust-qualification-') as temp:
            temp = Path(temp)
            run([sys.executable, str(SOURCE / 'scripts/release/verify-rust-package.py'), '--image', args.image,
                 '--identity', str(temp / 'identity.json'), '--evidence', str(temp / 'package.json'),
                 '--commit', args.commit, '--version', args.version, '--require-clean'])
            require(raw_identity == (temp / 'identity.json').read_bytes(), 'identity is not exact raw immutable packaged bytes')
            require(package == json.loads((temp / 'package.json').read_bytes()), 'package witness differs from actual verifier')
            require(package['status'] == 'passed' and package['sourceDirty'] is False, 'package is not clean and qualified')
            metadata = temp / 'cargo-metadata.json'
            metadata.write_bytes(run(['cargo', 'metadata', '--locked', '--all-features', '--format-version', '1'],
                                     cwd=source, timeout=600))
            run([sys.executable, str(SOURCE / 'scripts/release/check-rust-licenses.py'), '--source', str(source),
                 '--metadata', str(metadata), '--policy', str(SOURCE / 'scripts/release/rust-license-policy.json'),
                 '--output', str(temp / 'licenses.json')])
            require(normalized_license(licenses) == normalized_license(json.loads((temp / 'licenses.json').read_bytes())),
                    'license evidence differs from complete locked policy verifier')
        require(identity['inputs']['cargoLockSha256'] == sha((source / 'Cargo.lock').read_bytes()), 'packaged lock differs from license source')
        actual_sbom = json.loads(run(['docker', 'buildx', 'imagetools', 'inspect', args.image, '--format', '{{ json .SBOM }}']))
        package_count = validate_sbom(sbom, actual_sbom, args.platform)
        catalogue = json.loads(run(docker_base(inspected['Id']) + ['-ec', 'cat /opt/rentemester/native-mcp-catalogue.json']))
        expected = original_inventory(source)
        discovery = discovery_report(expected, catalogue, native_discovery(inspected['Id'], sorted(w['id'] for w in expected['workflows'])), binding)
        if not discovery['ok']:
            errors.append('Original MCP tools/workflows or native full-coverage qualification are incomplete.')
        # Compose the existing strict v2 manifest creator instead of duplicating its validator.
        env = {**os.environ, 'RELEASE_VERSION': args.version, 'RELEASE_GIT_COMMIT': args.commit,
               'RELEASE_BUILT_AT': identity['product']['builtAt'], 'RELEASE_IMAGE_REPOSITORY': args.image.split('@')[0],
               'RELEASE_IMAGE_DIGEST': args.image.split('@')[1], 'RELEASE_WORKFLOW_RUN_ID': os.environ.get('GITHUB_RUN_ID', '1'),
               'RELEASE_WORKFLOW_RUN_ATTEMPT': os.environ.get('GITHUB_RUN_ATTEMPT', '1'), 'RELEASE_SBOM_SHA256': 'sha256:' + sha(sbom_raw),
               'RELEASE_SUPPLY_CHAIN_SHA256': 'sha256:' + sha(package_raw),
               'RELEASE_AGENT_DISCOVERY_SHA256': 'sha256:' + sha(render(discovery))}
        run(['bun', str(SOURCE / 'scripts/release/create-rust-manifest.ts'), str(args.identity.resolve())], env=env)
    except (ValueError, KeyError, TypeError, OSError, subprocess.SubprocessError) as error:
        # Command text/stderr can include host paths. Stable typed failure, no volatile output.
        errors.append('qualification_failed:' + type(error).__name__ + (':' + str(error) if isinstance(error, ValueError) else ''))
        package_count = None
    supply = {**binding, 'format': 'rentemester-rust-supply-chain-evidence/1', 'ok': not errors,
              'status': 'failed' if errors else 'passed', 'errors': errors,
              'inputs': {'identitySha256': sha(raw_identity), 'packageEvidenceSha256': sha(package_raw),
                         'licenseEvidenceSha256': sha(license_raw), 'sbomSha256': sha(sbom_raw),
                         'controllerLicensePolicySha256': sha((SOURCE / 'scripts/release/rust-license-policy.json').read_bytes()),
                         'approvedSourceLicensePolicySha256': sha((source / 'scripts/release/rust-license-policy.json').read_bytes())},
              'discoverySha256': sha(render(discovery)), 'spdxPackageCount': package_count,
              'limitations': ['No independent reproducibility, registry provenance/approval, installed platform or cutover acceptance is claimed.']}
    write_report(args.discovery_output, discovery)
    write_report(args.output, supply)
    return 0 if supply['ok'] else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, default=SOURCE, help='Exact approved source checkout; verifier code stays controller-owned')
    for name in ('image', 'commit', 'version', 'platform'):
        parser.add_argument('--' + name, required=True)
    for name in ('identity', 'package-evidence', 'license-evidence', 'sbom', 'output', 'discovery-output'):
        parser.add_argument('--' + name, required=True, type=Path)
    try:
        return qualify(parser.parse_args())
    except (ValueError, KeyError, TypeError, OSError) as error:
        print('Rust candidate qualification REFUSED: ' + str(error), file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main())
