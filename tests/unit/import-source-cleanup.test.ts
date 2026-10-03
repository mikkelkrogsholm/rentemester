import { expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { initialiseCompanyVolume } from "../../src/core/company";
import { openDb } from "../../src/core/db";
import { companyPaths } from "../../src/core/paths";
import { runImportFromSource } from "../../src/core/import/framework";
import type { SourceParser } from "../../src/core/import/types";

for (const mode of ["success", "reject", "throw"] as const) {
  test(`ZIP import removes its owned extraction after parser ${mode}`, () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-import-cleanup-"));
    const source = mkdtempSync(join(tmpdir(), "rentemester-import-cleanup-source-"));
    initialiseCompanyVolume(root, { cvr: "DK12345678" });
    const db = openDb(companyPaths(root).db);
    let extracted = "";
    try {
      writeFileSync(join(source, "export.txt"), "Synthetic export");
      const zip = join(root, "export.zip");
      expect(spawnSync("zip", ["-q", zip, "export.txt"], { cwd: source }).status).toBe(0);
      const parser: SourceParser = { system: "synthetic", label: "Synthetic", parseSource: resolved => {
        extracted = resolved.rootDir;
        expect(existsSync(extracted)).toBe(true);
        if (mode === "throw") throw new Error("synthetic parser failure");
        if (mode === "reject") return { ok: false, errors: ["synthetic rejection"] };
        return { ok: true, errors: [], source: { sourceSystem: "synthetic", cutOverDate: "2026-01-01", chartOfAccounts: [{ accountNo: "2000", name: "Bank" }, { accountNo: "5000", name: "Equity" }], openingBalances: [{ accountNo: "2000", debitAmount: 125 }, { accountNo: "5000", creditAmount: 125 }] } };
      } };
      if (mode === "throw") expect(() => runImportFromSource(db, parser, zip)).toThrow("synthetic parser failure");
      else expect(runImportFromSource(db, parser, zip).ok).toBe(mode === "success");
      expect(extracted).not.toBe(source);
      expect(existsSync(extracted)).toBe(false);
      expect(existsSync(zip)).toBe(true);
      expect(existsSync(join(source, "export.txt"))).toBe(true);
      expect(runImportFromSource(db, { system: "synthetic", label: "Synthetic", parseSource: () => ({ ok: false, errors: ["synthetic rejection"] }) }, source).ok).toBe(false);
      expect(existsSync(join(source, "export.txt"))).toBe(true);
    } finally {
      db.close();
      if (extracted) rmSync(extracted, { recursive: true, force: true });
      rmSync(root, { recursive: true, force: true });
      rmSync(source, { recursive: true, force: true });
    }
  });
}
