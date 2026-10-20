#!/usr/bin/env python3
"""Assemble native Mac artifacts, optionally authenticating updates; never builds, launches or Apple-signs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import shutil
import stat
import subprocess
import tempfile
import unittest

BUNDLE_ID = "dk.rentemester.desktop"
APPROVED_ICON = Path("assets/branding/rentemester-app-icon-v1.icns")


def regular(path):
    mode = path.lstat().st_mode
    if not stat.S_ISREG(mode):
        raise ValueError("expected a regular, non-symlink artifact: " + str(path))
    return path


def binaries(root, target_dir, profile, target):
    if not re.fullmatch(r"[A-Za-z0-9_-]+", profile):
        raise ValueError("invalid Cargo profile")
    if target and not re.fullmatch(r"[A-Za-z0-9_.-]+", target):
        raise ValueError("target must be a target triple, not a custom target JSON path")
    build = Path(target_dir)
    if not build.is_absolute():
        build = root / build
    if target:
        build /= target
    build /= "debug" if profile in ("dev", "test") else profile
    return tuple(regular(build / name) for name in ("rentemester-desktop", "rentemester-server", "rentemester-update"))


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def copy_artifact(source, destination, executable=False):
    regular(source)
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    destination.chmod(0o755 if executable else 0o644)


def assemble(args):
    root = Path(args.root).resolve(strict=True)
    desktop, server, updater = binaries(root, args.target_dir, args.profile, args.target)
    web = Path(args.web)
    if web.is_symlink() or not web.is_dir():
        raise ValueError("web source must be a real directory")
    web = web.resolve(strict=True)
    regular(web / "index.html")
    web_files = sorted(p for p in web.rglob("*"))
    for source in web_files:
        if source.is_symlink() or not (source.is_dir() or source.is_file()):
            raise ValueError("unsupported web artifact: " + str(source))
    icon = regular(Path(args.icon) if args.icon else root / APPROVED_ICON)
    if not re.fullmatch(r"[0-9a-f]{40}", args.revision):
        raise ValueError("source revision must be a full Git SHA")
    if not re.fullmatch(r"[0-9]+(?:\.[0-9]+){0,2}", args.version):
        raise ValueError("bundle version must be a numeric macOS version")
    update_flags = (getattr(args, "update_signing_key", None), getattr(args, "update_public_key", None), getattr(args, "release_sequence", None))
    if any(v is not None for v in update_flags) and not all(v is not None for v in update_flags):
        raise ValueError("update signing requires private key, independent public key and release sequence together")
    output = Path(args.output).absolute()
    if all(v is not None for v in update_flags) and output.exists():
        raise ValueError("authenticated assembly requires a fresh output; use rentemester-update for installed packages")
    if output.suffix != ".app" or output.is_symlink():
        raise ValueError("output must be a non-symlink .app path")
    output.parent.mkdir(parents=True, exist_ok=True)
    if output.resolve().is_relative_to(web):
        raise ValueError("bundle output must be outside the web source")
    if output.exists():
        if not output.is_dir():
            raise ValueError("existing output is not a bundle directory")
        info = regular(output / "Contents/Info.plist")
        with info.open("rb") as source:
            previous = plistlib.load(source)
        if previous.get("CFBundleIdentifier") != BUNDLE_ID or previous.get("CFBundleExecutable") != "rentemester-desktop":
            raise ValueError("refusing to replace an unrelated existing bundle")
    # Stage beside the destination: same filesystem, no old web assets copied forward.
    with tempfile.TemporaryDirectory(prefix=".rentemester-native-", dir=output.parent) as temporary:
        stage = Path(temporary) / output.name
        contents = stage / "Contents"
        resources = contents / "Resources"
        (resources / "web").mkdir(parents=True)
        copy_artifact(desktop, contents / "MacOS/rentemester-desktop", True)
        copy_artifact(server, contents / "MacOS/rentemester-server", True)
        copy_artifact(updater, contents / "MacOS/rentemester-update", True)
        copy_artifact(icon, resources / "Rentemester.icns")
        for source in web_files:
            if source.is_file():
                copy_artifact(source, resources / "web" / source.relative_to(web))
        with (contents / "Info.plist").open("wb") as destination:
            plistlib.dump({
                "CFBundleIdentifier": BUNDLE_ID,
                "CFBundleName": "Rentemester Desktop",
                "CFBundleExecutable": "rentemester-desktop",
                "CFBundlePackageType": "APPL",
                "CFBundleIconFile": "Rentemester.icns",
                "CFBundleVersion": args.version,
                "CFBundleShortVersionString": args.version,
                "NSHighResolutionCapable": True,
            }, destination, sort_keys=True)
        entries = [{"path": p.relative_to(stage).as_posix(), "size": p.stat().st_size, "sha256": digest(p)}
                   for p in sorted(stage.rglob("*")) if p.is_file()]
        manifest = {
            "format": "rentemester-native-macos/1",
            "source": {"revision": args.revision, "dirty": args.source_dirty},
            "version": args.version,
            "build": {"architecture": args.architecture, "target": args.target or None,
                      "profile": args.profile, "serverToolchain": args.server_toolchain,
                      "desktopToolchain": args.desktop_toolchain},
            "signing": "unsigned",
            "files": entries,
        }
        (resources / "native-artifact-manifest.json").write_text(
            json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        if all(v is not None for v in update_flags):
            subprocess.run(["python3", str(root / "scripts/release/sign-native-update.py"),
                "--bundle", str(stage), "--key", str(update_flags[0]), "--public-key", str(update_flags[1]),
                "--sequence", str(update_flags[2]), "--platform", "macos-" + args.architecture,
                "--api-epoch", "1", "--control-schema", "31", "--company-schema", "36"], check=True,
                stdout=subprocess.DEVNULL)
        previous = Path(temporary) / "previous.app"
        if output.exists():
            output.rename(previous)
        try:
            stage.rename(output)
        except OSError:
            if previous.exists():
                previous.rename(output)
            raise
    return output


class AssemblyFixture(unittest.TestCase):
    def test_custom_target_release_fresh_replace_exact_inventory_and_refusal(self):
        with tempfile.TemporaryDirectory(prefix="rentemester-native-fixture-") as temporary:
            root = Path(temporary)
            build = root / "custom-target/aarch64-apple-darwin/release"
            build.mkdir(parents=True)
            for name in ("rentemester-desktop", "rentemester-server", "rentemester-update"):
                (build / name).write_bytes(("fixture " + name).encode())
            web = root / "web"
            (web / "assets").mkdir(parents=True)
            (web / "index.html").write_text("<main>fixture</main>")
            (web / "assets/current.js").write_bytes(b"fixture javascript")
            icon = root / "fixture.icns"
            icon.write_bytes(b"fixture icon")
            args = argparse.Namespace(root=str(root), target_dir="custom-target", profile="release",
                target="aarch64-apple-darwin", web=str(web), icon=str(icon), output=str(root / "out/Fixture.app"),
                revision="a" * 40, version="0.1.0", architecture="aarch64", server_toolchain="rustc fixture1",
                desktop_toolchain="rustc fixture2", source_dirty=False)
            output = assemble(args)
            stale = output / "Contents/Resources/web/stale.js"
            stale.write_text("must disappear")
            assemble(args)
            self.assertFalse(stale.exists())
            manifest = json.loads((output / "Contents/Resources/native-artifact-manifest.json").read_text())
            self.assertEqual(manifest["source"], {"revision": "a" * 40, "dirty": False})
            self.assertEqual(manifest["build"]["profile"], "release")
            self.assertEqual(manifest["signing"], "unsigned")
            expected = {p.relative_to(output).as_posix() for p in output.rglob("*") if p.is_file()}
            expected.remove("Contents/Resources/native-artifact-manifest.json")
            self.assertEqual({e["path"] for e in manifest["files"]}, expected)
            for entry in manifest["files"]:
                self.assertEqual(entry["sha256"], digest(output / entry["path"]))
            self.assertTrue((output / "Contents/MacOS/rentemester-server").stat().st_mode & stat.S_IXUSR)
            # The default must select the approved artwork, not a stale legacy icon.
            approved = root / APPROVED_ICON
            approved.parent.mkdir(parents=True)
            approved.write_bytes((Path(__file__).resolve().parents[2] / APPROVED_ICON).read_bytes())
            self.assertEqual((output / "Contents/Resources/Rentemester.icns").read_bytes(), icon.read_bytes())
            args.icon = None
            assemble(args)
            self.assertEqual((output / "Contents/Resources/Rentemester.icns").read_bytes(), approved.read_bytes())
            with (output / "Contents/Info.plist").open("rb") as source:
                info = plistlib.load(source)
            self.assertEqual(info["CFBundleIconFile"], "Rentemester.icns")
            manifest = json.loads((output / "Contents/Resources/native-artifact-manifest.json").read_text())
            icon_entry = next(e for e in manifest["files"] if e["path"] == "Contents/Resources/Rentemester.icns")
            self.assertEqual(icon_entry["sha256"], digest(approved))
            self.assertEqual(icon_entry["size"], approved.stat().st_size)
            # A caller cannot replace an unrelated bundle merely by using its path.
            info_path = output / "Contents/Info.plist"
            original_info = info_path.read_bytes()
            info["CFBundleIdentifier"] = "invalid.unrelated.fixture"
            info_path.write_bytes(plistlib.dumps(info))
            protected_info = info_path.read_bytes()
            with self.assertRaisesRegex(ValueError, "unrelated existing bundle"):
                assemble(args)
            self.assertEqual(info_path.read_bytes(), protected_info)
            self.assertEqual((output / "Contents/Resources/Rentemester.icns").read_bytes(), approved.read_bytes())
            info_path.write_bytes(original_info)
            before = (output / "Contents/Resources/native-artifact-manifest.json").read_bytes()
            (web / "bad-link").symlink_to(icon)
            with self.assertRaises(ValueError):
                assemble(args)
            self.assertEqual((output / "Contents/Resources/native-artifact-manifest.json").read_bytes(), before)
            (web / "bad-link").unlink()
            # Default dev profile resolves debug, independently of custom release above.
            debug = root / "target/debug"
            debug.mkdir(parents=True)
            for name in ("rentemester-desktop", "rentemester-server", "rentemester-update"):
                (debug / name).write_bytes(b"debug fixture")
            self.assertEqual(binaries(root, "target", "dev", "")[0], debug / "rentemester-desktop")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--root")
    parser.add_argument("--target-dir")
    parser.add_argument("--profile", default="dev")
    parser.add_argument("--target", default="")
    parser.add_argument("--web")
    parser.add_argument("--icon", help="Explicit ICNS override; default: <root>/" + str(APPROVED_ICON))
    parser.add_argument("--output")
    parser.add_argument("--revision")
    parser.add_argument("--version")
    parser.add_argument("--architecture")
    parser.add_argument("--server-toolchain")
    parser.add_argument("--desktop-toolchain")
    parser.add_argument("--source-dirty", action="store_true")
    parser.add_argument("--update-signing-key", help="Explicit private Ed25519 PKCS8 update key; never a bundled trust anchor")
    parser.add_argument("--update-public-key", help="Separately configured raw32 Ed25519 public key")
    parser.add_argument("--release-sequence", type=int, help="Strictly increasing authenticated release sequence")
    args = parser.parse_args()
    if args.self_test:
        suite = unittest.defaultTestLoader.loadTestsFromTestCase(AssemblyFixture)
        raise SystemExit(0 if unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful() else 1)
    required = ("root", "target_dir", "web", "output", "revision", "version", "architecture", "server_toolchain", "desktop_toolchain")
    if any(not getattr(args, key) for key in required):
        parser.error("assembly requires " + ", ".join("--" + key.replace("_", "-") for key in required))
    try:
        print(assemble(args))
    except (OSError, ValueError, plistlib.InvalidFileException, subprocess.CalledProcessError) as error:
        parser.exit(2, "native bundle refused: " + str(error) + "\n")


if __name__ == "__main__":
    main()
