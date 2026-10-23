"""Synthetic fixtures only: no downloads, builds, or real PDFium archives."""
import copy
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location('pdfium_licenses', Path(__file__).with_name('check-pdfium-licenses.py'))
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class PdfiumLicenseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='rm-pdfium-license-test-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.archives = self.root / 'archives'
        self.archives.mkdir()
        self.bundle = self.root / 'bundle'
        self.bundle.mkdir()
        self.pins_path = self.root / 'pins.json'
        self.inventory_path = self.root / 'inventory.json'
        # Deliberately not real legal grants: integrity acceptance is not legal
        # approval, and the test does not teach a keyword license classifier.
        self.contents = {'LICENSE': b'synthetic reviewed package notice\r\n',
                         'licenses/nested/component.txt': b'synthetic reviewed notice\x80\n',
                         'lib/libpdfium.so': b'synthetic pinned library\x00'}
        self.pins = {'format': 'rentemester-pdfium-artifacts/1', 'artifacts': []}
        self.inventory = {'format': CHECK.FORMAT, 'pinsSha256': '',
                          'policyApprovalClaim': False, 'artifacts': []}
        for platform in ['mac-arm64', 'mac-x64', 'linux-x64', 'linux-arm64']:
            name = 'pdfium-' + platform + '.tgz'
            data = self.tar_bytes(list(self.contents.items()))
            (self.archives / name).write_bytes(data)
            pin = {'archive': name, 'sha256': CHECK.sha(data),
                   'library': 'lib/libpdfium.so',
                   'librarySha256': CHECK.sha(self.contents['lib/libpdfium.so'])}
            self.pins['artifacts'].append(pin)
            self.inventory['artifacts'].append({
                'archive': name, 'archiveSha256': pin['sha256'],
                'library': pin['library'], 'librarySha256': pin['librarySha256'],
                'notices': [{'path': n, 'size': len(b), 'sha256': CHECK.sha(b),
                             'licenseExpression': None,
                             'review': 'Synthetic identified fixture; no legal approval.'}
                            for n, b in self.contents.items() if CHECK.notice_name(n)]})
        self.save_contract()
        for name, body in self.contents.items():
            target = self.bundle / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(body)
        (self.bundle / 'artifact.json').write_text(json.dumps(self.pins['artifacts'][0]))

    @staticmethod
    def tar_bytes(entries):
        result = io.BytesIO()
        with tarfile.open(fileobj=result, mode='w:gz') as archive:
            for name, body in entries:
                if isinstance(name, tarfile.TarInfo):
                    archive.addfile(name, io.BytesIO(body))
                else:
                    info = tarfile.TarInfo(name)
                    info.size = len(body)
                    archive.addfile(info, io.BytesIO(body))
        return result.getvalue()

    def save_contract(self):
        self.pins_path.write_text(json.dumps(self.pins))
        self.inventory['pinsSha256'] = CHECK.sha(self.pins_path.read_bytes())
        self.inventory_path.write_text(json.dumps(self.inventory))

    def archives_check(self):
        return CHECK.verify(self.pins_path, self.inventory_path, archives=self.archives)

    def bundle_check(self, artifact=None):
        return CHECK.verify(self.pins_path, self.inventory_path, bundle=self.bundle, artifact=artifact)

    def replace_archive(self, entries, repin=True):
        data = self.tar_bytes(entries)
        (self.archives / self.pins['artifacts'][0]['archive']).write_bytes(data)
        if repin:
            # Rebind outer hashes so tests exercise inner validation rather
            # than passing just because the outer compressed bytes changed.
            self.pins['artifacts'][0]['sha256'] = CHECK.sha(data)
            self.inventory['artifacts'][0]['archiveSha256'] = CHECK.sha(data)
            self.save_contract()

    def test_complete_all_four_and_selected_staged_fixture(self):
        first = self.archives_check()
        self.assertEqual(first, self.archives_check())
        self.assertEqual(len(first['artifacts']), 4)
        self.assertTrue(first['integrityVerified'])
        self.assertIs(first['policyApprovalClaim'], False)
        name = self.pins['artifacts'][0]['archive']
        self.assertEqual(self.bundle_check(name)['artifacts'], [name])
        with self.assertRaisesRegex(ValueError, 'selected artifact'):
            self.bundle_check('unknown.tgz')

    def test_nonempty_wrong_notice_is_refused(self):
        (self.bundle / 'LICENSE').write_bytes(b'nonempty but not the pinned license')
        with self.assertRaisesRegex(ValueError, 'notice content drift'):
            self.bundle_check()
        entries = dict(self.contents, LICENSE=b'nonempty but not the pinned license')
        self.replace_archive(list(entries.items()))
        with self.assertRaisesRegex(ValueError, 'notice content drift'):
            self.archives_check()

    def test_missing_nested_notice_and_unexpected_notice_refused(self):
        (self.bundle / 'licenses/nested/component.txt').unlink()
        with self.assertRaisesRegex(ValueError, 'notice path set'):
            self.bundle_check()
        self.replace_archive([(n, b) for n, b in self.contents.items() if not n.startswith('licenses/')])
        with self.assertRaisesRegex(ValueError, 'notice path set'):
            self.archives_check()
        self.replace_archive(list(self.contents.items()) + [('licenses/extra', b'extra')])
        with self.assertRaisesRegex(ValueError, 'notice path set'):
            self.archives_check()

    def test_traversal_absolute_alias_backslash_refused_before_extraction(self):
        for name in ['../escape', '/absolute', 'licenses/../escape', './LICENSE',
                     'licenses//alias', 'licenses\\alias', 'C:/escape']:
            with self.subTest(name=name):
                self.replace_archive(list(self.contents.items()) + [(name, b'bad')])
                with self.assertRaisesRegex(ValueError, 'unsafe path'):
                    self.archives_check()
        self.assertFalse((self.root / 'escape').exists())

    def test_duplicate_archive_member_refused(self):
        self.replace_archive(list(self.contents.items()) + [('LICENSE', self.contents['LICENSE'])])
        with self.assertRaisesRegex(ValueError, 'duplicate archive member'):
            self.archives_check()

    def test_symlink_hardlink_fifo_archive_members_refused_even_outside_notices(self):
        for kind in [tarfile.SYMTYPE, tarfile.LNKTYPE, tarfile.FIFOTYPE]:
            info = tarfile.TarInfo('unrelated')
            info.type = kind
            info.linkname = 'LICENSE'
            self.replace_archive(list(self.contents.items()) + [(info, b'')])
            with self.assertRaisesRegex(ValueError, 'nonregular archive member'):
                self.archives_check()

    def test_staged_symlink_file_directory_and_hardlink_refused(self):
        path = self.bundle / 'LICENSE'
        body = path.read_bytes()
        path.unlink()
        path.symlink_to('artifact.json')
        with self.assertRaisesRegex(ValueError, 'symlink'):
            self.bundle_check()
        path.unlink()
        path.write_bytes(body)
        nested = self.bundle / 'licenses/nested'
        nested.rename(self.bundle / 'moved')
        nested.symlink_to('../../moved', target_is_directory=True)
        with self.assertRaisesRegex(ValueError, 'symlink'):
            self.bundle_check()
        nested.unlink()
        (self.bundle / 'moved').rename(nested)
        import os
        os.link(path, self.bundle / 'extra-hardlink')
        with self.assertRaisesRegex(ValueError, 'hardlinked'):
            self.bundle_check()

    def test_library_and_archive_drift_refused(self):
        (self.bundle / 'lib/libpdfium.so').write_bytes(b'changed library')
        with self.assertRaisesRegex(ValueError, 'library drift'):
            self.bundle_check()
        entries = dict(self.contents)
        entries['lib/libpdfium.so'] = b'changed library'
        self.replace_archive(list(entries.items()))
        with self.assertRaisesRegex(ValueError, 'library drift'):
            self.archives_check()
        self.replace_archive(list(self.contents.items()), repin=False)
        with self.assertRaisesRegex(ValueError, 'archive drift'):
            self.archives_check()

    def test_missing_archive_refuses_whole_set(self):
        (self.archives / self.pins['artifacts'][-1]['archive']).unlink()
        with self.assertRaises(OSError):
            self.archives_check()

    def test_pin_inventory_and_metadata_drift_refused(self):
        original = copy.deepcopy(self.inventory)
        mutations = [lambda: self.inventory.update(format='unknown/1'),
                     lambda: self.inventory.update(policyApprovalClaim=True),
                     lambda: self.inventory.update(pinsSha256='0' * 64),
                     lambda: self.inventory['artifacts'].pop(),
                     lambda: self.inventory['artifacts'].append(self.inventory['artifacts'][0]),
                     lambda: self.inventory['artifacts'][0].update(librarySha256='0' * 64),
                     lambda: self.inventory['artifacts'][0]['notices'][0].update(size=True),
                     lambda: self.inventory['artifacts'][0]['notices'][0].update(review=''),
                     lambda: self.inventory['artifacts'][0]['notices'][0].update(sha256='unknown')]
        for mutation in mutations:
            self.inventory = copy.deepcopy(original)
            mutation()
            self.inventory_path.write_text(json.dumps(self.inventory))
            with self.assertRaises(ValueError):
                self.archives_check()
        self.inventory = original
        self.save_contract()
        meta = dict(self.pins['artifacts'][0], sha256='0' * 64)
        (self.bundle / 'artifact.json').write_text(json.dumps(meta))
        with self.assertRaisesRegex(ValueError, 'staged artifact pin drift'):
            self.bundle_check()
        self.pins_path.write_bytes(self.pins_path.read_bytes() + b'\n')
        with self.assertRaisesRegex(ValueError, 'pins drift'):
            self.archives_check()

    def test_duplicate_json_keys_and_unknown_artifact_refused(self):
        self.inventory_path.write_text('{"format":"a","format":"b"}')
        with self.assertRaisesRegex(ValueError, 'duplicate JSON key'):
            self.archives_check()
        self.save_contract()
        (self.bundle / 'artifact.json').write_text('{"archive":"unknown"}')
        with self.assertRaisesRegex(ValueError, 'unknown staged artifact'):
            self.bundle_check()

    def test_cli_machine_report_and_failure_without_partial_pass(self):
        cmd = [sys.executable, str(Path(__file__).with_name('check-pdfium-licenses.py')),
               '--pins', str(self.pins_path), '--inventory', str(self.inventory_path),
               '--archives', str(self.archives)]
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIs(json.loads(result.stdout)['policyApprovalClaim'], False)
        (self.archives / self.pins['artifacts'][-1]['archive']).write_bytes(b'wrong archive')
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, '')
        self.assertIn('refused', result.stderr)


if __name__ == '__main__':
    unittest.main()
