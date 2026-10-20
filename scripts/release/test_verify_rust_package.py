import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import unittest

SPEC = importlib.util.spec_from_file_location('verify', Path(__file__).with_name('verify-rust-package.py'))
VERIFY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFY)


def archive(extra=None, changed=False, mode=0o444):
    body = b'synthetic UI'
    identity = json.dumps({'format': 'rentemester-rust-build/1', 'files': [
        dict(path='web/index.html', size=len(body), sha256=hashlib.sha256(body).hexdigest(), executable=False)]}).encode()
    stream = io.BytesIO()
    with tarfile.open(fileobj=stream, mode='w') as out:
        for name, data in [('web/index.html', body + (b'changed' if changed else b'')), ('build-identity.json', identity)]:
            info = tarfile.TarInfo(name); info.size = len(data); info.mode = mode
            out.addfile(info, io.BytesIO(data))
        if extra:
            out.addfile(extra, io.BytesIO(b''))
    return stream.getvalue()


class PackageTests(unittest.TestCase):
    def test_exact_bytes_and_mode(self):
        _, identity, payloads = VERIFY.package_inventory(archive())
        self.assertEqual(payloads['web/index.html'], b'synthetic UI')
        self.assertEqual(len(identity['files']), 1)
        with self.assertRaisesRegex(ValueError, 'inventory differs'):
            VERIFY.package_inventory(archive(changed=True))
        with self.assertRaisesRegex(ValueError, 'runtime mode'):
            VERIFY.package_inventory(archive(mode=0o644))

    def test_refuses_links_duplicate_and_traversal_without_extracting(self):
        for name in ['../escape', '/absolute']:
            with self.assertRaisesRegex(ValueError, 'unsafe archive'):
                VERIFY.package_inventory(archive(tarfile.TarInfo(name)))
        for kind in [tarfile.SYMTYPE, tarfile.LNKTYPE, tarfile.FIFOTYPE]:
            member = tarfile.TarInfo('link'); member.type = kind; member.linkname = 'web/index.html'
            with self.assertRaisesRegex(ValueError, 'nonregular'):
                VERIFY.package_inventory(archive(member))
        duplicate = tarfile.TarInfo('web/index.html')
        with self.assertRaisesRegex(ValueError, 'duplicate'):
            VERIFY.package_inventory(archive(duplicate))


if __name__ == '__main__':
    unittest.main()
