#!/usr/bin/env python3
"""Copy pinned native programs and web assets into a fresh private candidate; never builds or installs."""
import argparse
import ctypes
import errno
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import sys
import tempfile
import unittest
from unittest import mock


MANIFEST = "native-cli-candidate.json"


def absolute_path(value):
    path = Path(value)
    if not path.is_absolute():
        raise ValueError("artifact and output paths must be absolute: " + str(path))
    # Do not resolve away a symlink before validating the supplied path.
    for part in (path, *path.parents):
        if part.is_symlink():
            raise ValueError("symlink path is not allowed: " + str(part))
    return path.resolve()


def regular(path, executable=False):
    mode = path.lstat().st_mode
    if not stat.S_ISREG(mode):
        raise ValueError("expected a regular, non-symlink artifact: " + str(path))
    if executable and not mode & 0o111:
        raise ValueError("binary must be executable: " + str(path))
    return path


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def copy_artifact(source, destination, executable=False):
    regular(source, executable)
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    destination.chmod(0o755 if executable else 0o644)


def publish(stage, output):
    """Atomic no-replace directory rename, including a concurrently created empty output."""
    libc = ctypes.CDLL(None, use_errno=True)
    if sys.platform == "darwin":
        rename = libc.renamex_np
        rename.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_uint]
        arguments = (os.fsencode(stage), os.fsencode(output), 0x00000004)  # RENAME_EXCL
    elif sys.platform.startswith("linux") and hasattr(libc, "renameat2"):
        rename = libc.renameat2
        rename.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint]
        arguments = (-100, os.fsencode(stage), -100, os.fsencode(output), 1)  # RENAME_NOREPLACE
    else:
        raise ValueError("atomic fresh-output rename requires macOS or Linux renameat2")
    rename.restype = ctypes.c_int
    if rename(*arguments) != 0:
        code = ctypes.get_errno()
        raise OSError(code, os.strerror(code), str(output))


