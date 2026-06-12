// Tests: src/core/import/synthetic-csv.ts — the worked example parser (#185).
//
// The synthetic-CSV parser is the reference implementation of the
// `SourceParser` contract: it proves the framework end-to-end against an
// INVENTED sample (examples/import-synthetic.csv), so the framework has a
// deterministic test even before the real e-conomic/Billy parsers exist.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts, verifyAuditChain } from "../../src/core/ledger";
import { getOpeningBalance } from "../../src/core/opening-balance";
import { runImport } from "../../src/core/import/framework";
import { syntheticCsvParser } from "../../src/core/import/synthetic-csv";

import { cleanupDir } from "../helpers/cleanup";
const SAMPLE_PATH = join(import.meta.dir, "../../examples/import-synthetic.csv");

function freshCompany(prefix: string) {
  const root = mkdtempSync(join(tmpdir(), prefix));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  seedAccounts(db);
  return { root, db };
}

describe("synthetic-CSV example parser", () => {
  test("parses the synthetic sample into a balanced normalised source", () => {
    const csv = readFileSync(SAMPLE_PATH, "utf8");
    const parsed = syntheticCsvParser.parse(csv);
    expect(parsed.ok).toBe(true);
    const source = parsed.source!;
    expect(source.sourceSystem).toBe("synthetic-csv");
    expect(source.cutOverDate).toBe("2026-01-01");
    expect(source.openingBalances.length).toBeGreaterThan(0);

    // The sample is balanced by construction.
    const debit = source.openingBalances.reduce((s, l) => s + (l.debitAmount ?? 0), 0);
    const credit = source.openingBalances.reduce((s, l) => s + (l.creditAmount ?? 0), 0);
    expect(debit).toBe(credit);
  });

  test("imports the synthetic sample deterministically end-to-end", () => {
    const csv = readFileSync(SAMPLE_PATH, "utf8");
    const { root, db } = freshCompany("rentemester-import-synth-");
    try {
      const parsed = syntheticCsvParser.parse(csv);
      expect(parsed.ok).toBe(true);
      const result = runImport(db, parsed.source!, { createdBy: "user:tester" });
      expect(result.errors).toEqual([]);
      expect(result.ok).toBe(true);
      expect(result.sourceSystem).toBe("synthetic-csv");
      expect(verifyAuditChain(db).ok).toBe(true);

      const marker = getOpeningBalance(db);
      expect(marker).not.toBeNull();
      expect(marker!.cutOverDate).toBe("2026-01-01");

      // Deterministic: a second run on a fresh company yields the same entryNo.
      const { root: root2, db: db2 } = freshCompany("rentemester-import-synth2-");
      try {
        const result2 = runImport(db2, syntheticCsvParser.parse(csv).source!, {
          createdBy: "user:tester",
        });
        expect(result2.entryNo).toBe(result.entryNo);
        expect(result2.auditTrail).toEqual(result.auditTrail);
      } finally {
        db2.close();
        cleanupDir(root2);
      }
    } finally {
      db.close();
      cleanupDir(root);
    }
  });

  test("rejects a CSV whose opening balances do not balance", () => {
    const unbalanced = [
      "# source: synthetic-csv",
      "# cutOverDate: 2026-01-01",
      "section,accountNo,name,debit,credit",
      "account,2000,Bank,,",
      "account,5000,Egenkapital,,",
      "opening,2000,,80000,",
      "opening,5000,,,70000",
    ].join("\n");
    const parsed = syntheticCsvParser.parse(unbalanced);
    // Either the parser flags it, or the framework does — but it must not post.
    const { root, db } = freshCompany("rentemester-import-synth-unbal-");
    try {
      if (parsed.ok) {
        const result = runImport(db, parsed.source!, { createdBy: "user:tester" });
        expect(result.ok).toBe(false);
      } else {
        expect(parsed.errors.length).toBeGreaterThan(0);
      }
      expect(getOpeningBalance(db)).toBeNull();
    } finally {
      db.close();
      cleanupDir(root);
    }
  });

  test("rejects a CSV missing the cut-over date header", () => {
    const noDate = [
      "# source: synthetic-csv",
      "section,accountNo,name,debit,credit",
      "account,2000,Bank,,",
      "opening,2000,,80000,",
    ].join("\n");
    const parsed = syntheticCsvParser.parse(noDate);
    expect(parsed.ok).toBe(false);
    expect(parsed.errors.join(" ").toLowerCase()).toContain("cut-over");
  });
});
