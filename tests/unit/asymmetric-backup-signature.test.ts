import { describe, expect, test } from "bun:test";
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import {
  backupAsymmetricSignaturePath,
  backupEd25519PrivateKeyPath,
  backupEd25519PublicKeyPath,
  createSystemBackup,
  ensureEd25519Keypair,
  exportBackupPublicKey,
  publicKeyHint,
} from "../../src/core/system-backups";
import { restoreSystemBackup, verifyBackupSignature } from "../../src/core/system-restore";

// These tests lock in the asymmetric-signature contract from issue #99:
//  - HMAC remains the default (parallel, never weakened).
//  - --sign-with-ed25519 adds a 3rd-party-verifiable signature whose public
//    key ships in the backup under config/backup-manifest.pub.
//  - The private key (PEM at <companyRoot>/.backup-signing-key.pem) is
//    mode 0o600 and never copied into the backup.
//  - verifyBackupSignature() succeeds with just the public key, fails when
//    any file or the signature is tampered.

function listAllFiles(root: string): string[] {
  const out: string[] = [];
  if (!existsSync(root)) return out;
  const stack = [root];
  while (stack.length > 0) {
    const dir = stack.pop()!;
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const full = join(dir, entry.name);
      if (entry.isDirectory()) stack.push(full);
      else if (entry.isFile()) out.push(full);
    }
  }
  return out;
}