def assemble(args):
    if not re.fullmatch(r"[0-9a-f]{40}", args.revision):
        raise ValueError("source revision must be a full40SHA (lowercase Git SHA)")
    output = absolute_path(args.output)
    if os.path.lexists(output):
        raise ValueError("candidate assembly requires a fresh output path")
    cli, mcp, server = (regular(absolute_path(value), True) for value in
                        (args.cli_binary, args.mcp_binary, args.server_binary))
    web = absolute_path(args.web_dir)
    if not web.is_dir():
        raise ValueError("web source must be a real directory")
    regular(web / "index.html")
    web_files = sorted(web.rglob("*"))
    for source in web_files:
        mode = source.lstat().st_mode
        if not (stat.S_ISDIR(mode) or stat.S_ISREG(mode)):
            raise ValueError("unsupported web artifact: " + str(source))
    for source in (cli, mcp, server, web):
        if output.is_relative_to(source) or source.is_relative_to(output):
            raise ValueError("source and output must not overlap")
    output.parent.mkdir(parents=True, exist_ok=True)
    # Same-filesystem staging; an error never exposes a partial final tree.
    with tempfile.TemporaryDirectory(prefix=".rentemester-native-cli-", dir=output.parent) as temporary:
        stage = Path(temporary) / "candidate"
        (stage / "web").mkdir(parents=True)
        for source, name in ((cli, "rentemester"), (cli, "rentemester-rust"),
                             (mcp, "rentemester-mcp"), (server, "rentemester-server")):
            copy_artifact(source, stage / "bin" / name, True)
        for source in web_files:
            target = stage / "web" / source.relative_to(web)
            if source.is_dir():
                target.mkdir(parents=True, exist_ok=True)
            else:
                copy_artifact(source, target)
        entries = [{"path": p.relative_to(stage).as_posix(), "size": p.stat().st_size,
                    "sha256": digest(p), "executable": bool(p.stat().st_mode & 0o111)}
                   for p in sorted(stage.rglob("*")) if p.is_file()]
        (stage / MANIFEST).write_text(json.dumps({
            "format": "rentemester-native-cli-candidate/1",
            "sourceRevision": args.revision,
            "candidateOnly": True,
            "files": entries,
        }, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        publish(stage, output)
    return output


class AssemblyFixture(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="rentemester-native-cli-fixture-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.web = self.root / "web"
        (self.web / "assets/empty").mkdir(parents=True)
        (self.web / "index.html").write_bytes(b"<main>synthetic candidate</main>\n")
        (self.web / "assets/app.js").write_bytes(b"synthetic web\x00\xff")
        self.binaries = []
        for name in ("cli", "mcp", "server"):
            path = self.root / name
            path.write_bytes(b"\x7fELF synthetic " + name.encode() + b"\x00\xff")
            path.chmod(0o751)
            self.binaries.append(path)
        self.args = argparse.Namespace(cli_binary=str(self.binaries[0]), mcp_binary=str(self.binaries[1]),
            server_binary=str(self.binaries[2]), web_dir=str(self.web), output=str(self.root / "out/candidate"),
            revision="a" * 40)

    def test_exact_bytes_inventory_modes_and_independent_copies(self):
        output = assemble(self.args)
        expected = {"bin/rentemester": self.binaries[0], "bin/rentemester-rust": self.binaries[0],
                    "bin/rentemester-mcp": self.binaries[1], "bin/rentemester-server": self.binaries[2],
                    "web/index.html": self.web / "index.html", "web/assets/app.js": self.web / "assets/app.js"}
        manifest = json.loads((output / MANIFEST).read_text())
        self.assertEqual(set(manifest), {"format", "sourceRevision", "candidateOnly", "files"})
        self.assertEqual(manifest["format"], "rentemester-native-cli-candidate/1")
        self.assertEqual(manifest["sourceRevision"], self.args.revision)
        self.assertIs(manifest["candidateOnly"], True)
        self.assertEqual([e["path"] for e in manifest["files"]], sorted(expected))
        self.assertEqual({p.relative_to(output).as_posix() for p in output.rglob("*") if p.is_file()},
                         set(expected) | {MANIFEST})
        for entry in manifest["files"]:
            path = output / entry["path"]
            source = expected[entry["path"]]
            executable = entry["path"].startswith("bin/")
            self.assertEqual(path.read_bytes(), source.read_bytes())
            self.assertFalse(path.is_symlink())
            self.assertFalse(os.path.samefile(path, source))
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o755 if executable else 0o644)
            self.assertEqual(entry, {"path": entry["path"], "size": len(source.read_bytes()),
                                     "sha256": digest(source), "executable": executable})
        self.assertFalse(os.path.samefile(output / "bin/rentemester", output / "bin/rentemester-rust"))
        before = (output / "bin/rentemester").read_bytes()
        self.binaries[0].write_bytes(b"changed source")
        self.assertEqual((output / "bin/rentemester").read_bytes(), before)
        self.assertTrue((output / "web/assets/empty").is_dir())

    def test_existing_outputs_including_dangling_symlink_are_preserved(self):
        output = Path(self.args.output)
        output.parent.mkdir()
        for kind in ("file", "directory", "symlink", "dangling"):
            with self.subTest(kind=kind):
                if kind == "file":
                    output.write_bytes(b"preserve")
                elif kind == "directory":
                    output.mkdir()
                else:
                    output.symlink_to(self.web if kind == "symlink" else self.root / "missing")
                with self.assertRaises(ValueError):
                    assemble(self.args)
                self.assertTrue(os.path.lexists(output))
                if kind == "file":
                    self.assertEqual(output.read_bytes(), b"preserve")
                if kind == "directory":
                    output.rmdir()
                else:
                    output.unlink()

    def test_unsafe_sources_and_boundaries(self):
        def refused(**changes):
            args = argparse.Namespace(**(vars(self.args) | changes))
            with self.assertRaises((ValueError, OSError)):
                assemble(args)
            self.assertFalse(os.path.lexists(self.args.output))
        refused(revision="short")
        refused(cli_binary="relative")
        refused(output=str(self.web / "nested"))
        refused(output=str(self.root))
        refused(web_dir=str(self.binaries[0]))
        link = self.root / "linked-cli"
        link.symlink_to(self.binaries[0])
        refused(cli_binary=str(link))
        link.unlink()
        link.symlink_to(self.web, target_is_directory=True)
        refused(web_dir=str(link))
        refused(web_dir=str(link / "assets"))
        self.binaries[0].chmod(0o644)
        refused()
        self.binaries[0].chmod(0o755)
        index = self.web / "index.html"
        index.unlink()
        refused()
        index.symlink_to(self.binaries[0])
        refused()
        index.unlink()
        index.write_bytes(b"index")
        unsafe = self.web / "assets/unsafe"
        unsafe.symlink_to(self.binaries[0])
        refused()
        unsafe.unlink()
        unsafe.symlink_to(self.root, target_is_directory=True)
        refused()
        unsafe.unlink()
        os.mkfifo(unsafe)
        refused()

    def test_copy_failure_cleanup_and_atomic_fresh_publish(self):
        with mock.patch(__name__ + ".copy_artifact", side_effect=OSError("synthetic copy failure")):
            with self.assertRaises(OSError):
                assemble(self.args)
        output = Path(self.args.output)
        self.assertFalse(os.path.lexists(output))
        self.assertEqual(list(output.parent.iterdir()), [])
        original = publish

        def competing_output(stage, destination):
            destination.mkdir()
            original(stage, destination)

        with mock.patch(__name__ + ".publish", side_effect=competing_output):
            with self.assertRaises(OSError) as error:
                assemble(self.args)
        self.assertIn(error.exception.errno, (errno.EEXIST, errno.ENOTEMPTY))
        self.assertEqual(list(output.iterdir()), [])
        self.assertEqual(list(output.parent.iterdir()), [output])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    required = ("cli-binary", "mcp-binary", "server-binary", "web-dir", "output", "revision")
    for name in required:
        parser.add_argument("--" + name)
    args = parser.parse_args()
    if args.self_test:
        suite = unittest.defaultTestLoader.loadTestsFromTestCase(AssemblyFixture)
        raise SystemExit(0 if unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful() else 1)
    if any(getattr(args, name.replace("-", "_")) is None for name in required):
        parser.error("assembly requires " + ", ".join("--" + name for name in required))
    try:
        print(assemble(args))
    except (OSError, ValueError) as error:
        parser.exit(2, "native CLI candidate refused: " + str(error) + "\n")


if __name__ == "__main__":
    main()
