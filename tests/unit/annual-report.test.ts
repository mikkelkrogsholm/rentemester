// Tests: src/core/annual-report.ts, src/core/ixbrl.ts (#177)
// Year-end close + arsrapport (regnskabsklasse B) + deterministic iXBRL.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts, postJournalEntry } from "../../src/core/ledger";
import { ingestDocument } from "../../src/core/documents";
import { closeAccountingPeriod } from "../../src/core/periods";
import { buildAnnualReport } from "../../src/core/annual-report";
import { generateIxbrl, IXBRL_TAXONOMY_SUBSET } from "../../src/core/ixbrl";

function newCompany(prefix: string, cvr: string | null = "DK12345678") {
  const root = mkdtempSync(join(tmpdir(), prefix));
  const inbox = mkdtempSync(join(tmpdir(), `${prefix}inbox-`));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  seedAccounts(db);
  db.query(
    `INSERT INTO companies (id, name, country, currency, cvr, fiscal_year_start_month, fiscal_year_label_strategy)
     VALUES (1, 'Rentemester ApS', 'DK', 'DKK', ?, 1, 'end-year')`,
  ).run(cvr);
  return { root, inbox, db };
}

// Income/expense postings require a source document; ingest one to attach.
function ingestDoc(db: ReturnType<typeof openDb>, root: string, inbox: string): number {
  const sourceFile = join(inbox, "year-doc.txt");
  writeFileSync(sourceFile, "Bilag\n1250 DKK\n");
  const doc = ingestDocument(db, root, sourceFile, {
    source: "email",
    issueDate: "2025-06-15",
    invoiceNo: "AR-DOC-1",
    deliveryDescription: "Ydelse",
    amountIncVat: 1250,
    currency: "DKK",
    sender: { name: "Leverandor", address: "Saelgervej 1", vatOrCvr: "DK11223344" },
    recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
    vatAmount: 250,
    paymentDetails: "Bankoverforsel",
  });
  expect(doc.ok).toBe(true);
  return doc.documentId!;
}

// Posts a small but balanced year of activity: revenue, an expense, bank and
// equity so the resultatopgorelse has a profit and the balance balances.
function postYear(db: ReturnType<typeof openDb>, root: string, inbox: string) {
  const docId = ingestDoc(db, root, inbox);
  // Opening equity contribution (bank 2000 debit, equity 5000 credit).
  const open = postJournalEntry(db, {
    transactionDate: "2025-01-02",
    text: "Indskud egenkapital",
    lines: [
      { accountNo: "2000", debitAmount: 50000 },
      { accountNo: "5000", creditAmount: 50000 },
    ],
  });
  expect(open.ok).toBe(true);
  // Revenue: bank 2000 debit 1250, income 1000 credit 1000, salgsmoms 1200 credit 250.
  const sale = postJournalEntry(db, {
    transactionDate: "2025-06-15",
    text: "Konsulentsalg",
    documentId: docId,
    lines: [
      { accountNo: "2000", debitAmount: 1250 },
      { accountNo: "1000", creditAmount: 1000, vatCode: "DK_SALE_25" },
      { accountNo: "1200", creditAmount: 250 },
    ],
  });
  expect(sale.ok).toBe(true);
  // Expense: software 3000 debit 400, kobsmoms 4000 debit 100, bank 2000 credit 500.
  const expense = postJournalEntry(db, {
    transactionDate: "2025-09-10",
    text: "Softwarekob",
    documentId: docId,
    lines: [
      { accountNo: "3000", debitAmount: 400, vatCode: "DK_PURCHASE_25" },
      { accountNo: "4000", debitAmount: 100 },
      { accountNo: "2000", creditAmount: 500 },
    ],
  });
  expect(expense.ok).toBe(true);
}

function lockYear(db: ReturnType<typeof openDb>) {
  const closed = closeAccountingPeriod(db, {
    periodStart: "2025-01-01",
    periodEnd: "2025-12-31",
    kind: "fiscal_year",
    status: "closed",
    createdBy: "agent:test",
  });
  expect(closed.ok).toBe(true);
}

function cleanup(...dirs: string[]) {
  for (const dir of dirs) rmSync(dir, { recursive: true, force: true });
}

