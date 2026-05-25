// Tests: src/core/import/dinero-archive.ts — the pre-cut-over fiscal-year
// archive for a multi-year Dinero export (#197, epic #173).
//
// A real Dinero export spans several fiscal years; only the cut-over (latest)
// year is posted into the hash-chained live ledger (#194). The EARLIER years
// must NOT be posted but must be kept as a READ-ONLY ARCHIVE: their
// Posteringer / SaldoBalance rows queryable for audit and matching context.
//
// These tests run against the synthetic fixture in examples/import-dinero/
// (a 2024/ pre-cut-over folder + the existing 2025/ cut-over year). The real
// Dinero export is private and is never committed.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, mkdirSync, writeFileSync, cpSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts } from "../../src/core/ledger";
import { resolveSource } from "../../src/core/import/source";
import { dineroParser } from "../../src/core/import/dinero";
import { runImportFromSource } from "../../src/core/import/framework";
import {
  parseArchiveYears,
  archiveDineroYears,
  queryArchive,
  checkRollForward,
} from "../../src/core/import/dinero-archive";

const FIXTURE = join(import.meta.dir, "../../examples/import-dinero");

function freshCompany(prefix: string) {
  const root = mkdtempSync(join(tmpdir(), prefix));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  seedAccounts(db);
  return { root, db };
}

/**
 * Copies the multi-year fixture into a writable temp dir so a test can inject
 * an inconsistency without mutating the committed fixture.
 */
function copyFixture(prefix: string): string {
  const dir = mkdtempSync(join(tmpdir(), prefix));
  cpSync(FIXTURE, dir, { recursive: true });
  return dir;
}

describe("Dinero archive: parsing the pre-cut-over years", () => {
  test("identifies 2025 as cut-over and 2024 as a pre-cut-over year", () => {
    const parsed = parseArchiveYears(resolveSource(FIXTURE));
    expect(parsed.ok).toBe(true);
    expect(parsed.cutOverYear).toBe(2025);
    expect(parsed.years.map((y) => y.fiscalYear)).toEqual([2024]);
  });

  test("archives EVERY 2024 posting row plus its SaldoBalance lines", () => {
    const parsed = parseArchiveYears(resolveSource(FIXTURE));
    const y2024 = parsed.years.find((y) => y.fiscalYear === 2024)!;
    // 8 Primobeholdning rows + 6 movement rows = 14 postings.
    expect(y2024.postings.length).toBe(14);
    // 8 closing-balance lines.
    expect(y2024.balances.length).toBe(8);
    // Amounts parsed straight into kroner, comma decimals respected.
    const bank = y2024.balances.find((b) => b.accountNo === "5510")!;
    expect(bank.amount).toBe(88200.5);
  });
});

