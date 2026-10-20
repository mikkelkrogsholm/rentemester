#!/usr/bin/env python3
"""Sign an assembled native package using an external Ed25519 trust anchor.

This authenticates Rentemester update metadata; it is not Apple code signing.
Metadata must be absent. Neither the private key nor its bytes enter the bundle.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import tempfile

MANIFEST = "Contents/Resources/native-update.json"
SIGNATURE = "Contents/Resources/native-update.sig"
ARTIFACT = "Contents/Resources/native-artifact-manifest.json"
MAX_FILE = 512 * 1024 * 1024
MAX_TOTAL = 1024 * 1024 * 1024
MAX_FILES = 20_000
MAX_METADATA = 1024 * 1024
REQUIRED = {
    "Contents/MacOS/rentemester-desktop": True,
    "Contents/MacOS/rentemester-server": True,
    "Contents/MacOS/rentemester-update": True,
    "Contents/Resources/web/index.html": False,
    "Contents/Info.plist": False,
}
SPKI_PREFIX = bytes.fromhex("302a300506032b6570032100")


def fail(message):
    raise ValueError(message)


def canonical(path):
    value = Path(os.path.abspath(path))
    if value.resolve(strict=True) != value or value.is_symlink():
        fail("path must be canonical and contain no symlink")
    return value


def identity(st):
    return (st.st_dev, st.st_ino, st.st_mode, st.st_nlink, st.st_size,
            st.st_mtime_ns, st.st_ctime_ns)


def regular(path, limit, private=False):
    before = path.lstat()
    if (not stat.S_ISREG(before.st_mode) or before.st_nlink != 1
            or before.st_size > limit or before.st_mode & 0o7022):
        fail("unsafe or oversized regular file")
    if private and (before.st_uid != os.getuid() or before.st_mode & 0o077):
        fail("private key must be owned by the current user and private")
    return before


def read_file(path, limit, private=False):
    before = regular(path, limit, private)
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(fd, "rb") as source:
        if identity(before) != identity(os.fstat(source.fileno())):
            fail("file identity changed")
        data = source.read(limit + 1)
        if (len(data) != before.st_size
                or identity(before) != identity(os.fstat(source.fileno()))
                or identity(before) != identity(path.lstat())):
            fail("file changed during read")
    return data


def safe_path(value):
    if not value.startswith("Contents/") or len(value) > 1024:
        return False
    devices = {"CON", "PRN", "AUX", "NUL"}
    devices.update("COM" + str(i) for i in range(1, 10))
    devices.update("LPT" + str(i) for i in range(1, 10))
    return all(p not in ("", ".", "..") and len(p) <= 255
               and not p.endswith(".") and re.fullmatch(r"[A-Za-z0-9._-]+", p)
               and p.split(".")[0].upper() not in devices for p in value.split("/"))


def inventory(root):
    entries, directories, folded = [], [], set()

    def walk(directory):
        before = directory.lstat()
        if not stat.S_ISDIR(before.st_mode) or before.st_mode & 0o7022:
            fail("unsafe package directory")
        if len(directories) + len(entries) >= MAX_FILES * 2:
            fail("package traversal bound exceeded")
        directories.append((directory, before))
        for path in sorted(directory.iterdir()):
            relative = path.relative_to(root).as_posix()
            if relative != "Contents" and not safe_path(relative):
                fail("unsafe package path")
            meta = path.lstat()
            if stat.S_ISDIR(meta.st_mode):
                walk(path)
            elif stat.S_ISREG(meta.st_mode):
                if relative in (MANIFEST, SIGNATURE):
                    fail("update metadata already exists")
                if relative.lower() in folded:
                    fail("case ambiguous package path")
                folded.add(relative.lower())
                regular(path, MAX_FILE)
                h = hashlib.sha256()
                fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
                with os.fdopen(fd, "rb") as source:
                    if identity(meta) != identity(os.fstat(source.fileno())):
                        fail("package changed during inventory")
                    size = 0
                    for block in iter(lambda: source.read(65536), b""):
                        size += len(block)
                        if size > meta.st_size:
                            fail("package grew during inventory")
                        h.update(block)
                    if (size != meta.st_size
                            or identity(meta) != identity(os.fstat(source.fileno()))
                            or identity(meta) != identity(path.lstat())):
                        fail("package changed during inventory")
                entries.append(dict(path=relative, size=size, sha256=h.hexdigest(),
                                    executable=bool(meta.st_mode & 0o111)))
                if len(entries) > MAX_FILES:
                    fail("too many package files")
            else:
                fail("symlink or irregular package entry")
    walk(root)
    if sum(e["size"] for e in entries) > MAX_TOTAL:
        fail("package total bound exceeded")
    for path, before in directories:
        if identity(before) != identity(path.lstat()):
            fail("package directory changed")
    entries.sort(key=lambda e: e["path"])
    by_path = {e["path"]: e for e in entries}
    for path, executable in REQUIRED.items():
        if path not in by_path or (executable and not by_path[path]["executable"]):
            fail("required package artifact missing or not executable")
    return entries


def openssl(*args, data=None):
    result = subprocess.run(["openssl", *args], input=data, capture_output=True,
                            check=False, timeout=30)
    if result.returncode:
        fail("OpenSSL operation refused")
    return result.stdout


def sign(args):
    root, key, public = (canonical(p) for p in (args.bundle, args.key, args.public_key))
    if not root.is_dir():
        fail("bundle must be a real directory")
    if key.is_relative_to(root) or public.is_relative_to(root):
        fail("signing keys must be outside the package")
    # Validate without copying or printing private key bytes.
    key_before = regular(key, 32768, True)
    if key_before.st_size == 0:
        fail("empty private key")
    expected = read_file(public, 32)
    if len(expected) != 32:
        fail("trusted public key must be raw 32-byte Ed25519")
    derived = openssl("pkey", "-in", str(key), "-passin", "pass:", "-pubout", "-outform", "DER")
    if derived != SPKI_PREFIX + expected:
        fail("private key does not match independently supplied public key")
    if not 0 < args.sequence <= 9_007_199_254_740_991:
        fail("sequence must be positive and JavaScript-safe")
    if not re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)+", args.platform):
        fail("invalid platform")
    if any(not 0 <= v <= 0xFFFF_FFFF for v in (args.api_epoch, args.control_schema, args.company_schema)):
        fail("invalid schema or API epoch")
    entries = inventory(root)
    old_bytes = read_file(root / ARTIFACT, MAX_METADATA)
    old = json.loads(old_bytes)
    version = old["version"]
    if (not isinstance(version, str) or not re.fullmatch(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)", version)
            or any(int(v) > 0xFFFF_FFFF_FFFF_FFFF for v in version.split("."))):
        fail("version must be numeric three-part SemVer")
    revision = old["source"]["revision"]
    if not isinstance(revision, str) or not re.fullmatch(r"[0-9a-f]{40}", revision):
        fail("source revision must be a full SHA")
    toolchains = {name: old["build"][name + "Toolchain"] for name in ("server", "desktop")}
    if any(not isinstance(v, str) or not v or len(v) > 256 or any(ord(c) < 32 or ord(c) == 127 for c in v) for v in toolchains.values()):
        fail("invalid toolchain provenance")
    manifest = dict(format="rentemester-native-update/1", bundleId="dk.rentemester.desktop",
                    version=version, sequence=args.sequence, platform=args.platform,
                    apiEpoch=args.api_epoch, controlSchema=args.control_schema,
                    companySchema=args.company_schema, sourceRevision=revision,
                    toolchains=toolchains, files=entries)
    raw = (json.dumps(manifest, sort_keys=True, separators=(",", ":")) + "\n").encode()
    if len(raw) > MAX_METADATA:
        fail("update manifest too large")
    # pkeyutl raw Ed25519 input needs a seekable file on supported OpenSSL.
    # Only public manifest/signature bytes are placed in this private directory.
    with tempfile.TemporaryDirectory(prefix="rentemester-update-sign-") as temp:
        temp = Path(temp)
        source = temp / "manifest.json"
        source.write_bytes(raw)
        signature = openssl("pkeyutl", "-sign", "-rawin", "-inkey", str(key),
                            "-passin", "pass:", "-in", str(source))
        if len(signature) != 64 or identity(key_before) != identity(key.lstat()):
            fail("invalid signature or changed private key")
        trusted = temp / "public.der"
        trusted.write_bytes(SPKI_PREFIX + expected)
        signed = temp / "signature"
        signed.write_bytes(signature)
        openssl("pkeyutl", "-verify", "-rawin", "-pubin", "-keyform", "DER",
                "-inkey", str(trusted), "-in", str(source), "-sigfile", str(signed))
        if inventory(root) != entries or read_file(root / ARTIFACT, MAX_METADATA) != old_bytes:
            fail("package changed while signing")
        # Atomic no-replace publication of each metadata file. Both are required
        # for admission; a crash between these links leaves an unusable package.
        created = []
        staged = []
        try:
            for relative, data in ((MANIFEST, raw), (SIGNATURE, signature)):
                destination = root / relative
                fd, name = tempfile.mkstemp(prefix=".native-update-", dir=destination.parent)
                staged.append(name)
                with os.fdopen(fd, "wb") as out:
                    out.write(data)
                    out.flush()
                    os.fchmod(out.fileno(), 0o644)
                    os.fsync(out.fileno())
                os.link(name, destination, follow_symlinks=False)
                created.append(destination)
                os.unlink(name)
                staged.remove(name)
            directory = os.open(root / "Contents/Resources", os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
            try:
                os.fsync(directory)
            finally:
                os.close(directory)
        except BaseException:
            for path in created:
                path.unlink()
            raise
        finally:
            for path in staged:
                os.unlink(path)
    return {"ok": True, "manifestSha256": hashlib.sha256(raw).hexdigest(),
            "version": version, "sequence": args.sequence, "files": len(entries)}


def main():
    os.umask(0o077)
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("bundle", "key", "public-key", "platform"):
        parser.add_argument("--" + name, required=True)
    for name in ("sequence", "api-epoch", "control-schema", "company-schema"):
        parser.add_argument("--" + name, required=True, type=int)
    args = parser.parse_args()
    try:
        print(json.dumps(sign(args), sort_keys=True))
    except (ValueError, OSError, KeyError, TypeError, subprocess.SubprocessError):
        parser.exit(1, "native update signing refused; package/key/provenance validation failed\n")


if __name__ == "__main__":
    main()
