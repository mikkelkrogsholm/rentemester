#!/usr/bin/env python3
"""Synthetic ordinary-user macOS proof of authenticated native package updates.

No GUI, real workspace, Apple signing or production trust anchor is involved.
One private-staging crash observation is attempted; a missed window fails without replay.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import shutil
import signal
import stat
import subprocess
import sys
import time
import uuid

REPO = Path(__file__).resolve().parents[2]
SIGNER = REPO / "scripts/release/sign-native-update.py"
ASSEMBLER = REPO / "scripts/release/assemble-native-macos.py"
META = "Contents/Resources/native-update.json"
SIG = "Contents/Resources/native-update.sig"
FILLER = "Contents/Resources/zz-crash-filler.bin"
FILLER_SIZE = 256 * 1024 * 1024
PREFIX = "Rentemester-Synthetic-Update-"


def need(value, message):
    if not value:
        raise RuntimeError(message)


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def real_file(value):
    path = Path(value).absolute()
    need(path.resolve(strict=True) == path and not path.is_symlink(), "canonical input file required")
    meta = path.lstat()
    need(stat.S_ISREG(meta.st_mode) and meta.st_nlink == 1, "regular unlinked input required")
    return path


def tree_digest(root):
    entries = []
    for path in sorted(root.rglob("*")):
        meta = path.lstat()
        need(not path.is_symlink() and (path.is_dir() or path.is_file()), "unexpected fixture entry")
        if path.is_file():
            entries.append([path.relative_to(root).as_posix(), meta.st_size,
                            stat.S_IMODE(meta.st_mode), digest(path)])
    return hashlib.sha256(json.dumps(entries, separators=(",", ":")).encode()).hexdigest()


class Proof:
    def __init__(self, args, work, report):
        self.args, self.work, self.report = args, work, report
        self.children = []
        self.updater = work / "rentemester-update"
        source = real_file(args.updater_binary)
        shutil.copyfile(source, self.updater)
        self.updater.chmod(0o700)
        report["updater"] = {"source": str(source), "sha256": digest(self.updater)}
        self.key, self.public = work / "synthetic-private.pem", work / "synthetic-public.raw"
        self.command(["openssl", "genpkey", "-algorithm", "Ed25519", "-out", str(self.key)])
        self.key.chmod(0o600)
        public = self.command(["openssl", "pkey", "-in", str(self.key), "-pubout", "-outform", "DER"], binary=True)
        need(public.startswith(bytes.fromhex("302a300506032b6570032100")) and len(public) == 44,
             "synthetic Ed25519 public key")
        self.public.write_bytes(public[-32:])
        self.public.chmod(0o600)
        report["syntheticPublicKeySha256"] = digest(self.public)
        self.current = work / "Rentemester.app"
        self.stage = work / ".Rentemester.app.update-stage"
        self.previous = work / ".Rentemester.app.update-previous"
        self.lock = work / ".Rentemester.app.update-lock"
        self.workspace = work / "synthetic-workspace"
        self.workspace.mkdir(mode=0o700)
        self.sentinel = self.workspace / "never-opened-by-updater.txt"
        self.sentinel.write_bytes(b"Synthetic workspace sentinel; never a ledger.\n")
        self.sentinel_digest = tree_digest(self.workspace)
        self.revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=REPO, text=True).strip()
        self.report["sourceRevision"] = self.revision
        self.report["toolchains"] = {"fixture": "synthetic-no-compiler", "openssl": self.command(["openssl", "version"]).strip()}

    def command(self, args, binary=False, timeout=120):
        result = subprocess.run([str(v) for v in args], capture_output=True, timeout=timeout)
        need(result.returncode == 0, "fixture command refused: " + Path(str(args[0])).name)
        return result.stdout if binary else result.stdout.decode("utf-8")

    def call(self, action, package=None, key=None, expected=True):
        args = [str(self.updater), action, "--trusted-key", str(key or self.public)]
        if package:
            args += ["--package", str(package)]
        if action != "verify":
            args += ["--installed", str(self.current)]
        if action in ("apply", "recover"):
            args += ["--confirm"]
        result = subprocess.run(args, capture_output=True, timeout=120)
        try:
            value = json.loads(result.stdout)
        except (ValueError, UnicodeError):
            raise RuntimeError("updater returned no canonical JSON") from None
        need((result.returncode == 0) == expected and value.get("ok") is expected,
             "unexpected updater outcome for " + action)
        need(value.get("workspaceOpened") is False, "updater must report workspace unopened")
        return value

    def sign(self, root, sequence, platform="macos-aarch64", schema=36):
        self.command([sys.executable, SIGNER, "--bundle", root, "--key", self.key,
                      "--public-key", self.public, "--sequence", str(sequence),
                      "--platform", platform, "--api-epoch", "1", "--control-schema", "31",
                      "--company-schema", str(schema)])

    def package(self, path, version, sequence, platform="macos-aarch64", schema=36, filler=False):
        (path / "Contents/MacOS").mkdir(parents=True, mode=0o700)
        (path / "Contents/Resources/web").mkdir(parents=True, mode=0o700)
        for name in ("desktop", "server"):
            executable = path / ("Contents/MacOS/rentemester-" + name)
            executable.write_text("#!/bin/sh\n[ \"$1\" = --help ] || exit 2\nprintf '%s\\n' 'Synthetic harmless " + name + " " + version + "'\n")
            executable.chmod(0o755)
        shutil.copyfile(self.updater, path / "Contents/MacOS/rentemester-update")
        (path / "Contents/MacOS/rentemester-update").chmod(0o755)
        (path / "Contents/Resources/web/index.html").write_text("<title>Synthetic update " + version + "</title>\n")
        (path / "Contents/Info.plist").write_bytes(plistlib.dumps({
            "CFBundleIdentifier": "dk.rentemester.desktop", "CFBundleExecutable": "rentemester-desktop",
            "CFBundleVersion": version, "CFBundleShortVersionString": version}))
        (path / "Contents/Resources/native-artifact-manifest.json").write_text(json.dumps({
            "format": "rentemester-native-macos/1", "version": version,
            "source": {"revision": self.revision, "dirty": False},
            "build": {"serverToolchain": "synthetic-shell", "desktopToolchain": "synthetic-shell"},
            "signing": "unsigned-synthetic-fixture"}))
        if filler:
            with (path / FILLER).open("wb") as destination:
                destination.truncate(FILLER_SIZE)
        self.sign(path, sequence, platform, schema)
        return path

    def assert_workspace(self):
        need(tree_digest(self.workspace) == self.sentinel_digest, "workspace sentinel changed")

    def refused(self, name, package, key=None):
        before = tree_digest(self.current)
        result = self.call("apply", package, key, expected=False)
        need(tree_digest(self.current) == before and not self.stage.exists(), "refusal changed installation/stage")
        self.assert_workspace()
        self.report["refusals"][name] = {"code": result.get("code"), "installedBytesUnchanged": True}

    def lease_proof(self, candidate):
        code = "import fcntl,sys; f=open(sys.argv[1],'a+b'); fcntl.flock(f,fcntl.LOCK_SH); print('ready',flush=True); sys.stdin.buffer.read(1)"
        child = subprocess.Popen([sys.executable, "-u", "-c", code, str(self.lock)],
                                 stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.children.append(child)
        import selectors
        selector = selectors.DefaultSelector()
        selector.register(child.stdout, selectors.EVENT_READ)
        try:
            need(selector.select(timeout=10), "lease child readiness deadline")
            need(child.stdout.readline() == b"ready\n", "lease child readiness")
            self.refused("runningSharedLease", candidate)
            need(self.report["refusals"]["runningSharedLease"]["code"] == "application_running_or_update_busy", "actual held lease must cause refusal")
        finally:
            selector.close()
            self.stop(child)
        self.report["sharedRunningLease"] = {"separateProcess": True, "realFlock": True, "guiProof": False}

    @staticmethod
    def stop(child):
        if child.poll() is None:
            child.terminate()
            try:
                child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait(timeout=5)
        for pipe in (child.stdin, child.stdout, child.stderr):
            if pipe:
                pipe.close()

    def crash_proof(self):
        candidate = self.package(self.work / "CrashCandidate.app", "0.1.2", 3, filler=True)
        before = tree_digest(self.current)
        args = [str(self.updater), "apply", "--package", str(candidate), "--installed",
                str(self.current), "--trusted-key", str(self.public), "--confirm"]
        child = subprocess.Popen(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.children.append(child)
        staged_filler = self.stage / "candidate.app" / FILLER
        deadline = time.monotonic() + 90
        observed = None
        while time.monotonic() < deadline:
            if child.poll() is not None:
                break
            try:
                size = staged_filler.stat().st_size
            except FileNotFoundError:
                size = 0
            if (self.stage / "candidate.app").is_dir():
                observed = size
                child.kill()  # Only this exact owned updater PID, no process groups.
                break
            time.sleep(0.001)
        if observed is None:
            self.stop(child)
            raise RuntimeError("missed actual private staging window; no retry or crash-proof claim")
        need(child.wait(timeout=10) == -signal.SIGKILL, "owned updater actually killed by SIGKILL")
        need(tree_digest(self.current) == before, "preexchange SIGKILL must preserve current bytes")
        status = self.call("status")
        need(status["version"] == "0.1.1" and status["pendingRecovery"] is True, "staged crash status")
        # Synthetic shell is harmless; this confirms the installation still runs.
        self.command([self.current / "Contents/MacOS/rentemester-desktop", "--help"])
        recovered = self.call("recover")
        need(recovered.get("state") == "staging_discarded" and not self.stage.exists(), "explicit preexchange recovery")
        need(tree_digest(self.current) == before, "recovery changed installed bytes")
        self.assert_workspace()
        self.report["actualCrash"] = {"signal": "SIGKILL", "phase": "private-staging-before-publication",
                                      "observedFillerBytes": observed, "fullFillerBytes": FILLER_SIZE,
                                      "partialFillerObserved": 0 < observed < FILLER_SIZE,
                                      "attempts": 1, "currentRunnable": True,
                                      "status": status, "recovery": recovered,
                                      "workspaceSentinelUnchanged": True, "postexchangeCrashProof": False}
        self.stop(child)
        shutil.rmtree(candidate)

    def optional_full_bundle(self):
        if not self.args.desktop_binary:
            self.report["compiledFullBundle"] = {"executed": False}
            return
        build = self.work / "pinned-build"
        binaries = build / "debug"
        binaries.mkdir(parents=True, mode=0o700)
        inputs = {"rentemester-desktop": real_file(self.args.desktop_binary),
                  "rentemester-server": real_file(self.args.server_binary),
                  "rentemester-update": self.updater}
        for name, source in inputs.items():
            shutil.copyfile(source, binaries / name)
            (binaries / name).chmod(0o700)
        web_source = Path(self.args.web_dir).absolute()
        need(web_source.resolve(strict=True) == web_source and web_source.is_dir(), "canonical web directory required")
        for path in web_source.rglob("*"):
            need(not path.is_symlink() and (path.is_file() or path.is_dir()), "regular compiled web tree")
        web = self.work / "pinned-web"
        shutil.copytree(web_source, web)
        package = self.work / "CompiledCandidate.app"
        toolchains = {"server": self.args.server_toolchain, "desktop": self.args.desktop_toolchain}
        self.command([sys.executable, ASSEMBLER, "--root", REPO, "--target-dir", build,
                      "--profile", "dev", "--web", web, "--output", package,
                      "--revision", self.revision, "--version", "0.2.0", "--architecture", "aarch64",
                      "--server-toolchain", toolchains["server"], "--desktop-toolchain", toolchains["desktop"],
                      "--update-signing-key", self.key, "--update-public-key", self.public,
                      "--release-sequence", "4", *(["--source-dirty"] if subprocess.check_output(["git", "status", "--porcelain"], cwd=REPO) else [])])
        verified = self.call("verify", package)
        published = self.call("apply", package)
        for name in ("desktop", "server", "update"):
            self.command([self.current / ("Contents/MacOS/rentemester-" + name), "--help"], timeout=30)
        self.assert_workspace()
        self.report["compiledFullBundle"] = {
            "executed": True, "binaries": {name: digest(path) for name, path in inputs.items()},
            "webTreeSha256": tree_digest(web), "operatorSuppliedToolchains": toolchains, "verify": verified,
            "publication": published, "headlessHelpOnly": True, "guiProof": False}

    def run(self):
        self.command([self.updater, "--help"])
        self.package(self.current, "0.1.0", 1)
        initial = self.call("verify", self.current)
        candidate = self.package(self.work / "Candidate.app", "0.1.1", 2)
        self.call("verify", candidate)
        self.report["initial"] = initial
        self.report["refusals"] = {}
        bad_key = self.work / "wrong-public.raw"
        bad_key.write_bytes(bytes([42]) * 32)
        self.refused("wrongIndependentPublicKey", candidate, bad_key)
        for name in ("signature", "tamper", "missing", "unlisted", "schema", "platform", "versionDowngrade", "sequenceDowngrade"):
            path = self.work / "Rejected.app"
            version = "0.0.9" if name == "versionDowngrade" else "0.1.1"
            sequence = 1 if name == "sequenceDowngrade" else 2
            self.package(path, version, sequence,
                         platform="linux-aarch64" if name == "platform" else "macos-aarch64",
                         schema=37 if name == "schema" else 36)
            if name == "signature":
                (path / SIG).write_bytes(bytes(64))
            elif name == "tamper":
                (path / "Contents/Resources/web/index.html").write_text("tampered")
            elif name == "missing":
                (path / "Contents/Resources/web/index.html").unlink()
            elif name == "unlisted":
                (path / "Contents/Resources/extra").write_text("unlisted")
            self.refused(name, path)
            shutil.rmtree(path)
        self.lease_proof(candidate)
        old_fingerprint = tree_digest(self.current)
        publication = self.call("apply", candidate)
        status = self.call("status")
        need(status["version"] == "0.1.1" and status["sequence"] == 2 and status["previousPackageRetained"] is True,
             "strict monotonic valid upgrade")
        need(self.previous.is_dir() and tree_digest(self.previous) == old_fingerprint, "old package exactly retained")
        need(not self.stage.exists(), "completed publication removes staging transaction")
        self.report["publication"] = publication
        self.report["previousExactlyRetained"] = True
        self.report["installedAfterUpgrade"] = status
        self.crash_proof()
        self.optional_full_bundle()
        self.assert_workspace()
        self.report["final"] = self.call("status")
        self.report["finalInstalledTreeSha256"] = tree_digest(self.current)
        self.report["workspaceSentinelSha256"] = digest(self.sentinel)
        self.report["workspaceSentinelUnchanged"] = True


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--updater-binary", required=True, type=Path)
    parser.add_argument("--desktop-binary", type=Path)
    parser.add_argument("--server-binary", type=Path)
    parser.add_argument("--web-dir", type=Path)
    parser.add_argument("--server-toolchain", default="supplied-prebuilt; compiler-not-independently-attested")
    parser.add_argument("--desktop-toolchain", default="supplied-prebuilt; compiler-not-independently-attested")
    parser.add_argument("--evidence", required=True, type=Path)
    args = parser.parse_args()
    if any((args.desktop_binary, args.server_binary, args.web_dir)) and not all((args.desktop_binary, args.server_binary, args.web_dir)):
        parser.error("desktop/server/web options must be supplied together")
    if sys.platform != "darwin" or os.getuid() == 0 or os.getuid() != os.geteuid():
        parser.error("ordinary macOS fixture owner required")
    os.umask(0o077)
    evidence = args.evidence.absolute()
    if evidence.exists() or evidence.is_symlink():
        parser.error("evidence path must be new")
    evidence.parent.mkdir(parents=True, exist_ok=True)
    evidence = evidence.parent.resolve(strict=True) / evidence.name
    fd = os.open(evidence, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    report = {"ok": False, "runnerSha256": digest(Path(__file__)), "ordinaryMacOsUser": True,
              "applicationSigningQualified": False, "guiQualified": False,
              "fullR52Qualified": False, "productionKeysUsed": False,
              "networkUsed": False, "workspaceOpened": False,
              "openGates": ["Apple signing/notarization", "GUI update/offline window", "full R52 platform/data cutover"]}
    work = proof = identity = None
    try:
        parent = (Path.home() / "Library/Application Support").resolve(strict=True)
        work = parent / (PREFIX + uuid.uuid4().hex)
        work.mkdir(mode=0o700)
        identity = work.stat()
        report["ownedFixture"] = str(work)
        proof = Proof(args, work, report)
        proof.run()
        report["ok"] = True
    except Exception as error:
        report["failure"] = {"type": type(error).__name__, "detail": str(error)[:500] if isinstance(error, RuntimeError) else "fixture operation failed"}
    finally:
        try:
            if proof:
                for child in proof.children:
                    proof.stop(child)
            if work and identity:
                current = work.lstat()
                need(stat.S_ISDIR(current.st_mode) and (current.st_dev, current.st_ino, current.st_uid) == (identity.st_dev, identity.st_ino, os.getuid()) and work.name.startswith(PREFIX), "owned cleanup identity changed")
                shutil.rmtree(work)
                report["ownedFixtureCleaned"] = True
        except Exception:
            report["ok"] = False
            report["ownedFixtureCleaned"] = False
            report["cleanupFailure"] = "owned children or fixture cleanup refused"
        with os.fdopen(fd, "w") as output:
            json.dump(report, output, indent=2, sort_keys=True)
            output.write("\n")
            output.flush()
            os.fsync(output.fileno())
    print(json.dumps({"ok": report["ok"], "evidence": str(evidence)}))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
