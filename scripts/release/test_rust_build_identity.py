import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location('identity', Path(__file__).with_name('create-rust-build-identity.py'))
IDENTITY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(IDENTITY)


class IdentityTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='rm-rust-identity-')
        self.root = Path(self.temp.name)
        self.source = self.root / 'source'; self.bundle = self.root / 'bundle'
        def put(base, name, data):
            p = base / name; p.parent.mkdir(parents=True, exist_ok=True); p.write_text(data)
            return p
        put(self.source, 'Cargo.toml', '[workspace.package]\nversion="0.2.0"\nrust-version="1.94.1"\n')
        for p in ['package.json', 'app/package.json']:
            put(self.source, p, '{"version":"0.2.0"}')
        for p in ['Cargo.lock', 'bun.lock', 'vendor/synthetic/LICENSE', 'scripts/release/rust-entrypoint.sh']:
            put(self.source, p, 'synthetic ' + p)
        library = put(self.bundle, 'bin/pdfium/lib/libpdfium.so', 'synthetic library')
        artifact = {'archive': 'pdfium-linux-arm64.tgz', 'sha256': 'a' * 64,
                    'library': 'lib/libpdfium.so', 'librarySha256': IDENTITY.digest(library)}
        pins = {'artifacts': [artifact], 'binding': {'version': '0.9.4'}, 'release': 'synthetic'}
        put(self.source, 'scripts/rust/pdf-worker/artifacts.json', json.dumps(pins))
        put(self.bundle, 'bin/pdfium/artifact.json', json.dumps(artifact))
        put(self.bundle, 'bin/pdfium/LICENSE', 'synthetic license')
        put(self.bundle, 'bin/pdfium/licenses/test', 'synthetic third-party license')
        put(self.bundle, 'native-server.json', json.dumps({'format': 'rentemester-native-server/1',
            'version': '0.2.0', 'platform': 'linux-aarch64', 'apiEpoch': 1, 'controlSchema': 31, 'companySchema': 36}))
        put(self.bundle, 'native-mcp-catalogue.json', json.dumps({'format': 'rentemester-native-mcp-catalogue/1',
            'version': '0.2.0', 'tools': [{'name': 'synthetic'}]}))
        put(self.bundle, 'bin/rustc.txt', 'rustc 1.94.1 (synthetic)\nhost: aarch64-unknown-linux-gnu\n')
        put(self.bundle, 'bin/cargo.txt', 'cargo 1.94.1 (synthetic)\n')
        for name in ['server', 'rust', 'mcp', 'pdf-worker']:
            put(self.bundle, 'bin/rentemester-' + name, 'synthetic binary').chmod(0o700)
        put(self.bundle, 'web/index.html', 'synthetic UI')
        put(self.bundle, 'debian-packages.txt', 'synthetic OS packages')
        self.images = {k: 'synthetic@sha256:' + 'b' * 64 for k in ['rustBuilder', 'webBuilder', 'runtime']}

    def tearDown(self):
        for p in self.root.rglob('*'):
            if p.is_dir(): p.chmod(0o700)
        self.temp.cleanup()

    def create(self):
        return IDENTITY.create(self.source, self.bundle, 'c' * 40, 1792533287, True, self.images, '0.2.0')

    def test_exact_inventory_versions_compiler_vendor_and_readonly_modes(self):
        result = self.create()
        self.assertEqual(result['product']['version'], '0.2.0')
        self.assertTrue(result['product']['sourceDirty'])
        self.assertEqual(result['toolchain']['target'], 'aarch64-unknown-linux-gnu')
        self.assertEqual(result['schemas']['controlSchema'], 31)
        self.assertTrue(result['inputs']['vendor'])
        for row in result['files']:
            p = self.bundle / row['path']
            self.assertEqual(row['sha256'], hashlib.sha256(p.read_bytes()).hexdigest())
            self.assertEqual(p.stat().st_mode & 0o777, 0o555 if row['executable'] else 0o444)
        self.assertNotIn('build-identity.json', [v['path'] for v in result['files']])

    def test_refuses_missing_runtime_and_version_drift(self):
        (self.bundle / 'bin/rentemester-server').unlink()
        with self.assertRaisesRegex(ValueError, 'required runtime'):
            self.create()
        (self.source / 'app/package.json').write_text('{"version":"0.3.0"}')
        with self.assertRaisesRegex(ValueError, 'version drift'):
            self.create()

    def test_refuses_unpinned_image_and_changed_library(self):
        self.images['runtime'] = 'synthetic:latest'
        with self.assertRaisesRegex(ValueError, 'digest-pinned'):
            self.create()
        self.images['runtime'] = 'synthetic@sha256:' + 'b' * 64
        (self.bundle / 'bin/pdfium/lib/libpdfium.so').write_text('changed')
        with self.assertRaisesRegex(ValueError, 'PDF artifact'):
            self.create()

    def test_refuses_symlink_and_wrong_compiler(self):
        (self.bundle / 'unexpected-link').symlink_to('web/index.html')
        with self.assertRaisesRegex(ValueError, 'unsafe staged'):
            self.create()
        (self.bundle / 'bin/rustc.txt').write_text('rustc 1.95.0 (synthetic)\nhost: aarch64-unknown-linux-gnu')
        with self.assertRaisesRegex(ValueError, 'compiler identity'):
            self.create()

    def test_empty_license_directories_are_not_license_evidence(self):
        (self.bundle / 'bin/pdfium/licenses/test').unlink()
        (self.bundle / 'bin/pdfium/licenses/empty').mkdir()
        with self.assertRaisesRegex(ValueError, 'licenses must be retained'):
            self.create()


if __name__ == '__main__':
    unittest.main()
