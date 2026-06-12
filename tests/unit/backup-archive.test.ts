import { describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { existsSync, mkdtempSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { migrate, openDb } from "../../src/core/db";
import {
  backupManifestKeyPath,
  createSystemBackup,
  packBackupArchive,
} from "../../src/core/system-backups";
import { restoreSystemBackup } from "../../src/core/system-restore";
import { verifyAuditChain } from "../../src/core/ledger";

import { cleanupDir } from "../helpers/cleanup";
describe("backup archive", () => {
  test("packs a backup directory into one deterministic .tar with a sha256 sidecar", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-archive-"));
    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    try {
      migrate(db);
      const backup = createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:09:00.000Z" });
      expect(backup.ok).toBe(true);

      const packed = packBackupArchive(db, companyRoot, { backupId: backup.backupId });
      expect(packed.ok).toBe(true);
      expect(existsSync(packed.archivePath!)).toBe(true);
      expect(existsSync(packed.sha256Path!)).toBe(true);

      const archive = readFileSync(packed.archivePath!);
      expect(createHash("sha256").update(archive).digest("hex")).toBe(packed.archiveSha256);
      expect(readFileSync(packed.sha256Path!, "utf8")).toContain(packed.archiveSha256!);
    } finally {
      db.close();
      cleanupDir(companyRoot);
    }
  });

  test("restores from a single-file archive with an explicit verification key", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-archive-src-"));
    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    let archivePath: string;
    try {
      migrate(db);
      const backup = createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:09:00.000Z" });
      const packed = packBackupArchive(db, companyRoot, { backupId: backup.backupId });
      archivePath = packed.archivePath!;
    } finally {
      db.close();
    }

    const target = join(mkdtempSync(join(tmpdir(), "rentemester-archive-dst-")), "restored");
    try {
      const result = restoreSystemBackup({
        backupDir: archivePath,
        targetCompanyRoot: target,
        verificationKeyPath: backupManifestKeyPath(companyRoot),
      });
      expect(result.ok).toBe(true);

      const restoredDb = openDb(join(target, "data", "ledger.sqlite"));
      try {
        expect(verifyAuditChain(restoredDb).ok).toBe(true);
      } finally {
        restoredDb.close();
      }
    } finally {
      cleanupDir(companyRoot);
      cleanupDir(target);
    }
  });

  test("fails clearly when there is no backup to archive", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-archive-empty-"));
    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    try {
      migrate(db);
      const packed = packBackupArchive(db, companyRoot);
      expect(packed.ok).toBe(false);
      expect(packed.errors[0]).toContain("no backup found");
    } finally {
      db.close();
      cleanupDir(companyRoot);
    }
  });
});