describe("Dinero archive: persisting to import_archive_* tables", () => {
  test("writes the pre-cut-over years and leaves the live journal untouched", () => {
    const { root, db } = freshCompany("rentemester-dinero-archive-");
    try {
      const before = db.query("SELECT COUNT(*) AS n FROM journal_entries").get() as { n: number };
      const result = archiveDineroYears(db, resolveSource(FIXTURE));
      expect(result.ok).toBe(true);
      expect(result.archivedYears).toEqual([2024]);

      // The archive lands in its dedicated tables...
      const years = db
        .query("SELECT fiscal_year, posting_count, balance_count FROM import_archive_years")
        .all() as Array<{ fiscal_year: number; posting_count: number; balance_count: number }>;
      expect(years).toEqual([{ fiscal_year: 2024, posting_count: 14, balance_count: 8 }]);

      // ...and NOT in the hash-chained live ledger.
      const after = db.query("SELECT COUNT(*) AS n FROM journal_entries").get() as { n: number };
      expect(after.n).toBe(before.n);
    } finally {
      db.close();
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("is idempotent — a second archive run skips the already-archived year", () => {
    const { root, db } = freshCompany("rentemester-dinero-archive-idem-");
    try {
      const first = archiveDineroYears(db, resolveSource(FIXTURE));
      expect(first.archivedYears).toEqual([2024]);
      const second = archiveDineroYears(db, resolveSource(FIXTURE));
      expect(second.archivedYears).toEqual([]);
      expect(second.skippedYears).toEqual([2024]);
      const count = db
        .query("SELECT COUNT(*) AS n FROM import_archive_years")
        .get() as { n: number };
      expect(count.n).toBe(1);
    } finally {
      db.close();
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("the archive tables are append-only — UPDATE/DELETE are rejected", () => {
    const { root, db } = freshCompany("rentemester-dinero-archive-ro-");
    try {
      archiveDineroYears(db, resolveSource(FIXTURE));
      expect(() => db.run("UPDATE import_archive_years SET fiscal_year = 9999")).toThrow();
      expect(() => db.run("DELETE FROM import_archive_postings")).toThrow();
    } finally {
      db.close();
      rmSync(root, { recursive: true, force: true });
    }
  });
});

describe("Dinero archive: the read path", () => {
  test("queryArchive lists the archived years (headers only)", () => {
    const { root, db } = freshCompany("rentemester-dinero-archive-list-");
    try {
      archiveDineroYears(db, resolveSource(FIXTURE));
      const result = queryArchive(db);
      expect(result.ok).toBe(true);
      expect(result.years.map((y) => y.fiscalYear)).toEqual([2024]);
      // A header-only listing carries counts but no detail rows.
      expect(result.years[0]!.postingCount).toBe(14);
      expect(result.years[0]!.postings).toBeUndefined();
    } finally {
      db.close();
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("queryArchive with a fiscalYear returns the full detail rows", () => {
    const { root, db } = freshCompany("rentemester-dinero-archive-year-");
    try {
      archiveDineroYears(db, resolveSource(FIXTURE));
      const result = queryArchive(db, { fiscalYear: 2024 });
      expect(result.ok).toBe(true);
      const year = result.years[0]!;
      expect(year.postings!.length).toBe(14);
      expect(year.balances!.length).toBe(8);
      const bank = year.balances!.find((b) => b.accountNo === "5510")!;
      expect(bank.amount).toBe(88200.5);
    } finally {
      db.close();
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("queryArchive for an unknown year fails clearly", () => {
    const { root, db } = freshCompany("rentemester-dinero-archive-miss-");
    try {
      archiveDineroYears(db, resolveSource(FIXTURE));
      const result = queryArchive(db, { fiscalYear: 1999 });
      expect(result.ok).toBe(false);
      expect(result.errors.join(" ")).toContain("1999");
    } finally {
      db.close();
      rmSync(root, { recursive: true, force: true });
    }
  });
});

describe("Dinero archive: the roll-forward consistency check", () => {
  test("passes on the consistent fixture: 2024 closing -> 2025 opening", () => {
    const { root, db } = freshCompany("rentemester-dinero-rollfwd-ok-");
    try {
      archiveDineroYears(db, resolveSource(FIXTURE));
      const result = checkRollForward(db, resolveSource(FIXTURE));
      expect(result.errors).toEqual([]);
      expect(result.ok).toBe(true);
      expect(result.breaks).toEqual([]);
      // The single step rolls into the cut-over year.
      expect(result.steps.length).toBe(1);
      expect(result.steps[0]!.fromYear).toBe(2024);
      expect(result.steps[0]!.toYear).toBe(2025);
      expect(result.steps[0]!.toIsCutOver).toBe(true);
    } finally {
      db.close();
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("P&L accounts non-zero at close do NOT produce false breaks (#198)", () => {
    // En realistisk Dinero-eksport beholder P&L-konti i SaldoBalance med deres
    // pre-close-saldo (fx 1000 Salg = -154375,64), mens næste års
    // Primobeholdning kun har balance-sheet-rækker. En naiv 'closing ==
    // opening'-check vil tolke det som et brud — det er korrekt regnskab.
    const { root, db } = freshCompany("rentemester-dinero-rollfwd-pl-");
    const tampered = copyFixture("rentemester-dinero-rollfwd-pl-fix-");
    try {
      // Tilføj P&L-konti til 2024's SaldoBalance — 1000 (income) og 3000
      // (expense) er seedet med korrekt type i kontoplanen, så accountTypeOf
      // klassificerer dem som P&L og skipper dem.
      writeFileSync(
        join(tampered, "2024", "SaldoBalance.csv"),
        [
          "Konto;Kontonavn;Beløb",
          "1000;Omsætning, ydelser;-154375,64",
          "3000;Software og SaaS;42000,00",
          "5500;Driftsmidler;65000,00",
          "5510;Bankkonto;88200,50",
          "5520;Tilgodehavender fra salg;42000,00",
          "55000;Skyldig moms;-31000,00",
          "55010;Anden gæld;-12000,00",
          "60000;Registreret kapital mv.;-40000,00",
          "60010;Overført resultat fra tidligere år;-87200,50",
          "60040;Udbytte;-25000,00",
          "",
        ].join("\n"),
      );
      archiveDineroYears(db, resolveSource(tampered));
      const result = checkRollForward(db, resolveSource(tampered));
      // ZERO false breaks selv om P&L-konti har closing != 0 og opening 0.
      expect(result.breaks).toEqual([]);
      expect(result.ok).toBe(true);
    } finally {
      db.close();
      rmSync(root, { recursive: true, force: true });
      rmSync(tampered, { recursive: true, force: true });
    }
  });

  test("flags an injected break: a tampered 2024 SaldoBalance closing amount", () => {
    const { root, db } = freshCompany("rentemester-dinero-rollfwd-break-");
    const tampered = copyFixture("rentemester-dinero-rollfwd-break-fix-");
    try {
      // Inject a break: 2024's Bankkonto closes at 99999,00 — but 2025's
      // Primobeholdning still opens 5510 at 88200,50. The roll-forward must
      // catch this.
      writeFileSync(
        join(tampered, "2024", "SaldoBalance.csv"),
        [
          "Konto;Kontonavn;Beløb",
          "5500;Driftsmidler;65000,00",
          "5510;Bankkonto;99999,00",
          "5520;Tilgodehavender fra salg;42000,00",
          "55000;Skyldig moms;-31000,00",
          "55010;Anden gæld;-12000,00",
          "60000;Registreret kapital mv.;-40000,00",
          "60010;Overført resultat fra tidligere år;-87200,50",
          "60040;Udbytte;-25000,00",
          "",
        ].join("\n"),
      );
      archiveDineroYears(db, resolveSource(tampered));
      const result = checkRollForward(db, resolveSource(tampered));
      expect(result.ok).toBe(false);
      expect(result.breaks.length).toBe(1);
      const brk = result.breaks[0]!;
      expect(brk.accountNo).toBe("5510");
      expect(brk.fromYear).toBe(2024);
      expect(brk.toYear).toBe(2025);
      expect(brk.closingAmount).toBe(99999);
      expect(brk.openingAmount).toBe(88200.5);
    } finally {
      db.close();
      rmSync(root, { recursive: true, force: true });
      rmSync(tampered, { recursive: true, force: true });
    }
  });
});

describe("Dinero import flow: archiving wired into runImportFromSource", () => {
  test("posts the cut-over primobalance AND archives 2024, leaving 2024 out of the journal", () => {
    const { root, db } = freshCompany("rentemester-dinero-flow-");
    try {
      const result = runImportFromSource(db, dineroParser, FIXTURE, { createdBy: "user:tester" });
      expect(result.errors).toEqual([]);
      expect(result.ok).toBe(true);
      // Only the cut-over year (2025) reached the live ledger: the primobalance
      // plus the cut-over year's replayed year-to-date postings (#195). Nothing
      // from the archived 2024 year is posted — no journal entry predates the
      // cut-over date.
      expect(result.cutOverDate).toBe("2025-01-01");
      const preCutOver = db
        .query("SELECT COUNT(*) AS n FROM journal_entries WHERE transaction_date < '2025-01-01'")
        .get() as { n: number };
      expect(preCutOver.n).toBe(0);
      // 2024 was archived as read-only reference data.
      const archived = queryArchive(db).years.map((y) => y.fiscalYear);
      expect(archived).toEqual([2024]);
      // The audit trail records both the archive and a passing roll-forward.
      const trail = result.auditTrail.join("\n");
      expect(trail).toContain("Archived fiscal year 2024");
      expect(trail.toLowerCase()).toContain("roll-forward");
    } finally {
      db.close();
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("a single-year export archives nothing and still posts the primobalance", () => {
    const { root, db } = freshCompany("rentemester-dinero-flow-single-");
    // Build a one-year export (just 2025 + Firmaoplysninger).
    const single = mkdtempSync(join(tmpdir(), "rentemester-dinero-single-"));
    try {
      cpSync(join(FIXTURE, "Firmaoplysninger.csv"), join(single, "Firmaoplysninger.csv"));
      mkdirSync(join(single, "2025"));
      cpSync(join(FIXTURE, "2025"), join(single, "2025"), { recursive: true });
      const result = runImportFromSource(db, dineroParser, single, { createdBy: "user:tester" });
      expect(result.ok).toBe(true);
      expect(queryArchive(db).years).toEqual([]);
    } finally {
      db.close();
      rmSync(root, { recursive: true, force: true });
      rmSync(single, { recursive: true, force: true });
    }
  });
});
