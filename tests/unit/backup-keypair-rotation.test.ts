// Tests: src/core/system-backups.ts (rotateBackupKeypair) +
// scripts/verify-rentemester-backup.mjs (standalone verifier).
import { describe, expect, test } from "bun:test";
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import {
  backupEd25519PrivateKeyPath,
  backupEd25519PublicKeyPath,
  createSystemBackup,
  packBackupArchive,
  publicKeyHint,
  rotateBackupKeypair,
} from "../../src/core/system-backups";

function makeCompany(label: string) {
  const root = mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
  const paths = ensureCompanyDirs(root);
  const db = openDb(paths.db);
  migrate(db);
  return { root, db };
}

describe("rotateBackupKeypair", () => {
  test("archives the old keypair, installs a new one, and audit-logs the rotation", () => {
    const { root, db } = makeCompany("rotate-ok");
    try {
      // Bootstrap a keypair via a real signed backup so the rotate path has
      // something to rotate AWAY from.
      const backup = createSystemBackup(db, root, { signWithEd25519: true });
      expect(backup.ok).toBe(true);
      const oldPriv = readFileSync(backupEd25519PrivateKeyPath(root), "utf8");
      const oldPub = readFileSync(backupEd25519PublicKeyPath(root), "utf8");
      const oldHint = publicKeyHint(oldPub);

      const rotated = rotateBackupKeypair(db, root, {
        reason: "Annual rotation",
        rotatedAt: "2026-06-01T08:00:00.000Z",
      });
      expect(rotated.ok).toBe(true);
      expect(rotated.oldPublicKeyHint).toBe(oldHint);
      expect(rotated.newPublicKeyHint).toBeDefined();
      expect(rotated.newPublicKeyHint).not.toBe(oldHint);

      // The old keypair is archived under backup-keys-archive/ with its
      // fingerprint in the filename — a verifier holding the OLD public key
      // can recognise it.
      const archiveDir = join(root, "backup-keys-archive");
      const archived = readdirSync(archiveDir);
      expect(archived.some((f) => f.includes(oldHint) && f.endsWith(".pub.pem"))).toBe(true);
      expect(archived.some((f) => f.includes(oldHint) && f.endsWith(".key.pem"))).toBe(true);
      const archivedPub = readFileSync(rotated.archivedPublicKeyPath!, "utf8");
      expect(archivedPub).toBe(oldPub);

      // The live key files now hold a DIFFERENT keypair.
      const newPriv = readFileSync(backupEd25519PrivateKeyPath(root), "utf8");
      const newPub = readFileSync(backupEd25519PublicKeyPath(root), "utf8");
      expect(newPriv).not.toBe(oldPriv);
      expect(newPub).not.toBe(oldPub);
      expect(publicKeyHint(newPub)).toBe(rotated.newPublicKeyHint);

      // The audit log records WHY the key was rotated.
      const audit = db.query(
        "SELECT event_type, message FROM audit_log WHERE event_type = 'backup_keypair_rotated'",
      ).get() as { event_type: string; message: string } | null;
      expect(audit).not.toBeNull();
      expect(audit!.message).toContain("Annual rotation");
      expect(audit!.message).toContain(oldHint);
      expect(audit!.message).toContain(rotated.newPublicKeyHint!);
    } finally {
      db.close();
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("refuses an empty reason", () => {
    const { root, db } = makeCompany("rotate-no-reason");
    try {
      createSystemBackup(db, root, { signWithEd25519: true });
      const result = rotateBackupKeypair(db, root, { reason: "   " });
      expect(result.ok).toBe(false);
      expect(result.errors[0]).toContain("reason is required");
    } finally {
      db.close();
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("refuses to rotate when no keypair has been bootstrapped yet", () => {
    const { root, db } = makeCompany("rotate-no-key");
    try {
      const result = rotateBackupKeypair(db, root, { reason: "premature" });
      expect(result.ok).toBe(false);
      expect(result.errors[0]).toContain("no existing ed25519 keypair");
    } finally {
      db.close();
      rmSync(root, { recursive: true, force: true });
    }
  });
});

async function runVerifyScript(tarPath: string, pubKeyPath: string) {
  const proc = Bun.spawn([
    "bun",
    "run",
    "scripts/verify-rentemester-backup.mjs",
    tarPath,
    pubKeyPath,
  ], { stdout: "pipe", stderr: "pipe" });
  const stdout = await new Response(proc.stdout).text();
  const stderr = await new Response(proc.stderr).text();
  const exitCode = await proc.exited;
  return { stdout, stderr, exitCode };
}

describe("standalone verify script", () => {
  test("OK against a real signed backup .tar with the matching public key", async () => {
    const { root, db } = makeCompany("verify-script-ok");
    try {
      const backup = createSystemBackup(db, root, { signWithEd25519: true });
      expect(backup.ok).toBe(true);
      const archive = packBackupArchive(db, root, { backupId: backup.backupId! });
      expect(archive.ok).toBe(true);
      const pubKeyPath = backupEd25519PublicKeyPath(root);

      const verify = await runVerifyScript(archive.archivePath!, pubKeyPath);
      expect(verify.exitCode).toBe(0);
      expect(verify.stdout).toContain("OK:");
    } finally {
      db.close();
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("FAIL when verifying with a wrong (unrelated) public key", async () => {
    const { root, db } = makeCompany("verify-script-wrong-key");
    try {
      const backup = createSystemBackup(db, root, { signWithEd25519: true });
      const archive = packBackupArchive(db, root, { backupId: backup.backupId! });

      // Generate an UNRELATED keypair and use its public key to verify.
      const { generateKeyPairSync } = await import("node:crypto");
      const { publicKey } = generateKeyPairSync("ed25519");
      const otherPubPath = join(root, "other-pub.pem");
      writeFileSync(otherPubPath, publicKey.export({ type: "spki", format: "pem" }).toString());

      const verify = await runVerifyScript(archive.archivePath!, otherPubPath);
      expect(verify.exitCode).toBe(1);
      expect(verify.stderr).toContain("FAIL");
    } finally {
      db.close();
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("FAIL with a clear message when the supplied tar is not a Rentemester backup", async () => {
    const { root, db } = makeCompany("verify-script-bad-tar");
    try {
      // A real signed backup gives us a real public key to use.
      createSystemBackup(db, root, { signWithEd25519: true });
      const pubKeyPath = backupEd25519PublicKeyPath(root);

      // A 1024-byte zero buffer is a syntactically-valid empty tar
      // (no entries before the terminating zero block) — it should be
      // rejected because it carries no manifest.
      const garbage = join(root, "garbage.tar");
      writeFileSync(garbage, Buffer.alloc(1024));

      const verify = await runVerifyScript(garbage, pubKeyPath);
      expect(verify.exitCode).toBe(1);
      expect(verify.stderr).toContain("manifest.json");
    } finally {
      db.close();
      rmSync(root, { recursive: true, force: true });
    }
  });
});

// Touch a referenced export so it is not pruned by tree-shakers in the future.
void existsSync;
