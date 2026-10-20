"""Check disk and cache refusals without filling a real disk or compiling Rust."""
import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
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

    def test_override_and_compiler_output_cannot_create_separate_cache(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            cache = root / "shared"
            with patch.dict(os.environ, {"CARGO_TARGET_DIR": str(root / "other")}, clear=True):
                with self.assertRaisesRegex(ValueError, "CARGO_TARGET_DIR"):
                    guard.prepare_cache(root, cache)
            with patch.dict(os.environ, {}, clear=True):
                for args in [("--out-dir", str(root / "other")), ("--out-dir=" + str(root / "other"),)]:
                    with self.assertRaisesRegex(ValueError, "separate compiler output"):
                        guard.prepare_cache(root, cache, args)

    def test_real_cargo_in_linked_worktree_and_refused_override(self):
        cargo = Path.home() / ".cargo/bin/cargo"
        if not cargo.is_file():
            self.skipTest("Rust toolchain unavailable")
        guard.check_space(guard.shared_cache())
        with tempfile.TemporaryDirectory(prefix="rentemester-cache-fixture-") as tmp:
            root = Path(tmp) / "primary"
            root.mkdir()
            (root / "src").mkdir()
            (root / "scripts/rust").mkdir(parents=True)
            (root / ".cargo").mkdir()
            shutil.copy2(guard.__file__, root / "scripts/rust/cargo-guard.py")
            shutil.copy2(guard.ROOT / ".cargo/config.toml", root / ".cargo/config.toml")
            (root / "Cargo.toml").write_text('[package]\nname="synthetic-cache-guard"\nversion="0.0.0"\nedition="2024"\n')
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
            run(str(cargo), "check", "--offline")
            run(str(cargo), "check", "--offline", cwd=worker)
            self.assertTrue((worker / "target").is_symlink())
            self.assertEqual((worker / "target").resolve(), (root / "target").resolve())
            self.assertTrue(list((root / "target/debug/deps").glob("libsynthetic_cache_guard*.rmeta")))
            other = Path(tmp) / "refused-cache"
            rejected = run(str(cargo), "check", "--offline", "--target-dir", str(other), cwd=worker, check=False)
            self.assertNotEqual(rejected.returncode, 0)
            self.assertIn("Refusing separate compiler output", rejected.stderr)
            self.assertFalse(list(other.rglob("*.rmeta")))


if __name__ == "__main__":
    unittest.main()