describe("buildAnnualReport (arsrapport, regnskabsklasse B)", () => {
  test("assembles resultatopgorelse, balance, notes skeleton and ledelsespategning for a locked year", () => {
    const { root, inbox, db } = newCompany("rentemester-annual-ok-");
    postYear(db, root, inbox);
    lockYear(db);

    const report = buildAnnualReport(db, "2025-01-01", "2025-12-31");
    expect(report.ok).toBe(true);
    expect(report.errors).toEqual([]);
    expect(report.regnskabsklasse).toBe("B");
    expect(report.fiscalYearStart).toBe("2025-01-01");
    expect(report.fiscalYearEnd).toBe("2025-12-31");

    // Company master data surfaces on the report.
    expect(report.company.cvr).toBe("DK12345678");
    expect(report.company.name).toBe("Rentemester ApS");

    // Resultatopgorelse: revenue 1000, expense 400, profit 600.
    expect(report.profitAndLoss.totalIncome).toBe(1000);
    expect(report.profitAndLoss.totalExpense).toBe(400);
    expect(report.profitAndLoss.result).toBe(600);
    expect(report.aretsResultat).toBe(600);

    // Balance balances and is taken as of the fiscal-year end.
    expect(report.balanceSheet.asOfDate).toBe("2025-12-31");
    expect(report.balanceSheet.balanced).toBe(true);

    // Notes skeleton + ledelsespategning placeholder are present.
    expect(report.notes.length).toBeGreaterThan(0);
    expect(report.ledelsespategning.placeholder).toBe(true);
    expect(report.ledelsespategning.text.toLowerCase()).toContain("ledelse");

    // Conservative claim language: Rentemester prepares, owner files.
    expect(report.preparedBy).toBe("Rentemester");
    expect(report.disclaimer.toLowerCase()).toContain("forbereder");

    db.close();
    cleanup(root, inbox);
  });

  test("is deterministic: identical input yields a byte-identical report", () => {
    const a = newCompany("rentemester-annual-det-a-");
    const b = newCompany("rentemester-annual-det-b-");
    for (const c of [a, b]) {
      postYear(c.db, c.root, c.inbox);
      lockYear(c.db);
    }
    const reportA = buildAnnualReport(a.db, "2025-01-01", "2025-12-31");
    const reportB = buildAnnualReport(b.db, "2025-01-01", "2025-12-31");
    expect(JSON.stringify(reportA)).toBe(JSON.stringify(reportB));

    a.db.close();
    b.db.close();
    cleanup(a.root, a.inbox, b.root, b.inbox);
  });

  test("fails clearly when the fiscal year is not locked (period still open)", () => {
    const { root, inbox, db } = newCompany("rentemester-annual-unlocked-");
    postYear(db, root, inbox);
    // No closeAccountingPeriod call -> year is open.

    const report = buildAnnualReport(db, "2025-01-01", "2025-12-31");
    expect(report.ok).toBe(false);
    expect(report.errors.length).toBeGreaterThan(0);
    // #242: the error is Danish — "ikke låst", "period close".
    expect(report.errors.some((e) => /låst|lukket|close/i.test(e))).toBe(true);

    db.close();
    cleanup(root, inbox);
  });

  test("fails clearly when company CVR master data is missing", () => {
    const { root, inbox, db } = newCompany("rentemester-annual-nocvr-", null);
    postYear(db, root, inbox);
    lockYear(db);

    const report = buildAnnualReport(db, "2025-01-01", "2025-12-31");
    expect(report.ok).toBe(false);
    expect(report.errors.some((e) => /cvr/i.test(e))).toBe(true);

    db.close();
    cleanup(root, inbox);
  });

  test("fails clearly when the fiscal year is only partially locked", () => {
    const { root, inbox, db } = newCompany("rentemester-annual-partial-");
    // Closing a *narrower* period than the requested year means the year is
    // not fully covered by a locked period — it must not be reported.
    postYear(db, root, inbox);
    closeAccountingPeriod(db, {
      periodStart: "2025-01-01",
      periodEnd: "2025-06-30",
      kind: "custom",
      status: "closed",
      createdBy: "agent:test",
    });

    const report = buildAnnualReport(db, "2025-01-01", "2025-12-31");
    expect(report.ok).toBe(false);
    expect(report.errors.length).toBeGreaterThan(0);
    // #242: the error is Danish — "ikke låst", "period close".
    expect(report.errors.some((e) => /låst|lukket|close/i.test(e))).toBe(true);

    db.close();
    cleanup(root, inbox);
  });

  test("rejects invalid fiscal-year dates", () => {
    const { root, inbox, db } = newCompany("rentemester-annual-baddate-");
    const report = buildAnnualReport(db, "not-a-date", "2025-12-31");
    expect(report.ok).toBe(false);
    expect(report.errors.length).toBeGreaterThan(0);

    db.close();
    cleanup(root, inbox);
  });
});

