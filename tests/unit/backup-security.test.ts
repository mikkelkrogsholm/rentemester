import { describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { join, relative } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { ingestDocument } from "../../src/core/documents";
import { backupManifestKeyPath, createSystemBackup } from "../../src/core/system-backups";

// These tests lock in the threat-model assumptions documented in
// docs/backup-security.md. If any of them fail, the audit document is no
// longer accurate and must be revisited (likely together with the chain
// of trust itself).

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

describe("backup signing chain-of-trust (issue #87)", () => {
  test("signing key lives at <companyRoot>/.backup-manifest.key with 0o600 mode and is never copied into the backup", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-backup-security-"));
    const inboxRoot = mkdtempSync(join(tmpdir(), "rentemester-backup-security-inbox-"));
    const sourceFile = join(inboxRoot, "vendor-invoice.txt");
    writeFileSync(sourceFile, "Invoice 1001\nAmount 1250 DKK\n");

    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    migrate(db);
    ingestDocument(db, companyRoot, sourceFile, {
      source: "email",
      issueDate: "2026-05-16",
      invoiceNo: "INV-1001",
      deliveryDescription: "Bogføring og momsafstemning",
      amountIncVat: 1250,
      currency: "DKK",
      sender: { name: "Leverandør ApS", address: "Sælgervej 1", vatOrCvr: "DK11223344" },
      recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
      vatAmount: 250,
      paymentDetails: "Bankoverførsel",
    });

    const backup = createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:09:00.000Z" });
    expect(backup.ok).toBe(true);

    // 1. Key is at the documented path, OUTSIDE the backups directory.
    const expectedKeyPath = join(companyRoot, ".backup-manifest.key");
    expect(backupManifestKeyPath(companyRoot)).toBe(expectedKeyPath);
    expect(existsSync(expectedKeyPath)).toBe(true);

    const relativeFromBackups = relative(paths.backups, expectedKeyPath);
    expect(relativeFromBackups.startsWith("..")).toBe(true);

    // 2. Filemode is owner-only (0o600). Mask off the file-type bits.
    const mode = statSync(expectedKeyPath).mode & 0o777;
    expect(mode).toBe(0o600);

    // 3. No file matching *.key, *.pem, or *signing* leaks into the backup tree.
    const backupFiles = listAllFiles(paths.backups);
    expect(backupFiles.length).toBeGreaterThan(0); // sanity: we did make a backup
    const leaked = backupFiles.filter((p) => /\.key($|\.)|\.pem$|signing/i.test(p));
    expect(leaked).toEqual([]);

    db.close();
    rmSync(companyRoot, { recursive: true, force: true });
    rmSync(inboxRoot, { recursive: true, force: true });
  });

  test("HMAC signature file ships with the backup but the verifying key does not", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-backup-security-sig-"));
    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    migrate(db);

    const backup = createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:09:00.000Z" });
    expect(backup.ok).toBe(true);

    // The signature file (manifest.json.hmac) MUST be inside the backup so a
    // restorer can check authenticity. The key itself MUST NOT be.
    expect(existsSync(join(backup.backupDir!, "manifest.json.hmac"))).toBe(true);
    expect(existsSync(join(backup.backupDir!, ".backup-manifest.key"))).toBe(false);

    // Walk the entire backup dir; assert the bytes of the key do not appear
    // anywhere in there as a filename.
    const backupFiles = listAllFiles(backup.backupDir!);
    const keyFiles = backupFiles.filter((p) => p.endsWith(".backup-manifest.key"));
    expect(keyFiles).toEqual([]);

    db.close();
    rmSync(companyRoot, { recursive: true, force: true });
  });
});
