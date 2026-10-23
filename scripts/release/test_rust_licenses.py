"""Synthetic offline controls for the complete Cargo license-policy boundary."""
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SPEC = importlib.util.spec_from_file_location('rust_licenses', Path(__file__).with_name('check-rust-licenses.py'))
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class RustLicenseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.source = self.base / 'source'
        self.source.mkdir()
        self.recorded = Path('/synthetic/original')
        self.policy_path = self.base / 'policy.json'
        self.meta_path = self.base / 'metadata.json'
        self.policy = {'format': 'rentemester-rust-license-policy/1',
                       'allowedLicenses': ['MIT', 'Apache-2.0', 'BSD-3-Clause'],
                       'packageExceptions': [], 'normalizations': [], 'ownWorkspace': True}
        (self.source / 'Cargo.toml').write_text('''[workspace]
members = ["crates/product"]
exclude = ["vendor/sqlite"]
[workspace.package]
version = "1.0.0"
''')
        for directory, manifest in [
            ('crates/product', '[package]\nname = "product"\nversion.workspace = true\n'),
            ('vendor/sqlite', '[package]\nname = "sqlite"\nversion = "2.0.0"\nlicense = "MIT"\n')]:
            path = self.source / directory
            path.mkdir(parents=True)
            (path / 'Cargo.toml').write_text(manifest)
        self.origin = 'registry+https://github.com/rust-lang/crates.io-index'
        self.lock = '''version = 4
[[package]]
name = "product"
version = "1.0.0"
[[package]]
name = "sqlite"
version = "2.0.0"
[[package]]
name = "external"
version = "3.0.0"
source = "ORIGIN"
checksum = "CHECKSUM"
'''.replace('ORIGIN', self.origin).replace('CHECKSUM', 'a' * 64)
        own = self.package('product', '1.0.0', None, 'crates/product', None)
        vendor = self.package('sqlite', '2.0.0', None, 'vendor/sqlite', 'MIT')
        external = self.package('external', '3.0.0', self.origin, None, 'MIT OR Apache-2.0')
        packages = [own, vendor, external]
        self.meta = {'version': 1, 'workspace_root': str(self.recorded), 'packages': packages,
                     'workspace_members': [own['id']],
                     'resolve': {'nodes': [{'id': p['id'], 'dependencies': []} for p in packages]}}

    def package(self, name, version, origin, directory, license):
        identity = (origin + '#' + name + '@' + version if origin else
                    'path+file://' + str(self.recorded / directory) + '#' + version)
        return {'id': identity, 'name': name, 'version': version, 'source': origin,
                'manifest_path': str(self.recorded / directory / 'Cargo.toml') if directory else '/cache/external/Cargo.toml',
                'license': license, 'license_file': None, 'metadata': {}}

    def write_inputs(self):
        (self.source / 'Cargo.lock').write_text(self.lock)
        self.meta_path.write_text(json.dumps(self.meta))
        self.policy_path.write_text(json.dumps(self.policy))

    def verify(self):
        self.write_inputs()
        return CHECK.verify(self.source, self.meta_path, self.policy_path)

    def refused(self, pattern):
        with self.assertRaisesRegex(ValueError, pattern):
            self.verify()

    def expression(self, expression):
        return CHECK.license_check('external', '3.0.0', expression, self.policy,
                                   CHECK.policy_check(self.policy))

    def test_complete_relocated_inventory_and_reproducible_hashes(self):
        result = self.verify()
        self.assertEqual(result, CHECK.verify(self.source, self.meta_path, self.policy_path))
        self.assertEqual(result['status'], 'PASS')
        self.assertEqual(result['packageCount'], 3)
        rows = {p['name']: p for p in result['packages']}
        self.assertEqual(rows['product']['license']['classification'], 'own-workspace')
        self.assertIsNone(rows['product']['license']['original'])
        self.assertEqual(rows['sqlite']['license']['classification'], 'third-party')
        self.assertEqual(rows['external']['checksum'], 'a' * 64)
        self.assertEqual(rows['external']['license']['selectedTerms'], ['MIT'])
        self.assertEqual(result['inputs']['cargoLockSha256'], CHECK.sha(self.lock.encode()))
        self.assertEqual(result['inputs']['policySha256'], CHECK.sha(self.policy_path.read_bytes()))
        self.assertEqual(len(result['inputs']['manifests']), 3)
        self.assertTrue(result['limitations'])

    def test_no_deps_metadata_refused(self):
        self.meta['resolve'] = None
        self.refused('complete resolve')

    def test_filtered_metadata_and_extra_packages_refused(self):
        original = copy.deepcopy(self.meta)
        self.meta['packages'].pop()
        self.refused('coverage drift')
        self.meta = original
        self.meta['packages'].append(self.package('extra', '1', self.origin, None, 'MIT'))
        self.refused('coverage drift')

    def test_missing_resolve_node_or_dependency_refused(self):
        original = copy.deepcopy(self.meta)
        self.meta['resolve']['nodes'].pop()
        self.refused('incomplete/duplicate resolve')
        self.meta = original
        self.meta['resolve']['nodes'][0]['dependencies'] = ['missing']
        self.refused('resolve dependency')

    def test_lock_identity_and_metadata_source_drift(self):
        for field, value in [('name', 'forged'), ('version', '9.9.9'), ('source', 'registry+https://other.invalid')]:
            with self.subTest(field=field):
                original = copy.deepcopy(self.meta)
                self.meta['packages'][2][field] = value
                self.refused('coverage drift')
                self.meta = original

    def test_registry_checksum_and_package_id_drift(self):
        self.lock = self.lock.replace('a' * 64, 'broken')
        self.refused('checksum')
        self.lock = self.lock.replace('broken', 'a' * 64)
        self.meta['packages'][2]['checksum'] = 'b' * 64
        self.refused('checksum drift')
        del self.meta['packages'][2]['checksum']
        old = self.meta['packages'][2]['id']
        self.meta['packages'][2]['id'] = old + '-forged'
        self.meta['resolve']['nodes'][2]['id'] = old + '-forged'
        self.refused('package ID drift')

    def test_duplicate_lock_or_metadata_refused(self):
        self.meta['packages'].append(copy.deepcopy(self.meta['packages'][2]))
        self.refused('duplicate/invalid metadata')
        self.meta['packages'].pop()
        self.lock += '\n[[package]]\nname = "product"\nversion = "1.0.0"\n'
        self.refused('duplicate lock')

    def test_workspace_membership_cannot_be_inferred_from_path(self):
        self.meta['workspace_members'] = []
        self.refused('missing/null license')

    def test_vendor_cannot_be_promoted_to_own(self):
        self.meta['workspace_members'].append(self.meta['packages'][1]['id'])
        self.refused('not owned by root')

    def test_foreign_path_refused(self):
        self.meta['packages'][1]['manifest_path'] = '/foreign/sqlite/Cargo.toml'
        self.refused('foreign path')

    def test_local_name_version_license_and_id_drift(self):
        path = self.source / 'vendor/sqlite/Cargo.toml'
        original = path.read_text()
        for before, after, error in [('sqlite', 'forged', 'identity drift'),
                                     ('2.0.0', '2.0.1', 'identity drift'),
                                     ('MIT', 'Apache-2.0', 'license drift')]:
            with self.subTest(after=after):
                path.write_text(original.replace(before, after))
                self.refused(error)
        path.write_text(original)
        old = self.meta['packages'][1]['id']
        self.meta['packages'][1]['id'] = old.replace('2.0.0', '2.0.1')
        self.meta['resolve']['nodes'][1]['id'] = self.meta['packages'][1]['id']
        self.refused('path package ID drift')

    def test_unknown_and_null_license_refused(self):
        for license in ['UnknownLicense', None, '', 'LicenseRef-private', 'MPL-2.0']:
            with self.subTest(license=license):
                self.meta['packages'][2]['license'] = license
                self.refused('license')

    def test_or_selects_permissive_and_retains_all_and_terms(self):
        self.assertEqual(self.expression('MPL-2.0 OR MIT')['selectedTerms'], ['MIT'])
        self.assertEqual(self.expression('(MPL-2.0 OR MIT) AND Apache-2.0')['selectedTerms'],
                         ['Apache-2.0', 'MIT'])
        with self.assertRaisesRegex(ValueError, 'unapproved'):
            self.expression('MIT AND MPL-2.0')
        with self.assertRaisesRegex(ValueError, 'unapproved'):
            self.expression('MIT AND (MPL-2.0 OR UnknownLicense)')

    def test_with_requires_whole_branch_approval(self):
        term = 'Apache-2.0 WITH LLVM-exception'
        with self.assertRaisesRegex(ValueError, 'unapproved'):
            self.expression(term)
        self.assertEqual(self.expression(term + ' OR MIT')['selectedTerms'], ['MIT'])
        self.policy['allowedLicenses'].append(term)
        self.assertEqual(self.expression(term)['selectedTerms'], [term])
        with self.assertRaisesRegex(ValueError, 'unapproved'):
            self.expression('Apache-2.0 WITH Other-exception')

    def test_package_exception_exact_full_original_identity(self):
        row = {'name': 'external', 'version': '3.0.0', 'expression': 'Apache-2.0 WITH LLVM-exception',
               'reason': 'Synthetic approval only', 'sourceUrl': 'https://example.invalid/license'}
        self.policy['packageExceptions'] = [row]
        result = self.expression(row['expression'])
        self.assertEqual(result['packageException'], row)
        for field, value in [('name', 'other'), ('version', '3.0.1'),
                             ('expression', row['expression'] + ' AND MIT')]:
            changed = dict(row, **{field: value})
            self.policy['packageExceptions'] = [changed]
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, 'unapproved'):
                self.expression(row['expression'])

    def test_slash_normalization_is_package_version_and_original_bound(self):
        self.policy['normalizations'] = [{'name': 'external', 'version': '3.0.0',
                                          'from': 'MIT/Apache-2.0', 'to': 'MIT OR Apache-2.0',
                                          'reason': 'Synthetic historical spelling'}]
        self.assertEqual(self.expression('MIT/Apache-2.0')['selectedTerms'], ['MIT'])
        for name, version, expression in [('other', '3.0.0', 'MIT/Apache-2.0'),
                                           ('external', '3.0.1', 'MIT/Apache-2.0'),
                                           ('external', '3.0.0', 'MIT / Apache-2.0')]:
            with self.subTest(name=name, version=version, expression=expression), self.assertRaises(ValueError):
                CHECK.license_check(name, version, expression, self.policy, CHECK.policy_check(self.policy))

    def test_malformed_expressions_never_hidden_by_approved_or(self):
        for expression in ['MIT OR', 'MIT AND', 'MIT GPL-3.0', 'MIT OR (Apache-2.0',
                           'MIT OR Apache-2.0)', 'MIT OR /', 'MIT OR Unknown/Thing',
                           '(MIT) WITH LLVM-exception', 'MIT WITH', 'MIT WITH x WITH y',
                           'MIT or Apache-2.0', 'MIT || Apache-2.0', '()', 'MIT;']:
            with self.subTest(expression=expression), self.assertRaises(ValueError):
                self.expression(expression)

    def test_policy_malformed_or_wildcard_refused(self):
        original = copy.deepcopy(self.policy)
        for mutate in [lambda p: p.update(ownWorkspace=False),
                       lambda p: p.update(format='unknown'),
                       lambda p: p.update(allowedLicenses=['MIT OR Apache-2.0']),
                       lambda p: p.update(allowedLicenses=['MIT', 'MIT']),
                       lambda p: p.update(extra=True),
                       lambda p: p.update(packageExceptions=[{'name': '*', 'version': '1',
                           'expression': 'MPL-2.0', 'reason': 'test', 'sourceUrl': 'https://example.invalid'}])]:
            self.policy = copy.deepcopy(original)
            mutate(self.policy)
            self.refused('policy|allowedLicenses')

    def test_json_duplicate_keys_refused(self):
        self.write_inputs()
        self.policy_path.write_text(self.policy_path.read_text().replace('"ownWorkspace": true',
                                    '"ownWorkspace": false, "ownWorkspace": true'))
        with self.assertRaisesRegex(ValueError, 'duplicate JSON key'):
            CHECK.verify(self.source, self.meta_path, self.policy_path)

    def test_cli_reports_only_after_success_and_does_not_overwrite(self):
        self.write_inputs()
        output = self.base / 'report.json'
        command = [sys.executable, str(Path(CHECK.__file__)), '--source', str(self.source),
                   '--metadata', str(self.meta_path), '--policy', str(self.policy_path), '--output', str(output)]
        result = subprocess.run(command, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        accepted = output.read_bytes()
        self.assertEqual(json.loads(accepted)['status'], 'PASS')
        result = subprocess.run(command, capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(output.read_bytes(), accepted)
        output.unlink()
        self.meta['packages'].pop()
        self.write_inputs()
        result = subprocess.run(command, capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('REFUSED', result.stderr)
        self.assertFalse(output.exists())


if __name__ == '__main__':
    unittest.main()
