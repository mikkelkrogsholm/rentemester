// Tests: src/core/import/dinero.ts — the Dinero export parser, chart of
// accounts & company master data (#193).
//
// A Dinero data export carries the company's full chart of accounts and master
// data. The parser turns the multi-file export (`Firmaoplysninger.csv` +
// `<year>/Kontoplan.csv`) into a normalised `ImportSource`; the reconciler
// lands the chart in `accounts` and the master data in `companies`. The
// opening balance from `<year>/Posteringer.csv` is covered separately by
// import-dinero-opening.test.ts (#194); postings after the cut-over date
// remain out of scope (#195).
//
// Tests run against the synthetic fixture in examples/import-dinero/ — the real
// Dinero export is private and is never committed.
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { postJournalEntry, seedAccounts, verifyAuditChain } from "../../src/core/ledger";
import { resolveSource } from "../../src/core/import/source";
import { dineroParser } from "../../src/core/import/dinero";
import { reconcileChartOfAccounts, reconcileCompanyMasterData } from "../../src/core/import/reconcile";

import { cleanupDir } from "../helpers/cleanup";
const FIXTURE = join(import.meta.dir, "../../examples/import-dinero");

function freshCompany(prefix: string) {
  const root = mkdtempSync(join(tmpdir(), prefix));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  seedAccounts(db);
  return { root, db };
}

describe("Dinero parser: multi-file export -> normalised source", () => {
  test("declares the files it needs and parses the synthetic fixture", () => {
    expect(dineroParser.requiredFiles).toContain("Firmaoplysninger.csv");
    const parsed = dineroParser.parseSource!(resolveSource(FIXTURE));
    expect(parsed.errors).toEqual([]);
    expect(parsed.ok).toBe(true);
    expect(parsed.source!.sourceSystem).toBe("dinero");
    expect(parsed.source!.chartOfAccounts.length).toBe(17);
  });

  test("fails clearly when a required file is missing", () => {
    const dir = mkdtempSync(join(tmpdir(), "rentemester-dinero-missing-"));
    try {
      const parsed = dineroParser.parseSource!(resolveSource(dir));
      expect(parsed.ok).toBe(false);
      expect(parsed.errors.join(" ")).toContain("Firmaoplysninger.csv");
    } finally {
      cleanupDir(dir);
    }
  });

  test("classifies every Dinero account type, splitting equity out of Passiv", () => {
    const chart = dineroParser.parseSource!(resolveSource(FIXTURE)).source!.chartOfAccounts;
    const by = (no: string) => chart.find((a) => a.accountNo === no)!;
    expect(by("1000").normalizedType).toBe("income");
    expect(by("1000").normalBalance).toBe("credit");
    expect(by("3000").normalizedType).toBe("expense");
    expect(by("3000").normalBalance).toBe("debit");
    expect(by("5510").normalizedType).toBe("asset");
    expect(by("5510").normalBalance).toBe("debit");
    expect(by("55000").normalizedType).toBe("liability");
    expect(by("55000").normalBalance).toBe("credit");
    // Equity accounts (60000-60040) sit under Passiv but classify as equity.
    expect(by("60000").normalizedType).toBe("equity");
    expect(by("60000").normalBalance).toBe("credit");
    expect(by("60040").normalizedType).toBe("equity");
  });

  test("maps Dinero Momstype codes to Rentemester VAT codes", () => {
    const chart = dineroParser.parseSource!(resolveSource(FIXTURE)).source!.chartOfAccounts;
    const by = (no: string) => chart.find((a) => a.accountNo === no)!;
    expect(by("1000").defaultVatCode).toBe("DK_SALE_25");
    expect(by("3000").defaultVatCode).toBe("DK_PURCHASE_25");
    expect(by("3010").defaultVatCode).toBe("EU_SERVICE_REVERSE_CHARGE");
    expect(by("3070").defaultVatCode).toBe("REPRESENTATION_SPECIAL");
    // An account with an empty Momstype has no default VAT code.
    expect(by("3090").defaultVatCode ?? null).toBeNull();
  });

  test("surfaces unmapped Momstype codes instead of dropping them", () => {
    const parsed = dineroParser.parseSource!(resolveSource(FIXTURE));
    const unmapped = parsed.source!.unmappedVatCodes ?? [];
    // The fixture deliberately carries codes Rentemester has no equivalent for.
    expect(unmapped.length).toBeGreaterThan(0);
    expect(unmapped.some((c) => c.startsWith("IVV"))).toBe(true);
    // An account whose Momstype is unmapped carries no default VAT code.
    const chart = parsed.source!.chartOfAccounts;
    expect(chart.find((a) => a.accountNo === "3020")!.defaultVatCode ?? null).toBeNull();
  });

  test("parses company master data from Firmaoplysninger.csv", () => {
    const md = dineroParser.parseSource!(resolveSource(FIXTURE)).source!.companyMasterData!;
    expect(md.name).toBe("Eksempel Bogføring ApS");
    expect(md.cvr).toBe("12345678");
    expect(md.address).toContain("Eksempelvej");
    expect(md.city).toBe("København Ø");
  });
});

