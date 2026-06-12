// Tests: src/agent/loop.ts + src/agent/run.ts (runtime bookkeeper agent #183)
//
// A deterministic end-to-end agent-run against a fixture company. The same
// fixture + same --as-of must produce a stable, asserted run report:
//   - the unambiguous DK expenses (DSB, Elgiganten) book automatically;
//   - everything uncertain — EU reverse-charge purchases blocked by the VIES
//     guardrail, the cash-register receipt, the Stripe payout — lands in the
//     exception queue, never guessed into a posting;
//   - upcoming VAT / year-end deadlines are surfaced.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { writeFileSync } from "node:fs";
import { initialiseCompanyVolume } from "../../src/core/company";
import { runAgentLoop } from "../../src/agent/loop";
import { formatRunReport } from "../../src/agent/run";
import { AGENT_ACTOR_ID } from "../../src/agent/contract";
import { openDb, migrate } from "../../src/core/db";
import { companyPaths } from "../../src/core/paths";
import { closeAccountingPeriod } from "../../src/core/periods";
import { ingestDocument } from "../../src/core/documents";
import { registerPayable } from "../../src/core/payables";
import { registerAccrual } from "../../src/core/accruals";
import { registerAsset, postDepreciationPeriod } from "../../src/core/assets";

const DEMO_DIR = join(import.meta.dir, "..", "..", "examples", "agent-demo");
const INBOX = join(DEMO_DIR, "inbox");
const METADATA = join(DEMO_DIR, "metadata");
const BANK_CSV = join(DEMO_DIR, "bank.csv");
const AS_OF = "2026-05-20";

function freshCompany(): string {
  const root = mkdtempSync(join(tmpdir(), "rentemester-agent-run-"));
  initialiseCompanyVolume(root, { cvr: "DK12345678" });
  return root;
}

/** A fresh company initialised on a specific VAT cadence (#318). */
function freshCompanyWithVatPeriod(vatPeriodType: "month" | "quarter" | "half-year"): string {
  const root = mkdtempSync(join(tmpdir(), "rentemester-agent-run-"));
  initialiseCompanyVolume(root, { cvr: "DK12345678", vatPeriodType });
  return root;
}

