import { expect, test } from "bun:test";
import { mkdtempSync, rmSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { migrate, openDb } from "../../src/core/db";
import { seedAccounts, postJournalEntry } from "../../src/core/ledger";
import { HISTORICAL_IMPORT_PROGRAM } from "../../src/core/import-provenance";
import { postDineroPostings } from "../../src/core/import/dinero-postings";
import { buildVatReport } from "../../src/core/vat";
import { buildVatFiling } from "../../src/core/vat-filing";
import { effectivePeriodState, reopenAccountingPeriod, setCompanyVatPeriodType } from "../../src/core/periods";
import { closeAccountingPeriod, seedHistoricalClosedPeriod } from "../helpers/close-period";
import { closeAccountingPeriod as closeAccountingPeriodCore } from "../../src/core/periods";
import { vatRubrikkerForPeriod } from "../../src/server/data/vat";
import { vatPositionForPeriod } from "../../src/server/data/vat";
import { resolveAccountRole } from "../../src/core/account-roles";
import { exportAuthorityPackage } from "../../src/core/authority-export";

function freshDb() {
  const root = mkdtempSync(join(tmpdir(), "rentemester-vat-critical-"));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  seedAccounts(db);
  db.run("INSERT INTO companies (id, name) VALUES (1, 'Test ApS')");
  return { root, db };
}

test("historical import alone may use an account default; manual JSON provenance cannot", () => {
  const { root, db } = freshDb();
  const trusted = postDineroPostings(db, [{
    transactionDate: "2026-05-01",
    text: "trusted import",
    voucherRef: "VAT-1",
    lines: [
      { accountNo: "3000", debitAmount: 100 },
      { accountNo: "4000", debitAmount: 25 },
      { accountNo: "2000", creditAmount: 125 },
    ],
  }], new Set(["3000", "4000", "2000"]));
  expect(trusted.ok).toBe(true);
  expect((db.query("SELECT vat_code FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id WHERE a.account_no = '3000'").get() as { vat_code: string }).vat_code).toBe("DK_PURCHASE_25");

  const forged = postJournalEntry(db, {
    transactionDate: "2026-05-02", text: "manual JSON",
    createdByProgram: HISTORICAL_IMPORT_PROGRAM,
    importedHistorical: true,
    historicalImportProvenance: JSON.parse("{}"),
    lines: [{ accountNo: "3000", debitAmount: 50 }, { accountNo: "2000", creditAmount: 50 }],
  } as never);
  expect(forged.ok).toBe(false);
  expect(forged.errors.join(" ")).toContain("reserved for the verified historical-import adapter");

  // Simulate an immutable legacy Dinero voucher that predates persisted
  // `vat_code`: the exact trusted marker may infer account 3000's reviewed
  // default at report time, and its booked VAT still reconciles.
  const legacyEntry = db.query(
    `INSERT INTO journal_entries
       (entry_no, transaction_date, text, rule_version, created_by_program, entry_hash)
     VALUES ('LEGACY-VAT-1', '2026-05-03', 'legacy trusted import', 'legacy', ?, 'legacy-hash')
     RETURNING id`,
  ).get(HISTORICAL_IMPORT_PROGRAM) as { id: number };
  for (const [accountNo, debit, credit] of [
    ["3000", 100, 0],
    ["4000", 25, 0],
    ["2000", 0, 125],
  ] as const) {
    db.query(
      `INSERT INTO journal_lines (journal_entry_id, account_id, debit_amount, credit_amount)
       SELECT ?, id, ?, ? FROM accounts WHERE account_no = ?`,
    ).run(legacyEntry.id, debit, credit, accountNo);
  }
  const report = buildVatReport(db, "2026-05-01", "2026-05-31");
  expect(report.ok).toBe(true);
  expect(report.inputVat).toBe(50);
  expect(report.purchaseBase25).toBe(200);
  db.close(); rmSync(root, { recursive: true, force: true });
});

test("manual VAT control journals require a supported code on a real base line", () => {
  const { root, db } = freshDb();
  const uncoded = postJournalEntry(db, {
    transactionDate: "2026-05-10",
    text: "manual VAT without base code",
    lines: [
      { accountNo: "2000", debitAmount: 125 },
      { accountNo: "1000", creditAmount: 100 },
      { accountNo: "1200", creditAmount: 25 },
    ],
  });
  expect(uncoded.ok).toBe(false);
  expect(uncoded.errors.join(" ")).toContain("require an explicit vatCode on the VAT base line");

  const misplaced = postJournalEntry(db, {
    transactionDate: "2026-05-11",
    text: "code on bank line",
    lines: [
      { accountNo: "2000", debitAmount: 125, vatCode: "DK_SALE_25" },
      { accountNo: "1000", creditAmount: 100 },
      { accountNo: "1200", creditAmount: 25 },
    ],
  });
  expect(misplaced.ok).toBe(false);
  expect(misplaced.errors.join(" ")).toContain("require an explicit vatCode on the VAT base line");

  const typo = postJournalEntry(db, {
    transactionDate: "2026-05-12",
    text: "unknown VAT code",
    lines: [
      { accountNo: "3000", debitAmount: 100, vatCode: "DK_PURCHASE_52" },
      { accountNo: "2000", creditAmount: 100 },
    ],
  });
  expect(typo.ok).toBe(false);
  expect(typo.errors.join(" ")).toContain("is not a supported VAT code");
  db.close(); rmSync(root, { recursive: true, force: true });
});

test("VAT codes are trimmed before persistence", () => {
  const { root, db } = freshDb();
  const imported = postDineroPostings(db, [{
    transactionDate: "2026-05-13",
    text: "whitespace code",
    voucherRef: "WS-1",
    lines: [
      { accountNo: "3000", debitAmount: 100, vatCode: "  DK_PURCHASE_25  " },
      { accountNo: "4000", debitAmount: 25 },
      { accountNo: "2000", creditAmount: 125 },
    ],
  }], new Set(["3000", "4000", "2000"]));
  expect(imported.ok).toBe(true);
  expect(db.query(
    `SELECT jl.vat_code AS vat_code
       FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
      WHERE jl.journal_entry_id = ? AND a.account_no = '3000'`,
  ).get(imported.posted[0]!.entryId)).toEqual({ vat_code: "DK_PURCHASE_25" });
  db.close(); rmSync(root, { recursive: true, force: true });
});

test("pure VAT settlement transfers are allowed and excluded from the period report", () => {
  const { root, db } = freshDb();
  const outputVat = resolveAccountRole(db, "output_vat");
  const settlement = resolveAccountRole(db, "vat_settlement");
  expect(outputVat.ok).toBe(true);
  expect(settlement.ok).toBe(true);
  if (!outputVat.ok || !settlement.ok) throw new Error("native VAT roles missing");
  const posted = postJournalEntry(db, {
    transactionDate: "2026-05-15",
    text: "settle prior VAT return",
    lines: [
      { accountNo: outputVat.accountNo, debitAmount: 25 },
      { accountNo: settlement.accountNo, creditAmount: 25 },
    ],
  });
  expect(posted.ok).toBe(true);
  const report = buildVatReport(db, "2026-05-01", "2026-05-31");
  expect(report.ok).toBe(true);
  expect(report.outputVat).toBe(0);
  expect(report.inputVat).toBe(0);
  db.close(); rmSync(root, { recursive: true, force: true });
});

test("legacy manual amount-only VAT is blocking in report and cockpit projection", () => {
  const { root, db } = freshDb();
  const entry = db.query(
    `INSERT INTO journal_entries
       (entry_no, transaction_date, text, rule_version, created_by_program, entry_hash)
     VALUES ('LEGACY-MANUAL-VAT', '2026-05-20', 'legacy manual VAT', 'legacy', 'rentemester', 'legacy-manual-hash')
     RETURNING id`,
  ).get() as { id: number };
  for (const [accountNo, debit, credit] of [
    ["2000", 125, 0],
    ["1000", 0, 100],
    ["1200", 0, 25],
  ] as const) {
    db.query(
      `INSERT INTO journal_lines (journal_entry_id, account_id, debit_amount, credit_amount)
       SELECT ?, id, ?, ? FROM accounts WHERE account_no = ?`,
    ).run(entry.id, debit, credit, accountNo);
  }
  const report = buildVatReport(db, "2026-05-01", "2026-05-31");
  expect(report.ok).toBe(false);
  expect(report.errors.join(" ")).toContain("has no explicit vat_code on a VAT base line");
  const cockpit = vatPositionForPeriod(db, "2026-05-01", "2026-05-31");
  expect(cockpit.reportOk).toBe(false);
  expect(cockpit.reportErrors).toEqual(report.errors);
  const closed = seedHistoricalClosedPeriod(db, {
    periodStart: "2026-04-01",
    periodEnd: "2026-06-30",
    kind: "vat_period",
  });
  const filing = buildVatFiling(db, "2026-04-01", "2026-06-30");
  expect(filing.ok).toBe(false);
  expect(filing.periodStatus).toBe("closed");
  expect(filing.filingDeadline).toBe("2026-09-01");
  expect(filing.errors.join(" ")).toContain("has no explicit vat_code");
  db.close(); rmSync(root, { recursive: true, force: true });
});

test("a historical amount-only correction cannot hide an unrelated VAT mismatch", () => {
  const { root, db } = freshDb();
  const imported = postDineroPostings(db, [
    {
      transactionDate: "2026-05-21",
      text: "misbooked sale",
      voucherRef: "BAD-BASE",
      lines: [
        { accountNo: "2000", debitAmount: 145 },
        { accountNo: "1000", creditAmount: 100, vatCode: "DK_SALE_25" },
        { accountNo: "1200", creditAmount: 45 },
      ],
    },
    {
      transactionDate: "2026-05-22",
      text: "amount-only correction",
      voucherRef: "CORRECTION",
      lines: [
        { accountNo: "2000", debitAmount: 10 },
        { accountNo: "1200", creditAmount: 10 },
      ],
    },
  ], new Set(["2000", "1000", "1200"]));
  expect(imported.ok).toBe(true);
  const report = buildVatReport(db, "2026-05-01", "2026-05-31");
  expect(report.ok).toBe(false);
  expect(report.errors.join(" ")).toContain("output VAT mismatch");
  db.close(); rmSync(root, { recursive: true, force: true });
});

test("Dinero VAT accounts are recognised while account number 1200 is not inferred as VAT", () => {
  const { root, db } = freshDb();
  db.run("INSERT INTO accounts (account_no,name,type,normal_balance,default_vat_code) VALUES ('64000','Dinero output','liability','credit',NULL),('64040','Dinero reverse','liability','credit',NULL),('64060','Dinero input','liability','debit',NULL)");
  const imported = postDineroPostings(db, [
    {
      transactionDate: "2026-05-03", text: "Dinero sale", voucherRef: "D-1",
      lines: [
        { accountNo: "2000", debitAmount: 125 },
        { accountNo: "1000", creditAmount: 100, vatCode: "DK_SALE_25" },
        { accountNo: "64000", creditAmount: 25 },
      ],
    },
    {
      transactionDate: "2026-05-04", text: "Dinero reverse charge", voucherRef: "D-2",
      lines: [
        { accountNo: "3020", debitAmount: 100, vatCode: "EU_SERVICE_REVERSE_CHARGE" },
        { accountNo: "64060", debitAmount: 25 },
        { accountNo: "2000", creditAmount: 100 },
        { accountNo: "64040", creditAmount: 25 },
      ],
    },
  ], new Set(["2000", "1000", "3020", "64000", "64040", "64060"]));
  expect(imported.ok).toBe(true);
  db.run("UPDATE accounts SET type = 'income', name = 'Imported income 1200' WHERE account_no = '1200'");
  expect(postDineroPostings(db, [{ transactionDate: "2026-05-05", text: "not VAT", voucherRef: "D-3", lines: [{ accountNo: "2000", debitAmount: 99 }, { accountNo: "1200", creditAmount: 99 }] }], new Set(["2000", "1200"])).ok).toBe(true);
  const report = buildVatReport(db, "2026-05-01", "2026-05-31");
  expect(report.outputVat).toBe(50);
  expect(report.inputVat).toBe(25);
  db.close(); rmSync(root, { recursive: true, force: true });
});

test("exact Dinero 64040/64060 reverse-charge controls override one matching account default", () => {
  const { root, db } = freshDb();
  db.run("INSERT INTO accounts (account_no,name,type,normal_balance,default_vat_code) VALUES ('3090','Imported service','expense','debit','DK_PURCHASE_25'),('64040','Dinero reverse','liability','credit',NULL),('64060','Dinero input','liability','debit',NULL)");
  const imported = postDineroPostings(db, [{
    transactionDate: "2026-05-06",
    text: "Dinero reverse charge without Momstype",
    voucherRef: "D-RC-BLANK",
    lines: [
      { accountNo: "3090", debitAmount: 100 },
      { accountNo: "64060", debitAmount: 25 },
      { accountNo: "2000", creditAmount: 100 },
      { accountNo: "64040", creditAmount: 25 },
    ],
  }], new Set(["3090", "64060", "2000", "64040"]));
  expect(imported.ok).toBe(true);
  expect(db.query(
    `SELECT jl.vat_code
       FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
      WHERE a.account_no = '3090'`,
  ).get()).toEqual({ vat_code: "EU_SERVICE_REVERSE_CHARGE" });
  const report = buildVatReport(db, "2026-05-01", "2026-05-31");
  expect(report.ok).toBe(true);
  expect(report.reverseChargePurchaseBase).toBe(100);
  expect(report.rubrikker.rubrikAYdelser).toBe(100);
  expect(report.rubrikker.momsAfYdelseskobUdland).toBe(25);
  expect(report.rubrikker.kobsmoms).toBe(25);
  db.close(); rmSync(root, { recursive: true, force: true });
});

test("normalized Dinero VAT controls classify a blank-Momstype reverse-charge voucher end to end", () => {
  const { root, db } = freshDb();
  db.run("INSERT INTO accounts (account_no,name,type,normal_balance,default_vat_code) VALUES ('3090','Imported service','expense','debit','DK_PURCHASE_25'),('64040','Dinero reverse','vat','credit',NULL),('64060','Dinero input','vat','debit',NULL)");
  const imported = postDineroPostings(db, [{
    transactionDate: "2026-05-06",
    text: "Normalized Dinero reverse charge without Momstype",
    voucherRef: "D-RC-NORMALIZED",
    lines: [
      { accountNo: "3090", debitAmount: 100.03 },
      { accountNo: "64060", debitAmount: 25.01 },
      { accountNo: "2000", creditAmount: 100.03 },
      { accountNo: "64040", creditAmount: 25.01 },
    ],
  }], new Set(["3090", "64060", "2000", "64040"]));
  expect(imported.ok).toBe(true);
  expect(db.query(
    `SELECT jl.vat_code
       FROM journal_lines jl JOIN accounts a ON a.id = jl.account_id
      WHERE a.account_no = '3090'`,
  ).get()).toEqual({ vat_code: "EU_SERVICE_REVERSE_CHARGE" });
  const report = buildVatReport(db, "2026-05-01", "2026-05-31");
  expect(report.ok).toBe(true);
  expect(report.reverseChargePurchaseBase).toBe(100.03);
  expect(report.rubrikker).toMatchObject({
    rubrikAYdelser: 100,
    momsAfYdelseskobUdland: 25,
    kobsmoms: 25,
  });
  db.close(); rmSync(root, { recursive: true, force: true });
});

test("normalized Dinero VAT controls preserve an explicit non-EU service classification end to end", () => {
  const { root, db } = freshDb();
  db.run("INSERT INTO accounts (account_no,name,type,normal_balance,default_vat_code) VALUES ('3090','Imported service','expense','debit','DK_PURCHASE_25'),('64040','Dinero reverse','vat','credit',NULL),('64060','Dinero input','vat','debit',NULL)");
  const imported = postDineroPostings(db, [{
    transactionDate: "2026-05-06",
    text: "Normalized Dinero non-EU reverse charge",
    voucherRef: "D-RC-NON-EU",
    lines: [
      { accountNo: "3090", debitAmount: 100.03, vatCode: "NON_EU_SERVICE_REVERSE_CHARGE" },
      { accountNo: "64060", debitAmount: 25.01, vatCode: "NON_EU_SERVICE_REVERSE_CHARGE" },
      { accountNo: "2000", creditAmount: 100.03 },
      { accountNo: "64040", creditAmount: 25.01, vatCode: "NON_EU_SERVICE_REVERSE_CHARGE" },
    ],
  }], new Set(["3090", "64060", "2000", "64040"]));
  expect(imported.ok).toBe(true);
  const report = buildVatReport(db, "2026-05-01", "2026-05-31");
  expect(report.ok).toBe(true);
  expect(report.reverseChargePurchaseBase).toBe(0);
  expect(report.nonEuServiceReverseChargePurchaseBase).toBe(100.03);
  expect(report.rubrikker).toMatchObject({
    rubrikAYdelser: 0,
    momsAfYdelseskobUdland: 25,
    kobsmoms: 25,
  });
  db.close(); rmSync(root, { recursive: true, force: true });
});

test("report recovers the net base from Dinero's collapsed representation shape", () => {
  const { root, db } = freshDb();
  db.run("INSERT INTO accounts (account_no,name,type,normal_balance,default_vat_code) VALUES ('4140','Imported representation','expense','debit','REPRESENTATION_SPECIAL'),('64060','Dinero input','vat','debit',NULL)");
  const imported = postDineroPostings(db, [{
    transactionDate: "2026-05-22",
    text: "Dinero collapsed representation purchase",
    voucherRef: "D-REP-COLLAPSED",
    lines: [
      { accountNo: "4140", debitAmount: 286.90, vatCode: "REPRESENTATION_SPECIAL" },
      { accountNo: "64060", debitAmount: 15.10, vatCode: "REPRESENTATION_SPECIAL" },
      { accountNo: "2000", creditAmount: 302 },
    ],
  }], new Set(["4140", "64060", "2000"]));
  expect(imported.ok).toBe(true);
  const report = buildVatReport(db, "2026-05-01", "2026-05-31");
  expect(report.ok).toBe(true);
  expect(report.inputVat).toBe(15.10);
  expect(report.representationPurchaseBase).toBe(241.60);
  expect(report.rubrikker.kobsmoms).toBe(15);
  db.close(); rmSync(root, { recursive: true, force: true });
});

test("Dinero control pairs reject explicit conflicts and two-øre drift at import and report time", () => {
  for (const shape of [
    { label: "normalized", outputType: "vat", inputType: "vat", inputBalance: "debit" },
    { label: "legacy", outputType: "liability", inputType: "liability", inputBalance: "credit" },
  ] as const) {
    for (const scenario of [
      { label: "explicit conflict", baseVatCode: "DK_PURCHASE_25", vatAmount: 25.01, expected: "conflicting VAT code" },
      { label: "two-øre drift", baseVatCode: null, vatAmount: 24.99, expected: "no single unclassified expense base matching 25%" },
    ] as const) {
      const { root: importRoot, db: importDb } = freshDb();
      importDb.run(
        `INSERT INTO accounts (account_no,name,type,normal_balance,default_vat_code)
         VALUES ('3090','Imported service','expense','debit','DK_PURCHASE_25'),
                ('64040','Dinero reverse',?,'credit',NULL),
                ('64060','Dinero input',?,?,NULL)`,
        [shape.outputType, shape.inputType, shape.inputBalance],
      );
      const imported = postDineroPostings(importDb, [{
        transactionDate: "2026-05-07",
        text: `${shape.label} ${scenario.label} import`,
        voucherRef: `D-RC-${shape.label}-${scenario.label}`,
        lines: [
          {
            accountNo: "3090",
            debitAmount: 100.03,
            ...(scenario.baseVatCode ? { vatCode: scenario.baseVatCode } : {}),
          },
          { accountNo: "64060", debitAmount: scenario.vatAmount },
          { accountNo: "2000", creditAmount: 100.03 },
          { accountNo: "64040", creditAmount: scenario.vatAmount },
        ],
      }], new Set(["3090", "64060", "2000", "64040"]));
      expect(imported.ok, `${shape.label} ${scenario.label} import`).toBe(false);
      expect(imported.errors.join(" ")).toContain(scenario.expected);
      expect(importDb.query("SELECT COUNT(*) AS n FROM journal_entries").get()).toEqual({ n: 0 });
      importDb.close(); rmSync(importRoot, { recursive: true, force: true });

      const { root: reportRoot, db: reportDb } = freshDb();
      reportDb.run(
        `INSERT INTO accounts (account_no,name,type,normal_balance,default_vat_code)
         VALUES ('3090','Imported service','expense','debit','DK_PURCHASE_25'),
                ('64040','Dinero reverse',?,'credit',NULL),
                ('64060','Dinero input',?,?,NULL)`,
        [shape.outputType, shape.inputType, shape.inputBalance],
      );
      const entry = reportDb.query(
        `INSERT INTO journal_entries
           (entry_no, transaction_date, text, rule_version, created_by_program, entry_hash)
         VALUES (?, '2026-05-07', ?, 'legacy', ?, ?)
         RETURNING id`,
      ).get(
        `LEGACY-${shape.label}-${scenario.label}`,
        `${shape.label} ${scenario.label} report`,
        HISTORICAL_IMPORT_PROGRAM,
        `legacy-${shape.label}-${scenario.label}-hash`,
      ) as { id: number };
      for (const [accountNo, debit, credit, vatCode] of [
        ["3090", 100.03, 0, scenario.baseVatCode],
        ["64060", scenario.vatAmount, 0, null],
        ["2000", 0, 100.03, null],
        ["64040", 0, scenario.vatAmount, null],
      ] as const) {
        reportDb.query(
          `INSERT INTO journal_lines
             (journal_entry_id, account_id, debit_amount, credit_amount, vat_code)
           SELECT ?, id, ?, ?, ? FROM accounts WHERE account_no = ?`,
        ).run(entry.id, debit, credit, vatCode, accountNo);
      }
      const report = buildVatReport(reportDb, "2026-05-01", "2026-05-31");
      expect(report.ok, `${shape.label} ${scenario.label} report`).toBe(false);
      expect(report.errors.join(" ")).toContain(scenario.expected);
      reportDb.close(); rmSync(reportRoot, { recursive: true, force: true });
    }
  }
});

test("report infers an exact reverse-charge base in an already-imported Dinero ledger", () => {
  const { root, db } = freshDb();
  db.run("INSERT INTO accounts (account_no,name,type,normal_balance,default_vat_code) VALUES ('3090','Imported service','expense','debit',NULL),('64040','Dinero reverse','liability','credit',NULL),('64060','Dinero input','liability','debit',NULL)");
  const entry = db.query(
    `INSERT INTO journal_entries
       (entry_no, transaction_date, text, rule_version, created_by_program, entry_hash)
     VALUES ('LEGACY-RC-BLANK', '2026-05-06', 'legacy reverse charge', 'legacy', ?, 'legacy-rc-blank-hash')
     RETURNING id`,
  ).get(HISTORICAL_IMPORT_PROGRAM) as { id: number };
  for (const [accountNo, debit, credit] of [
    ["3090", 100, 0],
    ["64060", 25, 0],
    ["2000", 0, 100],
    ["64040", 0, 25],
  ] as const) {
    db.query(
      `INSERT INTO journal_lines (journal_entry_id, account_id, debit_amount, credit_amount)
       SELECT ?, id, ?, ? FROM accounts WHERE account_no = ?`,
    ).run(entry.id, debit, credit, accountNo);
  }

  const report = buildVatReport(db, "2026-05-01", "2026-05-31");
  expect(report.ok).toBe(true);
  expect(report.reverseChargePurchaseBase).toBe(100);
  expect(report.rubrikker.rubrikAYdelser).toBe(100);
  expect(report.rubrikker.momsAfYdelseskobUdland).toBe(25);
  expect(report.rubrikker.salgsmoms).toBe(0);
  db.close(); rmSync(root, { recursive: true, force: true });
});

test("sanitized pre-normalization Dinero 64060 liability credit is trusted only for the exact historical control pair", () => {
  const { root, db } = freshDb();
  expect(setCompanyVatPeriodType(db, "month").ok).toBe(true);
  const fixture = JSON.parse(readFileSync(
    join(import.meta.dir, "../fixtures/vat-legacy/dinero-64060-liability-credit.json"),
    "utf8",
  )) as {
    entryNo: string;
    transactionDate: string;
    text: string;
    lines: Array<{ accountNo: string; debitAmount: number; creditAmount: number }>;
  };
  db.run("INSERT INTO accounts (account_no,name,type,normal_balance,default_vat_code) VALUES ('3090','Imported service','expense','debit','DK_PURCHASE_25'),('64040','Dinero reverse','liability','credit',NULL),('64060','Dinero input','liability','credit',NULL)");
  const entry = db.query(
    `INSERT INTO journal_entries
       (entry_no, transaction_date, text, rule_version, created_by_program, entry_hash)
     VALUES (?, ?, ?, 'legacy', ?, 'legacy-64060-liability-credit')
     RETURNING id`,
  ).get(fixture.entryNo, fixture.transactionDate, fixture.text, HISTORICAL_IMPORT_PROGRAM) as { id: number };
  for (const line of fixture.lines) {
    db.query(
      `INSERT INTO journal_lines (journal_entry_id, account_id, debit_amount, credit_amount)
       SELECT ?, id, ?, ? FROM accounts WHERE account_no = ?`,
    ).run(entry.id, line.debitAmount, line.creditAmount, line.accountNo);
  }

  const report = buildVatReport(db, "2026-05-01", "2026-05-31");
  expect(report.ok).toBe(true);
  expect(report.inputVat).toBe(25);
  expect(report.reverseChargePurchaseBase).toBe(100.03);
  expect(report.rubrikker).toMatchObject({
    rubrikAYdelser: 100,
    momsAfYdelseskobUdland: 25,
    kobsmoms: 25,
  });
  seedHistoricalClosedPeriod(db, {
    periodStart: "2026-05-01", periodEnd: "2026-05-31", kind: "vat_period",
  });
  const filing = buildVatFiling(db, "2026-05-01", "2026-05-31");
  expect(filing.ok).toBe(true);
  expect(filing.rubrikker).toEqual(report.rubrikker);

  // The same chart shape without the immutable historical-import marker does
  // not turn liability/credit 64060 into deductible input VAT.
  const { root: untrustedRoot, db: untrustedDb } = freshDb();
  untrustedDb.run("INSERT INTO accounts (account_no,name,type,normal_balance,default_vat_code) VALUES ('3090','Imported service','expense','debit','DK_PURCHASE_25'),('64040','Dinero reverse','liability','credit',NULL),('64060','Dinero input','liability','credit',NULL)");
  const untrustedEntry = untrustedDb.query(
    `INSERT INTO journal_entries
       (entry_no, transaction_date, text, rule_version, created_by_program, entry_hash)
     VALUES (?, ?, ?, 'legacy', 'rentemester', 'untrusted-64060-liability-credit')
     RETURNING id`,
  ).get(fixture.entryNo, fixture.transactionDate, fixture.text) as { id: number };
  for (const line of fixture.lines) {
    untrustedDb.query(
      `INSERT INTO journal_lines (journal_entry_id, account_id, debit_amount, credit_amount)
       SELECT ?, id, ?, ? FROM accounts WHERE account_no = ?`,
    ).run(untrustedEntry.id, line.debitAmount, line.creditAmount, line.accountNo);
  }
  const untrusted = buildVatReport(untrustedDb, "2026-05-01", "2026-05-31");
  expect(untrusted.ok).toBe(false);
  expect(untrusted.inputVat).toBe(0);
  const manual = postJournalEntry(db, {
    transactionDate: "2026-06-01", text: "new manual Dinero-shaped entry",
    lines: [
      { accountNo: "3090", debitAmount: 100 },
      { accountNo: "64060", debitAmount: 25 },
      { accountNo: "2000", creditAmount: 100 },
      { accountNo: "64040", creditAmount: 25 },
    ],
  });
  expect(manual.ok).toBe(false);
  expect(manual.errors.join(" ")).toContain("require an explicit vatCode on the VAT base line");
  untrustedDb.close(); rmSync(untrustedRoot, { recursive: true, force: true });
  db.close(); rmSync(root, { recursive: true, force: true });
});

test("legacy VAT roles are recovered from voucher evidence without depending on account numbers", () => {
  const { root, db } = freshDb();
  expect(setCompanyVatPeriodType(db, "month").ok).toBe(true);
  const fixture = JSON.parse(readFileSync(
    join(import.meta.dir, "../fixtures/vat-legacy/custom-vat-controls.json"),
    "utf8",
  )) as {
    accounts: Array<{
      accountNo: string;
      name: string;
      type: string;
      normalBalance: string;
      defaultVatCode: string | null;
    }>;
    entries: Array<{
      entryNo: string;
      transactionDate: string;
      program: string;
      lines: Array<{
        accountNo: string;
        debitAmount: number;
        creditAmount: number;
        vatCode: string | null;
      }>;
    }>;
  };

  for (const account of fixture.accounts) {
    db.query(
      `INSERT INTO accounts
         (account_no, name, type, normal_balance, default_vat_code)
       VALUES (?, ?, ?, ?, ?)`,
    ).run(
      account.accountNo,
      account.name,
      account.type,
      account.normalBalance,
      account.defaultVatCode,
    );
  }
  for (const fixtureEntry of fixture.entries) {
    const entry = db.query(
      `INSERT INTO journal_entries
         (entry_no, transaction_date, text, rule_version, created_by_program, entry_hash)
       VALUES (?, ?, ?, 'legacy-fixture', ?, ?)
       RETURNING id`,
    ).get(
      fixtureEntry.entryNo,
      fixtureEntry.transactionDate,
      fixtureEntry.entryNo,
      fixtureEntry.program,
      `hash-${fixtureEntry.entryNo}`,
    ) as { id: number };
    for (const line of fixtureEntry.lines) {
      db.query(
        `INSERT INTO journal_lines
           (journal_entry_id, account_id, debit_amount, credit_amount, vat_code)
         SELECT ?, id, ?, ?, ? FROM accounts WHERE account_no = ?`,
      ).run(
        entry.id,
        line.debitAmount,
        line.creditAmount,
        line.vatCode,
        line.accountNo,
      );
    }
  }

  const report = buildVatReport(db, "2026-05-01", "2026-05-31");
  expect(report.ok, report.errors.join("\n")).toBe(true);
  expect(report).toMatchObject({
    outputVat: 4519.75,
    inputVat: 1123.55,
    salesBase25: 17829.02,
    purchaseBase25: 4244.2,
    reverseChargePurchaseBase: 250,
    netVatPayable: 3396.2,
  });
  expect(report.rubrikker).toMatchObject({
    salgsmoms: 4457,
    momsAfYdelseskobUdland: 62,
    kobsmoms: 1123,
    momsIAlt: 3396,
    rubrikAYdelser: 250,
  });

  seedHistoricalClosedPeriod(db, {
    periodStart: "2026-05-01",
    periodEnd: "2026-05-31",
    kind: "vat_period",
  });
  const filing = buildVatFiling(db, "2026-05-01", "2026-05-31");
  expect(filing.ok, filing.errors.join("\n")).toBe(true);
  expect(filing.rubrikker).toEqual(report.rubrikker);
  expect(vatRubrikkerForPeriod(db, "2026-05-01", "2026-05-31")).toEqual(
    report.rubrikker,
  );

  const exported = exportAuthorityPackage(db, root, {
    periodStart: "2026-05-01",
    periodEnd: "2026-05-31",
    outputDir: join(root, "authority-export"),
  });
  expect(exported.ok, exported.errors.join("\n")).toBe(true);
  const exportedReport = JSON.parse(readFileSync(
    join(exported.exportDir!, "machine-readable", "vat-report.json"),
    "utf8",
  ));
  expect(exportedReport).toEqual(report);

  db.close(); rmSync(root, { recursive: true, force: true });
});

test("a trusted historical voucher can establish a custom VAT role for later periods, but an untrusted one cannot", () => {
  const prepareLedger = (
    db: ReturnType<typeof openDb>,
    historicalProgram: string,
  ) => {
    db.run(
      `INSERT INTO accounts
         (account_no, name, type, normal_balance, default_vat_code)
       VALUES
         ('78199', 'Custom purchase base', 'expense', 'debit', 'DK_PURCHASE_25'),
         ('91999', 'Custom input control', 'liability', 'credit', NULL)`,
    );
    for (const entry of [
      {
        no: "ROLE-EVIDENCE",
        date: "2026-04-30",
        program: historicalProgram,
        base: 100,
        vat: 25,
      },
      {
        no: "LATER-PURCHASE",
        date: "2026-05-01",
        program: "bookkeeping-2026",
        base: 200,
        vat: 50,
      },
    ]) {
      const journal = db.query(
        `INSERT INTO journal_entries
           (entry_no, transaction_date, text, rule_version, created_by_program, entry_hash)
         VALUES (?, ?, ?, 'legacy-fixture', ?, ?)
         RETURNING id`,
      ).get(entry.no, entry.date, entry.no, entry.program, `hash-${entry.no}`) as { id: number };
      for (const [accountNo, debit, credit, vatCode] of [
        ["78199", entry.base, 0, entry.no === "LATER-PURCHASE" ? "DK_PURCHASE_25" : null],
        ["91999", entry.vat, 0, null],
        ["2000", 0, entry.base + entry.vat, null],
      ] as const) {
        db.query(
          `INSERT INTO journal_lines
             (journal_entry_id, account_id, debit_amount, credit_amount, vat_code)
           SELECT ?, id, ?, ?, ? FROM accounts WHERE account_no = ?`,
        ).run(journal.id, debit, credit, vatCode, accountNo);
      }
    }
  };

  const trusted = freshDb();
  prepareLedger(trusted.db, HISTORICAL_IMPORT_PROGRAM);
  const trustedReport = buildVatReport(
    trusted.db,
    "2026-05-01",
    "2026-05-31",
  );
  expect(trustedReport.ok, trustedReport.errors.join("\n")).toBe(true);
  expect(trustedReport.purchaseBase25).toBe(200);
  expect(trustedReport.inputVat).toBe(50);
  trusted.db.close(); rmSync(trusted.root, { recursive: true, force: true });

  const untrusted = freshDb();
  prepareLedger(untrusted.db, "rentemester");
  const untrustedReport = buildVatReport(
    untrusted.db,
    "2026-05-01",
    "2026-05-31",
  );
  expect(untrustedReport.ok).toBe(false);
  expect(untrustedReport.purchaseBase25).toBe(200);
  expect(untrustedReport.inputVat).toBe(0);
  expect(untrustedReport.errors.join(" ")).toContain("input VAT mismatch");
  untrusted.db.close(); rmSync(untrusted.root, { recursive: true, force: true });
});

test("report blocks ambiguous legacy Dinero reverse-charge bases", () => {
  const { root, db } = freshDb();
  db.run("INSERT INTO accounts (account_no,name,type,normal_balance,default_vat_code) VALUES ('3090','Imported service A','expense','debit',NULL),('3091','Imported service B','expense','debit',NULL),('64040','Dinero reverse','liability','credit',NULL),('64060','Dinero input','liability','debit',NULL)");
  const entry = db.query(
    `INSERT INTO journal_entries
       (entry_no, transaction_date, text, rule_version, created_by_program, entry_hash)
     VALUES ('LEGACY-RC-AMBIGUOUS', '2026-05-07', 'ambiguous reverse charge', 'legacy', ?, 'legacy-rc-ambiguous-hash')
     RETURNING id`,
  ).get(HISTORICAL_IMPORT_PROGRAM) as { id: number };
  for (const [accountNo, debit, credit] of [
    ["3090", 100, 0],
    ["3091", 100, 0],
    ["64060", 25, 0],
    ["2000", 0, 200],
    ["64040", 0, 25],
  ] as const) {
    db.query(
      `INSERT INTO journal_lines (journal_entry_id, account_id, debit_amount, credit_amount)
       SELECT ?, id, ?, ? FROM accounts WHERE account_no = ?`,
    ).run(entry.id, debit, credit, accountNo);
  }

  const report = buildVatReport(db, "2026-05-01", "2026-05-31");
  expect(report.ok).toBe(false);
  expect(report.errors.join(" ")).toContain(
    "no single unclassified expense base matching 25%",
  );
  db.close(); rmSync(root, { recursive: true, force: true });
});

test("Dinero reverse-charge controls fail closed when no single 25% expense base can be identified", () => {
  const { root, db } = freshDb();
  db.run("INSERT INTO accounts (account_no,name,type,normal_balance,default_vat_code) VALUES ('3090','Imported service A','expense','debit',NULL),('3091','Imported service B','expense','debit',NULL),('64040','Dinero reverse','liability','credit',NULL),('64060','Dinero input','liability','debit',NULL)");
  const imported = postDineroPostings(db, [{
    transactionDate: "2026-05-07",
    text: "Ambiguous reverse charge",
    voucherRef: "D-RC-AMBIGUOUS",
    lines: [
      { accountNo: "3090", debitAmount: 50 },
      { accountNo: "3091", debitAmount: 50 },
      { accountNo: "64060", debitAmount: 25 },
      { accountNo: "2000", creditAmount: 100 },
      { accountNo: "64040", creditAmount: 25 },
    ],
  }], new Set(["3090", "3091", "64060", "2000", "64040"]));
  expect(imported.ok).toBe(false);
  expect(imported.errors.join(" ")).toContain("no single unclassified expense base matching 25%");
  expect(db.query("SELECT COUNT(*) AS n FROM journal_entries").get()).toEqual({ n: 0 });
  db.close(); rmSync(root, { recursive: true, force: true });
});

test("a mixed Dinero voucher keeps ordinary sales VAT out of the reverse-charge rubric", () => {
  const { root, db } = freshDb();
  db.run("INSERT INTO accounts (account_no,name,type,normal_balance,default_vat_code) VALUES ('3090','Imported EU service','expense','debit',NULL),('64000','Dinero output','liability','credit',NULL),('64040','Dinero reverse','liability','credit',NULL),('64060','Dinero input','liability','debit',NULL)");
  const imported = postDineroPostings(db, [{
    transactionDate: "2026-05-08",
    text: "Mixed sale and reverse charge",
    voucherRef: "D-MIXED-VAT",
    lines: [
      { accountNo: "2000", debitAmount: 125 },
      { accountNo: "1000", creditAmount: 100, vatCode: "DK_SALE_25" },
      { accountNo: "64000", creditAmount: 25 },
      { accountNo: "3090", debitAmount: 100, vatCode: "EU_SERVICE_REVERSE_CHARGE" },
      { accountNo: "64060", debitAmount: 25 },
      { accountNo: "2000", creditAmount: 100 },
      { accountNo: "64040", creditAmount: 25 },
    ],
  }], new Set(["2000", "1000", "64000", "3090", "64060", "64040"]));
  expect(imported.ok).toBe(true);
  const report = buildVatReport(db, "2026-05-01", "2026-05-31");
  expect(report.ok).toBe(true);
  expect(report.outputVat).toBe(50);
  expect(report.inputVat).toBe(25);
  expect(report.rubrikker).toMatchObject({
    salgsmoms: 25,
    momsAfYdelseskobUdland: 25,
    kobsmoms: 25,
    momsIAlt: 25,
    rubrikAYdelser: 100,
  });
  db.close(); rmSync(root, { recursive: true, force: true });
});

test("historical Dinero output VAT cannot cross-subsidise SKAT categories", () => {
  const { root, db } = freshDb();
  db.run("INSERT INTO accounts (account_no,name,type,normal_balance,default_vat_code) VALUES ('64000','Dinero output','liability','credit',NULL),('64040','Dinero reverse','liability','credit',NULL),('64060','Dinero input','liability','debit',NULL)");
  const entry = db.query(
    `INSERT INTO journal_entries
       (entry_no, transaction_date, text, rule_version, created_by_program, entry_hash)
     VALUES ('LEGACY-CROSS-SUBSIDY', '2026-05-09', 'legacy mixed VAT', 'legacy', ?, 'legacy-cross-subsidy-hash')
     RETURNING id`,
  ).get(HISTORICAL_IMPORT_PROGRAM) as { id: number };
  for (const [accountNo, debit, credit, vatCode] of [
    ["2000", 50, 0, null],
    ["1000", 0, 120, "DK_SALE_25"],
    ["64000", 0, 35, null],
    ["3000", 100, 0, "EU_SERVICE_REVERSE_CHARGE"],
    ["64060", 25, 0, null],
    ["64040", 0, 20, null],
  ] as const) {
    db.query(
      `INSERT INTO journal_lines
         (journal_entry_id, account_id, debit_amount, credit_amount, vat_code)
       SELECT ?, id, ?, ?, ? FROM accounts WHERE account_no = ?`,
    ).run(entry.id, debit, credit, vatCode, accountNo);
  }

  const report = buildVatReport(db, "2026-05-01", "2026-05-31");
  expect(report.outputVat).toBe(55);
  expect(report.ok).toBe(false);
  expect(report.errors.join(" ")).toContain("reverse-charge output VAT mismatch");
  expect(report.errors.join(" ")).toContain("ordinary output VAT mismatch");
  expect(report.rubrikker.salgsmoms).toBe(35);
  expect(report.rubrikker.momsAfYdelseskobUdland).toBe(20);
  db.close(); rmSync(root, { recursive: true, force: true });
});

test("cadence-neutral VAT periods file with the same shared rubric as cockpit", () => {
  const { root, db } = freshDb();
  expect(setCompanyVatPeriodType(db, "month").ok).toBe(true);
  expect(postDineroPostings(db, [{ transactionDate: "2026-06-01", text: "sale", voucherRef: "JUNE-1", lines: [{ accountNo: "2000", debitAmount: 125 }, { accountNo: "1000", creditAmount: 100, vatCode: "DK_SALE_25" }, { accountNo: "1200", creditAmount: 25 }] }], new Set(["2000", "1000", "1200"])).ok).toBe(true);
  expect(closeAccountingPeriod(db, { periodStart: "2026-06-01", periodEnd: "2026-06-30", kind: "vat_period", force: true }).ok).toBe(true);
  const filing = buildVatFiling(db, "2026-06-01", "2026-06-30");
  expect(filing.ok).toBe(true);
  expect(filing.filingDeadline).toBe("2026-08-17");
  expect(buildVatReport(db, "2026-06-01", "2026-06-30").rubrikker).toEqual(filing.rubrikker);
  expect(filing.rubrikker).toEqual(vatRubrikkerForPeriod(db, "2026-06-01", "2026-06-30"));
  db.close(); rmSync(root, { recursive: true, force: true });
});

test("VAT close rejects noncanonical bounds for the registered cadence", () => {
  const { root, db } = freshDb();
  expect(setCompanyVatPeriodType(db, "month").ok).toBe(true);
  const close = closeAccountingPeriod(db, {
    periodStart: "2026-04-01",
    periodEnd: "2026-05-15",
    kind: "vat_period",
    force: true,
  });
  expect(close.ok).toBe(false);
  expect(close.errors.join(" ")).toContain("registered month cadence");
  expect(buildVatFiling(db, "2026-04-01", "2026-05-15").filingDeadline).toBeNull();
  db.close(); rmSync(root, { recursive: true, force: true });
});

test("VAT cadence changes fail closed once VAT periods exist", () => {
  const { root, db } = freshDb();
  const closed = closeAccountingPeriod(db, {
    periodStart: "2026-01-01",
    periodEnd: "2026-03-31",
    kind: "vat_period",
    force: true,
  });
  expect(closed.ok).toBe(true);
  const changed = setCompanyVatPeriodType(db, "half-year");
  expect(changed.ok).toBe(false);
  expect(changed.changed).toBe(false);
  expect(changed.errors.join(" ")).toContain("effective-dated kadencemigration");
  expect(db.query("SELECT vat_period_type FROM companies WHERE id = 1").get()).toEqual({
    vat_period_type: "quarter",
  });
  expect(buildVatFiling(db, "2026-01-01", "2026-03-31").ok).toBe(true);
  db.close(); rmSync(root, { recursive: true, force: true });
});

test("a closed VAT period can be marked reported once and remains terminal", () => {
  const { root, db } = freshDb();
  const closed = closeAccountingPeriod(db, {
    periodStart: "2026-01-01",
    periodEnd: "2026-03-31",
    kind: "vat_period",
    status: "closed",
  });
  expect(closed.ok).toBe(true);
  const reported = closeAccountingPeriodCore(db, {
    periodStart: "2026-01-01",
    periodEnd: "2026-03-31",
    kind: "vat_period",
    status: "reported",
    reference: "SKAT-RECEIPT-42",
    readinessPacketHash: closed.readinessPacket!.hash,
    createdBy: "user:test",
  });
  expect(reported.ok).toBe(true);
  expect(reported.reference).toBe("SKAT-RECEIPT-42");
  expect(db.query("SELECT status, reference FROM accounting_periods WHERE id = ?").get(closed.periodId!)).toEqual({
    status: "reported",
    reference: "SKAT-RECEIPT-42",
  });
  expect(effectivePeriodState(db, closed.periodId!, "closed")).toBe("reported");
  expect(buildVatFiling(db, "2026-01-01", "2026-03-31")).toMatchObject({
    ok: true,
    periodStatus: "reported",
    periodReference: "SKAT-RECEIPT-42",
  });
  expect(closeAccountingPeriodCore(db, {
    periodStart: "2026-01-01",
    periodEnd: "2026-03-31",
    kind: "vat_period",
    status: "reported",
    reference: "SKAT-RECEIPT-42",
    readinessPacketHash: closed.readinessPacket!.hash,
  }).ok).toBe(true);
  expect(reopenAccountingPeriod(db, {
    periodStart: "2026-01-01",
    periodEnd: "2026-03-31",
    kind: "vat_period",
    reason: "must remain terminal",
  }).ok).toBe(false);
  db.close(); rmSync(root, { recursive: true, force: true });
});

test("legacy VAT-period migration preserves ids and fails closed on filed overlaps", () => {
  const canonicalRoot = mkdtempSync(join(tmpdir(), "rentemester-vat-legacy-in-new-schema-"));
  const canonicalDb = openDb(ensureCompanyDirs(canonicalRoot).db);
  migrate(canonicalDb);
  canonicalDb.run("INSERT INTO accounting_periods (id,period_start,period_end,kind,status) VALUES (17,'2025-07-01','2025-09-30','vat_quarter','closed')");
  migrate(canonicalDb);
  migrate(canonicalDb);
  expect(canonicalDb.query("SELECT id, kind, status FROM accounting_periods").get()).toEqual({ id: 17, kind: "vat_period", status: "closed" });
  canonicalDb.close(); rmSync(canonicalRoot, { recursive: true, force: true });

  const root = mkdtempSync(join(tmpdir(), "rentemester-vat-legacy-"));
  const db = openDb(ensureCompanyDirs(root).db);
  const legacySchema = readFileSync(join(import.meta.dir, "../../src/core/schema.sql"), "utf8")
    .replace("('vat_period','vat_quarter','fiscal_year','custom')", "('vat_quarter','fiscal_year','custom')");
  db.exec(legacySchema);
  db.run("INSERT INTO accounting_periods (id,period_start,period_end,kind,status,reference) VALUES (41,'2026-01-01','2026-03-31','vat_quarter','closed','legacy')");
  migrate(db);
  expect(db.query("SELECT id, kind, status, reference FROM accounting_periods").get()).toEqual({ id: 41, kind: "vat_period", status: "closed", reference: "legacy" });
  db.close(); rmSync(root, { recursive: true, force: true });

  const conflictRoot = mkdtempSync(join(tmpdir(), "rentemester-vat-legacy-conflict-"));
  const conflictDb = openDb(ensureCompanyDirs(conflictRoot).db);
  conflictDb.exec(legacySchema);
  conflictDb.run("INSERT INTO accounting_periods (period_start,period_end,kind,status) VALUES ('2026-01-01','2026-03-31','vat_quarter','reported'),('2026-03-01','2026-04-30','vat_quarter','reported')");
  expect(() => migrate(conflictDb)).toThrow("does not match the registered quarter cadence");
  conflictDb.close(); rmSync(conflictRoot, { recursive: true, force: true });

  const noncanonicalRoot = mkdtempSync(join(tmpdir(), "rentemester-vat-legacy-noncanonical-"));
  const noncanonicalDb = openDb(ensureCompanyDirs(noncanonicalRoot).db);
  noncanonicalDb.exec(legacySchema);
  noncanonicalDb.run("INSERT INTO accounting_periods (id,period_start,period_end,kind,status,reference) VALUES (52,'2026-05-01','2026-05-31','vat_quarter','reported','legacy-month')");
  expect(() => migrate(noncanonicalDb)).toThrow("does not match the registered quarter cadence");
  expect(noncanonicalDb.query("SELECT id, kind, status, reference FROM accounting_periods WHERE id = 52").get()).toEqual({
    id: 52,
    kind: "vat_quarter",
    status: "reported",
    reference: "legacy-month",
  });
  noncanonicalDb.close(); rmSync(noncanonicalRoot, { recursive: true, force: true });

  for (const legacyCadence of [
    { type: "month", start: "2026-05-01", end: "2026-05-31" },
    { type: "half-year", start: "2026-01-01", end: "2026-06-30" },
  ] as const) {
    const cadenceRoot = mkdtempSync(
      join(tmpdir(), `rentemester-vat-legacy-${legacyCadence.type}-`),
    );
    const cadenceDb = openDb(ensureCompanyDirs(cadenceRoot).db);
    migrate(cadenceDb);
    cadenceDb.run(
      "INSERT INTO companies (id, name, vat_period_type) VALUES (1, 'Legacy cadence', ?)",
      legacyCadence.type,
    );
    cadenceDb.run(
      "INSERT INTO accounting_periods (period_start,period_end,kind,status) VALUES (?,?, 'vat_quarter','closed')",
      legacyCadence.start,
      legacyCadence.end,
    );
    migrate(cadenceDb);
    expect(
      cadenceDb.query(
        "SELECT period_start, period_end, kind FROM accounting_periods",
      ).get(),
    ).toEqual({
      period_start: legacyCadence.start,
      period_end: legacyCadence.end,
      kind: "vat_period",
    });
    cadenceDb.close();
    rmSync(cadenceRoot, { recursive: true, force: true });
  }

  for (const partialMigration of [false, true]) {
    const mixedRoot = mkdtempSync(
      join(
        tmpdir(),
        `rentemester-vat-legacy-mixed-${partialMigration ? "partial" : "legacy"}-`,
      ),
    );
    const mixedDb = openDb(ensureCompanyDirs(mixedRoot).db);
    migrate(mixedDb);
    mixedDb.run(
      "INSERT INTO companies (id, name, vat_period_type) VALUES (1, 'Deregistered legacy', NULL)",
    );
    mixedDb.run(
      `INSERT INTO accounting_periods
         (id, period_start, period_end, kind, status)
       VALUES (61, '2026-01-01', '2026-01-31', 'vat_quarter', 'closed'),
              (62, '2026-01-01', '2026-03-31', ?, 'closed')`,
      partialMigration ? "vat_period" : "vat_quarter",
    );
    expect(() => migrate(mixedDb)).toThrow(
      "do not share one canonical historical cadence",
    );
    expect(
      mixedDb.query(
        "SELECT id, kind FROM accounting_periods ORDER BY id ASC",
      ).all(),
    ).toEqual([
      { id: 61, kind: "vat_quarter" },
      { id: 62, kind: partialMigration ? "vat_period" : "vat_quarter" },
    ]);
    mixedDb.close();
    rmSync(mixedRoot, { recursive: true, force: true });
  }
});
