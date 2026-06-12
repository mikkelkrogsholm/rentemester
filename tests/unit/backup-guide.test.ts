import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { migrate, openDb } from "../../src/core/db";
import { createSystemBackup } from "../../src/core/system-backups";
import { getBackupGovernanceStatus } from "../../src/core/backup-governance";
import { renderBackupGuide } from "../../src/core/backup-guide";

import { cleanupDir } from "../helpers/cleanup";
describe("backup guide page", () => {
  test("renders the legal rules and is deterministic for the same input", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-guide-"));
    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    try {
      migrate(db);
      createSystemBackup(db, companyRoot, { createdAt: "2026-05-17T02:09:00.000Z" });
      const governance = getBackupGovernanceStatus(db, companyRoot, "2026-05-17T03:00:00.000Z");

      const input = {
        generatedAt: "2026-05-17T03:00:00.000Z",
        companyName: "Rentemester ApS",
        governance,
      };
      const html = renderBackupGuide(input);

      expect(html).toContain("<!doctype html>");
      expect(html).toContain("BEK 205/2024");
      expect(html).toContain("§ 4, stk. 2");
      expect(html).toContain("EU- eller EØS-land");
      expect(html).toContain("Rentemester ApS");
      expect(html).toContain("Din status nu");

      // Deterministic: identical input -> byte-identical output.
      expect(renderBackupGuide(input)).toBe(html);
    } finally {
      db.close();
      cleanupDir(companyRoot);
    }
  });

  test("escapes the company name", () => {
    const companyRoot = mkdtempSync(join(tmpdir(), "rentemester-guide-esc-"));
    const paths = ensureCompanyDirs(companyRoot);
    const db = openDb(paths.db);
    try {
      migrate(db);
      const governance = getBackupGovernanceStatus(db, companyRoot, "2026-05-17T03:00:00.000Z");
      const html = renderBackupGuide({
        generatedAt: "2026-05-17T03:00:00.000Z",
        companyName: "<script>alert(1)</script>",
        governance,
      });
      expect(html).not.toContain("<script>alert(1)</script>");
      expect(html).toContain("&lt;script&gt;");
    } finally {
      db.close();
      cleanupDir(companyRoot);
    }
  });
});