describe("Dinero reconciliation into the live ledger", () => {
  test("creates missing accounts with correct type / normal_balance / VAT code", () => {
    const { root, db } = freshCompany("rentemester-dinero-chart-");
    try {
      const source = dineroParser.parseSource!(resolveSource(FIXTURE)).source!;
      const result = reconcileChartOfAccounts(db, source, { createdBy: "user:tester" });
      // 1000 is seeded ("Omsætning, ydelser") so it must be left intact.
      expect(result.existing).toContain("1000");
      expect(result.created).toContain("60000");

      const acct = db
        .query("SELECT type, normal_balance, default_vat_code FROM accounts WHERE account_no = ?")
        .get("60000") as { type: string; normal_balance: string; default_vat_code: string | null };
      expect(acct.type).toBe("equity");
      expect(acct.normal_balance).toBe("credit");

      const eu = db
        .query("SELECT default_vat_code FROM accounts WHERE account_no = ?")
        .get("3010") as { default_vat_code: string | null };
      expect(eu.default_vat_code).toBe("EU_SERVICE_REVERSE_CHARGE");

      // The reconciliation is audited.
      const audit = db
        .query("SELECT COUNT(*) AS n FROM audit_log WHERE event_type = 'import_chart_reconcile'")
        .get() as { n: number };
      expect(audit.n).toBeGreaterThan(0);
      expect(verifyAuditChain(db).ok).toBe(true);
    } finally {
      db.close();
      cleanupDir(root);
    }
  });

  test("reclassifies a postings-free colliding account to the source definition", () => {
    const { root, db } = freshCompany("rentemester-dinero-existing-");
    try {
      const source = dineroParser.parseSource!(resolveSource(FIXTURE)).source!;
      // Mimic the real Helheim bug: an account number the Rentemester seed
      // classified one way collides with a Dinero account classified another.
      // Seeded account 3000 is 'Software og SaaS' (expense/debit); the Dinero
      // fixture's 3000 is 'Vareforbrug'. Mis-seed it as an asset so the import
      // has to correct it (the same shape as Dinero 2000 colliding with Bank).
      db.prepare(
        "UPDATE accounts SET type = 'asset', normal_balance = 'debit', name = 'Old seed name' WHERE account_no = '3000'",
      ).run();
      const source3000 = source.chartOfAccounts.find((a) => a.accountNo === "3000")!;
      expect(source3000.normalizedType).toBe("expense");

      const result = reconcileChartOfAccounts(db, source, { createdBy: "user:tester" });
      // A freshly seeded company has no journal lines, so the source chart is
      // authoritative — 3000 is reclassified, not merely reported.
      expect(result.updated).toContain("3000");
      expect(result.conflicts).toEqual([]);

      const after = db
        .query("SELECT name, type, normal_balance FROM accounts WHERE account_no = ?")
        .get("3000") as { name: string; type: string; normal_balance: string };
      expect(after.type).toBe("expense");
      expect(after.normal_balance).toBe("debit");
      expect(after.name).toBe(source3000.name);
      expect(result.differences.some((d) => d.includes("account 3000"))).toBe(true);
      expect(verifyAuditChain(db).ok).toBe(true);
    } finally {
      db.close();
      cleanupDir(root);
    }
  });

  test("does NOT reclassify an existing account that already has journal lines", () => {
    const { root, db } = freshCompany("rentemester-dinero-conflict-");
    try {
      const source = dineroParser.parseSource!(resolveSource(FIXTURE)).source!;
      // Seeded account 3000 ('Software og SaaS', expense/debit) differs from
      // the fixture's 3000 ('Vareforbrug') — but this time it already carries
      // a posting, so it must NOT be reclassified.
      const posted = postJournalEntry(db, {
        transactionDate: "2026-01-15",
        text: "Seed activity on account 3000",
        importedHistorical: true,
        createdByProgram: "rentemester-import-postings",
        lines: [
          { accountNo: "3000", debitAmount: 100 },
          { accountNo: "2000", creditAmount: 100 },
        ],
      });
      expect(posted.ok).toBe(true);

      const result = reconcileChartOfAccounts(db, source, { createdBy: "user:tester" });
      // 3000 has postings -> it is a conflict, never reclassified.
      expect(result.updated).not.toContain("3000");
      expect(result.conflicts.some((c) => c.includes("account 3000"))).toBe(true);

      const after = db
        .query("SELECT name FROM accounts WHERE account_no = ?")
        .get("3000") as { name: string };
      expect(after.name).toBe("Software og SaaS");
      expect(verifyAuditChain(db).ok).toBe(true);
    } finally {
      db.close();
      cleanupDir(root);
    }
  });

  test("reports unmapped VAT codes in the reconciliation result", () => {
    const { root, db } = freshCompany("rentemester-dinero-unmapped-");
    try {
      const source = dineroParser.parseSource!(resolveSource(FIXTURE)).source!;
      const result = reconcileChartOfAccounts(db, source, { createdBy: "user:tester" });
      expect(result.unmappedVatCodes.length).toBeGreaterThan(0);
    } finally {
      db.close();
      cleanupDir(root);
    }
  });

  test("populates the companies row without overwriting a non-empty field", () => {
    const { root, db } = freshCompany("rentemester-dinero-company-");
    try {
      // The company starts with the default name and no CVR.
      const source = dineroParser.parseSource!(resolveSource(FIXTURE)).source!;
      const result = reconcileCompanyMasterData(db, source, { createdBy: "user:tester" });
      expect(result.updatedFields).toContain("cvr");
      const company = db
        .query("SELECT name, cvr FROM companies WHERE id = 1")
        .get() as { name: string; cvr: string | null };
      expect(company.name).toBe("Eksempel Bogføring ApS");
      expect(company.cvr).toBe("DK12345678");

      // A second reconcile must not overwrite the now non-empty name.
      const second = reconcileCompanyMasterData(db, source, { createdBy: "user:tester" });
      expect(second.notes.join(" ")).toContain("name");
    } finally {
      db.close();
      cleanupDir(root);
    }
  });
});