describe("asymmetric backup signatures (issue #99)", () => {
  test("ensureEd25519Keypair generates one PEM keypair, private at 0o600, public under config/", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-ed25519-genkey-"));
    ensureCompanyDirs(companyRoot);

    const kp1 = ensureEd25519Keypair(companyRoot);
    expect(existsSync(kp1.privateKeyPath)).toBe(true);
    expect(existsSync(kp1.publicKeyPath)).toBe(true);
    expect(kp1.privateKeyPath).toBe(backupEd25519PrivateKeyPath(companyRoot));
    expect(kp1.publicKeyPath).toBe(backupEd25519PublicKeyPath(companyRoot));

    // Private key is owner-only, public is normal-readable.
    const privMode = statSync(kp1.privateKeyPath).mode & 0o777;
    expect(privMode).toBe(0o600);

    expect(kp1.publicKeyPem).toContain("BEGIN PUBLIC KEY");
    expect(kp1.privateKeyPem).toContain("BEGIN PRIVATE KEY");

    // Idempotent: second call returns the SAME bytes.
    const kp2 = ensureEd25519Keypair(companyRoot);
    expect(kp2.publicKeyPem).toBe(kp1.publicKeyPem);
    expect(kp2.privateKeyPem).toBe(kp1.privateKeyPem);

    rmSync(companyRoot, { recursive: true, force: true });
  });

  test("backup signed with --sign-with-ed25519 ships public key inside backup but never private key", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-ed25519-backup-"));
    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    migrate(db);

    const backup = createSystemBackup(db, companyRoot, {
      createdAt: "2026-05-17T02:09:00.000Z",
      signWithEd25519: true,
    });
    expect(backup.ok).toBe(true);
    expect(backup.backupDir).toBeDefined();

    // Ed25519 signature file ships with the backup.
    expect(existsSync(backupAsymmetricSignaturePath(backup.backupDir!))).toBe(true);
    // HMAC also still ships (parallel coverage, never weakened).
    expect(existsSync(join(backup.backupDir!, "manifest.json.hmac"))).toBe(true);

    // Public key shipped inside backup config dir.
    const shippedPub = join(backup.backupDir!, "config", "backup-manifest.pub");
    expect(existsSync(shippedPub)).toBe(true);
    const shippedPubText = readFileSync(shippedPub, "utf8");
    expect(shippedPubText).toContain("BEGIN PUBLIC KEY");

    // Private key files are NEVER copied into the backup.
    const backupFiles = listAllFiles(backup.backupDir!);
    const leakedPrivate = backupFiles.filter(
      (p) => p.endsWith(".backup-signing-key.pem") || /private[-_]?key/i.test(p),
    );
    expect(leakedPrivate).toEqual([]);

    // Manifest records asymmetric block with hint matching the public key.
    const manifest = JSON.parse(readFileSync(join(backup.backupDir!, "manifest.json"), "utf8"));
    expect(manifest.asymmetricSignature.algorithm).toBe("ed25519");
    expect(manifest.asymmetricSignature.signaturePath).toBe("manifest.json.ed25519.sig");
    expect(manifest.asymmetricSignature.publicKeyPath).toBe("config/backup-manifest.pub");
    expect(manifest.asymmetricSignature.publicKeyHint).toBe(publicKeyHint(shippedPubText));

    db.close();
    rmSync(companyRoot, { recursive: true, force: true });
  });

  test("a 3rd-party with only the public key can verify the backup end-to-end", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-ed25519-3rdparty-"));
    const auditorRoot = mkdtempSync(join(tmpdir(), "rentemester-ed25519-auditor-"));
    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    migrate(db);

    const backup = createSystemBackup(db, companyRoot, {
      createdAt: "2026-05-17T02:09:00.000Z",
      signWithEd25519: true,
    });
    expect(backup.ok).toBe(true);

    // Auditor receives the public key out-of-band (e.g., signed email).
    const auditorPubPath = join(auditorRoot, "company-x.pub");
    const exportResult = exportBackupPublicKey(companyRoot, auditorPubPath);
    expect(exportResult.ok).toBe(true);
    expect(existsSync(auditorPubPath)).toBe(true);

    // Auditor verifies WITHOUT source access (no HMAC key, no company root).
    const verify = verifyBackupSignature({
      backupDir: backup.backupDir!,
      publicKeyPath: auditorPubPath,
    });
    expect(verify.ok).toBe(true);
    expect(verify.algorithms).toContain("ed25519");
    expect(verify.errors).toEqual([]);

    db.close();
    rmSync(companyRoot, { recursive: true, force: true });
    rmSync(auditorRoot, { recursive: true, force: true });
  });

  test("tampering with a backup file is caught even when both signatures are present", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-ed25519-tamper-file-"));
    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    migrate(db);

    const backup = createSystemBackup(db, companyRoot, {
      createdAt: "2026-05-17T02:09:00.000Z",
      signWithEd25519: true,
    });
    expect(backup.ok).toBe(true);

    // Tamper: append a byte to the SQLite snapshot.
    const dbSnap = join(backup.backupDir!, "ledger.sqlite");
    const original = readFileSync(dbSnap);
    writeFileSync(dbSnap, Buffer.concat([original, Buffer.from([0x00])]));

    const verify = verifyBackupSignature({
      backupDir: backup.backupDir!,
      publicKeyPath: join(companyRoot, "config", "backup-manifest.pub"),
    });
    expect(verify.ok).toBe(false);
    // Either size or sha256 mismatch on ledger.sqlite.
    expect(verify.errors.some((e) => e.includes("ledger.sqlite"))).toBe(true);

    db.close();
    rmSync(companyRoot, { recursive: true, force: true });
  });

  test("tampering with the ed25519 signature itself fails verification", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-ed25519-tamper-sig-"));
    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    migrate(db);

    const backup = createSystemBackup(db, companyRoot, {
      createdAt: "2026-05-17T02:09:00.000Z",
      signWithEd25519: true,
    });
    expect(backup.ok).toBe(true);

    // Tamper: flip one byte inside the base64-encoded ed25519 signature.
    const sigPath = backupAsymmetricSignaturePath(backup.backupDir!);
    const sigText = readFileSync(sigPath, "utf8").trim();
    // Replace first char with a different valid base64 char.
    const corrupted = (sigText.startsWith("A") ? "B" : "A") + sigText.slice(1) + "\n";
    writeFileSync(sigPath, corrupted);

    const verify = verifyBackupSignature({
      backupDir: backup.backupDir!,
      publicKeyPath: join(companyRoot, "config", "backup-manifest.pub"),
    });
    expect(verify.ok).toBe(false);
    expect(verify.errors.some((e) => /ed25519/i.test(e))).toBe(true);

    db.close();
    rmSync(companyRoot, { recursive: true, force: true });
  });

  test("HMAC-only backup (no ed25519 opt-in) still verifies, restore still works", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-ed25519-hmac-only-"));
    const targetRoot = mkdtempSync(join(tmpdir(), "rentemester-ed25519-hmac-target-"));
    rmSync(targetRoot, { recursive: true, force: true }); // restore wants empty/absent
    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    migrate(db);

    const backup = createSystemBackup(db, companyRoot, {
      createdAt: "2026-05-17T02:09:00.000Z",
      // signWithEd25519 omitted -> default HMAC-only
    });
    expect(backup.ok).toBe(true);
    expect(existsSync(backupAsymmetricSignaturePath(backup.backupDir!))).toBe(false);

    // Standalone verify with HMAC key still works.
    const verify = verifyBackupSignature({
      backupDir: backup.backupDir!,
      verificationKeyPath: join(companyRoot, ".backup-manifest.key"),
    });
    expect(verify.ok).toBe(true);
    expect(verify.algorithms).toEqual(["hmac-sha256"]);

    // Full restore path unaffected.
    const restore = restoreSystemBackup({
      backupDir: backup.backupDir!,
      targetCompanyRoot: targetRoot,
    });
    expect(restore.ok).toBe(true);

    db.close();
    rmSync(companyRoot, { recursive: true, force: true });
    rmSync(targetRoot, { recursive: true, force: true });
  });

  test("dual-signed backup: restore verifies BOTH HMAC and ed25519", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-ed25519-dual-"));
    const targetRoot = mkdtempSync(join(tmpdir(), "rentemester-ed25519-dual-target-"));
    rmSync(targetRoot, { recursive: true, force: true });
    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    migrate(db);

    const backup = createSystemBackup(db, companyRoot, {
      createdAt: "2026-05-17T02:09:00.000Z",
      signWithEd25519: true,
    });
    expect(backup.ok).toBe(true);

    const restore = restoreSystemBackup({
      backupDir: backup.backupDir!,
      targetCompanyRoot: targetRoot,
    });
    expect(restore.ok).toBe(true);
    expect(restore.backupId).toBe(backup.backupId);

    db.close();
    rmSync(companyRoot, { recursive: true, force: true });
    rmSync(targetRoot, { recursive: true, force: true });
  });

  test("verify-backup-signature CLI runs end-to-end with only public key", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-ed25519-cli-"));
    const company = join(root, "company");
    const auditorDir = join(root, "auditor");

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts system backup --company ${company} --at 2026-05-17T02:09:00.000Z --sign-with-ed25519`.quiet();

    // Export the public key to the auditor.
    await Bun.$`bun run src/cli.ts system export-public-key --company ${company} --out ${auditorDir}/company.pub`.quiet();
    expect(existsSync(join(auditorDir, "company.pub"))).toBe(true);

    const backupDir = join(company, "backups", "backup-20260517T020900Z");

    // Auditor verifies with only the public key — no HMAC key, no company root.
    const verifyProc = Bun.spawn(
      [
        "bun",
        "run",
        "src/cli.ts",
        "system",
        "verify-backup-signature",
        "--backup-dir",
        backupDir,
        "--public-key",
        join(auditorDir, "company.pub"),
      ],
      { cwd: process.cwd(), stdout: "pipe", stderr: "pipe" },
    );
    const stdout = await new Response(verifyProc.stdout).text();
    const stderr = await new Response(verifyProc.stderr).text();
    const exitCode = await verifyProc.exited;

    rmSync(root, { recursive: true, force: true });

    expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.algorithms).toContain("ed25519");
  });
});
