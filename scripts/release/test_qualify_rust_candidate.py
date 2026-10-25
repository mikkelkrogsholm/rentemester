"""Synthetic contract tests; no Docker, provider, Cargo or release publication."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from types import SimpleNamespace

SPEC = importlib.util.spec_from_file_location('qualifier', Path(__file__).with_name('qualify-rust-candidate.py'))
Q = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(Q)


def fixture():
    step = dict(operation={'surface': 'mcp', 'name': 'original_read'}, expectedSafety='read',
                expectedIdempotent=True, requiresActor=False, requiresConfirmation=False, retryClass='safe-read')
    workflow = dict(id='original-flow', title='Read original', intendedOutcome='Read source', prerequisites=['Select'],
                    nonGoals=['No writes'], steps=[step], blockers=['Missing'], recovery=['Stop'], stopConditions=['Denied'])
    schema = {'type': 'object', 'properties': {'company': {'type': 'string'}}, 'required': ['company']}
    annotations = dict(readOnlyHint=True, destructiveHint=False, idempotentHint=True, openWorldHint=False)
    expected = {'tools': [{'name': 'original_read', 'inputSchema': schema, 'annotations': annotations}], 'workflows': [workflow]}
    packaged = {'format': 'rentemester-native-mcp-catalogue/1', 'tools': [{'name': 'original_read', 'description': 'Read canonical state', 'inputSchema': copy.deepcopy(schema), 'annotations': dict(annotations)}]}
    records = {'__about__': {'status': 200, 'body': {'fullCoverage': True, 'catalogue': {'fullCoverage': True}}},
               'original-flow': {'status': 200, 'body': {'workflow': workflow}}}
    return expected, packaged, records


def license_report():
    return {'format': 'rentemester-rust-license-report/1', 'status': 'PASS', 'packageCount': 1,
            'inputs': {'cargoLockSha256': 'b' * 64, 'policySha256': 'c' * 64,
                       'metadataSha256': 'd' * 64, 'manifests': {'Cargo.toml': 'e' * 64}},
            'packages': [{'id': 'path+file:///checkout#synthetic@1.0.0', 'name': 'synthetic', 'version': '1.0.0',
                          'source': None, 'checksum': None, 'manifestPath': '/checkout/Cargo.toml',
                          'licenseFile': None, 'license': {'selectedTerms': ['MIT']}}]}


class QualifierTests(unittest.TestCase):
    def report(self, *args):
        return Q.discovery_report(*args, {'image': 'synthetic@sha256:' + 'f' * 64})

    def test_complete_synthetic_contract_is_deterministic(self):
        values = fixture()
        self.assertTrue(self.report(*values)['ok'])
        self.assertEqual(Q.render(self.report(*values)), Q.render(self.report(*values)))

    def test_extra_native_names_do_not_replace_original(self):
        expected, packaged, records = fixture()
        packaged['tools'] = [{'name': 'native_extra', 'description': 'Useful read'}]
        report = self.report(expected, packaged, records)
        self.assertFalse(report['ok'])
        self.assertEqual(report['missingOriginalToolNames'], ['original_read'])

    def test_refusal_only_alias_never_qualifies(self):
        expected, packaged, records = fixture()
        packaged['tools'][0]['description'] = 'Refusal-only legacy binding; not implemented'
        report = self.report(expected, packaged, records)
        self.assertFalse(report['ok'])
        self.assertEqual(report['refusalOnlyOriginalToolNames'], ['original_read'])

    def test_original_gdpr_flat_signature_cannot_be_replaced_by_native_body(self):
        baseline = json.loads((Q.SOURCE / 'docs/architecture/rust-target/implementation/baseline/contracts-mcp.json').read_bytes())
        row = next(row for row in baseline['rows'] if row['id'] == 'mcp:gdpr_export')
        expected, packaged, records = fixture()
        expected['tools'][0]['name'] = packaged['tools'][0]['name'] = 'gdpr_export'
        expected['workflows'][0]['steps'][0]['operation']['name'] = 'gdpr_export'
        expected['tools'][0]['inputSchema'] = row['contracts']['input']['definition']['schema']
        packaged['tools'][0]['inputSchema'] = {'type': 'object', 'properties': {
            'company': {'type': 'string'}, 'body': {'type': 'string'}, 'confirm': {'type': 'boolean'}},
            'required': ['company', 'body', 'confirm']}
        report = self.report(expected, packaged, records)
        self.assertEqual(report['missingOriginalToolNames'], [])
        self.assertTrue(report['nativeFullCoverage'])
        self.assertFalse(report['ok'])
        self.assertEqual(report['incompatibleOriginalToolSignatures'], ['gdpr_export'])
        # Corrected validation bounds/prose do not change the public field/type ABI.
        packaged['tools'][0]['inputSchema'] = copy.deepcopy(expected['tools'][0]['inputSchema'])
        packaged['tools'][0]['inputSchema']['properties']['company'].update(description='Canonical slug', maxLength=256)
        self.assertTrue(self.report(expected, packaged, records)['ok'])
        packaged['tools'][0]['annotations']['readOnlyHint'] = False
        self.assertEqual(self.report(expected, packaged, records)['incompatibleOriginalToolSignatures'], ['gdpr_export'])

    def test_missing_original_workflow_report_is_exact(self):
        expected, packaged, records = fixture()
        records['original-flow'] = {'status': 404, 'body': {'code': 'workflow_not_found'}}
        report = self.report(expected, packaged, records)
        self.assertFalse(report['ok'])
        self.assertEqual(report['missingOriginalWorkflowIds'], ['original-flow'])

    def test_partial_native_metadata_is_not_full_parity(self):
        expected, packaged, records = fixture()
        records['__about__']['body']['fullCoverage'] = False
        self.assertFalse(self.report(expected, packaged, records)['ok'])

    def test_workflow_title_cannot_replace_original_step_or_safety(self):
        for alteration in ('operation', 'retryClass', 'requiresConfirmation'):
            expected, packaged, records = fixture()
            records = copy.deepcopy(records)
            records['original-flow']['body']['workflow']['steps'][0][alteration] = None
            report = self.report(expected, packaged, records)
            self.assertFalse(report['ok'])
            self.assertEqual(report['invalidOriginalWorkflowIds'], ['original-flow'])

    def test_duplicate_packaged_names_refuse(self):
        expected, packaged, records = fixture()
        packaged['tools'] *= 2
        with self.assertRaisesRegex(ValueError, 'duplicate packaged'):
            self.report(expected, packaged, records)

    def test_actual_platform_spdx_document_required(self):
        doc = {'spdxVersion': 'SPDX-2.3', 'SPDXID': 'SPDXRef-DOCUMENT', 'dataLicense': 'CC0-1.0',
               'packages': [{'name': 'synthetic'}], 'creationInfo': {'creators': ['Tool: synthetic']}}
        self.assertEqual(Q.validate_sbom(doc, {'linux/amd64': {'SPDX': doc}}, 'linux/amd64'), 1)
        for actual in ({}, {'linux/arm64': {'SPDX': doc}}, {'SPDX': {**doc, 'packages': []}}, None):
            with self.assertRaises(ValueError):
                Q.validate_sbom(doc, actual, 'linux/amd64')
        with self.assertRaises(ValueError):
            Q.validate_sbom({'packageCount': 1}, {'SPDX': {'packageCount': 1}}, 'linux/amd64')

    def test_license_relocation_preserves_full_locked_decisions(self):
        left = license_report(); right = copy.deepcopy(left)
        right['inputs']['metadataSha256'] = 'f' * 64
        right['packages'][0]['manifestPath'] = '/other/Cargo.toml'
        right['packages'][0]['id'] = 'path+file:///other#synthetic@1.0.0'
        self.assertEqual(Q.normalized_license(left), Q.normalized_license(right))
        right['packages'][0]['license']['selectedTerms'] = ['Unapproved']
        self.assertNotEqual(Q.normalized_license(left), Q.normalized_license(right))
        right['packageCount'] = 2
        with self.assertRaisesRegex(ValueError, 'incomplete'):
            Q.normalized_license(right)

    def test_reports_exclusive_raw_bytes_sidecar_and_no_paths(self):
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder) / 'report.json'
            Q.write_report(target, {'status': 'failed', 'ok': False})
            self.assertEqual(target.read_bytes(), Q.render({'status': 'failed', 'ok': False}))
            self.assertEqual(Path(str(target) + '.sha256').read_text(), Q.sha(target.read_bytes()) + '  report.json\n')
            with self.assertRaises(FileExistsError):
                Q.write_report(target, {'ok': True})

    def test_mutable_image_refuses_before_docker(self):
        class Args:
            image = 'ghcr.io/mikkelkrogsholm/rentemester:latest'
        with patch.object(Q, 'run') as command:
            with self.assertRaisesRegex(ValueError, 'immutable'):
                Q.qualify(Args())
            command.assert_not_called()

    def test_original_inventory_reads_explicit_approved_checkout(self):
        row = {'id': 'mcp:original', 'contracts': {'input': {'definition': {'schema': {'type': 'object'}}}},
               'wire': {'interfaceOnlyBoundaries': {'annotations': {'readOnlyHint': True}}}}
        baseline = {'format': 'rentemester-r01-contract-inventory/1', 'surface': 'mcp', 'rows': [row]}
        with patch.object(Q, 'run', return_value=json.dumps({'workflows': [{'id': 'flow'}]}).encode()) as command:
            with patch.object(Q, 'load', return_value=(b'original baseline', baseline)) as load:
                result = Q.original_inventory(Path('/synthetic-approved-source'))
                self.assertEqual(result['tools'][0]['inputSchema'], {'type': 'object'})
                self.assertEqual(command.call_args.kwargs['cwd'], Path('/synthetic-approved-source'))
                self.assertEqual(load.call_args.args[0], Path('/synthetic-approved-source/docs/architecture/rust-target/implementation/baseline/contracts-mcp.json'))

    def test_failed_source_binding_writes_honest_deterministic_witnesses(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            paths = {}
            for name in ['identity', 'package_evidence', 'license_evidence', 'sbom']:
                paths[name] = root / (name + '.json')
                paths[name].write_text('{}\n')
            args = SimpleNamespace(image='ghcr.io/mikkelkrogsholm/rentemester@sha256:' + 'a' * 64,
                                   commit='b' * 40, version='0.2.0', platform='linux/amd64',
                                   output=root / 'supply-chain-evidence.json',
                                   discovery_output=root / 'agent-discovery-coverage-report.json', **paths)
            with patch.object(Q, 'run', return_value=('c' * 40 + '\n').encode()):
                self.assertEqual(Q.qualify(args), 1)
            first = args.output.read_bytes()
            supply = json.loads(first)
            self.assertFalse(supply['ok'])
            self.assertIn('checkout source differs', supply['errors'][0])
            self.assertEqual(supply['inputs']['identitySha256'], Q.sha(paths['identity'].read_bytes()))
            self.assertNotIn(folder, first.decode())
            self.assertFalse(json.loads(args.discovery_output.read_bytes())['ok'])
            args.output = root / 'second.json'; args.discovery_output = root / 'second-discovery.json'
            with patch.object(Q, 'run', return_value=('c' * 40 + '\n').encode()):
                self.assertEqual(Q.qualify(args), 1)
            self.assertEqual(first, args.output.read_bytes())

    def test_dockerfile_keeps_pins_guard_and_independent_cache(self):
        docker = (Q.SOURCE / 'Dockerfile.rust').read_text()
        self.assertIn('ARG RUST_BUILD_CACHE_ID', docker)
        self.assertIn('id=${RUST_BUILD_CACHE_ID}-${TARGETARCH}', docker)
        self.assertNotIn('sharing=locked', docker)
        self.assertIn('scripts/rust/cargo-guard.sh cargo build --release --locked', docker)
        self.assertIn('USER 10001:10001', docker)
        self.assertIn('case "$RENTEMESTER_SOURCE_DIRTY" in true|false)', docker)
        self.assertNotIn('test "$RENTEMESTER_SOURCE_DIRTY" = false', docker)
        self.assertIn('RENTEMESTER_BUILD_REVISION="$RENTEMESTER_GIT_COMMIT"', docker)
        self.assertEqual(docker.count('@sha256:'), 4)


if __name__ == '__main__':
    unittest.main()