describe("runtime bookkeeper agent — deterministic agent-run (#183)", () => {
  test("books the unambiguous and routes everything uncertain to exceptions", () => {
    const root = freshCompany();
    try {
      const report = runAgentLoop({
        companyRoot: root,
        asOf: AS_OF,
        inboxDir: INBOX,
        metadataDir: METADATA,
        bankCsvPath: BANK_CSV,
      });

      expect(report.ok).toBe(true);
      expect(report.actor).toBe(AGENT_ACTOR_ID);
      expect(report.asOf).toBe(AS_OF);

      // The ordered loop ran every phase.
      expect(report.phases).toEqual([
        "ingest",
        "book",
        "route",
        "payables",
        "reconcile",
        "deadlines",
        "report",
      ]);

      // All 6 bilag ingested; 7 bank transactions imported.
      expect(report.documentsIngested).toBe(6);
      expect(report.documentsRejected).toBe(0);
      expect(report.bankTransactionsImported).toBe(7);

      // The unambiguous standard-VAT operating expenses (deterministic account
      // rule, no foreign-VAT guardrail) book automatically: DSB is a DK
      // supplier; Google Ireland bills DK VAT for Workspace so it is standard
      // too. The Elgiganten purchase is a 12.000 DKK MacBook — an asset-like
      // category — so the loop does NOT auto-book it as an operating expense;
      // it routes it for a fixed-asset decision (#223). The reverse-charge EU
      // purchases (OpenAI, AWS) do NOT auto-book either — the VIES guardrail
      // fires and the agent obeys it.
      const bookedSuppliers = report.expensesBooked.map((e) => e.supplier).sort();
      expect(bookedSuppliers).toEqual(["DSB", "Google Ireland Limited"]);
      for (const e of report.expensesBooked) {
        expect(e.journalEntryNo).toBeTruthy();
        expect(e.vatTreatment).toBe("standard");
      }
      // The asset-sized hardware purchase is never silently expensed.
      expect(bookedSuppliers).not.toContain("Elgiganten A/S");

      // Everything uncertain is in the exception queue — never guessed.
      expect(report.openExceptions.length).toBeGreaterThan(0);
      const exTypes = report.openExceptions.map((x) => x.type).sort();
      // EU reverse-charge purchases blocked by the ledger guardrail.
      expect(exTypes).toContain("AGENT_BOOKING_BLOCKED");
      // The Stripe payout + restaurant receipt are unmatched bank lines.
      expect(exTypes).toContain("UNMATCHED_BANK_TRANSACTION");
      // The 12.000 DKK MacBook is routed for a fixed-asset decision, not
      // booked straight to a P&L expense account (#223).
      expect(exTypes).toContain("AGENT_POSSIBLE_FIXED_ASSET");
      const assetException = report.openExceptions.find(
        (x) => x.type === "AGENT_POSSIBLE_FIXED_ASSET",
      );
      expect(assetException).toBeDefined();
      expect(assetException!.message).toContain("anlægsaktiv");
      expect(assetException!.requiredAction).toContain("asset register");

      // The deadline check surfaces the VAT quarter the company is currently
      // accruing in (Q2 2026, the one containing the as-of date).
      expect(report.upcomingDeadlines.length).toBeGreaterThan(0);
      const vatQuarters = report.upcomingDeadlines.filter((d) => d.kind === "vat_quarter");
      expect(vatQuarters.length).toBeGreaterThan(0);
      const currentQuarter = vatQuarters.find((d) => d.periodStart === "2026-04-01");
      expect(currentQuarter).toBeDefined();
      expect(currentQuarter!.periodEnd).toBe("2026-06-30");
      // The fiscal-year (årsrapport) obligation is surfaced too.
      expect(report.upcomingDeadlines.some((d) => d.kind === "fiscal_year")).toBe(true);

      // The summary is plain-language and non-empty.
      expect(report.summary.length).toBeGreaterThan(0);
      const formatted = formatRunReport(report);
      expect(formatted).toContain("Rentemester runtime-agent");
      expect(formatted).toContain(AGENT_ACTOR_ID);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("is deterministic: same fixture + same as-of yields an identical report", () => {
    const rootA = freshCompany();
    const rootB = freshCompany();
    try {
      const run = (root: string) =>
        runAgentLoop({
          companyRoot: root,
          asOf: AS_OF,
          inboxDir: INBOX,
          metadataDir: METADATA,
          bankCsvPath: BANK_CSV,
        });
      const a = run(rootA);
      const b = run(rootB);

      // The company root differs, so normalise it before comparing.
      const normalise = (r: ReturnType<typeof run>) => ({ ...r, company: "<root>" });
      expect(normalise(a)).toEqual(normalise(b));

      // The rendered report is byte-identical too (after normalising the root).
      const textA = formatRunReport(a).replaceAll(rootA, "<root>");
      const textB = formatRunReport(b).replaceAll(rootB, "<root>");
      expect(textA).toBe(textB);
    } finally {
      rmSync(rootA, { recursive: true, force: true });
      rmSync(rootB, { recursive: true, force: true });
    }
  });

  test("re-running the loop is idempotent and never double-books", () => {
    const root = freshCompany();
    try {
      const first = runAgentLoop({
        companyRoot: root,
        asOf: AS_OF,
        inboxDir: INBOX,
        metadataDir: METADATA,
        bankCsvPath: BANK_CSV,
      });
      const second = runAgentLoop({
        companyRoot: root,
        asOf: AS_OF,
        inboxDir: INBOX,
        metadataDir: METADATA,
        bankCsvPath: BANK_CSV,
      });
      expect(first.ok).toBe(true);
      expect(second.ok).toBe(true);
      // The second run ingests nothing new (duplicate content) and books
      // nothing new (the expenses are already booked).
      expect(second.documentsIngested).toBe(0);
      expect(second.expensesBooked.length).toBe(0);
      // The open-exception set is stable across re-runs (dedup holds).
      expect(second.openExceptions.length).toBe(first.openExceptions.length);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("rejects an invalid --as-of date without touching the ledger", () => {
    const root = freshCompany();
    try {
      const report = runAgentLoop({ companyRoot: root, asOf: "not-a-date" });
      expect(report.ok).toBe(false);
      expect(report.errors.join(" ")).toContain("--as-of");
      expect(report.expensesBooked).toEqual([]);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  // #282: the agent-run deadline note for a closed VAT period must report the
  // momstilsvar in kroner — `netVatPayable` is already a kroner amount, so
  // labelling it "øre" understates the obligation by a factor of 100.
  test("a closed VAT period's deadline note shows momstilsvar in kroner, not øre (#282)", () => {
    const root = freshCompany();
    try {
      // Close the previous VAT quarter so its deadline note fires the
      // "Momsperioden er lukket" branch with the momstilsvar amount.
      const db = openDb(companyPaths(root).db);
      migrate(db);
      const closed = closeAccountingPeriod(db, {
        periodStart: "2026-01-01",
        periodEnd: "2026-03-31",
        kind: "vat_quarter",
        createdBy: "system:test",
        createdByProgram: "agent-run-test",
      });
      expect(closed.ok).toBe(true);
      db.close();

      const report = runAgentLoop({ companyRoot: root, asOf: AS_OF });
      expect(report.ok).toBe(true);

      const closedQuarter = report.upcomingDeadlines.find(
        (d) => d.kind === "vat_quarter" && d.periodStart === "2026-01-01" && d.ready,
      );
      expect(closedQuarter).toBeDefined();
      // The note must use the kroner formatter ("kr.") and must NOT label the
      // amount as "øre".
      expect(closedQuarter!.note).toContain("momstilsvar");
      expect(closedQuarter!.note).toContain("kr.");
      expect(closedQuarter!.note).not.toContain("øre");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  // Islands → control surfaces: the agent loop must reach the accruals and
  // payables features. They are additive AgentRunReport fields — the existing
  // report shape stays backward-compatible.
  describe("agent loop wires in payables and accruals", () => {
    test("auto-settles an unambiguous creditor payment and surfaces an overdue one", () => {
      const root = freshCompany();
      const inboxDir = mkdtempSync(join(tmpdir(), "rentemester-agent-payables-inbox-"));
      try {
        const db = openDb(companyPaths(root).db);
        migrate(db);

        // Two registered creditor items: one paid by an exact bank line, one
        // left overdue with no payment.
        const billA = join(inboxDir, "bill-a.txt");
        writeFileSync(billA, "Leverandørbilag A\n2500 DKK\n");
        const docA = ingestDocument(db, root, billA, {
          source: "email",
          issueDate: "2026-03-01",
          invoiceNo: "KRED-A",
          deliveryDescription: "Leverandørydelse",
          amountIncVat: 2500,
          currency: "DKK",
          sender: { name: "Kreditor A ApS", address: "Vej 1", vatOrCvr: "DK11112222" },
          recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
          vatAmount: 500,
          paymentDetails: "Bank",
        });
        expect(docA.ok).toBe(true);
        const payableA = registerPayable(db, {
          documentId: docA.documentId!,
          billDate: "2026-03-01",
          dueDate: "2026-03-31",
          expenseAccountNo: "3000",
        });
        expect(payableA.ok).toBe(true);

        const billB = join(inboxDir, "bill-b.txt");
        writeFileSync(billB, "Leverandørbilag B\n4000 DKK\n");
        const docB = ingestDocument(db, root, billB, {
          source: "email",
          issueDate: "2026-01-05",
          invoiceNo: "KRED-B",
          deliveryDescription: "Leverandørydelse",
          amountIncVat: 4000,
          currency: "DKK",
          sender: { name: "Kreditor B ApS", address: "Vej 2", vatOrCvr: "DK33334444" },
          recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
          vatAmount: 800,
          paymentDetails: "Bank",
        });
        expect(docB.ok).toBe(true);
        const payableB = registerPayable(db, {
          documentId: docB.documentId!,
          billDate: "2026-01-05",
          dueDate: "2026-02-04",
          expenseAccountNo: "3000",
        });
        expect(payableB.ok).toBe(true);
        db.close();

        // A bank CSV with one outgoing payment that exactly matches payable A.
        const bankCsv = join(inboxDir, "bank.csv");
        writeFileSync(
          bankCsv,
          "transaction_date,text,amount,currency\n2026-04-02,Betaling Kreditor A,-2500,DKK\n",
        );

        const report = runAgentLoop({ companyRoot: root, asOf: AS_OF, bankCsvPath: bankCsv });
        expect(report.ok).toBe(true);

        // The exact-match outgoing payment auto-settles creditor item A.
        expect(report.payablesMatched.length).toBe(1);
        expect(report.payablesMatched[0]!.payableId).toBe(payableA.payableId);
        expect(report.payablesMatched[0]!.journalEntryNo).toBeTruthy();

        // Creditor item B is overdue and unpaid — surfaced, never paid.
        const overdueEx = report.openExceptions.find((x) => x.type === "AGENT_PAYABLE_OVERDUE");
        expect(overdueEx).toBeDefined();
        expect(overdueEx!.message).toContain("Kreditor B ApS");

        // The payables phase ran.
        expect(report.phases).toContain("payables");
      } finally {
        rmSync(root, { recursive: true, force: true });
        rmSync(inboxDir, { recursive: true, force: true });
      }
    });

    // #cockpit-wiring-review-2: an amount-only match is NOT safe. An owner
    // draw / salary / tax payment with no bilag that happens to equal a
    // payable's open balance must NOT be auto-settled against an unrelated
    // creditor — that is a wrong write to the append-only ledger. When the
    // amount matches but there is no corroboration (counterparty/text does not
    // name the supplier), the agent SURFACES it, it does not post.
    test("does NOT auto-settle a payable on an amount-only match without corroboration", () => {
      const root = freshCompany();
      const inboxDir = mkdtempSync(join(tmpdir(), "rentemester-agent-payables-unsafe-inbox-"));
      try {
        const db = openDb(companyPaths(root).db);
        migrate(db);

        const bill = join(inboxDir, "bill.txt");
        writeFileSync(bill, "Leverandørbilag\n2500 DKK\n");
        const doc = ingestDocument(db, root, bill, {
          source: "email",
          issueDate: "2026-03-01",
          invoiceNo: "KRED-X",
          deliveryDescription: "Leverandørydelse",
          amountIncVat: 2500,
          currency: "DKK",
          sender: { name: "Webhotellet ApS", address: "Vej 1", vatOrCvr: "DK11112222" },
          recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
          vatAmount: 500,
          paymentDetails: "Bank",
        });
        expect(doc.ok).toBe(true);
        const payable = registerPayable(db, {
          documentId: doc.documentId!,
          billDate: "2026-03-01",
          dueDate: "2026-03-31",
          expenseAccountNo: "3000",
        });
        expect(payable.ok).toBe(true);
        db.close();

        // An outgoing payment of the EXACT same amount, but the text is an
        // owner draw — it names nothing about the "Webhotellet ApS" creditor.
        const bankCsv = join(inboxDir, "bank.csv");
        writeFileSync(
          bankCsv,
          "transaction_date,text,amount,currency\n2026-04-02,Privathævning ejer,-2500,DKK\n",
        );

        const report = runAgentLoop({ companyRoot: root, asOf: AS_OF, bankCsvPath: bankCsv });
        expect(report.ok).toBe(true);

        // The amount matches but nothing corroborates the supplier — the agent
        // must NOT have auto-settled the payable.
        expect(report.payablesMatched.length).toBe(0);

        // It is surfaced for the human as a suggestion, never guessed.
        const suggestEx = report.openExceptions.find(
          (x) => x.type === "AGENT_PAYABLE_MATCH_UNCERTAIN",
        );
        expect(suggestEx).toBeDefined();
        expect(suggestEx!.requiredAction).toContain("payable pay");

        // The payable is still open (unpaid) — no ledger settlement happened.
        const after = openDb(companyPaths(root).db);
        const settlement = after
          .query("SELECT COUNT(*) AS n FROM payable_payments")
          .get() as { n: number };
        after.close();
        expect(settlement.n).toBe(0);
      } finally {
        rmSync(root, { recursive: true, force: true });
        rmSync(inboxDir, { recursive: true, force: true });
      }
    });

    test("surfaces closed-year tax-return needs-review flags as exceptions", () => {
      // #cockpit-wiring-review-5: syncTaxReturnReviewExceptions must actually
      // be wired into the loop. A closed prior fiscal year with a deterministic
      // needs-review flag (book depreciation) reaches the exception queue.
      const root = freshCompany();
      const inboxDir = mkdtempSync(join(tmpdir(), "rentemester-agent-tax-inbox-"));
      try {
        const db = openDb(companyPaths(root).db);
        migrate(db);
        const bilag = join(inboxDir, "udstyr.txt");
        writeFileSync(bilag, "Driftsmiddel\n40000 DKK\n");
        const doc = ingestDocument(db, root, bilag, {
          source: "email",
          issueDate: "2025-01-15",
          invoiceNo: "EQ-2025-1",
          deliveryDescription: "Maskine",
          amountIncVat: 40000,
          currency: "DKK",
          sender: { name: "Maskinhandel ApS", address: "Vej 1", vatOrCvr: "DK55556666" },
          recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
          vatAmount: 0,
          paymentDetails: "Bank",
        });
        expect(doc.ok).toBe(true);
        const asset = registerAsset(db, {
          name: "Maskine",
          category: "equipment",
          acquisitionDate: "2025-01-15",
          cost: 40000,
          usefulLifeMonths: 60,
          purchaseDocumentId: doc.documentId!,
        });
        expect(asset.ok).toBe(true);
        expect(
          postDepreciationPeriod(db, {
            assetId: asset.assetId!,
            periodIndex: 1,
            transactionDate: "2025-02-15",
          }).ok,
        ).toBe(true);
        // Close the 2025 fiscal year so the tax return can be prepared.
        expect(
          closeAccountingPeriod(db, {
            periodStart: "2025-01-01",
            periodEnd: "2025-12-31",
            kind: "fiscal_year",
            status: "closed",
            createdBy: "system:test",
          }).ok,
        ).toBe(true);
        db.close();

        // The run's as-of date is in 2026 — the closed year is the prior one.
        const report = runAgentLoop({ companyRoot: root, asOf: AS_OF });
        expect(report.ok).toBe(true);
        const taxEx = report.openExceptions.find(
          (x) => x.type === "AGENT_TAX_RETURN_NEEDS_REVIEW",
        );
        expect(taxEx).toBeDefined();
        expect(taxEx!.message).toContain("Oplysningsskema");
      } finally {
        rmSync(root, { recursive: true, force: true });
        rmSync(inboxDir, { recursive: true, force: true });
      }
    });

    test("surfaces a due accrual recognition period without auto-posting it", () => {
      const root = freshCompany();
      const inboxDir = mkdtempSync(join(tmpdir(), "rentemester-agent-accrual-inbox-"));
      try {
        const db = openDb(companyPaths(root).db);
        migrate(db);
        const bilag = join(inboxDir, "forsikring.txt");
        writeFileSync(bilag, "Forsikring helår\n9000 DKK\n");
        const doc = ingestDocument(db, root, bilag, {
          source: "email",
          issueDate: "2026-01-05",
          invoiceNo: "FORS-1",
          deliveryDescription: "Forsikring helår",
          amountIncVat: 9000,
          currency: "DKK",
          sender: { name: "Forsikring ApS", address: "Vej 1", vatOrCvr: "DK11223344" },
          recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
          vatAmount: 0,
          paymentDetails: "Bank",
        });
        expect(doc.ok).toBe(true);
        // 3 recognition periods on the last of Jan/Feb/Mar 2026 — all due as
        // of the 2026-05-20 run date, none posted.
        const reg = registerAccrual(db, {
          accrualType: "prepaid_expense",
          description: "Forsikring Q1",
          totalAmount: 9000,
          recognitionPeriods: 3,
          firstRecognitionDate: "2026-01-31",
          registrationDate: "2026-01-05",
          resultAccountNo: "3150",
          documentId: doc.documentId!,
        });
        expect(reg.ok).toBe(true);

        // The journal-entry count before the run — the loop must NOT post any
        // recognition entry (it surfaces, it does not auto-post).
        const beforeEntries = (
          db.query("SELECT COUNT(*) AS n FROM journal_entries").get() as { n: number }
        ).n;
        db.close();

        const report = runAgentLoop({ companyRoot: root, asOf: AS_OF });
        expect(report.ok).toBe(true);
        expect(report.accrualRecognitionsDue).toBe(3);
        const accrualEx = report.openExceptions.find(
          (x) => x.type === "AGENT_ACCRUAL_RECOGNITION_DUE",
        );
        expect(accrualEx).toBeDefined();
        expect(accrualEx!.requiredAction).toContain("accrual recognize");

        // No recognition entry was posted by the loop.
        const afterDb = openDb(companyPaths(root).db);
        const afterEntries = (
          afterDb.query("SELECT COUNT(*) AS n FROM journal_entries").get() as { n: number }
        ).n;
        afterDb.close();
        expect(afterEntries).toBe(beforeEntries);
      } finally {
        rmSync(root, { recursive: true, force: true });
        rmSync(inboxDir, { recursive: true, force: true });
      }
    });
  });

  test("a deadline-only run (no inbox, no bank) still checks deadlines", () => {
    const root = freshCompany();
    try {
      const report = runAgentLoop({ companyRoot: root, asOf: AS_OF });
      expect(report.ok).toBe(true);
      expect(report.documentsIngested).toBe(0);
      expect(report.bankTransactionsImported).toBe(0);
      expect(report.phases).toContain("deadlines");
      expect(report.upcomingDeadlines.length).toBeGreaterThan(0);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  // #318 — the agent loop drives VAT-period selection through the company's
  // `vatPeriodType`. A quarterly company keeps the historical Q1..Q4 windows;
  // monthly and half-yearly companies get correct cadence-driven windows.
  describe("VAT deadlines follow the company's vatPeriodType (#318)", () => {
    test("a quarterly company still gets the three-month VAT window", () => {
      const root = freshCompanyWithVatPeriod("quarter");
      try {
        const report = runAgentLoop({ companyRoot: root, asOf: AS_OF });
        expect(report.ok).toBe(true);
        const current = report.upcomingDeadlines.find(
          (d) => d.kind === "vat_quarter" && d.periodStart === "2026-04-01",
        );
        expect(current).toBeDefined();
        // Q2 2026: Apr 1 .. Jun 30, due 1 September.
        expect(current!.periodEnd).toBe("2026-06-30");
        expect(current!.dueDate).toBe("2026-09-01");
      } finally {
        rmSync(root, { recursive: true, force: true });
      }
    });

    test("a monthly company gets a one-month VAT window and deadline", () => {
      const root = freshCompanyWithVatPeriod("month");
      try {
        const report = runAgentLoop({ companyRoot: root, asOf: AS_OF });
        expect(report.ok).toBe(true);
        // The as-of date is in May 2026 — a monthly filer's current VAT period
        // is May 2026, NOT the hardcoded Q2 quarter.
        const current = report.upcomingDeadlines.find(
          (d) => d.kind === "vat_quarter" && d.periodStart === "2026-05-01",
        );
        expect(current).toBeDefined();
        expect(current!.periodEnd).toBe("2026-05-31");
        // Filing deadline: 1st of the third month after period end (Aug 1).
        expect(current!.dueDate).toBe("2026-08-01");
        // No hardcoded quarter window leaks through.
        expect(
          report.upcomingDeadlines.some(
            (d) => d.kind === "vat_quarter" && d.periodEnd === "2026-06-30",
          ),
        ).toBe(false);
      } finally {
        rmSync(root, { recursive: true, force: true });
      }
    });

    test("a half-yearly company gets a six-month VAT window and deadline", () => {
      const root = freshCompanyWithVatPeriod("half-year");
      try {
        const report = runAgentLoop({ companyRoot: root, asOf: AS_OF });
        expect(report.ok).toBe(true);
        // May 2026 falls in the first half-year period: Jan 1 .. Jun 30.
        const current = report.upcomingDeadlines.find(
          (d) => d.kind === "vat_quarter" && d.periodStart === "2026-01-01",
        );
        expect(current).toBeDefined();
        expect(current!.periodEnd).toBe("2026-06-30");
        // Filing deadline: 1st of the third month after period end (Sep 1).
        expect(current!.dueDate).toBe("2026-09-01");
      } finally {
        rmSync(root, { recursive: true, force: true });
      }
    });

    test("a non-quarter VAT-deadline exception uses cadence-aware wording", () => {
      const root = freshCompanyWithVatPeriod("month");
      try {
        const report = runAgentLoop({ companyRoot: root, asOf: AS_OF });
        expect(report.ok).toBe(true);
        const vatEx = report.openExceptions.find((x) => x.type === "AGENT_VAT_DEADLINE_OPEN");
        expect(vatEx).toBeDefined();
        // A monthly filer's escalation reads "Momsmåneden", never the
        // quarter-specific "Momskvartalet".
        expect(vatEx!.message).toContain("Momsmåneden");
        expect(vatEx!.message).not.toContain("Momskvartalet");
      } finally {
        rmSync(root, { recursive: true, force: true });
      }
    });
  });
});
