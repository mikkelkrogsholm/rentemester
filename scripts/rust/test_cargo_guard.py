"""Check disk and cache refusals without filling a real disk or compiling Rust."""
import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import sys
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("cargo_guard", Path(__file__).with_name("cargo-guard.py"))
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)


class CargoGuardTests(unittest.TestCase):
    def test_low_space_never_starts_compiler(self):
        with patch.object(guard, "shared_cache", return_value=Path("unused")), \
             patch.object(guard, "free_bytes", return_value=14 * guard.GIB), \
             patch.object(guard.os, "execvp") as compiler:
            with self.assertRaisesRegex(ValueError, "CRITICAL"):
                guard.main(["rustc", "synthetic.rs"])
            compiler.assert_not_called()

    def test_space_boundaries(self):
        for gib, allowed in [(0, False), (14, False), (15, False), (29, False), (30, True), (31, True)]:
            with self.subTest(gib=gib), patch.object(guard, "free_bytes", return_value=gib * guard.GIB):
                if allowed:
                    self.assertEqual(guard.check_space(Path("unused")), gib * guard.GIB)
                else:
                    with self.assertRaisesRegex(ValueError, "CRITICAL" if gib < 15 else "STOP"):
                        guard.check_space(Path("unused"))

    def test_worktrees_share_physical_artifacts(self):
        with tempfile.TemporaryDirectory() as tmp, patch.dict(os.environ, {}, clear=True):
            root = Path(tmp)
            cache = root / "primary/target"
            for name in ("worker-a", "worker-b"):
                checkout = root / name
                checkout.mkdir()
                guard.prepare_cache(checkout, cache)
            (root / "worker-a/target/proof").write_text("same artifact")
            self.assertEqual((root / "worker-b/target/proof").read_text(), "same artifact")
            self.assertTrue((root / "worker-a/target").is_symlink())

    def test_existing_cache_is_never_deleted(self):
        with tempfile.TemporaryDirectory() as tmp, patch.dict(os.environ, {}, clear=True):
            root = Path(tmp)
            (root / "target").mkdir()
            proof = root / "target/keep"
            proof.write_text("keep")
            with self.assertRaisesRegex(ValueError, "Separate worktree cache"):
                guard.prepare_cache(root, root / "shared")
            self.assertEqual(proof.read_text(), "keep")

    def test_explicit_worker_targets_preserve_existing_data(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "target").mkdir()
            proof = root / "target/unfinished"
            proof.write_text("preserve")
            for name in ("worker-a", "worker-b"):
                target = root / name
                with patch.dict(os.environ, {"CARGO_TARGET_DIR": str(target),
                                             "RENTEMESTER_CARGO_CACHE": str(root / "legacy")}, clear=True):
                    self.assertEqual(guard.shared_cache(root), target.resolve())
                    guard.prepare_cache(root, target, ["--out-dir", str(target / "debug")])
                self.assertTrue(target.is_dir())
            self.assertEqual(proof.read_text(), "preserve")
            self.assertFalse((root / "target").is_symlink())

    def test_relative_target_refused(self):
        with patch.dict(os.environ, {"CARGO_TARGET_DIR": "relative"}, clear=True):
            with self.assertRaisesRegex(ValueError, "absolute"):
                guard.shared_cache()

    def test_both_filesystems_checked_before_compiler(self):
        for free in ((14, 40), (40, 14), (29, 40), (40, 29)):
            with self.subTest(free=free), patch.object(guard, "shared_cache", return_value=Path("unused")), \
                 patch.object(guard, "free_bytes", side_effect=[g * guard.GIB for g in free]), \
                 patch.object(guard.os, "execvp") as compiler:
                with self.assertRaisesRegex(ValueError, "CRITICAL|STOP"):
                    guard.main(["rustc", "synthetic.rs"])
                compiler.assert_not_called()

    def test_override_and_compiler_output_cannot_create_separate_cache(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            cache = root / "shared"
            with patch.dict(os.environ, {"CARGO_TARGET_DIR": str(root / "other")}, clear=True):
                with self.assertRaisesRegex(ValueError, "CARGO_TARGET_DIR"):
                    guard.prepare_cache(root, cache)
            for key in ("CARGO_BUILD_TARGET_DIR", "CARGO_BUILD_BUILD_DIR"):
                with patch.dict(os.environ, {"CARGO_TARGET_DIR": str(cache), key: str(root / "other")}, clear=True):
                    with self.assertRaisesRegex(ValueError, key):
                        guard.prepare_cache(root, cache)
            with patch.dict(os.environ, {}, clear=True):
                for args in [("--out-dir", str(root / "other")), ("--out-dir=" + str(root / "other"),)]:
                    with self.assertRaisesRegex(ValueError, "separate compiler output"):
                        guard.prepare_cache(root, cache, args)

    def test_no_python_shell_admission_and_refusals(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            tools = root / "bin"
            tools.mkdir()
            for name in ("dirname", "mkdir", "awk", "ln"):
                os.symlink(shutil.which(name), tools / name)
            realpath = tools / "realpath"
            realpath.write_text(f"#!{sys.executable}\nimport pathlib,sys\nprint(pathlib.Path(sys.argv[-1]).resolve())\n")
            realpath.chmod(0o755)
            df = tools / "df"
            df.write_text('#!/bin/sh\nprintf "Filesystem 1024-blocks Used Available Capacity Mounted\\noutput 999999999 0 %s 0%% /output\\nsource 999999999 0 %s 0%% /source\\n" "$OUTPUT_KIB" "$SOURCE_KIB"\n')
            df.chmod(0o755)
            marker = root / "compiler-started"
            compiler = root / "compiler"
            compiler.write_text(f"#!/bin/sh\nprintf started > '{marker}'\n")
            compiler.chmod(0o755)
            target = root / "worker-output"
            existing = root / "keep"
            existing.write_text("unfinished data")
            base = {"PATH": str(tools), "CARGO_TARGET_DIR": str(target),
                    "RENTEMESTER_CARGO_CACHE": str(root / "legacy"),
                    "OUTPUT_KIB": "41943040", "SOURCE_KIB": "41943040"}
            launcher = str(guard.ROOT / "scripts/rust/cargo-guard.sh")
            cases = [({}, [], True, ""),
                     ({"SOURCE_KIB": "14680064"}, [], False, "CRITICAL"),
                     ({"OUTPUT_KIB": "14680064"}, [], False, "CRITICAL"),
                     ({"SOURCE_KIB": "30408704"}, [], False, "STOP"),
                     ({"OUTPUT_KIB": "30408704"}, [], False, "STOP"),
                     ({"CARGO_TARGET_DIR": "relative"}, [], False, "absolute"),
                     ({"CARGO_BUILD_TARGET_DIR": str(root / "other")}, [], False, "Separate"),
                     ({"CARGO_BUILD_BUILD_DIR": str(root / "other")}, [], False, "Separate"),
                     ({}, ["--out-dir", str(root / "other")], False, "Separate"),
                     ({}, ["--out-dir=" + str(root / "other")], False, "Separate")]
            for changes, args, admitted, diagnostic in cases:
                with self.subTest(changes=changes, args=args):
                    marker.unlink(missing_ok=True)
                    result = subprocess.run(["/bin/sh", launcher, str(compiler), *args],
                                            env={**base, **changes}, text=True, capture_output=True)
                    self.assertEqual(result.returncode == 0, admitted, result.stderr)
                    self.assertEqual(marker.exists(), admitted)
                    self.assertIn(diagnostic, result.stderr)
                    self.assertEqual(existing.read_text(), "unfinished data")
            self.assertFalse((root / "legacy").exists())

    def test_real_cargo_in_linked_worktrees_builds_concurrently(self):
        cargo = Path.home() / ".cargo/bin/cargo"
        if not cargo.is_file():
            self.skipTest("Rust toolchain unavailable")
        guard.check_space(guard.ROOT)
        guard.check_space(Path(tempfile.gettempdir()))
        with tempfile.TemporaryDirectory(prefix="rentemester-cache-fixture-") as tmp:
            root = Path(tmp) / "primary"
            root.mkdir()
            (root / "src").mkdir()
            (root / "scripts/rust").mkdir(parents=True)
            (root / ".cargo").mkdir()
            shutil.copy2(guard.__file__, root / "scripts/rust/cargo-guard.py")
            shutil.copy2(guard.ROOT / "scripts/rust/cargo-guard.sh", root / "scripts/rust/cargo-guard.sh")
            shutil.copy2(guard.ROOT / ".cargo/config.toml", root / ".cargo/config.toml")
            (root / "Cargo.toml").write_text('[package]\nname="synthetic-cache-guard"\nversion="0.0.0"\nedition="2024"\n')
            # Both build scripts must be alive together to pass the barrier.
            (root / "build.rs").write_text('''
use std::{env, fs, thread, time::{Duration, Instant}};
fn main() {
    let barrier = env::var("SYNTHETIC_BARRIER").unwrap();
    let id = env::var("SYNTHETIC_WORKER").unwrap();
    fs::write(format!("{barrier}/{id}"), "ready").unwrap();
    let peer = if id == "a" { "b" } else { "a" };
    let start = Instant::now();
    while !std::path::Path::new(&format!("{barrier}/{peer}")).exists() {
        assert!(start.elapsed() < Duration::from_secs(30), "builds did not overlap");
        thread::sleep(Duration::from_millis(50));
    }
}
''')
            (root / "src/lib.rs").write_text("pub fn synthetic() -> u8 { 7 }\n")
            (root / ".gitignore").write_text("/target/\n")
            env = {key: value for key, value in os.environ.items()
                   if key not in ("CARGO_TARGET_DIR", "CARGO_BUILD_TARGET_DIR", "CARGO_BUILD_BUILD_DIR",
                                  "RENTEMESTER_CARGO_CACHE", "RUSTC_WRAPPER", "CARGO_BUILD_RUSTC_WRAPPER")}

            def run(*args, cwd=root, check=True):
                return subprocess.run(args, cwd=cwd, env=env, text=True, capture_output=True, check=check)

            run("git", "init", "-q")
            run("git", "add", ".")
            run("git", "-c", "user.name=Synthetic", "-c", "user.email=synthetic@example.invalid",
                "commit", "-qm", "Synthetic cache fixture")
            worker = Path(tmp) / "worker"
            run("git", "worktree", "add", "--detach", str(worker), "HEAD")
            barrier = Path(tmp) / "barrier"
            barrier.mkdir()
            targets = [Path(tmp) / "output-a", Path(tmp) / "output-b"]
            processes = []
            try:
                for checkout, target, identity in zip((root, worker), targets, ("a", "b")):
                    build_env = {**env, "CARGO_TARGET_DIR": str(target),
                                 "SYNTHETIC_BARRIER": str(barrier), "SYNTHETIC_WORKER": identity}
                    processes.append(subprocess.Popen([str(cargo), "check", "--offline"], cwd=checkout,
                                                      env=build_env, text=True, stdout=subprocess.PIPE,
                                                      stderr=subprocess.PIPE))
                for process in processes:
                    stdout, stderr = process.communicate(timeout=60)
                    self.assertEqual(process.returncode, 0, stdout + stderr)
            finally:
                for process in processes:
                    if process.poll() is None:
                        process.kill()
                        process.communicate()
            self.assertEqual({p.name for p in barrier.iterdir()}, {"a", "b"})
            for target in targets:
                self.assertTrue(list((target / "debug/deps").glob("libsynthetic_cache_guard*.rmeta")))
            self.assertFalse((root / "target").exists())
            self.assertFalse((worker / "target").exists())



if __name__ == "__main__":
    unittest.main()
