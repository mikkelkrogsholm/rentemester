// Tests: src/core/system-backups.ts
import { describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync, readdirSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { createHmac } from "node:crypto";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { ingestDocument } from "../../src/core/documents";
import { backupManifestKeyPath, createSystemBackup, getBackupComplianceStatus } from "../../src/core/system-backups";
import { writeFileAtomic } from "../../src/core/atomic-file";

describe("system backups", () => {
  test("creates a full backup snapshot with manifest and copied documents", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-backup-"));
    const inboxRoot = mkdtempSync(join(tmpdir(), "rentemester-backup-inbox-"));
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

    const result = createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:09:00.000Z" });
    expect(result.ok).toBe(true);
    expect(existsSync(result.dbSnapshotPath!)).toBe(true);
    expect(existsSync(result.manifestPath!)).toBe(true);
    expect(existsSync(join(result.backupDir!, "manifest.json.hmac"))).toBe(true);
    expect(existsSync(join(companyRoot, ".backup-manifest.key"))).toBe(true);

    const manifest = JSON.parse(readFileSync(result.manifestPath!, "utf8"));
    expect(manifest.backupId).toBe("backup-20260517T020900Z");
    expect(manifest.dbSnapshot.path).toBe("ledger.sqlite");
    expect(manifest.manifestSignature.algorithm).toBe("hmac-sha256");
    expect(manifest.manifestSignature.signaturePath).toBe("manifest.json.hmac");
    expect(manifest.copiedFiles.documentsOriginals[0].path).toStartWith("documents-originals/");
    expect(manifest.copiedFiles.documentsOriginals.length).toBe(1);
    expect(manifest.ledgerStats.documents).toBe(1);

    db.close();
    rmSync(companyRoot, { recursive: true, force: true });
    rmSync(inboxRoot, { recursive: true, force: true });
  });

  test("takes a locked snapshot so concurrent writes wait and stay out of the backup", async () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-backup-lock-"));
    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    migrate(db);

    const writerScript = join(companyRoot, "writer.ts");
    writeFileSync(writerScript, `
      await Bun.sleep(50);
      const { openDb } = await import(${JSON.stringify(join(process.cwd(), "src/core/db.ts"))});
      const db = openDb(process.argv[2]);
      const started = Date.now();
      db.run(
        "INSERT INTO bank_transactions (transaction_date, booking_date, text, amount, currency, reference, import_batch_id, source_file_hash, transaction_hash) VALUES (?, ?, ?, ?, 'DKK', ?, ?, ?, ?)",
        "2026-05-17",
        "2026-05-17",
        "Concurrent customer payment",
        500,
        "LOCK-REF-1",
        "batch-lock-1",
        "hash-lock-a",
        "tx-lock-1",
      );
      console.log(String(Date.now() - started));
      db.close();
    `);

    const writer = Bun.spawn(["bun", "run", writerScript, paths.db], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });

    const backup = createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:09:00.000Z", debugHoldMs: 400 });
    expect(backup.ok).toBe(true);

    const writerStdout = await new Response(writer.stdout).text();
    const writerStderr = await new Response(writer.stderr).text();
    const writerExit = await writer.exited;
    expect({ writerExit, writerStderr }).toEqual({ writerExit: 0, writerStderr: "" });

    const waitedMs = Number(writerStdout.trim());
    expect(Number.isFinite(waitedMs)).toBe(true);
    expect(waitedMs).toBeGreaterThanOrEqual(250);

    const manifest = JSON.parse(readFileSync(backup.manifestPath!, "utf8"));
    expect(manifest.ledgerStats.bankTransactions).toBe(0);
    const liveCount = (db.query("SELECT COUNT(*) AS n FROM bank_transactions").get() as { n: number }).n;
    expect(liveCount).toBe(1);

    db.close();
    rmSync(companyRoot, { recursive: true, force: true });
  });

  test("issue #151: backup leaves no half-written temp files and the manifest matches its signature", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-backup-atomic-"));
    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    migrate(db);

    const backup = createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:09:00.000Z" });
    expect(backup.ok).toBe(true);

    // No leftover atomic-write temp files anywhere in the backup directory.
    const leftoverTemps = readdirSync(backup.backupDir!).filter((name) => name.endsWith(".tmp"));
    expect(leftoverTemps).toEqual([]);

    // The HMAC signature on disk must match the manifest bytes on disk — if
    // the signature were written before the final manifest text was promoted,
    // or the manifest promoted without its signature, this would diverge.
    const manifestText = readFileSync(join(backup.backupDir!, "manifest.json"), "utf8");
    const signature = readFileSync(join(backup.backupDir!, "manifest.json.hmac"), "utf8").trim();
    const keyHex = readFileSync(backupManifestKeyPath(companyRoot), "utf8").trim();
    const expected = createHmac("sha256", Buffer.from(keyHex, "hex")).update(manifestText).digest("hex");
    expect(signature).toBe(expected);

    db.close();
    rmSync(companyRoot, { recursive: true, force: true });
  });

  test("issue #151: writeFileAtomic refuses to follow a pre-planted same-directory symlink", () => {
    const dir = mkdtempSync(join(tmpdir(), "rentemester-atomic-symlink-"));
    const finalPath = join(dir, "manifest.json");
    const victim = join(dir, "victim.json");
    writeFileSync(victim, "original-victim\n");

    // An attacker pre-plants a predictable temp name as a symlink pointing at
    // a victim file. An exclusive-create temp open must NOT follow it.
    const guessedTemp = join(dir, `.manifest.json.${process.pid}.0.0000000000000000.tmp`);
    symlinkSync(victim, guessedTemp);

    writeFileAtomic(finalPath, "real-content\n");

    // The victim is untouched; the real write landed at finalPath.
    expect(readFileSync(victim, "utf8")).toBe("original-victim\n");
    expect(readFileSync(finalPath, "utf8")).toBe("real-content\n");

    rmSync(dir, { recursive: true, force: true });
  });

  test("treats same-day bank activity as newer than an earlier same-day backup", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-backup-sameday-"));
    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    migrate(db);

    const backup = createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:09:00.000Z" });
    expect(backup.ok).toBe(true);

    db.run(
      "INSERT INTO bank_transactions (transaction_date, booking_date, text, amount, currency, reference, import_batch_id, source_file_hash, transaction_hash) VALUES (?, ?, ?, ?, 'DKK', ?, ?, ?, ?)",
      "2026-05-17",
      "2026-05-17",
      "Later same-day bank activity",
      500,
      "REF-SAMEDAY-1",
      "batch-sameday-1",
      "hash-sameday-a",
      "tx-sameday-1",
    );

    const status = getBackupComplianceStatus(db, companyRoot, "2026-05-17T03:00:00.000Z");
    expect(status.hasActivitySinceBackup).toBe(true);
    expect(status.latestBackupId).toBe("backup-20260517T020900Z");

    db.close();
    rmSync(companyRoot, { recursive: true, force: true });
  });

  test("flags weekly backup duty when activity exists after an old backup", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-backup-due-"));
    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    migrate(db);

    // Dato-relativ til "now" — så testen ikke flagre med kalenderdrift når
    // kør-datoen passerer de hardkodede fixture-datoer. Den gamle backup
    // ligger 8 dage tilbage; den tidlige transaktion ligger 10 dage tilbage
    // (FØR backuppen); den sene transaktion ligger 5 dage tilbage (EFTER
    // backuppen). På den måde er "hasActivitySinceBackup" altid sand.
    const now = new Date();
    const isoDateAt = (offsetDays: number) => {
      const d = new Date(now.getTime() - offsetDays * 24 * 60 * 60 * 1000);
      return d.toISOString().slice(0, 10);
    };

    db.run(
      "INSERT INTO bank_transactions (transaction_date, booking_date, text, amount, currency, reference, import_batch_id, source_file_hash, transaction_hash) VALUES (?, ?, ?, ?, 'DKK', ?, ?, ?, ?)",
      isoDateAt(10),
      isoDateAt(10),
      "Customer payment",
      1250,
      "REF-1",
      "batch-1",
      "hash-a",
      "tx-1",
    );

    const oldBackupAt = new Date(now.getTime() - 8 * 24 * 60 * 60 * 1000).toISOString();
    const statusCheckAt = now.toISOString();
    const backup = createSystemBackup(db, companyRoot, { createdAt: oldBackupAt });
    expect(backup.ok).toBe(true);

    db.run(
      "INSERT INTO bank_transactions (transaction_date, booking_date, text, amount, currency, reference, import_batch_id, source_file_hash, transaction_hash) VALUES (?, ?, ?, ?, 'DKK', ?, ?, ?, ?)",
      isoDateAt(5),
      isoDateAt(5),
      "Late customer payment",
      500,
      "REF-2",
      "batch-2",
      "hash-b",
      "tx-2",
    );

    const status = getBackupComplianceStatus(db, companyRoot, statusCheckAt);
    expect(status.ok).toBe(false);
    expect(status.backupDue).toBe(true);
    expect(status.hasActivitySinceBackup).toBe(true);
    expect(status.appliedRules).toContain("DK-BOOKKEEPING-BACKUP-001");

    db.close();
    rmSync(companyRoot, { recursive: true, force: true });
  });
});
