// Tests: src/core/system-restore.ts
import { describe, expect, test } from "bun:test";
import { copyFileSync, existsSync, mkdtempSync, mkdirSync, readdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { createHash, createHmac } from "node:crypto";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts, postJournalEntry } from "../../src/core/ledger";
import { ingestDocument } from "../../src/core/documents";
import { backupManifestKeyPath, createSystemBackup } from "../../src/core/system-backups";
import { restoreSystemBackup } from "../../src/core/system-restore";

import { cleanupDir } from "../helpers/cleanup";
function sha256File(path: string) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function rewriteSignedManifest(companyRoot: string, backupDir: string, manifest: Record<string, any>) {
  const manifestPath = join(backupDir, "manifest.json");
  const signaturePath = join(backupDir, "manifest.json.hmac");
  const keyHex = readFileSync(backupManifestKeyPath(companyRoot), "utf8").trim();
  const manifestText = `${JSON.stringify(manifest, null, 2)}\n`;
  const signature = createHmac("sha256", Buffer.from(keyHex, "hex")).update(manifestText).digest("hex");
  writeFileSync(manifestPath, manifestText);
  writeFileSync(signaturePath, `${signature}\n`);
}

describe("system restore", () => {
  test("restores a moved backup into a fresh company root and records a restore audit event", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-restore-"));
    const companyRoot = join(root, "company");
    const movedBackupsRoot = join(root, "moved-backups");
    const restoredRoot = join(root, "restored-company");
    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    migrate(db);
    seedAccounts(db);

    const ingested = ingestDocument(db, companyRoot, join(process.cwd(), "examples/vendor-invoice.txt"), JSON.parse(readFileSync(join(process.cwd(), "examples/vendor-invoice.metadata.json"), "utf8")));
    expect(ingested.ok).toBe(true);
    const journal = postJournalEntry(db, JSON.parse(readFileSync(join(process.cwd(), "examples/journal-entry.expense.json"), "utf8")));
    expect(journal.ok).toBe(true);

    const backup = createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:39:00.000Z" });
    expect(backup.ok).toBe(true);
    db.close();

    mkdirSync(movedBackupsRoot, { recursive: true });
    const movedBackupDir = join(movedBackupsRoot, "portable-backup");
    renameSync(backup.backupDir!, movedBackupDir);

    const prevActor = process.env.RENTEMESTER_ACTOR;
    const prevVia = process.env.RENTEMESTER_ACTOR_VIA;
    process.env.RENTEMESTER_ACTOR = "user:mikkel";
    process.env.RENTEMESTER_ACTOR_VIA = "restore-cli";
    const restored = restoreSystemBackup({ backupDir: movedBackupDir, targetCompanyRoot: restoredRoot, verificationKeyPath: backupManifestKeyPath(companyRoot) });
    if (prevActor === undefined) delete process.env.RENTEMESTER_ACTOR; else process.env.RENTEMESTER_ACTOR = prevActor;
    if (prevVia === undefined) delete process.env.RENTEMESTER_ACTOR_VIA; else process.env.RENTEMESTER_ACTOR_VIA = prevVia;
    expect(restored.ok).toBe(true);
    expect(existsSync(restored.restoredDbPath!)).toBe(true);
    expect(restored.restoredFiles?.documentsOriginals).toBe(1);

    const restoredDb = openDb(join(restoredRoot, "data", "ledger.sqlite"));
    migrate(restoredDb);
    const documentCount = (restoredDb.query("SELECT COUNT(*) AS n FROM documents").get() as { n: number }).n;
    const journalCount = (restoredDb.query("SELECT COUNT(*) AS n FROM journal_entries").get() as { n: number }).n;
    const restoreEvent = restoredDb.query(
      "SELECT event_type, actor, message FROM audit_log WHERE event_type = 'system_restore' ORDER BY id DESC LIMIT 1"
    ).get() as { event_type: string; actor: string | null; message: string } | null;
    restoredDb.close();

    expect(documentCount).toBe(1);
    expect(journalCount).toBe(1);
    expect(restoreEvent?.event_type).toBe("system_restore");
    expect(restoreEvent?.actor).toBe("user:mikkel via restore-cli");
    expect(restoreEvent?.message).toContain("backup-20260517T023900Z");

    cleanupDir(root);
  });

  test("rejects a manifest path that escapes the backup directory", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-restore-escape-"));
    const companyRoot = join(root, "company");
    const restoredRoot = join(root, "restored-company");
    const outsideRoot = join(root, "outside");
    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    migrate(db);
    seedAccounts(db);

    const ingested = ingestDocument(db, companyRoot, join(process.cwd(), "examples/vendor-invoice.txt"), JSON.parse(readFileSync(join(process.cwd(), "examples/vendor-invoice.metadata.json"), "utf8")));
    expect(ingested.ok).toBe(true);

    const backup = createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:39:00.000Z" });
    expect(backup.ok).toBe(true);
    db.close();

    mkdirSync(outsideRoot, { recursive: true });
    const outsideDb = join(outsideRoot, "ledger.sqlite");
    copyFileSync(join(backup.backupDir!, "ledger.sqlite"), outsideDb);

    const manifestPath = join(backup.backupDir!, "manifest.json");
    const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
    manifest.dbSnapshot.path = outsideDb;
    manifest.dbSnapshot.sha256 = sha256File(outsideDb);
    manifest.dbSnapshot.sizeBytes = readFileSync(outsideDb).byteLength;
    rewriteSignedManifest(companyRoot, backup.backupDir!, manifest);

    const restored = restoreSystemBackup({ backupDir: backup.backupDir!, targetCompanyRoot: restoredRoot });
    expect(restored.ok).toBe(false);
    expect(restored.errors[0]).toContain("manifest path escapes backup dir");

    cleanupDir(root);
  });

  test("rejects a backup whose files and manifest are rewritten without a valid manifest signature", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-restore-tampered-"));
    const companyRoot = join(root, "company");
    const restoredRoot = join(root, "restored-company");
    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    migrate(db);
    seedAccounts(db);

    const ingested = ingestDocument(db, companyRoot, join(process.cwd(), "examples/vendor-invoice.txt"), JSON.parse(readFileSync(join(process.cwd(), "examples/vendor-invoice.metadata.json"), "utf8")));
    expect(ingested.ok).toBe(true);

    const backup = createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:39:00.000Z" });
    expect(backup.ok).toBe(true);
    db.close();

    const snapshotPath = join(backup.backupDir!, "ledger.sqlite");
    const snapshotDb = openDb(snapshotPath);
    snapshotDb.exec("PRAGMA foreign_keys = OFF");
    snapshotDb.exec("DROP TRIGGER IF EXISTS documents_no_update");
    snapshotDb.run("UPDATE documents SET original_filename = 'tampered.txt' WHERE id = 1");
    snapshotDb.close();

    const manifestPath = join(backup.backupDir!, "manifest.json");
    const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
    manifest.dbSnapshot.sha256 = sha256File(snapshotPath);
    manifest.dbSnapshot.sizeBytes = readFileSync(snapshotPath).byteLength;
    writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);

    const restored = restoreSystemBackup({ backupDir: backup.backupDir!, targetCompanyRoot: restoredRoot });
    expect(restored.ok).toBe(false);
    expect(restored.errors[0]).toContain("authenticity");

    cleanupDir(root);
  });

  test("rejects a backup whose snapshot passes file hash checks but fails audit validation", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-restore-auditfail-"));
    const companyRoot = join(root, "company");
    const restoredRoot = join(root, "restored-company");
    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    migrate(db);
    seedAccounts(db);

    const ingested = ingestDocument(db, companyRoot, join(process.cwd(), "examples/vendor-invoice.txt"), JSON.parse(readFileSync(join(process.cwd(), "examples/vendor-invoice.metadata.json"), "utf8")));
    expect(ingested.ok).toBe(true);
    const journal = postJournalEntry(db, JSON.parse(readFileSync(join(process.cwd(), "examples/journal-entry.expense.json"), "utf8")));
    expect(journal.ok).toBe(true);

    const backup = createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:39:00.000Z" });
    expect(backup.ok).toBe(true);
    db.close();

    const snapshotDb = openDb(join(backup.backupDir!, "ledger.sqlite"));
    snapshotDb.exec("PRAGMA foreign_keys = OFF");
    snapshotDb.exec("DROP TRIGGER IF EXISTS journal_entries_no_update");
    snapshotDb.run("UPDATE journal_entries SET previous_hash = 'BROKEN' WHERE id = 1");
    snapshotDb.close();

    const manifestPath = join(backup.backupDir!, "manifest.json");
    const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
    manifest.dbSnapshot.sha256 = sha256File(join(backup.backupDir!, "ledger.sqlite"));
    manifest.dbSnapshot.sizeBytes = readFileSync(join(backup.backupDir!, "ledger.sqlite")).byteLength;
    rewriteSignedManifest(companyRoot, backup.backupDir!, manifest);

    const restored = restoreSystemBackup({ backupDir: backup.backupDir!, targetCompanyRoot: restoredRoot });
    expect(restored.ok).toBe(false);
    expect(restored.errors[0]).toContain("broken audit chain");

    cleanupDir(root);
  });

  test("issue #139: a failed restore leaves the target with no clobbered ledger or document files", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-restore-rollback-"));
    const companyRoot = join(root, "company");
    const restoredRoot = join(root, "restored-company");
    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    migrate(db);
    seedAccounts(db);

    const ingested = ingestDocument(db, companyRoot, join(process.cwd(), "examples/vendor-invoice.txt"), JSON.parse(readFileSync(join(process.cwd(), "examples/vendor-invoice.metadata.json"), "utf8")));
    expect(ingested.ok).toBe(true);
    const journal = postJournalEntry(db, JSON.parse(readFileSync(join(process.cwd(), "examples/journal-entry.expense.json"), "utf8")));
    expect(journal.ok).toBe(true);

    const backup = createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:39:00.000Z" });
    expect(backup.ok).toBe(true);
    db.close();

    // Corrupt the audit chain inside the snapshot; hashes still match the
    // manifest, so file checks pass but validateRestoredDb must reject it.
    const snapshotDb = openDb(join(backup.backupDir!, "ledger.sqlite"));
    snapshotDb.exec("PRAGMA foreign_keys = OFF");
    snapshotDb.exec("DROP TRIGGER IF EXISTS journal_entries_no_update");
    snapshotDb.run("UPDATE journal_entries SET previous_hash = 'BROKEN' WHERE id = 1");
    snapshotDb.close();
    const manifest = JSON.parse(readFileSync(join(backup.backupDir!, "manifest.json"), "utf8"));
    manifest.dbSnapshot.sha256 = sha256File(join(backup.backupDir!, "ledger.sqlite"));
    manifest.dbSnapshot.sizeBytes = readFileSync(join(backup.backupDir!, "ledger.sqlite")).byteLength;
    rewriteSignedManifest(companyRoot, backup.backupDir!, manifest);

    const restored = restoreSystemBackup({ backupDir: backup.backupDir!, targetCompanyRoot: restoredRoot });
    expect(restored.ok).toBe(false);
    expect(restored.errors[0]).toContain("broken audit chain");

    // The target must NOT be left half-restored: no ledger DB, no copied
    // document files should have survived the failed validation.
    expect(existsSync(join(restoredRoot, "data", "ledger.sqlite"))).toBe(false);
    const docsDir = join(restoredRoot, "documents", "originals");
    const leakedDocs = existsSync(docsDir) ? readdirSync(docsDir) : [];
    expect(leakedDocs).toEqual([]);

    cleanupDir(root);
  });
});