describe("generateIxbrl (deterministic iXBRL, micro/small subset)", () => {
  test("produces XHTML+iXBRL that uses only the declared taxonomy subset", () => {
    const { root, inbox, db } = newCompany("rentemester-ixbrl-ok-");
    postYear(db, root, inbox);
    lockYear(db);
    const report = buildAnnualReport(db, "2025-01-01", "2025-12-31");
    expect(report.ok).toBe(true);

    const ixbrl = generateIxbrl(report);
    expect(ixbrl.ok).toBe(true);
    expect(ixbrl.xhtml).toContain("<?xml");
    expect(ixbrl.xhtml).toContain("xmlns:ix=");
    // iXBRL facts present for the core regnskabsklasse-B elements.
    for (const element of IXBRL_TAXONOMY_SUBSET.elements) {
      // Every declared element name appears as an ix fact name attribute.
      expect(ixbrl.xhtml).toContain(`name="${element.name}"`);
    }
    // The CVR identifier is in the xbrli context entity.
    expect(ixbrl.xhtml).toContain("12345678");
    // No element outside the declared subset is emitted: every ix:nonFraction /
    // ix:nonNumeric name attribute must be a known taxonomy element.
    const declared = new Set(IXBRL_TAXONOMY_SUBSET.elements.map((e) => e.name));
    for (const match of ixbrl.xhtml.matchAll(/<ix:(?:nonFraction|nonNumeric)[^>]*\bname="([^"]+)"/g)) {
      expect(declared.has(match[1])).toBe(true);
    }

    db.close();
    cleanup(root, inbox);
  });

  test("is byte-stable across reruns for identical input", () => {
    const a = newCompany("rentemester-ixbrl-det-a-");
    const b = newCompany("rentemester-ixbrl-det-b-");
    for (const c of [a, b]) {
      postYear(c.db, c.root, c.inbox);
      lockYear(c.db);
    }
    const ixbrlA = generateIxbrl(buildAnnualReport(a.db, "2025-01-01", "2025-12-31"));
    const ixbrlB = generateIxbrl(buildAnnualReport(b.db, "2025-01-01", "2025-12-31"));
    expect(ixbrlA.ok).toBe(true);
    expect(ixbrlA.xhtml).toBe(ixbrlB.xhtml);
    expect(ixbrlA.sha256).toBe(ixbrlB.sha256);

    a.db.close();
    b.db.close();
    cleanup(a.root, a.inbox, b.root, b.inbox);
  });

  test("refuses to generate iXBRL from a failed annual report", () => {
    const { root, inbox, db } = newCompany("rentemester-ixbrl-fail-", null);
    postYear(db, root, inbox);
    lockYear(db);
    const report = buildAnnualReport(db, "2025-01-01", "2025-12-31");
    expect(report.ok).toBe(false);

    const ixbrl = generateIxbrl(report);
    expect(ixbrl.ok).toBe(false);
    expect(ixbrl.errors.length).toBeGreaterThan(0);

    db.close();
    cleanup(root, inbox);
  });

  test("declares it covers only the micro/small taxonomy subset", () => {
    expect(IXBRL_TAXONOMY_SUBSET.scope).toMatch(/micro|small|klasse B/i);
    expect(IXBRL_TAXONOMY_SUBSET.elements.length).toBeGreaterThan(0);
    // Every element carries a stable name and an xbrli item type.
    for (const element of IXBRL_TAXONOMY_SUBSET.elements) {
      expect(typeof element.name).toBe("string");
      expect(element.name.length).toBeGreaterThan(0);
    }
  });

  // #177 expansion: the subset is explicitly versioned so it never looks like
  // a finished full Erhvervsstyrelsen taxonomy.
  test("is clearly versioned and named as a partial subset", () => {
    expect(typeof IXBRL_TAXONOMY_SUBSET.name).toBe("string");
    expect(IXBRL_TAXONOMY_SUBSET.name.length).toBeGreaterThan(0);
    // A semantic-ish version string, e.g. "0.2.0".
    expect(IXBRL_TAXONOMY_SUBSET.version).toMatch(/^\d+\.\d+\.\d+$/);
    // The name signals it is bounded/partial, not the full taxonomy.
    expect(IXBRL_TAXONOMY_SUBSET.name).toMatch(/subset|udsnit|partial|bounded/i);
  });

  // #177 expansion: every element is assigned to one of the four class-B
  // statement sections so the document is grouped, not a flat list.
  test("covers the four regnskabsklasse-B sections with grouped elements", () => {
    const sections = new Set(IXBRL_TAXONOMY_SUBSET.elements.map((e) => e.section));
    for (const required of [
      "income-statement",
      "balance-sheet",
      "management-statement",
      "accounting-policies",
    ] as const) {
      expect(sections.has(required)).toBe(true);
    }
    // Each section carries at least one element.
    for (const element of IXBRL_TAXONOMY_SUBSET.elements) {
      expect(typeof element.section).toBe("string");
    }
  });

  test("element names are unique across the whole subset", () => {
    const names = IXBRL_TAXONOMY_SUBSET.elements.map((e) => e.name);
    expect(new Set(names).size).toBe(names.length);
  });

  // GOLDEN TEST: the generated iXBRL must be structurally valid (well-formed
  // XML, balanced ix tags, every fact in the declared subset) and must
  // round-trip — re-parsing the document recovers every declared fact value.
  test("generated iXBRL is structurally valid and round-trips every fact", () => {
    const { root, inbox, db } = newCompany("rentemester-ixbrl-golden-");
    postYear(db, root, inbox);
    lockYear(db);
    const report = buildAnnualReport(db, "2025-01-01", "2025-12-31");
    expect(report.ok).toBe(true);

    const ixbrl = generateIxbrl(report);
    expect(ixbrl.ok).toBe(true);

    // 1. Well-formed XML: every opened tag is closed. A loose well-formedness
    //    check — count opening vs closing tags for the ix elements.
    const xhtml = ixbrl.xhtml;
    const countOpen = (re: RegExp) => (xhtml.match(re) ?? []).length;
    expect(countOpen(/<ix:nonFraction\b/g)).toBe(countOpen(/<\/ix:nonFraction>/g));
    expect(countOpen(/<ix:nonNumeric\b/g)).toBe(countOpen(/<\/ix:nonNumeric>/g));
    expect(countOpen(/<xbrli:context\b/g)).toBe(countOpen(/<\/xbrli:context>/g));
    // Both context dimensions present: a duration (P&L) and an instant (balance).
    expect(xhtml).toContain('id="duration"');
    expect(xhtml).toContain('id="instant"');

    // 2. Every declared element is emitted exactly once as an ix fact.
    for (const element of IXBRL_TAXONOMY_SUBSET.elements) {
      const occurrences = xhtml.split(`name="${element.name}"`).length - 1;
      expect(occurrences).toBe(1);
    }

    // 3. No element outside the declared subset is emitted.
    const declared = new Set(IXBRL_TAXONOMY_SUBSET.elements.map((e) => e.name));
    for (const match of xhtml.matchAll(
      /<ix:(?:nonFraction|nonNumeric)[^>]*\bname="([^"]+)"/g,
    )) {
      expect(declared.has(match[1])).toBe(true);
    }

    // 4. ROUND-TRIP: re-extract every fact from the rendered XHTML and confirm
    //    the value matches what generateIxbrl resolved (exposed via .facts).
    expect(ixbrl.facts.length).toBe(IXBRL_TAXONOMY_SUBSET.elements.length);
    for (const fact of ixbrl.facts) {
      const tag = fact.element.kind === "monetary" ? "nonFraction" : "nonNumeric";
      const re = new RegExp(
        `<ix:${tag}[^>]*\\bname="${fact.element.name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}"[^>]*>([^<]*)</ix:${tag}>`,
      );
      const m = xhtml.match(re);
      expect(m).not.toBeNull();
      // The rendered value is XML-escaped; the resolved value is raw. For the
      // deterministic test inputs there are no XML-special characters, so a
      // direct compare is a true round-trip.
      expect(m![1]).toBe(fact.value);
    }

    db.close();
    cleanup(root, inbox);
  });
});
